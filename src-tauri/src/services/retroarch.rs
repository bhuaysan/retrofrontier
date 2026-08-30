// `LaunchFailure` is the normalized launch contract: a stable code, a fixed safe message, and
// typed context. Boxing it to satisfy the large-error lint would obscure that contract for no
// behavioural gain, and a launch failure is never on a hot path.
#![allow(clippy::result_large_err)]

use crate::adapters::game_process::{GameProcessLauncher, SpawnRequest, SpawnedGame};
use crate::application::runtime_manager::VerifiedLaunchRuntime;
use crate::domain::core::{current_core_target, CoreId};
use crate::domain::launch::{LaunchDiagnostic, LaunchErrorCode, LaunchFailure};
use crate::domain::library::{ContentFileRole, ContentUnit, ContentUnitAvailability};
use crate::domain::runtime::SafeIdentifier;
use crate::domain::system::{SystemCatalog, SystemId};
use crate::services::retroarch_config::RetroArchConfig;
use crate::services::retroarch_env::{build_child_environment, host_environment};
use crate::services::retroarch_host::HostPrerequisiteInspector;
use crate::services::retroarch_paths::LaunchPaths;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The approved core one launch resolved, with the absolute managed paths it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCore {
    pub core_id: CoreId,
    pub component_id: SafeIdentifier,
    pub core_path: PathBuf,
    /// Verified managed support data, as (path below the system directory, managed source).
    pub support_assets: Vec<(String, PathBuf)>,
}

/// Everything the spawn step needs, fully resolved and validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchContext {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub environment: BTreeMap<String, String>,
    pub working_directory: PathBuf,
    pub config_path: PathBuf,
    pub core_path: PathBuf,
    pub content_path: PathBuf,
    pub diagnostics: Vec<LaunchDiagnostic>,
}

/// What the application layer hands to `prepare`. It contains only already-validated values.
#[derive(Debug, Clone)]
pub struct LaunchPreparation<'a> {
    pub app_run_path: &'a Path,
    pub core: &'a ResolvedCore,
    pub content_path: &'a Path,
    /// Validated user BIOS files, as (documented filename, absolute user path).
    pub bios_files: &'a [(String, PathBuf)],
}

/// Builds and starts one controlled managed RetroArch launch.
///
/// This service owns core resolution, content-target resolution, prerequisite validation,
/// configuration generation, environment construction, and spawning. It never touches SQLite.
pub struct RetroArchService {
    paths: LaunchPaths,
    launcher: Arc<dyn GameProcessLauncher>,
    host: Arc<dyn HostPrerequisiteInspector>,
}

impl RetroArchService {
    pub fn new(
        paths: LaunchPaths,
        launcher: Arc<dyn GameProcessLauncher>,
        host: Arc<dyn HostPrerequisiteInspector>,
    ) -> Self {
        Self {
            paths,
            launcher,
            host,
        }
    }

    pub fn paths(&self) -> &LaunchPaths {
        &self.paths
    }

    /// Resolve one content unit to its deterministic launch target.
    ///
    /// The primary descriptor, playlist, or standalone file is launched; a member track or disc
    /// image inside a multi-file unit never is, and multi-file content is never collapsed into a
    /// single-file model.
    pub fn resolve_content_target(
        root_path: &Path,
        unit: &ContentUnit,
    ) -> Result<PathBuf, LaunchFailure> {
        let unavailable = || LaunchFailure::new(LaunchErrorCode::ContentUnavailable);
        if unit.availability != ContentUnitAvailability::Available {
            return Err(unavailable());
        }
        let primary = unit
            .files
            .iter()
            .min_by_key(|membership| membership.ordinal)
            .ok_or_else(unavailable)?;
        if primary.ordinal != 0
            || !matches!(
                primary.role,
                ContentFileRole::Standalone
                    | ContentFileRole::Descriptor
                    | ContentFileRole::Playlist
            )
            || primary.file.availability
                != crate::domain::library::ContentFileAvailability::Available
            || primary.file.relative_path != unit.primary_relative_path
        {
            return Err(unavailable());
        }

        // Containment is re-checked after canonicalization, so a symlink or `..` inside a stored
        // relative path cannot make the launch target escape its content root.
        let root = fs::canonicalize(root_path).map_err(|_| unavailable())?;
        let target =
            fs::canonicalize(root.join(&unit.primary_relative_path)).map_err(|_| unavailable())?;
        if !target.starts_with(&root) {
            return Err(unavailable());
        }
        if !fs::symlink_metadata(&target).is_ok_and(|metadata| metadata.is_file()) {
            return Err(unavailable());
        }
        Ok(target)
    }

    /// Resolve the approved core: a valid per-game override, otherwise the approved system
    /// default, and only when the managed runtime actually has that authenticated component.
    pub fn resolve_core(
        catalog: &SystemCatalog,
        system_id: SystemId,
        core_override: Option<&CoreId>,
        runtime: &VerifiedLaunchRuntime,
    ) -> Result<ResolvedCore, LaunchFailure> {
        let system = catalog
            .system(system_id)
            .ok_or_else(|| LaunchFailure::new(LaunchErrorCode::CorePolicyUnresolved))?;
        // An unresolved system approves nothing, so a stale override cannot make it launchable.
        if !matches!(
            system.core_policy.decision,
            crate::domain::core::CorePolicyDecision::Resolved
        ) {
            return Err(
                LaunchFailure::new(LaunchErrorCode::CorePolicyUnresolved).with_system(system_id)
            );
        }

        let core_id = match core_override {
            Some(core_id) => {
                if !catalog.approves_core_for_system(system_id, core_id) {
                    return Err(LaunchFailure::new(LaunchErrorCode::CoreNotApproved)
                        .with_system(system_id)
                        .with_core(core_id.clone()));
                }
                core_id.clone()
            }
            None => system.core_policy.default_core_id.clone().ok_or_else(|| {
                LaunchFailure::new(LaunchErrorCode::CorePolicyUnresolved).with_system(system_id)
            })?,
        };

        let definition = catalog.core(&core_id).ok_or_else(|| {
            LaunchFailure::new(LaunchErrorCode::CoreNotApproved)
                .with_system(system_id)
                .with_core(core_id.clone())
        })?;
        let Some(target) = current_core_target() else {
            return Err(LaunchFailure::new(LaunchErrorCode::CoreNotApproved)
                .with_system(system_id)
                .with_core(core_id));
        };
        if !definition.supports_target(target) {
            return Err(LaunchFailure::new(LaunchErrorCode::CoreNotApproved)
                .with_system(system_id)
                .with_core(core_id));
        }

        let component_id = SafeIdentifier::new(definition.managed_component_id.as_str())
            .map_err(|_| LaunchFailure::new(LaunchErrorCode::InternalLaunchFailure))?;
        let not_installed = || {
            LaunchFailure::new(LaunchErrorCode::CoreNotInstalled)
                .with_system(system_id)
                .with_core(core_id.clone())
        };
        let component = runtime.cores.get(&component_id).ok_or_else(not_installed)?;
        if !fs::symlink_metadata(&component.core_path).is_ok_and(|metadata| metadata.is_file()) {
            return Err(not_installed());
        }
        // The authenticated release, not only the local catalog, must approve this core for the
        // system being launched.
        let release_system = SafeIdentifier::new(system_id.as_str())
            .map_err(|_| LaunchFailure::new(LaunchErrorCode::InternalLaunchFailure))?;
        if !component.systems.contains(&release_system) {
            return Err(LaunchFailure::new(LaunchErrorCode::CoreNotApproved)
                .with_system(system_id)
                .with_core(core_id));
        }

        let mut support_assets = Vec::new();
        for asset in &definition.support_assets {
            let asset_component = SafeIdentifier::new(asset.component_id.as_str())
                .map_err(|_| LaunchFailure::new(LaunchErrorCode::InternalLaunchFailure))?;
            // Support data comes only from the verified managed runtime boundary; an arbitrary
            // user directory is never accepted as a substitute.
            let source = runtime
                .support_assets
                .get(&asset_component)
                .ok_or_else(not_installed)?;
            if !fs::symlink_metadata(source).is_ok_and(|metadata| metadata.is_dir()) {
                return Err(not_installed());
            }
            support_assets.push((asset.system_directory_path.clone(), source.clone()));
        }

        Ok(ResolvedCore {
            core_id,
            component_id,
            core_path: component.core_path.clone(),
            support_assets,
        })
    }

    /// Prepare the controlled launch: owned directories, composed system directory, generated
    /// configuration, constructed environment, and host prerequisite validation.
    pub fn prepare(&self, request: LaunchPreparation<'_>) -> Result<LaunchContext, LaunchFailure> {
        let config_failed = || LaunchFailure::new(LaunchErrorCode::ConfigPreparationFailed);
        self.paths.prepare().map_err(|_| config_failed())?;
        self.compose_system_directory(request.bios_files, &request.core.support_assets)?;

        let core_directory = request
            .core
            .core_path
            .parent()
            .ok_or_else(config_failed)?
            .to_path_buf();
        let config_path = self.paths.config_file();
        RetroArchConfig::build(&self.paths, &core_directory)
            .write(&config_path)
            .map_err(|_| config_failed())?;

        let environment = build_child_environment(&self.paths, &host_environment());
        let mut diagnostics = Vec::new();
        for prerequisite in self.host.inspect(&environment) {
            if prerequisite.blocks_launch() {
                return Err(LaunchFailure::new(LaunchErrorCode::HostPrerequisiteMissing)
                    .with_host_prerequisite(prerequisite));
            }
            diagnostics.push(LaunchDiagnostic::new(prerequisite));
        }

        Ok(LaunchContext {
            arguments: vec![
                OsString::from("--config"),
                config_path.clone().into_os_string(),
                OsString::from("-L"),
                request.core.core_path.clone().into_os_string(),
                request.content_path.to_path_buf().into_os_string(),
            ],
            program: request.app_run_path.to_path_buf(),
            environment,
            // A RetroFrontier-owned working directory means a relative path can never resolve
            // into user content.
            working_directory: self.paths.runtime_user_root().to_path_buf(),
            config_path,
            core_path: request.core.core_path.clone(),
            content_path: request.content_path.to_path_buf(),
            diagnostics,
        })
    }

    pub fn spawn(&self, context: &LaunchContext) -> Result<SpawnedGame, LaunchFailure> {
        self.launcher
            .spawn(&SpawnRequest {
                program: context.program.clone(),
                arguments: context.arguments.clone(),
                environment: context.environment.clone(),
                working_directory: context.working_directory.clone(),
            })
            .map_err(|error| {
                tracing::warn!(error = %error, "the managed RetroArch process could not be started");
                LaunchFailure::new(LaunchErrorCode::SpawnFailed)
            })
    }

    /// Compose the RetroArch system directory RetroFrontier owns.
    ///
    /// Validated user BIOS files and verified managed support data are linked in; the user's own
    /// files are never modified, moved, renamed, or copied, and the managed runtime tree never
    /// receives user data. Only links RetroFrontier created are replaced.
    fn compose_system_directory(
        &self,
        bios_files: &[(String, PathBuf)],
        support_assets: &[(String, PathBuf)],
    ) -> Result<(), LaunchFailure> {
        let config_failed = || LaunchFailure::new(LaunchErrorCode::ConfigPreparationFailed);
        let system_root = self.paths.system_root();

        // Only previously created links are removed. An unexpected regular file or directory is
        // left alone, because RetroFrontier did not create it.
        for entry in fs::read_dir(&system_root).map_err(|_| config_failed())? {
            let entry = entry.map_err(|_| config_failed())?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|_| config_failed())?;
            if metadata.file_type().is_symlink() {
                fs::remove_file(entry.path()).map_err(|_| config_failed())?;
            }
        }

        for (filename, source) in bios_files {
            if filename.contains('/') || filename.contains('\\') || filename.starts_with('.') {
                return Err(config_failed());
            }
            link(&system_root.join(filename), source)?;
        }

        for (relative, source) in support_assets {
            let mut link_path = system_root.clone();
            for component in relative.split('/') {
                if component.is_empty() || component == "." || component == ".." {
                    return Err(config_failed());
                }
                link_path.push(component);
            }
            if let Some(parent) = link_path.parent() {
                fs::create_dir_all(parent).map_err(|_| config_failed())?;
            }
            link(&link_path, source)?;
        }
        Ok(())
    }
}

/// Replace a RetroFrontier-owned symbolic link. A pre-existing regular file or directory at the
/// same name is refused rather than silently destroyed.
fn link(link_path: &Path, target: &Path) -> Result<(), LaunchFailure> {
    let config_failed = || LaunchFailure::new(LaunchErrorCode::ConfigPreparationFailed);
    match fs::symlink_metadata(link_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            fs::remove_file(link_path).map_err(|_| config_failed())?;
        }
        Ok(_) => return Err(config_failed()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(config_failed()),
    }
    std::os::unix::fs::symlink(target, link_path).map_err(|_| config_failed())
}

#[cfg(test)]
mod tests {
    use super::{LaunchPreparation, ResolvedCore, RetroArchService};
    use crate::adapters::game_process::LinuxGameProcessLauncher;
    use crate::application::runtime_manager::{ManagedCoreComponent, VerifiedLaunchRuntime};
    use crate::domain::core::CoreId;
    use crate::domain::launch::{HostPrerequisite, LaunchErrorCode};
    use crate::domain::library::{
        ContentFile, ContentFileAvailability, ContentFileMembership, ContentFileRole,
        ContentRootId, ContentUnit, ContentUnitAvailability, ContentUnitId, ContentUnitKind,
        GameId,
    };
    use crate::domain::runtime::{RuntimeStatus, SafeIdentifier};
    use crate::domain::system::{SystemCatalog, SystemId};
    use crate::services::retroarch_host::HostPrerequisiteInspector;
    use crate::services::retroarch_paths::LaunchPaths;
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tempfile::{tempdir, TempDir};

    #[derive(Debug, Clone, Default)]
    struct StaticHostInspector {
        missing: Vec<HostPrerequisite>,
    }

    impl HostPrerequisiteInspector for StaticHostInspector {
        fn inspect(&self, _environment: &BTreeMap<String, String>) -> Vec<HostPrerequisite> {
            self.missing.clone()
        }
    }

    fn service(app_data: &Path, missing: Vec<HostPrerequisite>) -> RetroArchService {
        RetroArchService::new(
            LaunchPaths::new(app_data),
            Arc::new(LinuxGameProcessLauncher),
            Arc::new(StaticHostInspector { missing }),
        )
    }

    fn file(id: i64, relative_path: &str) -> ContentFile {
        ContentFile {
            id: crate::domain::library::ContentFileId(id),
            root_id: ContentRootId(1),
            relative_path: relative_path.to_owned(),
            size_bytes: 8,
            modified_at: 1,
            crc32: None,
            md5: None,
            sha1: None,
            availability: ContentFileAvailability::Available,
        }
    }

    fn unit(
        kind: ContentUnitKind,
        primary: &str,
        members: Vec<(i64, &str, ContentFileRole)>,
    ) -> ContentUnit {
        ContentUnit {
            id: ContentUnitId(1),
            game_id: GameId(1),
            root_id: ContentRootId(1),
            system_id: SystemId::PlayStation,
            kind,
            local_title: "Synthetic".to_owned(),
            primary_relative_path: primary.to_owned(),
            fingerprint: None,
            availability: ContentUnitAvailability::Available,
            created_at: 1,
            updated_at: 1,
            files: members
                .into_iter()
                .enumerate()
                .map(|(ordinal, (id, path, role))| ContentFileMembership {
                    ordinal: ordinal as i64,
                    role,
                    file: file(id, path),
                })
                .collect(),
        }
    }

    fn content_root(files: &[&str]) -> TempDir {
        let directory = tempdir().unwrap();
        for relative in files {
            let path = directory.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"synthetic").unwrap();
        }
        directory
    }

    /// A synthetic verified runtime: an AppRun, an approved core, and optional support data.
    fn launch_runtime(
        directory: &Path,
        component: &str,
        systems: &[SystemId],
        support: Option<(&str, &str)>,
    ) -> (VerifiedLaunchRuntime, PathBuf) {
        let installation = directory.join("runtime/versions/install-1");
        let app_run = installation.join("runtime/app/AppRun");
        fs::create_dir_all(app_run.parent().unwrap()).unwrap();
        fs::write(&app_run, "#!/bin/sh\nexit 0\n").unwrap();
        let core_path = installation.join(format!("cores/{component}/core.so"));
        fs::create_dir_all(core_path.parent().unwrap()).unwrap();
        fs::write(&core_path, b"synthetic core").unwrap();

        let mut support_assets = BTreeMap::new();
        if let Some((asset_component, relative)) = support {
            let asset = installation.join(relative);
            fs::create_dir_all(&asset).unwrap();
            support_assets.insert(SafeIdentifier::new(asset_component).unwrap(), asset);
        }

        (
            VerifiedLaunchRuntime {
                status: RuntimeStatus {
                    state: crate::domain::runtime::RuntimeState::Ready,
                    installation_id: Some("install-1".to_owned()),
                    release_id: Some("release-1".to_owned()),
                    can_rollback: false,
                    repair_required: false,
                },
                installation_id: SafeIdentifier::new("install-1").unwrap(),
                release_id: SafeIdentifier::new("release-1").unwrap(),
                app_run_path: app_run.clone(),
                cores: BTreeMap::from([(
                    SafeIdentifier::new(component).unwrap(),
                    ManagedCoreComponent {
                        component_id: SafeIdentifier::new(component).unwrap(),
                        core_path: core_path.clone(),
                        systems: systems
                            .iter()
                            .map(|system| SafeIdentifier::new(system.as_str()).unwrap())
                            .collect(),
                    },
                )]),
                support_assets,
            },
            app_run,
        )
    }

    #[test]
    fn the_primary_descriptor_playlist_or_standalone_file_is_the_launch_target() {
        let root = content_root(&[
            "PS1/Game.cue",
            "PS1/Game (Track 1).bin",
            "PS1/Game.m3u",
            "PS1/Game.chd",
        ]);

        let cue = unit(
            ContentUnitKind::CueBin,
            "PS1/Game.cue",
            vec![
                (1, "PS1/Game.cue", ContentFileRole::Descriptor),
                (2, "PS1/Game (Track 1).bin", ContentFileRole::Track),
            ],
        );
        assert_eq!(
            RetroArchService::resolve_content_target(root.path(), &cue).unwrap(),
            fs::canonicalize(root.path().join("PS1/Game.cue")).unwrap()
        );

        let playlist = unit(
            ContentUnitKind::M3u,
            "PS1/Game.m3u",
            vec![
                (3, "PS1/Game.m3u", ContentFileRole::Playlist),
                (1, "PS1/Game.cue", ContentFileRole::DiscDescriptor),
                (2, "PS1/Game (Track 1).bin", ContentFileRole::DiscTrack),
            ],
        );
        assert_eq!(
            RetroArchService::resolve_content_target(root.path(), &playlist).unwrap(),
            fs::canonicalize(root.path().join("PS1/Game.m3u")).unwrap()
        );

        let chd = unit(
            ContentUnitKind::Chd,
            "PS1/Game.chd",
            vec![(4, "PS1/Game.chd", ContentFileRole::Standalone)],
        );
        assert_eq!(
            RetroArchService::resolve_content_target(root.path(), &chd).unwrap(),
            fs::canonicalize(root.path().join("PS1/Game.chd")).unwrap()
        );
    }

    #[test]
    fn a_member_track_or_unavailable_content_is_never_launched() {
        let root = content_root(&["PS1/Game.cue", "PS1/Game (Track 1).bin"]);

        // Ordinal zero holding a track rather than the descriptor.
        let track_first = unit(
            ContentUnitKind::CueBin,
            "PS1/Game (Track 1).bin",
            vec![(2, "PS1/Game (Track 1).bin", ContentFileRole::Track)],
        );
        assert_eq!(
            RetroArchService::resolve_content_target(root.path(), &track_first)
                .unwrap_err()
                .code,
            LaunchErrorCode::ContentUnavailable
        );

        let mut incomplete = unit(
            ContentUnitKind::CueBin,
            "PS1/Game.cue",
            vec![(1, "PS1/Game.cue", ContentFileRole::Descriptor)],
        );
        incomplete.availability = ContentUnitAvailability::Incomplete;
        assert_eq!(
            RetroArchService::resolve_content_target(root.path(), &incomplete)
                .unwrap_err()
                .code,
            LaunchErrorCode::ContentUnavailable
        );

        let mut missing_file = unit(
            ContentUnitKind::CueBin,
            "PS1/Game.cue",
            vec![(1, "PS1/Game.cue", ContentFileRole::Descriptor)],
        );
        missing_file.files[0].file.availability = ContentFileAvailability::Missing;
        assert_eq!(
            RetroArchService::resolve_content_target(root.path(), &missing_file)
                .unwrap_err()
                .code,
            LaunchErrorCode::ContentUnavailable
        );

        let absent = unit(
            ContentUnitKind::SingleFile,
            "PS1/Absent.chd",
            vec![(9, "PS1/Absent.chd", ContentFileRole::Standalone)],
        );
        assert_eq!(
            RetroArchService::resolve_content_target(root.path(), &absent)
                .unwrap_err()
                .code,
            LaunchErrorCode::ContentUnavailable
        );
    }

    #[test]
    fn a_content_target_may_not_escape_its_content_root() {
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("elsewhere.chd"), b"synthetic").unwrap();
        let root = tempdir().unwrap();
        fs::create_dir_all(root.path().join("PS1")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("elsewhere.chd"),
            root.path().join("PS1/Escape.chd"),
        )
        .unwrap();

        let escaping = unit(
            ContentUnitKind::Chd,
            "PS1/Escape.chd",
            vec![(1, "PS1/Escape.chd", ContentFileRole::Standalone)],
        );

        assert_eq!(
            RetroArchService::resolve_content_target(root.path(), &escaping)
                .unwrap_err()
                .code,
            LaunchErrorCode::ContentUnavailable
        );
    }

    #[test]
    fn the_default_core_is_resolved_from_approved_policy_and_the_verified_runtime() {
        let directory = tempdir().unwrap();
        let catalog = SystemCatalog::v1();
        let (runtime, _) = launch_runtime(
            directory.path(),
            "beetle-psx",
            &[SystemId::PlayStation],
            None,
        );

        let resolved =
            RetroArchService::resolve_core(&catalog, SystemId::PlayStation, None, &runtime)
                .unwrap();

        assert_eq!(resolved.core_id, CoreId::new("beetle-psx").unwrap());
        assert!(resolved.core_path.is_absolute());
        assert!(resolved.core_path.is_file());
        assert!(resolved.support_assets.is_empty());
    }

    #[test]
    fn an_unresolved_system_is_never_launchable_even_with_an_override() {
        let directory = tempdir().unwrap();
        let catalog = SystemCatalog::v1();
        let (runtime, _) = launch_runtime(directory.path(), "nestopia", &[SystemId::Nes], None);
        let nestopia = CoreId::new("nestopia").unwrap();

        for core_override in [None, Some(&nestopia)] {
            let failure = RetroArchService::resolve_core(
                &catalog,
                SystemId::Nintendo64,
                core_override,
                &runtime,
            )
            .unwrap_err();
            assert_eq!(failure.code, LaunchErrorCode::CorePolicyUnresolved);
            assert_eq!(failure.context.system_id, Some(SystemId::Nintendo64));
        }
    }

    #[test]
    fn an_override_outside_approved_policy_never_falls_through_to_the_default() {
        let directory = tempdir().unwrap();
        let catalog = SystemCatalog::v1();
        let (runtime, _) = launch_runtime(
            directory.path(),
            "beetle-psx",
            &[SystemId::PlayStation],
            None,
        );

        // Approved for another system.
        let failure = RetroArchService::resolve_core(
            &catalog,
            SystemId::PlayStation,
            Some(&CoreId::new("nestopia").unwrap()),
            &runtime,
        )
        .unwrap_err();
        assert_eq!(failure.code, LaunchErrorCode::CoreNotApproved);

        // Not in the catalog at all.
        let failure = RetroArchService::resolve_core(
            &catalog,
            SystemId::PlayStation,
            Some(&CoreId::new("some-user-core").unwrap()),
            &runtime,
        )
        .unwrap_err();
        assert_eq!(failure.code, LaunchErrorCode::CoreNotApproved);
    }

    #[test]
    fn an_approved_core_that_is_not_installed_or_not_release_approved_cannot_launch() {
        let directory = tempdir().unwrap();
        let catalog = SystemCatalog::v1();

        // The runtime carries another core entirely.
        let (runtime, _) = launch_runtime(directory.path(), "nestopia", &[SystemId::Nes], None);
        assert_eq!(
            RetroArchService::resolve_core(&catalog, SystemId::PlayStation, None, &runtime)
                .unwrap_err()
                .code,
            LaunchErrorCode::CoreNotInstalled
        );

        // Installed, but the authenticated release does not approve it for this system.
        let other = tempdir().unwrap();
        let (runtime, _) =
            launch_runtime(other.path(), "beetle-psx", &[SystemId::SegaSaturn], None);
        assert_eq!(
            RetroArchService::resolve_core(&catalog, SystemId::PlayStation, None, &runtime)
                .unwrap_err()
                .code,
            LaunchErrorCode::CoreNotApproved
        );
    }

    #[test]
    fn dolphin_support_data_comes_only_from_the_verified_managed_runtime() {
        let directory = tempdir().unwrap();
        let catalog = SystemCatalog::v1();

        let (without, _) = launch_runtime(
            directory.path(),
            "dolphin",
            &[SystemId::NintendoGameCube],
            None,
        );
        assert_eq!(
            RetroArchService::resolve_core(&catalog, SystemId::NintendoGameCube, None, &without)
                .unwrap_err()
                .code,
            LaunchErrorCode::CoreNotInstalled
        );

        let complete = tempdir().unwrap();
        let (with, _) = launch_runtime(
            complete.path(),
            "dolphin",
            &[SystemId::NintendoGameCube],
            Some(("dolphin-sys", "runtime/support/dolphin/Sys")),
        );
        let resolved =
            RetroArchService::resolve_core(&catalog, SystemId::NintendoGameCube, None, &with)
                .unwrap();

        assert_eq!(resolved.support_assets.len(), 1);
        assert_eq!(resolved.support_assets[0].0, "dolphin-emu/Sys");
        assert!(resolved.support_assets[0]
            .1
            .starts_with(complete.path().join("runtime/versions/install-1")));
    }

    #[test]
    fn the_prepared_launch_uses_absolute_managed_paths_and_no_path_lookup() {
        let app_data = tempdir().unwrap();
        let runtime_directory = tempdir().unwrap();
        let content = content_root(&["PS1/Game.chd"]);
        let (runtime, app_run) = launch_runtime(
            runtime_directory.path(),
            "beetle-psx",
            &[SystemId::PlayStation],
            None,
        );
        let core = RetroArchService::resolve_core(
            &SystemCatalog::v1(),
            SystemId::PlayStation,
            None,
            &runtime,
        )
        .unwrap();
        let service = service(app_data.path(), Vec::new());
        let content_path = content.path().join("PS1/Game.chd");

        let context = service
            .prepare(LaunchPreparation {
                app_run_path: &app_run,
                core: &core,
                content_path: &content_path,
                bios_files: &[],
            })
            .unwrap();

        assert_eq!(context.program, app_run);
        assert!(context.program.is_absolute());
        assert!(context.core_path.is_absolute());
        assert!(context.content_path.is_absolute());
        assert_eq!(
            context.arguments,
            vec![
                std::ffi::OsString::from("--config"),
                service.paths().config_file().into_os_string(),
                std::ffi::OsString::from("-L"),
                core.core_path.clone().into_os_string(),
                content_path.clone().into_os_string(),
            ]
        );
        assert!(context.working_directory.starts_with(app_data.path()));
        assert!(context.config_path.is_file());
        assert!(context.diagnostics.is_empty());

        // The child cannot resolve a host RetroArch through PATH.
        assert_eq!(
            context.environment.get("PATH").map(String::as_str),
            Some("/usr/bin:/bin")
        );
        assert!(context
            .environment
            .get("XDG_CONFIG_HOME")
            .is_some_and(|value| value.starts_with(app_data.path().to_str().unwrap())));
    }

    #[test]
    fn the_composed_system_directory_links_validated_bios_and_managed_support_data() {
        let app_data = tempdir().unwrap();
        let runtime_directory = tempdir().unwrap();
        let bios_root = tempdir().unwrap();
        let bios_path = bios_root.path().join("scph5501.bin");
        fs::write(&bios_path, b"synthetic bios").unwrap();
        let content = content_root(&["GC/Game.rvz"]);
        let (runtime, app_run) = launch_runtime(
            runtime_directory.path(),
            "dolphin",
            &[SystemId::NintendoGameCube],
            Some(("dolphin-sys", "runtime/support/dolphin/Sys")),
        );
        let core = RetroArchService::resolve_core(
            &SystemCatalog::v1(),
            SystemId::NintendoGameCube,
            None,
            &runtime,
        )
        .unwrap();
        let service = service(app_data.path(), Vec::new());

        service
            .prepare(LaunchPreparation {
                app_run_path: &app_run,
                core: &core,
                content_path: &content.path().join("GC/Game.rvz"),
                bios_files: &[("scph5501.bin".to_owned(), bios_path.clone())],
            })
            .unwrap();

        let system_root = service.paths().system_root();
        assert_eq!(
            fs::read_link(system_root.join("scph5501.bin")).unwrap(),
            bios_path
        );
        assert_eq!(
            fs::read_link(system_root.join("dolphin-emu/Sys")).unwrap(),
            core.support_assets[0].1
        );
        // The user's own BIOS file is untouched where the user put it.
        assert_eq!(fs::read(&bios_path).unwrap(), b"synthetic bios");
        assert!(bios_root.path().join("scph5501.bin").is_file());

        // A second launch replaces only the links RetroFrontier owns.
        fs::write(system_root.join("user-placed.txt"), b"kept").unwrap();
        service
            .prepare(LaunchPreparation {
                app_run_path: &app_run,
                core: &core,
                content_path: &content.path().join("GC/Game.rvz"),
                bios_files: &[],
            })
            .unwrap();
        assert!(!system_root.join("scph5501.bin").exists());
        assert_eq!(
            fs::read(system_root.join("user-placed.txt")).unwrap(),
            b"kept"
        );
    }

    #[test]
    fn a_missing_display_session_blocks_the_launch_while_other_gaps_are_diagnostics() {
        let app_data = tempdir().unwrap();
        let runtime_directory = tempdir().unwrap();
        let content = content_root(&["PS1/Game.chd"]);
        let (runtime, app_run) = launch_runtime(
            runtime_directory.path(),
            "beetle-psx",
            &[SystemId::PlayStation],
            None,
        );
        let core = RetroArchService::resolve_core(
            &SystemCatalog::v1(),
            SystemId::PlayStation,
            None,
            &runtime,
        )
        .unwrap();
        let content_path = content.path().join("PS1/Game.chd");
        let preparation = |service: &RetroArchService| {
            service.prepare(LaunchPreparation {
                app_run_path: &app_run,
                core: &core,
                content_path: &content_path,
                bios_files: &[],
            })
        };

        let blocked = service(app_data.path(), vec![HostPrerequisite::DisplaySession]);
        let failure = preparation(&blocked).unwrap_err();
        assert_eq!(failure.code, LaunchErrorCode::HostPrerequisiteMissing);
        assert_eq!(
            failure.context.host_prerequisite,
            Some(HostPrerequisite::DisplaySession)
        );

        let degraded = service(
            app_data.path(),
            vec![
                HostPrerequisite::AudioService,
                HostPrerequisite::InputDevices,
            ],
        );
        let context = preparation(&degraded).unwrap();
        assert_eq!(context.diagnostics.len(), 2);
        assert!(context
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.is_empty()));
    }

    #[test]
    fn the_generated_configuration_never_references_a_host_retroarch_directory() {
        let app_data = tempdir().unwrap();
        let runtime_directory = tempdir().unwrap();
        let content = content_root(&["PS1/Game.chd"]);
        let (runtime, app_run) = launch_runtime(
            runtime_directory.path(),
            "beetle-psx",
            &[SystemId::PlayStation],
            None,
        );
        let core = RetroArchService::resolve_core(
            &SystemCatalog::v1(),
            SystemId::PlayStation,
            None,
            &runtime,
        )
        .unwrap();
        let service = service(app_data.path(), Vec::new());

        let context = service
            .prepare(LaunchPreparation {
                app_run_path: &app_run,
                core: &core,
                content_path: &content.path().join("PS1/Game.chd"),
                bios_files: &[],
            })
            .unwrap();

        let rendered = fs::read_to_string(&context.config_path).unwrap();
        for forbidden in [
            "/.config/retroarch",
            "/.retroarch",
            "/usr/share/libretro",
            "/etc/retroarch",
        ] {
            assert!(!rendered.contains(forbidden));
        }
        assert!(rendered.contains(&format!(
            "libretro_directory = \"{}\"",
            core.core_path.parent().unwrap().display()
        )));
        assert!(rendered.contains("config_save_on_exit = \"false\""));
    }

    #[test]
    fn a_resolved_core_carries_only_identifiers_and_managed_paths() {
        let resolved = ResolvedCore {
            core_id: CoreId::new("nestopia").unwrap(),
            component_id: SafeIdentifier::new("nestopia").unwrap(),
            core_path: PathBuf::from("/managed/cores/nestopia/nestopia_libretro.so"),
            support_assets: Vec::new(),
        };
        let identifiers: BTreeSet<_> = [resolved.core_id.as_str(), resolved.component_id.as_str()]
            .into_iter()
            .collect();

        assert_eq!(identifiers.len(), 1);
        assert!(resolved.core_path.is_absolute());
    }
}
