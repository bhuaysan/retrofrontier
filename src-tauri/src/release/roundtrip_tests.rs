//! End-to-end proof that a constructed, TUF-signed release installs through the production path.
//!
//! This is the deterministic counterpart to the manual M7.5 qualification. It builds a tiny but
//! *structurally real* release — an AppImage-shaped artefact with a genuine SquashFS AppDir, a
//! zipped core, and a tarred support asset — publishes it into a signed TUF 1.0 repository, and
//! installs it with the exact `ToughTrustedReleaseSource` and `RuntimeManager` the application
//! composes. No fixture bypasses trust: the same Ed25519 thresholds, consistent snapshots, target
//! digests, safe extraction, inventory verification, and activation protocol run here.
//!
//! It needs no network, no graphical desktop, no ROM, and no real RetroArch, so it is safe as an
//! ordinary automated test.

use crate::adapters::runtime_archive::LinuxRuntimeArchiveExtractor;
use crate::adapters::runtime_paths::RuntimePaths;
use crate::adapters::runtime_process::ManagedProcessInspector;
use crate::adapters::runtime_release_source::ConfiguredReleaseSource;
use crate::adapters::runtime_source::ToughTrustedReleaseSource;
use crate::application::runtime_manager::{RetentionPolicy, StructuralSmokeValidator};
use crate::application::{RuntimeApplicationService, RuntimeManager};
use crate::domain::runtime::{RuntimeError, RuntimeSourceOrigin, RuntimeState};
use crate::release::construct::{construct_release, InputCache};
use crate::release::tuf::{publish_release, KeyDirectory};
use backhand::{FilesystemWriter, NodeHeader};
use std::io::{Cursor, Write};
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

/// A process inspector that never reports a live game, so activation is not blocked in tests.
#[derive(Debug)]
struct NoManagedProcess;

impl ManagedProcessInspector for NoManagedProcess {
    fn ensure_no_active_game(&self, _paths: &RuntimePaths) -> Result<(), RuntimeError> {
        Ok(())
    }
}

/// Build an AppImage-shaped artefact: an opaque runtime prefix followed by a real SquashFS AppDir.
///
/// The prefix deliberately contains the literal `hsqs`/`sqsh`/`shsq`/`qshs` signature table the
/// official AppImage runtime carries, so this fixture reproduces the condition that made real
/// AppImage extraction fail before the superblock-validating offset scan.
fn synthetic_appimage(retroarch_bytes: &[u8]) -> Vec<u8> {
    let mut squashfs = Vec::new();
    {
        let mut writer = FilesystemWriter::default();
        let directory = NodeHeader::new(0o755, 0, 0, 0);
        let executable = NodeHeader::new(0o755, 0, 0, 0);
        writer.push_dir_all("usr/bin", directory).unwrap();
        writer
            .push_file(
                Cursor::new(retroarch_bytes.to_vec()),
                "usr/bin/retroarch",
                executable,
            )
            .unwrap();
        // The real RetroArch AppDir reaches its entry point through a symlink, so the fixture does
        // too: a file-only AppRun would not exercise the approved in-bundle link policy.
        writer
            .push_symlink("usr/bin/retroarch", "AppRun", directory)
            .unwrap();
        writer.write(Cursor::new(&mut squashfs)).unwrap();
    }

    let mut artifact = Vec::new();
    artifact.extend_from_slice(&[0x7f, b'E', b'L', b'F']);
    artifact.resize(512, 0);
    artifact.extend_from_slice(b"hsqs\x00sqsh\x00shsq\x00qshs");
    artifact.resize(4096, 0);
    artifact.extend_from_slice(&squashfs);
    artifact
}

fn zipped_core(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut output));
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default().unix_permissions(0o755),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
        writer.finish().unwrap();
    }
    output
}

fn zipped_support_subtree() -> Vec<u8> {
    let mut output = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut output));
        let options = zip::write::SimpleFileOptions::default();
        writer
            .start_file("dolphin-emu/Sys/GC/font.bin", options)
            .unwrap();
        writer.write_all(b"managed font data").unwrap();
        writer.finish().unwrap();
    }
    output
}

/// A joypad-autoconfig-shaped zip: a versioned repository root whose driver directories hold
/// profiles, plus the licence text the real database carries.
fn zipped_joypad_autoconfig() -> Vec<u8> {
    let mut output = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut output));
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in [
            (
                "retroarch-joypad-autoconfig-fixture/udev/Sony Interactive Entertainment DualSense Wireless Controller.cfg",
                &b"input_driver = \"udev\"\ninput_device = \"Sony Interactive Entertainment DualSense Wireless Controller\"\ninput_vendor_id = \"1356\"\ninput_product_id = \"3302\"\ninput_b_btn = \"0\"\n"[..],
            ),
            (
                "retroarch-joypad-autoconfig-fixture/sdl2/Some Other Pad.cfg",
                &b"input_driver = \"sdl2\"\n"[..],
            ),
            (
                "retroarch-joypad-autoconfig-fixture/COPYING",
                &b"MIT License\n"[..],
            ),
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    output
}

fn sha256_hex(bytes: &[u8]) -> String {
    crate::release::canonical::sha256_hex(bytes)
}

/// Serve the fixture inputs from a local directory instead of the network.
///
/// The definition still pins an HTTPS URL for every input, because the schema records provenance;
/// the cache makes those inputs available without a download, which is exactly how a maintainer
/// rebuilds an approved release offline.
struct Fixture {
    _directory: TempDir,
    definition_path: std::path::PathBuf,
    cache_directory: std::path::PathBuf,
    output_directory: std::path::PathBuf,
    keys_directory: std::path::PathBuf,
}

fn build_fixture() -> Fixture {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    let cache_directory = root.join("cache");
    std::fs::create_dir_all(&cache_directory).unwrap();

    let appimage = synthetic_appimage(b"#!/bin/sh\nexit 0\n");
    let core = zipped_core("example_libretro.so", b"native core bytes");
    let support = zipped_support_subtree();
    let profiles = zipped_joypad_autoconfig();

    std::fs::write(cache_directory.join("runtime-input"), &appimage).unwrap();
    std::fs::write(cache_directory.join("core-input"), &core).unwrap();
    std::fs::write(cache_directory.join("support-input"), &support).unwrap();
    std::fs::write(cache_directory.join("joypad-autoconfig-input"), &profiles).unwrap();

    // The support component's target artefact is the repackaged tar, so its pin is computed from
    // the same deterministic repackager construction will run.
    let support_tar = crate::release::inventory::repackage_zip_subtree_as_tar(
        &cache_directory.join("support-input"),
        "dolphin-emu/Sys",
        1024 * 1024,
    )
    .unwrap();
    let profiles_tar = crate::release::inventory::repackage_zip_subtree_as_tar(
        &cache_directory.join("joypad-autoconfig-input"),
        "retroarch-joypad-autoconfig-fixture",
        1024 * 1024,
    )
    .unwrap();

    let definition = serde_json::json!({
        "schema_version": 1,
        "manifest_id": "roundtrip-manifest",
        "release_id": "roundtrip-release-001",
        "release_sequence": 1,
        "channel": "stable",
        "min_retrofrontier_version": "0.1.0",
        "retrofrontier_runtime_version": "1",
        "retroarch_version": "1.22.2",
        "retroarch_core_api": "1",
        "save_state_policy": "isolated",
        "manifest_target_name": "roundtrip.manifest.json",
        "policy_target_name": "runtime-policy.json",
        "minimum_safe_release_sequence": 1,
        "app_run_path": "runtime/retroarch/AppRun",
        "inputs": [
            {
                "id": "runtime-input",
                "url": "https://buildbot.invalid/RetroArch.AppImage",
                "sha256": sha256_hex(&appimage),
                "size_bytes": appimage.len(),
                "license": "GPL-3.0-only",
                "provenance": "round-trip fixture"
            },
            {
                "id": "core-input",
                "url": "https://buildbot.invalid/example_libretro.so.zip",
                "sha256": sha256_hex(&core),
                "size_bytes": core.len(),
                "license": "GPL-2.0-or-later",
                "provenance": "round-trip fixture"
            },
            {
                "id": "support-input",
                "url": "https://buildbot.invalid/Dolphin.zip",
                "sha256": sha256_hex(&support),
                "size_bytes": support.len(),
                "license": "GPL-2.0-or-later",
                "provenance": "round-trip fixture"
            },
            {
                "id": "joypad-autoconfig-input",
                "url": "https://codeload.invalid/retroarch-joypad-autoconfig/zip/fixture",
                "sha256": sha256_hex(&profiles),
                "size_bytes": profiles.len(),
                "license": "MIT",
                "provenance": "round-trip fixture"
            }
        ],
        "components": [
            {
                "id": "retroarch",
                "kind": "runtime",
                "target_name": "retroarch.AppImage",
                "archive_format": "app_image",
                "install_path": "runtime/retroarch",
                "executable_relative_path": "usr/bin/retroarch",
                "display_version": "1.22.2",
                "source_revision": "fixture",
                "license": "GPL-3.0-only",
                "systems": [],
                "derivation": { "kind": "upstream_file", "input": "runtime-input" },
                "artifact_sha256": sha256_hex(&appimage),
                "artifact_size_bytes": appimage.len()
            },
            {
                "id": "nestopia",
                "kind": "core",
                "target_name": "example_libretro.so.zip",
                "archive_format": "zip",
                "install_path": "cores/nestopia",
                "executable_relative_path": "example_libretro.so",
                "display_version": null,
                "source_revision": null,
                "license": "GPL-2.0-or-later",
                "systems": ["nes"],
                "derivation": { "kind": "upstream_file", "input": "core-input" },
                "artifact_sha256": sha256_hex(&core),
                "artifact_size_bytes": core.len()
            },
            {
                "id": "dolphin-sys",
                "kind": "support_asset",
                "target_name": "dolphin-sys.tar",
                "archive_format": "tar",
                "install_path": "runtime/support/dolphin-sys",
                "executable_relative_path": null,
                "display_version": null,
                "source_revision": null,
                "license": "GPL-2.0-or-later",
                "systems": [],
                "derivation": {
                    "kind": "zip_subtree_tar",
                    "input": "support-input",
                    "subtree": "dolphin-emu/Sys"
                },
                "artifact_sha256": sha256_hex(&support_tar),
                "artifact_size_bytes": support_tar.len()
            },
            {
                "id": "joypad-autoconfig",
                "kind": "support_asset",
                "target_name": "joypad-autoconfig.tar",
                "archive_format": "tar",
                "install_path": "runtime/support/joypad-autoconfig",
                "executable_relative_path": null,
                "display_version": "fixture",
                "source_revision": "fixture",
                "license": "MIT",
                "systems": [],
                "derivation": {
                    "kind": "zip_subtree_tar",
                    "input": "joypad-autoconfig-input",
                    "subtree": "retroarch-joypad-autoconfig-fixture"
                },
                "artifact_sha256": sha256_hex(&profiles_tar),
                "artifact_size_bytes": profiles_tar.len()
            }
        ]
    });

    let definition_path = root.join("release.json");
    std::fs::write(
        &definition_path,
        serde_json::to_vec_pretty(&definition).unwrap(),
    )
    .unwrap();

    Fixture {
        definition_path,
        cache_directory,
        output_directory: root.join("out"),
        keys_directory: root.join("keys"),
        _directory: directory,
    }
}

fn directory_url(path: &Path) -> url::Url {
    // `Url::from_directory_path` guarantees the trailing slash the TUF client needs to join names.
    url::Url::from_directory_path(path).expect("temporary paths are absolute")
}

fn runtime_manager(paths: RuntimePaths, source: Arc<ToughTrustedReleaseSource>) -> RuntimeManager {
    RuntimeManager::new(
        paths,
        source,
        Arc::new(LinuxRuntimeArchiveExtractor),
        Arc::new(NoManagedProcess),
        Arc::new(StructuralSmokeValidator),
        RetentionPolicy::default(),
    )
    .unwrap()
}

#[tokio::test]
async fn a_constructed_release_installs_and_launches_through_the_real_tuf_path() {
    let fixture = build_fixture();
    let cache = InputCache::new(fixture.cache_directory.clone(), false);
    let release = construct_release(&fixture.definition_path, &fixture.output_directory, &cache)
        .await
        .expect("construction, manifest validation, and proof extraction succeed");
    let published = publish_release(
        &release,
        &fixture.output_directory,
        &KeyDirectory::new(fixture.keys_directory.clone()),
    )
    .await
    .expect("the release publishes into a signed TUF repository");

    let app_data = TempDir::new().unwrap();
    let paths = RuntimePaths::new(app_data.path());
    paths.prepare().unwrap();
    let source = Arc::new(
        ToughTrustedReleaseSource::new(
            std::fs::read(&published.root_json).unwrap(),
            directory_url(&published.metadata_directory),
            directory_url(&published.targets_directory),
            paths.trust_datastore().to_path_buf(),
            published.policy_target_name.clone(),
        )
        .expect("the generated root is self-authenticating"),
    );
    let manager = runtime_manager(paths.clone(), source.clone());

    assert_eq!(
        manager.status().unwrap().state,
        RuntimeState::NotInstalled,
        "a clean installation starts with no managed runtime"
    );

    let service = RuntimeApplicationService::new(manager.clone()).with_release_source(Some(
        ConfiguredReleaseSource {
            origin: RuntimeSourceOrigin::Qualification,
            source,
            manifest_target_name: published.manifest_target_name.clone(),
        },
    ));

    let before = service.get_install_state().unwrap();
    assert!(before.source_configured);
    assert_eq!(
        before.source_origin,
        Some(RuntimeSourceOrigin::Qualification)
    );
    assert_eq!(before.status.state, RuntimeState::NotInstalled);

    let response = service.install_runtime().await;
    assert!(
        response.installed,
        "installation failed: {:?}",
        response.error
    );
    assert_eq!(response.status.state, RuntimeState::Ready);
    assert_eq!(
        response.status.release_id.as_deref(),
        Some("roundtrip-release-001")
    );

    // The launch boundary M7 depends on must resolve from the same verified installation.
    let launch = manager
        .verified_launch_runtime()
        .expect("the installed runtime is launchable");
    assert!(launch.app_run_path.ends_with("runtime/retroarch/AppRun"));
    assert!(
        std::fs::symlink_metadata(&launch.app_run_path)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the AppDir entry point stays the authenticated symlink, not an inner payload path"
    );

    let core_id = crate::domain::runtime::SafeIdentifier::new("nestopia").unwrap();
    let core = launch
        .cores
        .get(&core_id)
        .expect("the approved core resolves");
    assert!(core.core_path.is_file());
    assert_eq!(
        core.systems,
        vec![crate::domain::runtime::SafeIdentifier::new("nes").unwrap()],
        "the release, not only the local catalog, approves the core for its system"
    );

    let support_id = crate::domain::runtime::SafeIdentifier::new("dolphin-sys").unwrap();
    let support = launch
        .support_assets
        .get(&support_id)
        .expect("the managed support component resolves");
    assert!(
        support.join("GC/font.bin").is_file(),
        "the support component is re-rooted at the directory the core expects"
    );

    // B2/B5: the managed controller profiles are an authenticated component of the release, they
    // survive the trusted path intact, and the driver directory RetroArch really scans is present
    // with a profile whose device identity matches a physical pad. Asserting only that a directory
    // exists is what let the empty `runtime-user/autoconfig` pass unnoticed for a whole milestone.
    let profiles_id = crate::domain::runtime::SafeIdentifier::new(
        crate::services::retroarch::JOYPAD_AUTOCONFIG_COMPONENT,
    )
    .unwrap();
    let profiles = launch
        .support_assets
        .get(&profiles_id)
        .expect("the managed controller-profile component resolves");
    let dualsense = profiles
        .join(crate::services::retroarch::MANAGED_JOYPAD_DRIVER)
        .join("Sony Interactive Entertainment DualSense Wireless Controller.cfg");
    assert!(
        dualsense.is_file(),
        "the udev DualSense profile is installed"
    );
    let profile = std::fs::read_to_string(&dualsense).unwrap();
    for expected in [
        "input_driver = \"udev\"",
        "input_device = \"Sony Interactive Entertainment DualSense Wireless Controller\"",
        "input_vendor_id = \"1356\"",
        "input_product_id = \"3302\"",
    ] {
        assert!(
            profile.contains(expected),
            "{expected} must be in the profile"
        );
    }
    assert!(
        profiles.join("COPYING").is_file(),
        "the database's own licence text is redistributed with it"
    );

    // B3: the launch boundary resolves that verified tree, and the generated configuration names it.
    let resolved =
        crate::services::retroarch::RetroArchService::resolve_controller_profiles(&launch).unwrap();
    assert_eq!(&resolved, profiles);
    let launch_paths = crate::services::retroarch_paths::LaunchPaths::new(app_data.path());
    let generated = crate::services::retroarch_config::RetroArchConfig::build(
        &launch_paths,
        core.core_path.parent().unwrap(),
        &resolved,
    );
    assert_eq!(
        generated.value("joypad_autoconfig_dir").map(Path::new),
        Some(resolved.as_path())
    );
    // B6: nothing else about the generated contract moved.
    assert_eq!(generated.value("video_fullscreen"), Some("true"));
    assert_eq!(generated.value("video_windowed_fullscreen"), Some("true"));
    assert_eq!(generated.value("config_save_on_exit"), Some("false"));
    // B7: process identity, runtime entry point, core, and BIOS authority are untouched.
    assert!(launch.app_run_path.ends_with("runtime/retroarch/AppRun"));
    assert_eq!(
        generated.value("system_directory").map(Path::new),
        Some(launch_paths.system_root().as_path())
    );

    // Nothing outside the authenticated inventory may exist in the installed tree.
    crate::adapters::runtime_installed::verify_tree(
        &paths.version_path(&launch.installation_id),
        &release.manifest,
    )
    .expect("the installed tree matches its authenticated inventory exactly");
}

/// B1: the defect this pass fixes, stated as a test.
///
/// Before this change the generated configuration named a private, always-empty
/// `runtime-user/autoconfig` directory. RetroArch therefore detected a real pad on `udev` and then
/// reported it *unconfigured*, because no profile could match — an unconfigured pad has no RetroPad
/// binds at all, which is exactly the "controller does nothing inside the game" the operator saw.
/// The launch layer now refuses a runtime that cannot provide the profiles rather than starting a
/// game whose controller cannot work, and it never composes a writable profile directory again.
#[tokio::test]
async fn a_release_without_managed_controller_profiles_cannot_launch() {
    let fixture = build_fixture();
    let cache = InputCache::new(fixture.cache_directory.clone(), false);
    let release = construct_release(&fixture.definition_path, &fixture.output_directory, &cache)
        .await
        .unwrap();

    let mut without = crate::application::runtime_manager::VerifiedLaunchRuntime {
        status: crate::domain::runtime::RuntimeStatus {
            state: RuntimeState::Ready,
            installation_id: Some("install-1".to_owned()),
            release_id: Some("roundtrip-release-001".to_owned()),
            can_rollback: false,
            repair_required: false,
        },
        installation_id: crate::domain::runtime::SafeIdentifier::new("install-1").unwrap(),
        release_id: release.manifest.release.release_id.clone(),
        app_run_path: fixture.output_directory.join("runtime/retroarch/AppRun"),
        cores: Default::default(),
        support_assets: Default::default(),
    };

    assert_eq!(
        crate::services::retroarch::RetroArchService::resolve_controller_profiles(&without)
            .unwrap_err()
            .code,
        crate::domain::launch::LaunchErrorCode::RuntimeNotReady
    );

    // The old shape — a writable, empty directory RetroFrontier composed itself — is gone: no
    // `LaunchPaths` directory is created for controller profiles any more, so nothing can silently
    // present an empty profile tree to RetroArch again.
    let app_data = TempDir::new().unwrap();
    let launch_paths =
        crate::services::retroarch_paths::LaunchPaths::new(app_data.path().join("RetroFrontier"));
    launch_paths.prepare().unwrap();
    assert!(
        !launch_paths.runtime_user_root().join("autoconfig").exists(),
        "no writable controller-profile directory may be composed"
    );

    // A component that is present but empty is refused too, for the same reason.
    let hollow = app_data.path().join("hollow");
    std::fs::create_dir_all(&hollow).unwrap();
    without.support_assets.insert(
        crate::domain::runtime::SafeIdentifier::new(
            crate::services::retroarch::JOYPAD_AUTOCONFIG_COMPONENT,
        )
        .unwrap(),
        hollow,
    );
    assert!(
        crate::services::retroarch::RetroArchService::resolve_controller_profiles(&without)
            .is_err()
    );
}

#[tokio::test]
async fn a_tampered_target_is_refused_and_no_runtime_is_activated() {
    let fixture = build_fixture();
    let cache = InputCache::new(fixture.cache_directory.clone(), false);
    let release = construct_release(&fixture.definition_path, &fixture.output_directory, &cache)
        .await
        .unwrap();
    let published = publish_release(
        &release,
        &fixture.output_directory,
        &KeyDirectory::new(fixture.keys_directory.clone()),
    )
    .await
    .unwrap();

    // Replace one published target's bytes while leaving trusted metadata untouched. This is the
    // compromised-mirror case: HTTPS would still succeed, and only authentication catches it.
    let core_target = std::fs::read_dir(&published.targets_directory)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("example_libretro.so.zip"))
        })
        .expect("the core target is published");
    let mut tampered = std::fs::read(&core_target).unwrap();
    tampered.extend_from_slice(b"malicious payload");
    std::fs::write(&core_target, tampered).unwrap();

    let app_data = TempDir::new().unwrap();
    let paths = RuntimePaths::new(app_data.path());
    paths.prepare().unwrap();
    let source = Arc::new(
        ToughTrustedReleaseSource::new(
            std::fs::read(&published.root_json).unwrap(),
            directory_url(&published.metadata_directory),
            directory_url(&published.targets_directory),
            paths.trust_datastore().to_path_buf(),
            published.policy_target_name.clone(),
        )
        .unwrap(),
    );
    let manager = runtime_manager(paths.clone(), source);

    let result = manager.install(&published.manifest_target_name).await;
    assert!(result.is_err(), "a tampered target must not install");
    assert_eq!(
        manager.status().unwrap().state,
        RuntimeState::NotInstalled,
        "a refused installation leaves no activated runtime behind"
    );
}
