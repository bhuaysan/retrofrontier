//! Managed RetroArch save-state input, derived from the authenticated controller profiles.
//!
//! M9 gives the player a deterministic way to *create* a save state, because RetroFrontier
//! deliberately has no API that tells a running RetroArch to save. RetroArch stays the component
//! that writes the state; RetroFrontier only configures the hotkeys that ask it to:
//!
//! | Combination | Effect |
//! | --- | --- |
//! | Select + R1 | Save State |
//! | Select + D-Pad Right | Next state slot |
//! | Select + D-Pad Left | Previous state slot |
//!
//! There is deliberately **no RetroFrontier-provided ingame Load State hotkey**. Controlled
//! loading happens through Game Detail, where the exact historical core binary, the exact content
//! unit, and the exact file identity can all be re-proved first. A hotkey could prove none of that.
//!
//! ## Why the numbers come from the authenticated profile
//!
//! The semantic intent above is fixed, but the physical values are not: RetroArch hotkey binds are
//! raw joypad button numbers and hat directions, and they differ per device. There is no universal
//! gamepad button table to fall back on, and inventing one would bind "Save State" to whatever
//! button happens to carry that index on the player's pad.
//!
//! So the values are read out of the **authenticated managed joypad-autoconfig database** — the
//! same immutable Runtime Release support component M8 points `joypad_autoconfig_dir` at. No host
//! RetroArch location is ever consulted, nothing is downloaded, and if the qualified profiles do
//! not agree on a role, or a role is missing, RetroFrontier writes **no hotkey at all** rather
//! than a guess.
//!
//! ## The accepted limitation
//!
//! RetroArch's hotkey binds are one global set, so exactly one device's numbers can be written per
//! launch. The set is therefore derived from the qualified managed controller path — the
//! configuration M8 physically qualified — and a pad outside it gets no RetroFrontier hotkeys in
//! M9. An unresolved set never blocks a launch: a game whose controller works must keep starting,
//! and losing the save hotkey is a smaller failure than losing the game. Broader per-controller
//! hotkey coverage is B10 work.
//!
//! ## MEDIUM-2: the qualified profile files existing is not proof they apply
//!
//! The qualified profile *files* are part of the immutable managed database — they exist and agree
//! with each other regardless of what is actually plugged in. Deriving hotkeys from that agreement
//! alone would silently bind "Save State" to DualSense button numbers on a launch where the
//! player's actual pad is something else entirely, because nothing about the file tree changes
//! with what is connected.
//!
//! So resolution additionally requires the frontend's own confirmed identity of the controller it
//! currently accepts (`active_gamepad_id`, `Gamepad.id` via the browser Gamepad API — ADR-014;
//! RetroFrontier's native code never reads a controller directly). That identity is the pad that
//! currently *owns* RetroFrontier input, published by the one ownership decision the input layer
//! makes (`src/input/activeController.ts`) rather than selected a second time at launch, so it can
//! never name a different controller than the one driving the UI.
//!
//! And it must qualify by **exact match against a device identity the authenticated profiles
//! themselves declare** — their own `input_device` and `input_device_alt<N>` values — not by
//! carrying a token. A substring rule accepted `"Generic DualSense-style Adapter"` and
//! `"MyDualSenseClone"` alike and would have bound the qualified pad's raw button numbers to a
//! device nobody has measured. `None`, an unsupported mapping, an unmeasured device, or a profile
//! set that declares no device at all all resolve nothing, exactly like a missing or disagreeing
//! profile — and none of them ever fails a launch.

use std::path::Path;

/// The autoconfig keys a profile declares its own device identities under.
///
/// `input_device` is the device name RetroArch matches on; `input_device_alt1..alt<N>` are the
/// additional names the *same* physical controller reports under another connection. The qualified
/// USB DualSense profile declares
/// `input_device = "Sony Interactive Entertainment DualSense Wireless Controller"` and
/// `input_device_alt1 = "DualSense Wireless Controller"` — the Bluetooth naming — which is why the
/// aliases have to be read rather than guessed at.
const DEVICE_ROLE: &str = "input_device";
const DEVICE_ALIAS_PREFIX: &str = "input_device_alt";
/// How many device identities one profile may declare. The real profiles declare one or two.
const MAX_PROFILE_DEVICES: usize = 8;

/// Whether a frontend-confirmed active-controller identity names a physically qualified controller
/// — by **exact match against an identity the authenticated profile database itself declares**.
///
/// MEDIUM-2: this used to be `id.to_ascii_lowercase().contains("dualsense")`. A substring test
/// accepts anything that merely carries the token — `"Generic DualSense-style Adapter"`,
/// `"MyDualSenseClone"`, an unmeasured future variant — and binds RetroArch's global hotkey set to
/// the qualified profile's raw button numbers for a pad nobody has ever measured. It also
/// contradicted its own documentation, which claimed a DualSense **Edge** id resolved nothing while
/// the substring plainly accepted it.
///
/// The authority is now the immutable managed database rather than a token: `declared` is every
/// `input_device` / `input_device_alt<N>` value the qualified profiles declare, so RetroFrontier
/// accepts exactly the devices whose button numbers it is about to write and nothing else.
/// Comparison is trimmed and ASCII-case-insensitive — a kernel device name is stable, so this
/// tolerates presentation differences without ever widening to a prefix or a substring.
fn active_controller_is_qualified(active_gamepad_id: Option<&str>, declared: &[String]) -> bool {
    let Some(id) = active_gamepad_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return false;
    };
    declared
        .iter()
        .any(|device| device.trim().eq_ignore_ascii_case(id))
}

/// The managed controller device profiles the save-state hotkeys are derived from.
///
/// These are *filenames inside the authenticated database*, never a button table. They name the
/// configuration M8 physically qualified: `Linux + WebKitGTK + USB Sony DualSense`, plus the Edge
/// variant the M8 coverage note lists. The Bluetooth DualSense is reached through the same file's
/// own `input_device_alt1` alias, so it needs no separate entry.
///
/// See `docs/M8_FINAL_HARDWARE_INPUT_REPORT.md` section S.
pub const QUALIFIED_CONTROLLER_PROFILES: &[&str] = &[
    "Sony Interactive Entertainment DualSense Wireless Controller.cfg",
    "Sony Interactive Entertainment DualSense Edge Wireless Controller.cfg",
];

/// The autoconfig keys carrying the RetroPad roles the M9 combinations are built from.
const SELECT_ROLE: &str = "input_select_btn";
const SHOULDER_RIGHT_ROLE: &str = "input_r_btn";
const DPAD_RIGHT_ROLE: &str = "input_right_btn";
const DPAD_LEFT_ROLE: &str = "input_left_btn";

/// Bounds on one authenticated profile file. The real database's largest profile is a few
/// kilobytes; anything beyond this is not a profile RetroFrontier is willing to parse.
const MAX_PROFILE_BYTES: u64 = 64 * 1024;
const MAX_PROFILE_LINES: usize = 512;

/// The RetroArch configuration keys the derived values are written to.
pub const ENABLE_HOTKEY_KEY: &str = "input_enable_hotkey_btn";
pub const SAVE_STATE_KEY: &str = "input_save_state_btn";
pub const SLOT_INCREASE_KEY: &str = "input_state_slot_increase_btn";
pub const SLOT_DECREASE_KEY: &str = "input_state_slot_decrease_btn";

/// One derived set of managed save-state hotkey values.
///
/// Each value is a RetroArch joypad bind exactly as the authenticated profile expressed it — a
/// button index such as `8`, or a hat direction such as `h0right`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveStateHotkeys {
    /// The modifier that must be held: Select.
    pub enable_hotkey: String,
    /// Save State: R1.
    pub save_state: String,
    /// Next state slot: D-Pad Right.
    pub slot_increase: String,
    /// Previous state slot: D-Pad Left.
    pub slot_decrease: String,
}

/// Derive the managed save-state hotkeys from the authenticated controller profiles.
///
/// `profiles_root` is the verified immutable joypad-autoconfig component and `driver` the joypad
/// driver subdirectory RetroArch will really scan — both already resolved by
/// `RetroArchService::resolve_controller_profiles`, so this function reads nothing it was not
/// handed. `active_gamepad_id` is the frontend's own confirmed identity of the controller this
/// exact launch's player is actually using (MEDIUM-2); it is the proof requirement, not the
/// profile database.
///
/// Returns `None` — meaning *write no hotkey* — when `active_gamepad_id` does not name a
/// physically qualified DualSense, when a qualified profile is absent, is not a regular file, is a
/// symbolic link, is too large, does not declare one of the four roles, or when two qualified
/// profiles disagree about a role. Every one of those is a refusal to guess, and none of them fails
/// a launch.
pub fn resolve_managed_save_state_hotkeys(
    profiles_root: &Path,
    driver: &str,
    active_gamepad_id: Option<&str>,
) -> Option<SaveStateHotkeys> {
    let mut agreed: Option<SaveStateHotkeys> = None;
    let mut declared: Vec<String> = Vec::new();
    for profile_name in QUALIFIED_CONTROLLER_PROFILES {
        let profile = read_profile(&profiles_root.join(driver).join(profile_name))?;
        declared.extend(profile.devices);
        match &agreed {
            // Two qualified profiles that disagree cannot both be honoured by one global hotkey
            // set, and picking one would silently mis-bind the other. Refusing is the only honest
            // answer.
            Some(existing) if *existing != profile.hotkeys => return None,
            Some(_) => {}
            None => agreed = Some(profile.hotkeys),
        }
    }
    // MEDIUM-2: the proof requirement, checked against what the qualified profiles actually
    // declare rather than against a token. A profile database that names no device at all proves
    // nothing about the pad in the player's hands, so it resolves nothing either.
    if !active_controller_is_qualified(active_gamepad_id, &declared) {
        return None;
    }
    agreed
}

/// One qualified profile, as far as M9 reads it.
struct QualifiedProfile {
    /// The device identities this profile declares it applies to.
    devices: Vec<String>,
    hotkeys: SaveStateHotkeys,
}

fn read_profile(profile_path: &Path) -> Option<QualifiedProfile> {
    let metadata = std::fs::symlink_metadata(profile_path).ok()?;
    // A symbolic link would let something outside the verified tree decide what RetroFrontier
    // binds, which is exactly the authority the managed database exists to hold.
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_PROFILE_BYTES
    {
        return None;
    }
    let contents = std::fs::read_to_string(profile_path).ok()?;

    let mut select = None;
    let mut shoulder_right = None;
    let mut dpad_right = None;
    let mut dpad_left = None;
    let mut devices: Vec<String> = Vec::new();
    for line in contents.lines().take(MAX_PROFILE_LINES) {
        let Some((key, value)) = split_profile_line(line) else {
            continue;
        };
        // The device identities this profile applies to. They are ordinary text rather than joypad
        // binds, so they are read here and never passed through `is_joypad_bind`, and they never
        // reach the generated configuration — they are only ever compared against.
        if key == DEVICE_ROLE || key.starts_with(DEVICE_ALIAS_PREFIX) {
            if value.is_empty() || devices.len() >= MAX_PROFILE_DEVICES {
                continue;
            }
            devices.push(value.to_owned());
            continue;
        }
        if !is_joypad_bind(value) {
            continue;
        }
        let slot = match key {
            SELECT_ROLE => &mut select,
            SHOULDER_RIGHT_ROLE => &mut shoulder_right,
            DPAD_RIGHT_ROLE => &mut dpad_right,
            DPAD_LEFT_ROLE => &mut dpad_left,
            _ => continue,
        };
        // A profile that declares one role twice is ambiguous, so it is refused rather than
        // resolved by first-wins or last-wins.
        if slot.is_some() {
            return None;
        }
        *slot = Some(value.to_owned());
    }

    Some(QualifiedProfile {
        devices,
        hotkeys: SaveStateHotkeys {
            enable_hotkey: select?,
            save_state: shoulder_right?,
            slot_increase: dpad_right?,
            slot_decrease: dpad_left?,
        },
    })
}

/// Split one `key = "value"` line of an autoconfig profile into its unquoted halves.
///
/// This performs no interpretation: `read_profile` decides per key whether the value is a joypad
/// bind (which must satisfy `is_joypad_bind` before it can ever reach RetroFrontier's generated
/// configuration) or a device identity (which is only ever compared against, never written).
fn split_profile_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    Some((key.trim(), value.trim().trim_matches('"').trim()))
}

/// Whether a value is a RetroArch joypad bind RetroFrontier is willing to write.
///
/// Two forms exist in the real database: a decimal button index, and a hat direction of the shape
/// `h<index><direction>` — the qualified DualSense profile expresses its D-Pad as `h0left` and
/// `h0right`, which is why the hat form has to be carried through verbatim rather than normalized
/// into a number.
fn is_joypad_bind(value: &str) -> bool {
    if value.is_empty() || value.len() > 16 {
        return false;
    }
    if value.bytes().all(|byte| byte.is_ascii_digit()) {
        return true;
    }
    let Some(hat) = value.strip_prefix('h') else {
        return false;
    };
    let index_length = hat.bytes().take_while(|byte| byte.is_ascii_digit()).count();
    if index_length == 0 {
        return false;
    }
    matches!(&hat[index_length..], "up" | "down" | "left" | "right")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::{tempdir, TempDir};

    const DRIVER: &str = "udev";
    /// The exact `Gamepad.id` WebKitGTK on Linux reports for the physically qualified DualSense
    /// over USB. See `docs/M8_FINAL_HARDWARE_INPUT_REPORT.md` section M and
    /// `src/input/gamepadQuirks.ts`'s `TRANSPOSED_FACE_BUTTON_DEVICE`.
    const QUALIFIED_ACTIVE_ID: &str =
        "Sony Interactive Entertainment DualSense Wireless Controller";

    /// The four roles exactly as the real authenticated DualSense profile declares them.
    ///
    /// Verified against
    /// `runtime/support/joypad-autoconfig/udev/Sony Interactive Entertainment DualSense Wireless Controller.cfg`
    /// in the installed managed runtime: `input_select_btn = "8"`, `input_r_btn = "5"`,
    /// `input_left_btn = "h0left"`, `input_right_btn = "h0right"`.
    fn dualsense_profile() -> String {
        [
            "input_driver = \"udev\"",
            "input_device = \"Sony Interactive Entertainment DualSense Wireless Controller\"",
            "input_device_alt1 = \"DualSense Wireless Controller\"",
            "input_b_btn = \"0\"",
            "input_y_btn = \"3\"",
            "input_select_btn = \"8\"",
            "input_start_btn = \"9\"",
            "input_up_btn = \"h0up\"",
            "input_down_btn = \"h0down\"",
            "input_left_btn = \"h0left\"",
            "input_right_btn = \"h0right\"",
            "input_a_btn = \"1\"",
            "input_x_btn = \"2\"",
            "input_l_btn = \"4\"",
            "input_r_btn = \"5\"",
            "input_l2_axis = \"+2\"",
            "input_select_btn_label = \"Create\"",
            "input_r_btn_label = \"R1\"",
        ]
        .join("\n")
    }

    fn expected_dualsense_hotkeys() -> SaveStateHotkeys {
        SaveStateHotkeys {
            enable_hotkey: "8".to_owned(),
            save_state: "5".to_owned(),
            slot_increase: "h0right".to_owned(),
            slot_decrease: "h0left".to_owned(),
        }
    }

    fn profile_tree(profiles: &[(&str, String)]) -> TempDir {
        let root = tempdir().unwrap();
        fs::create_dir_all(root.path().join(DRIVER)).unwrap();
        for (name, contents) in profiles {
            fs::write(root.path().join(DRIVER).join(name), contents).unwrap();
        }
        root
    }

    fn qualified_tree() -> TempDir {
        let profiles: Vec<(&str, String)> = QUALIFIED_CONTROLLER_PROFILES
            .iter()
            .map(|name| (*name, dualsense_profile()))
            .collect();
        profile_tree(&profiles)
    }

    #[test]
    fn the_hotkeys_are_derived_from_the_authenticated_profile_and_not_from_a_constant() {
        let root = qualified_tree();

        let hotkeys =
            resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(QUALIFIED_ACTIVE_ID))
                .expect("the qualified profiles resolve");
        assert_eq!(hotkeys, expected_dualsense_hotkeys());

        // The same semantic intent on a device with different numbers produces different values,
        // which is the whole point: nothing here is a universal button table.
        let rewritten = dualsense_profile()
            .replace("input_select_btn = \"8\"", "input_select_btn = \"6\"")
            .replace("input_r_btn = \"5\"", "input_r_btn = \"7\"")
            .replace(
                "input_right_btn = \"h0right\"",
                "input_right_btn = \"h1right\"",
            )
            .replace("input_left_btn = \"h0left\"", "input_left_btn = \"h1left\"");
        let profiles: Vec<(&str, String)> = QUALIFIED_CONTROLLER_PROFILES
            .iter()
            .map(|name| (*name, rewritten.clone()))
            .collect();
        let root = profile_tree(&profiles);

        assert_eq!(
            resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(QUALIFIED_ACTIVE_ID)),
            Some(SaveStateHotkeys {
                enable_hotkey: "6".to_owned(),
                save_state: "7".to_owned(),
                slot_increase: "h1right".to_owned(),
                slot_decrease: "h1left".to_owned(),
            })
        );
    }

    #[test]
    fn a_missing_component_driver_directory_or_profile_resolves_nothing() {
        let empty = tempdir().unwrap();
        assert_eq!(
            resolve_managed_save_state_hotkeys(empty.path(), DRIVER, Some(QUALIFIED_ACTIVE_ID)),
            None
        );

        // The driver directory exists but carries no qualified profile.
        let root = profile_tree(&[("Some Other Pad.cfg", dualsense_profile())]);
        assert_eq!(
            resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(QUALIFIED_ACTIVE_ID)),
            None
        );

        // Only one of the two qualified profiles is present.
        let root = profile_tree(&[(QUALIFIED_CONTROLLER_PROFILES[0], dualsense_profile())]);
        assert_eq!(
            resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(QUALIFIED_ACTIVE_ID)),
            None
        );

        // And a driver RetroArch would not scan is not silently substituted.
        let root = qualified_tree();
        assert_eq!(
            resolve_managed_save_state_hotkeys(root.path(), "linuxraw", Some(QUALIFIED_ACTIVE_ID)),
            None
        );
    }

    #[test]
    fn a_profile_missing_or_duplicating_a_required_role_resolves_nothing() {
        for mutation in [
            // Each of the four roles, absent.
            ("input_select_btn = \"8\"", ""),
            ("input_r_btn = \"5\"", ""),
            ("input_right_btn = \"h0right\"", ""),
            ("input_left_btn = \"h0left\"", ""),
            // A role declared twice is ambiguous, so it is refused rather than resolved by
            // first-wins or last-wins.
            (
                "input_select_btn = \"8\"",
                "input_select_btn = \"8\"\ninput_select_btn = \"9\"",
            ),
        ] {
            let mutated = dualsense_profile().replace(mutation.0, mutation.1);
            let profiles: Vec<(&str, String)> = QUALIFIED_CONTROLLER_PROFILES
                .iter()
                .map(|name| (*name, mutated.clone()))
                .collect();
            let root = profile_tree(&profiles);
            assert_eq!(
                resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(QUALIFIED_ACTIVE_ID)),
                None,
                "{mutation:?}"
            );
        }
    }

    #[test]
    fn qualified_profiles_that_disagree_about_a_role_resolve_nothing() {
        let disagreeing = dualsense_profile().replace("input_r_btn = \"5\"", "input_r_btn = \"6\"");
        let root = profile_tree(&[
            (QUALIFIED_CONTROLLER_PROFILES[0], dualsense_profile()),
            (QUALIFIED_CONTROLLER_PROFILES[1], disagreeing),
        ]);

        // One global hotkey set cannot honour both, and picking one would silently mis-bind the
        // other pad.
        assert_eq!(
            resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(QUALIFIED_ACTIVE_ID)),
            None
        );
    }

    #[test]
    fn a_symlinked_or_oversized_profile_resolves_nothing() {
        let root = qualified_tree();
        let target = root
            .path()
            .join(DRIVER)
            .join(QUALIFIED_CONTROLLER_PROFILES[0]);
        let elsewhere = root.path().join("planted.cfg");
        fs::write(&elsewhere, dualsense_profile()).unwrap();
        fs::remove_file(&target).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &target).unwrap();

        assert_eq!(
            resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(QUALIFIED_ACTIVE_ID)),
            None
        );

        // A file too large to be a profile is likewise refused rather than parsed.
        fs::remove_file(&target).unwrap();
        let mut oversized = dualsense_profile();
        oversized.push('\n');
        oversized.push_str(&"# padding\n".repeat(20_000));
        fs::write(&target, oversized).unwrap();
        assert_eq!(
            resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(QUALIFIED_ACTIVE_ID)),
            None
        );
    }

    #[test]
    fn only_a_real_joypad_bind_value_is_ever_carried_into_the_configuration() {
        for accepted in [
            "0", "5", "8", "999", "h0up", "h0down", "h0left", "h0right", "h12right",
        ] {
            assert!(is_joypad_bind(accepted), "{accepted}");
        }
        for refused in [
            "",
            " ",
            "nul",
            "+2",
            "-2",
            "h0",
            "h0sideways",
            "hleft",
            "8; input_load_state_btn = \"9\"",
            "\"",
            "../../etc/passwd",
            &"9".repeat(17),
        ] {
            assert!(!is_joypad_bind(refused), "{refused}");
        }

        // A profile whose role carries an unusable value is treated as not declaring it at all,
        // so nothing is written rather than something arbitrary.
        let mutated = dualsense_profile().replace("input_r_btn = \"5\"", "input_r_btn = \"nul\"");
        let profiles: Vec<(&str, String)> = QUALIFIED_CONTROLLER_PROFILES
            .iter()
            .map(|name| (*name, mutated.clone()))
            .collect();
        let root = profile_tree(&profiles);
        assert_eq!(
            resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(QUALIFIED_ACTIVE_ID)),
            None
        );
    }

    #[test]
    fn the_configuration_keys_are_the_ones_the_managed_retroarch_reads() {
        // Pinned against the managed RetroArch 1.22.2 binary, whose bind base names include
        // `enable_hotkey`, `save_state`, `state_slot_increase`, and `state_slot_decrease`, and
        // whose joypad binds use the `_btn` suffix.
        assert_eq!(ENABLE_HOTKEY_KEY, "input_enable_hotkey_btn");
        assert_eq!(SAVE_STATE_KEY, "input_save_state_btn");
        assert_eq!(SLOT_INCREASE_KEY, "input_state_slot_increase_btn");
        assert_eq!(SLOT_DECREASE_KEY, "input_state_slot_decrease_btn");

        // There is deliberately no ingame Load State key anywhere in this module.
        let source = include_str!("retroarch_input.rs");
        let production = source.split_once("#[cfg(test)]").unwrap().0;
        assert!(!production.contains("input_load_state"));
        assert!(!production.contains("load_state_btn"));
    }

    /// The derivation reads only what it was handed, and never a host RetroArch location.
    #[test]
    fn no_host_retroarch_autoconfig_location_is_ever_consulted() {
        let source = include_str!("retroarch_input.rs");
        let production = source.split_once("#[cfg(test)]").unwrap().0;
        for forbidden in [
            "/usr/share/libretro",
            "/usr/local/share/libretro",
            "/.config/retroarch",
            "/.local/share/retroarch",
            "/etc/retroarch",
        ] {
            assert!(!production.contains(forbidden), "{forbidden}");
        }
        // The only filesystem reads are relative to the caller-supplied verified root.
        assert_eq!(production.matches("std::fs::").count(), 2);
    }

    // ================================================================ MEDIUM-2: actual-controller proof

    /// MEDIUM-2 regression (qualified-actual-controller): the frontend's own confirmed identity
    /// naming the physically qualified DualSense is enough proof, and the expected mappings are
    /// still emitted exactly as before.
    #[test]
    fn a_confirmed_qualified_active_controller_still_produces_the_expected_mappings() {
        let root = qualified_tree();
        assert_eq!(
            resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(QUALIFIED_ACTIVE_ID)),
            Some(expected_dualsense_hotkeys())
        );
        // The Bluetooth connection naming names the very same physical device.
        assert_eq!(
            resolve_managed_save_state_hotkeys(
                root.path(),
                DRIVER,
                Some("DualSense Wireless Controller")
            ),
            Some(expected_dualsense_hotkeys())
        );
    }

    /// MEDIUM-2 regression (other-actual-controller): the qualified profile files exist and agree
    /// with each other, exactly as in a real installation — but the frontend confirms the player is
    /// actually using a different pad. The DualSense values must never be written merely because
    /// the files happen to be there.
    #[test]
    fn a_different_actual_controller_never_receives_the_dualsense_values() {
        let root = qualified_tree();
        for other in [
            "Xbox Wireless Controller",
            "Microsoft X-Box 360 pad",
            "8BitDo Pro 2",
        ] {
            assert_eq!(
                resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(other)),
                None,
                "{other}"
            );
        }
    }

    /// MEDIUM-2 regression (qualification is an exact match, not a substring): every identity here
    /// *contains* a qualified device name, and the previous
    /// `id.to_ascii_lowercase().contains("dualsense")` rule accepted all of them — binding
    /// RetroArch's global hotkey set to the qualified pad's raw button numbers on devices nobody
    /// has ever measured. Only an identity the authenticated profiles themselves declare qualifies.
    #[test]
    fn an_identity_that_merely_contains_a_qualified_name_is_never_accepted() {
        let root = qualified_tree();
        for impostor in [
            "Generic DualSense-style Adapter",
            "MyDualSenseClone",
            "DualSense",
            "dualsense",
            "Sony Interactive Entertainment DualSense Wireless Controller Clone",
            "Not a Sony Interactive Entertainment DualSense Wireless Controller",
            "DualSense Wireless Controller (copy)",
        ] {
            assert_eq!(
                resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(impostor)),
                None,
                "{impostor} must not qualify"
            );
        }
        // The exact declared identities still do, including with surrounding whitespace and a
        // different case — a kernel device name is stable, so tolerating presentation is safe
        // while widening to a prefix or substring is not.
        for exact in [
            QUALIFIED_ACTIVE_ID,
            "  Sony Interactive Entertainment DualSense Wireless Controller  ",
            "sony interactive entertainment dualsense wireless controller",
            "DualSense Wireless Controller",
        ] {
            assert_eq!(
                resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(exact)),
                Some(expected_dualsense_hotkeys()),
                "{exact} must qualify"
            );
        }
    }

    /// A qualified profile that declares no device at all names nobody, so it can qualify nobody —
    /// the identity check has no authority to fall back on and refuses rather than guessing.
    #[test]
    fn a_profile_declaring_no_device_qualifies_no_controller() {
        let without_device: String = dualsense_profile()
            .lines()
            .filter(|line| !line.starts_with(DEVICE_ROLE))
            .collect::<Vec<_>>()
            .join("\n");
        let profiles: Vec<(&str, String)> = QUALIFIED_CONTROLLER_PROFILES
            .iter()
            .map(|name| (*name, without_device.clone()))
            .collect();
        let root = profile_tree(&profiles);
        assert_eq!(
            resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(QUALIFIED_ACTIVE_ID)),
            None
        );
    }

    /// A device name is never a joypad bind: declaring devices must not make arbitrary profile text
    /// reachable by the four roles the generated configuration writes.
    #[test]
    fn a_device_declaration_can_never_become_a_written_hotkey_value() {
        let hostile = [
            "input_driver = \"udev\"",
            "input_device = \"Sony Interactive Entertainment DualSense Wireless Controller\"",
            // A role whose value is text rather than a bind is ignored, exactly as before.
            "input_select_btn = \"Sony Interactive Entertainment DualSense Wireless Controller\"",
            "input_r_btn = \"5\"",
            "input_left_btn = \"h0left\"",
            "input_right_btn = \"h0right\"",
        ]
        .join("\n");
        let profiles: Vec<(&str, String)> = QUALIFIED_CONTROLLER_PROFILES
            .iter()
            .map(|name| (*name, hostile.clone()))
            .collect();
        let root = profile_tree(&profiles);
        // The Select role never resolved, so the whole set refuses rather than writing text.
        assert_eq!(
            resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some(QUALIFIED_ACTIVE_ID)),
            None
        );
    }

    /// MEDIUM-2 regression (ambiguous/no-actual-profile): no confirmed active controller resolves
    /// no hotkeys, which is exactly the pre-existing "write nothing" outcome the rest of the launch
    /// pipeline already treats as non-fatal — nothing here ever fails a launch.
    #[test]
    fn no_confirmed_active_controller_resolves_nothing() {
        let root = qualified_tree();
        assert_eq!(
            resolve_managed_save_state_hotkeys(root.path(), DRIVER, None),
            None
        );
        // An empty identity string is not a confirmed controller either.
        assert_eq!(
            resolve_managed_save_state_hotkeys(root.path(), DRIVER, Some("")),
            None
        );
    }
}
