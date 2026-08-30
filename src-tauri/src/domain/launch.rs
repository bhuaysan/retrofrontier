use crate::domain::bios::BiosRequirementId;
use crate::domain::core::CoreId;
use crate::domain::library::{
    ContentUnitAvailability, ContentUnitId, ContentUnitKind, GameId, UnixTimestamp,
};
use crate::domain::runtime::RuntimeState;
use crate::domain::system::SystemId;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlaySessionId(pub i64);

impl fmt::Display for PlaySessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// The normalized end state of one managed RetroArch execution.
///
/// This is product history. It is never consulted to decide whether a managed process is alive;
/// that answer comes only from the durable process record plus OS process identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaySessionOutcome {
    Running,
    Completed,
    FailedToStart,
    Crashed,
    Interrupted,
}

impl PlaySessionOutcome {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::FailedToStart => "failed_to_start",
            Self::Crashed => "crashed",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed_to_start" => Some(Self::FailedToStart),
            "crashed" => Some(Self::Crashed),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }

    pub const fn is_open(self) -> bool {
        matches!(self, Self::Running)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaySession {
    pub id: PlaySessionId,
    pub game_id: GameId,
    pub content_unit_id: ContentUnitId,
    pub core_id: CoreId,
    pub runtime_installation_id: String,
    pub runtime_release_id: String,
    pub started_at: UnixTimestamp,
    pub ended_at: Option<UnixTimestamp>,
    pub exit_code: Option<i64>,
    pub outcome: PlaySessionOutcome,
}

/// A user-owned per-game core choice. It stores a `CoreId`, never a filesystem path, and lives in
/// its own table so scanner reconciliation and provider refresh can never overwrite it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLaunchOverride {
    pub game_id: GameId,
    pub core_id: CoreId,
    pub updated_at: UnixTimestamp,
}

/// A Linux host capability RetroArch depends on but the managed runtime cannot provide.
///
/// A missing host capability is never evidence that the managed runtime is damaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HostPrerequisite {
    DisplaySession,
    GraphicsDevice,
    AudioService,
    InputDevices,
}

impl HostPrerequisite {
    /// Only a missing display session prevents a launch. The rest are visible diagnostics, so a
    /// degraded host produces an explanation rather than a false "damaged runtime" state.
    pub const fn blocks_launch(self) -> bool {
        matches!(self, Self::DisplaySession)
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::DisplaySession => {
                "No desktop display session was found. Start RetroFrontier from a graphical \
                 session and try again."
            }
            Self::GraphicsDevice => {
                "No graphics device was available to RetroFrontier. Video may be slow or fail."
            }
            Self::AudioService => "No audio service was available. The game may run without sound.",
            Self::InputDevices => {
                "Game controllers could not be read. Check the input device permissions of this \
                 desktop session."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchDiagnostic {
    pub kind: HostPrerequisite,
    pub message: String,
}

impl LaunchDiagnostic {
    pub fn new(kind: HostPrerequisite) -> Self {
        Self {
            kind,
            message: kind.message().to_owned(),
        }
    }
}

/// One launchable content unit offered to the user when a game has more than one.
///
/// This is the same bounded projection Game Detail already renders; it carries no filesystem path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchContentOption {
    pub content_unit_id: ContentUnitId,
    pub kind: ContentUnitKind,
    pub local_title: String,
    pub file_count: u64,
    pub availability: ContentUnitAvailability,
}

/// Stable semantic launch failures. React selects its messaging from the code, never by parsing
/// text, and never receives a raw operating-system error or an internal path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LaunchErrorCode {
    GameNotFound,
    GameUnavailable,
    ContentSelectionRequired,
    ContentUnavailable,
    RuntimeNotReady,
    CorePolicyUnresolved,
    CoreNotInstalled,
    CoreNotApproved,
    BiosMissing,
    BiosInvalid,
    BiosNotCoveredByCatalog,
    HostPrerequisiteMissing,
    GameAlreadyRunning,
    ConfigPreparationFailed,
    SpawnFailed,
    ProcessIdentityFailed,
    ProcessExitedDuringLaunch,
    SessionPersistenceFailed,
    InternalLaunchFailure,
}

impl LaunchErrorCode {
    pub const ALL: &'static [Self] = &[
        Self::GameNotFound,
        Self::GameUnavailable,
        Self::ContentSelectionRequired,
        Self::ContentUnavailable,
        Self::RuntimeNotReady,
        Self::CorePolicyUnresolved,
        Self::CoreNotInstalled,
        Self::CoreNotApproved,
        Self::BiosMissing,
        Self::BiosInvalid,
        Self::BiosNotCoveredByCatalog,
        Self::HostPrerequisiteMissing,
        Self::GameAlreadyRunning,
        Self::ConfigPreparationFailed,
        Self::SpawnFailed,
        Self::ProcessIdentityFailed,
        Self::ProcessExitedDuringLaunch,
        Self::SessionPersistenceFailed,
        Self::InternalLaunchFailure,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GameNotFound => "gameNotFound",
            Self::GameUnavailable => "gameUnavailable",
            Self::ContentSelectionRequired => "contentSelectionRequired",
            Self::ContentUnavailable => "contentUnavailable",
            Self::RuntimeNotReady => "runtimeNotReady",
            Self::CorePolicyUnresolved => "corePolicyUnresolved",
            Self::CoreNotInstalled => "coreNotInstalled",
            Self::CoreNotApproved => "coreNotApproved",
            Self::BiosMissing => "biosMissing",
            Self::BiosInvalid => "biosInvalid",
            Self::BiosNotCoveredByCatalog => "biosNotCoveredByCatalog",
            Self::HostPrerequisiteMissing => "hostPrerequisiteMissing",
            Self::GameAlreadyRunning => "gameAlreadyRunning",
            Self::ConfigPreparationFailed => "configPreparationFailed",
            Self::SpawnFailed => "spawnFailed",
            Self::ProcessIdentityFailed => "processIdentityFailed",
            Self::ProcessExitedDuringLaunch => "processExitedDuringLaunch",
            Self::SessionPersistenceFailed => "sessionPersistenceFailed",
            Self::InternalLaunchFailure => "internalLaunchFailure",
        }
    }

    /// The user-facing sentence RetroFrontier generates for this code.
    ///
    /// Every message is a fixed string. No operating-system error text, path, PID, or internal
    /// identifier is ever interpolated into it.
    pub const fn message(self) -> &'static str {
        match self {
            Self::GameNotFound => "That game is no longer in the local library.",
            Self::GameUnavailable => {
                "The local content for this game is unavailable, so it cannot be started."
            }
            Self::ContentSelectionRequired => {
                "This game has more than one playable version. Choose which one to start."
            }
            Self::ContentUnavailable => {
                "The selected content is unavailable or incomplete, so it cannot be started."
            }
            Self::RuntimeNotReady => {
                "The managed RetroArch runtime is not ready. Check the runtime status and try \
                 again."
            }
            Self::CorePolicyUnresolved => {
                "RetroFrontier has not approved an emulation core for this system yet."
            }
            Self::CoreNotInstalled => {
                "The approved core for this system is not part of the installed managed runtime."
            }
            Self::CoreNotApproved => {
                "The selected core is not approved for this system on this platform."
            }
            Self::BiosMissing => {
                "A required BIOS file is missing. Add it to the RetroFrontier BIOS folder and try \
                 again."
            }
            Self::BiosInvalid => {
                "A required BIOS file does not match a known good dump, so it was not used."
            }
            Self::BiosNotCoveredByCatalog => {
                "RetroFrontier cannot yet verify the required BIOS identity for this system."
            }
            Self::HostPrerequisiteMissing => {
                "This computer is missing something RetroArch needs in order to start."
            }
            Self::GameAlreadyRunning => {
                "A game is already running. Close it before starting another one."
            }
            Self::ConfigPreparationFailed => {
                "RetroFrontier could not prepare its own RetroArch configuration."
            }
            Self::SpawnFailed => "RetroFrontier could not start the managed RetroArch process.",
            Self::ProcessIdentityFailed => {
                "RetroFrontier could not confirm the identity of the RetroArch process it \
                 started, so it was stopped again."
            }
            Self::ProcessExitedDuringLaunch => {
                "RetroArch stopped immediately after starting. Check the launch diagnostics."
            }
            Self::SessionPersistenceFailed => {
                "RetroFrontier could not record this play session, so the game was not started."
            }
            Self::InternalLaunchFailure => "RetroFrontier could not complete the launch.",
        }
    }
}

impl fmt::Display for LaunchErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Typed, already-safe detail a launch failure may carry.
///
/// Every field is an identifier the frontend is allowed to see. There is deliberately no free-text
/// field, no path, and no operating-system error.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchFailureContext {
    pub system_id: Option<SystemId>,
    pub core_id: Option<CoreId>,
    pub bios_requirement_ids: Vec<BiosRequirementId>,
    pub runtime_state: Option<RuntimeState>,
    pub host_prerequisite: Option<HostPrerequisite>,
    pub exit_code: Option<i64>,
    pub content_options: Vec<LaunchContentOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchFailure {
    pub code: LaunchErrorCode,
    pub message: String,
    pub context: LaunchFailureContext,
}

impl LaunchFailure {
    pub fn new(code: LaunchErrorCode) -> Self {
        Self {
            code,
            message: code.message().to_owned(),
            context: LaunchFailureContext::default(),
        }
    }

    pub fn with_system(mut self, system_id: SystemId) -> Self {
        self.context.system_id = Some(system_id);
        self
    }

    pub fn with_core(mut self, core_id: CoreId) -> Self {
        self.context.core_id = Some(core_id);
        self
    }

    pub fn with_bios_requirements(mut self, requirement_ids: Vec<BiosRequirementId>) -> Self {
        self.context.bios_requirement_ids = requirement_ids;
        self
    }

    pub fn with_runtime_state(mut self, state: RuntimeState) -> Self {
        self.context.runtime_state = Some(state);
        self
    }

    pub fn with_host_prerequisite(mut self, prerequisite: HostPrerequisite) -> Self {
        self.context.host_prerequisite = Some(prerequisite);
        self
    }

    pub fn with_exit_code(mut self, exit_code: Option<i64>) -> Self {
        self.context.exit_code = exit_code;
        self
    }

    pub fn with_content_options(mut self, options: Vec<LaunchContentOption>) -> Self {
        self.context.content_options = options;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningGameSession {
    pub session_id: PlaySessionId,
    pub game_id: GameId,
    pub content_unit_id: ContentUnitId,
    pub core_id: CoreId,
    pub started_at: UnixTimestamp,
}

/// The durable running-game projection. `blocked` is true only when the managed process record is
/// present but its identity is uncertain, so no running session can honestly be described while a
/// launch must still be refused.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchState {
    pub running: Option<RunningGameSession>,
    pub blocked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitedGameSession {
    pub session_id: PlaySessionId,
    pub game_id: GameId,
    pub outcome: PlaySessionOutcome,
    pub exit_code: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLaunchStateChanged {
    pub state: LaunchState,
    pub exited: Option<ExitedGameSession>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum LaunchResponse {
    Started {
        session: RunningGameSession,
        diagnostics: Vec<LaunchDiagnostic>,
    },
    ContentSelectionRequired {
        options: Vec<LaunchContentOption>,
    },
    Failed {
        error: LaunchFailure,
    },
}

impl LaunchResponse {
    pub fn failed(code: LaunchErrorCode) -> Self {
        Self::Failed {
            error: LaunchFailure::new(code),
        }
    }

    pub fn failure(error: LaunchFailure) -> Self {
        Self::Failed { error }
    }

    pub fn error_code(&self) -> Option<LaunchErrorCode> {
        match self {
            Self::Failed { error } => Some(error.code),
            Self::ContentSelectionRequired { .. } => {
                Some(LaunchErrorCode::ContentSelectionRequired)
            }
            Self::Started { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HostPrerequisite, LaunchContentOption, LaunchErrorCode, LaunchFailure, LaunchResponse,
        PlaySessionOutcome, RunningGameSession,
    };
    use crate::domain::core::CoreId;
    use crate::domain::library::{ContentUnitAvailability, ContentUnitId, ContentUnitKind, GameId};

    #[test]
    fn every_launch_error_code_has_a_stable_camel_case_wire_value() {
        let expected = [
            "gameNotFound",
            "gameUnavailable",
            "contentSelectionRequired",
            "contentUnavailable",
            "runtimeNotReady",
            "corePolicyUnresolved",
            "coreNotInstalled",
            "coreNotApproved",
            "biosMissing",
            "biosInvalid",
            "biosNotCoveredByCatalog",
            "hostPrerequisiteMissing",
            "gameAlreadyRunning",
            "configPreparationFailed",
            "spawnFailed",
            "processIdentityFailed",
            "processExitedDuringLaunch",
            "sessionPersistenceFailed",
            "internalLaunchFailure",
        ];

        let actual: Vec<_> = LaunchErrorCode::ALL
            .iter()
            .map(|code| code.as_str())
            .collect();
        assert_eq!(actual, expected);

        for code in LaunchErrorCode::ALL {
            let serialized = serde_json::to_value(code).unwrap();
            assert_eq!(serialized, serde_json::json!(code.as_str()));
            assert!(!code.message().trim().is_empty());
        }
    }

    #[test]
    fn a_launch_failure_never_carries_a_path_or_operating_system_error() {
        let failure = LaunchFailure::new(LaunchErrorCode::SpawnFailed)
            .with_core(CoreId::new("nestopia").unwrap())
            .with_exit_code(Some(1));
        let serialized = serde_json::to_string(&failure).unwrap();

        assert!(!serialized.contains('/'));
        assert!(!serialized.contains("os error"));
        assert!(!serialized.contains("No such file"));
        assert!(serialized.contains("\"code\":\"spawnFailed\""));
        assert!(serialized.contains("\"coreId\":\"nestopia\""));
    }

    #[test]
    fn the_launch_response_is_a_status_tagged_union() {
        let started = LaunchResponse::Started {
            session: RunningGameSession {
                session_id: super::PlaySessionId(7),
                game_id: GameId(3),
                content_unit_id: ContentUnitId(9),
                core_id: CoreId::new("beetle-psx").unwrap(),
                started_at: 1_756_500_000,
            },
            diagnostics: vec![super::LaunchDiagnostic::new(HostPrerequisite::AudioService)],
        };
        let selection = LaunchResponse::ContentSelectionRequired {
            options: vec![LaunchContentOption {
                content_unit_id: ContentUnitId(9),
                kind: ContentUnitKind::M3u,
                local_title: "Synthetic".to_owned(),
                file_count: 3,
                availability: ContentUnitAvailability::Available,
            }],
        };
        let failed = LaunchResponse::failed(LaunchErrorCode::GameAlreadyRunning);

        assert_eq!(
            serde_json::to_value(&started).unwrap()["status"],
            serde_json::json!("started")
        );
        assert_eq!(
            serde_json::to_value(&started).unwrap()["session"]["sessionId"],
            serde_json::json!(7)
        );
        assert_eq!(
            serde_json::to_value(&selection).unwrap()["status"],
            serde_json::json!("contentSelectionRequired")
        );
        assert_eq!(
            serde_json::to_value(&failed).unwrap()["error"]["code"],
            serde_json::json!("gameAlreadyRunning")
        );
        assert_eq!(started.error_code(), None);
        assert_eq!(
            selection.error_code(),
            Some(LaunchErrorCode::ContentSelectionRequired)
        );
    }

    #[test]
    fn play_session_outcomes_round_trip_through_their_database_representation() {
        for outcome in [
            PlaySessionOutcome::Running,
            PlaySessionOutcome::Completed,
            PlaySessionOutcome::FailedToStart,
            PlaySessionOutcome::Crashed,
            PlaySessionOutcome::Interrupted,
        ] {
            assert_eq!(PlaySessionOutcome::from_db(outcome.as_db()), Some(outcome));
        }
        assert_eq!(PlaySessionOutcome::from_db("unknown"), None);
        assert!(PlaySessionOutcome::Running.is_open());
        assert!(!PlaySessionOutcome::Interrupted.is_open());
        assert_eq!(
            serde_json::to_value(PlaySessionOutcome::FailedToStart).unwrap(),
            serde_json::json!("failedToStart")
        );
    }

    #[test]
    fn only_a_missing_display_session_blocks_a_launch() {
        assert!(HostPrerequisite::DisplaySession.blocks_launch());
        for prerequisite in [
            HostPrerequisite::GraphicsDevice,
            HostPrerequisite::AudioService,
            HostPrerequisite::InputDevices,
        ] {
            assert!(!prerequisite.blocks_launch());
            assert!(!prerequisite.message().trim().is_empty());
        }
    }
}
