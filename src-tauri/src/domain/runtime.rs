use serde::de::{DeserializeOwned, Error as DeserializeError, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Deref;
use std::path::PathBuf;
use thiserror::Error;

pub const RUNTIME_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const ACTIVE_POINTER_SCHEMA_VERSION: u32 = 1;
pub const COMPLETE_MARKER_SCHEMA_VERSION: u32 = 1;
pub const MANAGED_PROCESS_RECORD_SCHEMA_VERSION: u32 = 3;
pub const DETACHED_INVENTORY_SCHEMA_VERSION: u32 = 1;
pub const MAX_ACTIVE_POINTER_BYTES: u64 = 4 * 1024;
pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub const MAX_TRUST_STATE_BYTES: u64 = 1024 * 1024;

/// The independent bound on a *detached* installed-file inventory target.
///
/// ADR-012 permits the complete installed-file inventory to live in a separate immutable target
/// referenced by digest from the Runtime Release manifest. That option exists because the manifest
/// bound below is close to being exhausted by the core matrix, so the detached document needs
/// headroom the manifest does not have. It is deliberately a *second* bound rather than a relaxed
/// `MAX_MANIFEST_BYTES`: the manifest stays small, and nothing about the detached representation
/// widens what an inline manifest may contain.
pub const MAX_DETACHED_INVENTORY_BYTES: u64 = 16 * 1024 * 1024;

/// The bound on how many entries any installed-file inventory may describe, inline or detached.
///
/// The manifest byte bound already caps an inline inventory. A detached one is larger by design,
/// so the entry count is bounded explicitly: every entry becomes a filesystem path this process
/// stats, hashes, and permissions during installation and verification.
pub const MAX_INVENTORY_ENTRIES: usize = 200_000;

/// The headroom is the reason the detached representation exists at all.
const _: () = assert!(MAX_DETACHED_INVENTORY_BYTES > MAX_MANIFEST_BYTES);

/// The `representation` discriminant of a detached inventory reference.
///
/// The client never guesses which representation a manifest uses: an inline inventory is a JSON
/// array of entries, and a detached one is a JSON object carrying this exact tag.
pub const DETACHED_INVENTORY_REPRESENTATION: &str = "detached_target";

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("managed runtime is unsupported on this platform")]
    UnsupportedPlatform,

    #[error("runtime manifest is invalid: {0}")]
    Manifest(String),

    #[error("runtime trust verification failed: {0}")]
    Trust(String),

    #[error("runtime target download failed: {0}")]
    Download(String),

    #[error("runtime archive extraction failed: {0}")]
    Extraction(String),

    #[error("runtime integrity verification failed: {0}")]
    Integrity(String),

    #[error("installed runtime validation failed: {0}")]
    InstalledTree(String),

    #[error("runtime activation pointer is invalid: {0}")]
    Pointer(String),

    #[error("runtime mutation lock could not be acquired: {0}")]
    Lock(String),

    #[error("a managed RetroArch process is still active")]
    GameActive,

    #[error("managed process record schema is unsupported")]
    ProcessRecordSchema,

    #[error("no verified rollback runtime is available")]
    NoRollback,

    #[error("runtime storage policy would be exceeded")]
    StorageLimit,

    #[error("runtime operation could not prepare storage: {0}")]
    Storage(String),

    #[error("runtime filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeState {
    NotInstalled,
    Ready,
    Installing,
    Updating,
    Repairing,
    Broken,
    RollbackAvailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub state: RuntimeState,
    pub installation_id: Option<String>,
    pub release_id: Option<String>,
    pub can_rollback: bool,
    pub repair_required: bool,
}

/// Where the configured trusted managed-release source came from.
///
/// The UI must never present a locally published qualification repository as if it were the public
/// production release channel, so the origin travels with runtime install state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeSourceOrigin {
    /// A trusted root and repository compiled into a released RetroFrontier build.
    Production,
    /// A locally published qualification repository, selected by explicit opt-in.
    Qualification,
}

/// One trust-consistent read of the managed runtime. Systems/readiness queries must use this
/// snapshot so status and installed-core availability cannot come from separate verifications.
#[derive(Debug, Clone)]
pub struct VerifiedRuntimeSnapshot {
    pub status: RuntimeStatus,
    pub verified_core_ids: BTreeSet<SafeIdentifier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedProcessPhase {
    Launching,
    Running,
}

/// The durable managed-process identity record.
///
/// Schema 3 adds the launch and play-session identity, and makes the process identity optional so
/// a conservative `launching` record can be written *before* the child is spawned. That closes the
/// crash window between `exec` and persisting a PID, which is exactly where a live managed
/// RetroArch could otherwise become invisible to RuntimeManager (ADR-011).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedProcessRecord {
    pub schema_version: u32,
    pub phase: ManagedProcessPhase,
    /// Unique per launch attempt, so a record can be attributed to the attempt that wrote it.
    pub launch_id: SafeIdentifier,
    /// The play session this managed process belongs to. History follows process identity; it
    /// never replaces it.
    pub play_session_id: i64,
    pub boot_id: String,
    pub installation_id: SafeIdentifier,
    pub expected_apprun_path: String,
    /// Absent while `launching`, because the child does not exist yet.
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub process_start_time_ticks: Option<u64>,
    /// AppRun may be a script. In that case `/proc/<pid>/exe` is the interpreter rather than the
    /// AppRun path; the launch service records that observed executable separately.
    #[serde(default)]
    pub expected_executable_path: Option<String>,
}

impl ManagedProcessRecord {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.schema_version != MANAGED_PROCESS_RECORD_SCHEMA_VERSION {
            return Err(RuntimeError::ProcessRecordSchema);
        }
        if self.boot_id.trim().is_empty()
            || self.play_session_id <= 0
            || self.expected_apprun_path.is_empty()
            || !std::path::Path::new(&self.expected_apprun_path).is_absolute()
            || self
                .expected_executable_path
                .as_deref()
                .is_some_and(|path| path.is_empty() || !std::path::Path::new(path).is_absolute())
        {
            return Err(RuntimeError::GameActive);
        }
        match self.phase {
            // A running record must carry full process identity; PID alone is never identity.
            ManagedProcessPhase::Running => {
                if self.pid.is_none_or(|pid| pid == 0)
                    || self.process_start_time_ticks.is_none_or(|ticks| ticks == 0)
                    || self.expected_executable_path.is_none()
                {
                    return Err(RuntimeError::GameActive);
                }
            }
            // A launching record is written before the child exists, so claiming an identity
            // would be a lie that later liveness checks could not distinguish from a real one.
            ManagedProcessPhase::Launching => {
                if self.pid.is_some()
                    || self.process_start_time_ticks.is_some()
                    || self.expected_executable_path.is_some()
                {
                    return Err(RuntimeError::GameActive);
                }
            }
        }
        Ok(())
    }
}

impl RuntimeStatus {
    pub fn not_installed() -> Self {
        Self {
            state: RuntimeState::NotInstalled,
            installation_id: None,
            release_id: None,
            can_rollback: false,
            repair_required: false,
        }
    }

    pub fn broken() -> Self {
        Self {
            state: RuntimeState::Broken,
            installation_id: None,
            release_id: None,
            can_rollback: false,
            repair_required: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePlatform {
    Linux,
    Windows,
    Macos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeArchitecture {
    X86_64,
    Aarch64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseChannel {
    Stable,
    Beta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    Runtime,
    Core,
    SupportAsset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    AppImage,
    #[serde(rename = "7z", alias = "seven_z")]
    SevenZip,
    Zip,
    Tar,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SafeIdentifier(String);

impl SafeIdentifier {
    pub fn new(value: impl Into<String>) -> Result<Self, RuntimeError> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 {
            return Err(RuntimeError::Manifest(
                "identifier length is outside the allowed range".to_owned(),
            ));
        }

        let mut chars = value.chars();
        let Some(first) = chars.next() else {
            return Err(RuntimeError::Manifest("identifier is empty".to_owned()));
        };
        if !first.is_ascii_alphanumeric()
            || !chars.all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            })
        {
            return Err(RuntimeError::Manifest(format!(
                "identifier '{value}' contains an unsafe character"
            )));
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for SafeIdentifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for SafeIdentifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SafeIdentifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl TryFrom<String> for SafeIdentifier {
    type Error = RuntimeError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for SafeIdentifier {
    type Error = RuntimeError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelativePath(String);

impl RelativePath {
    pub fn new(value: impl Into<String>) -> Result<Self, RuntimeError> {
        let value = value.into();
        if value.is_empty() || value.len() > 4096 {
            return Err(RuntimeError::Manifest(
                "relative path length is outside the allowed range".to_owned(),
            ));
        }
        if value.starts_with('/') || value.starts_with('\\') || value.contains('\\') {
            return Err(RuntimeError::Manifest(format!(
                "path '{value}' must be a relative slash-separated path"
            )));
        }
        if value.contains('\0') {
            return Err(RuntimeError::Manifest("path contains NUL".to_owned()));
        }

        for component in value.split('/') {
            if component.is_empty() || component == "." || component == ".." {
                return Err(RuntimeError::Manifest(format!(
                    "path '{value}' contains an unsafe component"
                )));
            }
            if component.chars().any(char::is_control) {
                return Err(RuntimeError::Manifest(format!(
                    "path '{value}' contains a control character"
                )));
            }
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_path_buf(&self) -> PathBuf {
        self.0.split('/').collect()
    }

    pub fn starts_with(&self, prefix: &str) -> bool {
        self.0 == prefix || self.0.starts_with(&format!("{prefix}/"))
    }

    pub fn parent(&self) -> Option<Self> {
        self.0
            .rsplit_once('/')
            .map(|(parent, _)| Self(parent.to_owned()))
    }

    pub fn join(&self, child: &str) -> Result<Self, RuntimeError> {
        Self::new(format!("{}/{}", self.0, child))
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for RelativePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl TryFrom<String> for RelativePath {
    type Error = RuntimeError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for RelativePath {
    type Error = RuntimeError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// A symlink target is relative, but may contain `.` and `..` because AppDir trees commonly use
/// links between sibling directories. Resolution is checked against the installation root before
/// a link is ever created.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymlinkTarget(String);

impl SymlinkTarget {
    pub fn new(value: impl Into<String>) -> Result<Self, RuntimeError> {
        let value = value.into();
        if value.is_empty() || value.len() > 4096 {
            return Err(RuntimeError::Manifest(
                "symlink target length is outside the allowed range".to_owned(),
            ));
        }
        if value.starts_with('/') || value.starts_with('\\') || value.contains('\\') {
            return Err(RuntimeError::Manifest(format!(
                "symlink target '{value}' must be relative"
            )));
        }
        if value.contains('\0') {
            return Err(RuntimeError::Manifest(
                "symlink target contains NUL".to_owned(),
            ));
        }
        if value
            .split('/')
            .any(|component| component.is_empty() || component.chars().any(char::is_control))
        {
            return Err(RuntimeError::Manifest(format!(
                "symlink target '{value}' contains an unsafe component"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SymlinkTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for SymlinkTarget {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SymlinkTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The SHA-256 of `bytes`.
    ///
    /// Digesting in-memory bytes is a domain operation here because the manifest itself binds the
    /// detached inventory by digest, so the check that the received bytes *are* the referenced
    /// inventory belongs with the type that expresses that reference.
    pub fn of(bytes: &[u8]) -> Self {
        use sha2::Digest;
        let digest = sha2::Sha256::digest(bytes);
        let mut output = [0_u8; 32];
        output.copy_from_slice(&digest);
        Self(output)
    }

    pub fn from_hex(value: &str) -> Result<Self, RuntimeError> {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(RuntimeError::Manifest(
                "SHA-256 digest must be exactly 64 hexadecimal characters".to_owned(),
            ));
        }
        let mut bytes = [0_u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            let offset = index * 2;
            *byte = (hex_nibble(value.as_bytes()[offset]) << 4)
                | hex_nibble(value.as_bytes()[offset + 1]);
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        output
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_hex(&value).map_err(D::Error::custom)
    }
}

fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => unreachable!("validated hexadecimal input"),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionLimits {
    pub max_entries: u64,
    pub max_path_bytes: u64,
    pub max_expanded_bytes: u64,
    pub max_file_bytes: u64,
    pub max_compression_ratio: u64,
}

impl Default for ExtractionLimits {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            max_path_bytes: 4096,
            max_expanded_bytes: 2 * 1024 * 1024 * 1024,
            max_file_bytes: 2 * 1024 * 1024 * 1024,
            max_compression_ratio: 1000,
        }
    }
}

impl ExtractionLimits {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.max_entries == 0
            || self.max_entries > 1_000_000
            || self.max_path_bytes == 0
            || self.max_path_bytes > 4096
            || self.max_expanded_bytes == 0
            || self.max_file_bytes == 0
            || self.max_file_bytes > self.max_expanded_bytes
            || self.max_compression_ratio == 0
            || self.max_compression_ratio > 100_000
        {
            return Err(RuntimeError::Manifest(
                "extraction limits are outside the supported safety bounds".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeComponent {
    pub id: SafeIdentifier,
    pub kind: ComponentKind,
    pub target_name: String,
    pub source_id: Option<SafeIdentifier>,
    pub source_url: Option<String>,
    pub archive_format: ArchiveFormat,
    pub archive_size_bytes: u64,
    pub sha256: Sha256Digest,
    pub install_path: RelativePath,
    pub expected_root: Option<RelativePath>,
    pub payload_filename: Option<RelativePath>,
    pub executable_relative_path: Option<RelativePath>,
    pub display_version: Option<String>,
    pub source_revision: Option<String>,
    pub source_pinning: Option<String>,
    pub license: String,
    #[serde(default)]
    pub systems: Vec<SafeIdentifier>,
}

impl RuntimeComponent {
    pub fn parsed_target_name(&self) -> Result<RelativePath, RuntimeError> {
        RelativePath::new(self.target_name.clone())
            .map_err(|_| RuntimeError::Manifest("component target name is unsafe".to_owned()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstalledEntryType {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledEntry {
    pub path: RelativePath,
    pub entry_type: InstalledEntryType,
    pub size_bytes: u64,
    pub sha256: Option<Sha256Digest>,
    pub executable: bool,
    pub link_target: Option<SymlinkTarget>,
}

/// A bounded cryptographic reference from the Runtime Release manifest to a separate immutable
/// installed-file inventory target.
///
/// This is not a second trust root and it is not a URL. The named target is obtained only as an
/// authenticated TUF target, and these three values must equal what trusted TUF targets metadata
/// says about that target *before* any of its bytes are read — the same rule the manifest already
/// applies to component archives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedInventoryReference {
    /// The TUF target name. Flat, relative, and safe: it is a filename in the published repository.
    pub target_name: String,
    pub size_bytes: u64,
    pub sha256: Sha256Digest,
}

impl DetachedInventoryReference {
    fn validate(&self) -> Result<(), RuntimeError> {
        let path = RelativePath::new(self.target_name.clone()).map_err(|_| {
            RuntimeError::Manifest("detached inventory target name is unsafe".to_owned())
        })?;
        if path.as_str().contains('/') {
            return Err(RuntimeError::Manifest(
                "detached inventory target name must be a flat filename".to_owned(),
            ));
        }
        // An empty or oversized reference is refused before anything is fetched, so the bound is
        // enforced by the authenticated manifest rather than only by the download loop.
        if self.size_bytes == 0 {
            return Err(RuntimeError::Manifest(
                "detached inventory reference declares an empty length".to_owned(),
            ));
        }
        if self.size_bytes > MAX_DETACHED_INVENTORY_BYTES {
            return Err(RuntimeError::Manifest(
                "detached inventory reference exceeds the detached inventory size limit".to_owned(),
            ));
        }
        Ok(())
    }
}

/// The wire form of a detached reference: an explicitly tagged JSON object.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DetachedInventoryWire {
    representation: DetachedInventoryTag,
    target_name: String,
    size_bytes: u64,
    sha256: Sha256Digest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DetachedInventoryTag {
    DetachedTarget,
}

/// How a Runtime Release manifest carries its installed-file inventory.
///
/// Two representations are supported and the manifest states which one it uses; the client never
/// infers it. `Inline` is the legacy and currently published form — a JSON array of entries inside
/// the authenticated manifest, which is what Runtime Release 001 and 002 use and what they must
/// keep parsing to byte-identically. `Detached` is ADR-012's scalable form: a tagged JSON object
/// binding a separate immutable inventory target by name, length, and SHA-256.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseInventory {
    Inline(Vec<InstalledEntry>),
    Detached(DetachedInventoryReference),
}

impl ReleaseInventory {
    pub fn detached(&self) -> Option<&DetachedInventoryReference> {
        match self {
            Self::Inline(_) => None,
            Self::Detached(reference) => Some(reference),
        }
    }

    pub fn is_detached(&self) -> bool {
        self.detached().is_some()
    }

    /// The inline entries, for tests that build or perturb a synthetic manifest directly.
    ///
    /// Production code reads the inventory through [`VerifiedRuntimeManifest::inventory`], which is
    /// the only path that has been through length, digest, and schema validation.
    #[cfg(test)]
    pub(crate) fn inline_entries_mut(&mut self) -> &mut Vec<InstalledEntry> {
        match self {
            Self::Inline(entries) => entries,
            Self::Detached(_) => panic!("the fixture manifest declares a detached inventory"),
        }
    }

    fn validate(&self) -> Result<(), RuntimeError> {
        match self {
            Self::Inline(entries) => {
                if entries.len() > MAX_INVENTORY_ENTRIES {
                    return Err(RuntimeError::Manifest(
                        "installed-file inventory exceeds the entry limit".to_owned(),
                    ));
                }
                Ok(())
            }
            Self::Detached(reference) => reference.validate(),
        }
    }
}

impl Serialize for ReleaseInventory {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            // Byte-compatibility with every already published inline manifest: the array is the
            // inline representation, exactly as before.
            Self::Inline(entries) => entries.serialize(serializer),
            Self::Detached(reference) => {
                let mut map = serializer.serialize_map(Some(4))?;
                map.serialize_entry("representation", DETACHED_INVENTORY_REPRESENTATION)?;
                map.serialize_entry("sha256", &reference.sha256)?;
                map.serialize_entry("size_bytes", &reference.size_bytes)?;
                map.serialize_entry("target_name", &reference.target_name)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ReleaseInventory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct InventoryVisitor;

        impl<'de> Visitor<'de> for InventoryVisitor {
            type Value = ReleaseInventory;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(
                    "an inline array of installed-file entries, or a detached inventory reference",
                )
            }

            fn visit_seq<A>(self, sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let entries = Vec::<InstalledEntry>::deserialize(
                    serde::de::value::SeqAccessDeserializer::new(sequence),
                )?;
                Ok(ReleaseInventory::Inline(entries))
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let wire = DetachedInventoryWire::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )?;
                let DetachedInventoryTag::DetachedTarget = wire.representation;
                Ok(ReleaseInventory::Detached(DetachedInventoryReference {
                    target_name: wire.target_name,
                    size_bytes: wire.size_bytes,
                    sha256: wire.sha256,
                }))
            }
        }

        deserializer.deserialize_any(InventoryVisitor)
    }
}

/// The separate immutable inventory target's document.
///
/// It repeats the manifest and release identity it belongs to. The digest binding already prevents
/// substitution, but naming the owner makes a wrong-release document a stated refusal rather than
/// something only the digest happens to catch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetachedInventoryDocument {
    pub schema_version: u32,
    pub manifest_id: SafeIdentifier,
    pub release_id: SafeIdentifier,
    pub entries: Vec<InstalledEntry>,
}

impl DetachedInventoryDocument {
    /// Parse a detached inventory document under an explicit bound.
    pub fn parse(bytes: &[u8]) -> Result<Self, RuntimeError> {
        if bytes.len() as u64 > MAX_DETACHED_INVENTORY_BYTES {
            return Err(RuntimeError::Manifest(
                "detached inventory document is too large".to_owned(),
            ));
        }
        let document: Self =
            parse_strict_json(bytes).map_err(|error| RuntimeError::Manifest(error.to_owned()))?;
        if document.schema_version != DETACHED_INVENTORY_SCHEMA_VERSION {
            return Err(RuntimeError::Manifest(format!(
                "unsupported detached inventory schema version {}",
                document.schema_version
            )));
        }
        if document.entries.len() > MAX_INVENTORY_ENTRIES {
            return Err(RuntimeError::Manifest(
                "detached inventory exceeds the entry limit".to_owned(),
            ));
        }
        Ok(document)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCompatibility {
    pub retroarch_core_api: String,
    pub save_state_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRelease {
    pub release_id: SafeIdentifier,
    pub release_sequence: u64,
    pub retrofrontier_runtime_version: String,
    pub retroarch_version: String,
    pub platform: RuntimePlatform,
    pub architecture: RuntimeArchitecture,
    pub components: Vec<RuntimeComponent>,
    pub app_run_path: RelativePath,
    /// The explicit installed-file inventory representation: inline entries, or a bounded
    /// cryptographic reference to a separate immutable inventory target.
    pub inventory: ReleaseInventory,
    #[serde(default)]
    pub extraction: ExtractionLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeManifest {
    pub schema_version: u32,
    pub manifest_id: SafeIdentifier,
    pub channel: ReleaseChannel,
    pub min_retrofrontier_version: String,
    pub release: RuntimeRelease,
    pub compatibility: RuntimeCompatibility,
}

impl RuntimeManifest {
    /// Parse a manifest under the manifest bound and validate everything that does not depend on
    /// the installed-file inventory.
    ///
    /// The inventory-dependent half is [`RuntimeManifest::validate_inventory`], reached through
    /// [`VerifiedRuntimeManifest`]. Splitting them is what makes a detached inventory possible at
    /// all: the manifest is authenticated and structurally checked first, and only then does its
    /// stated representation decide whether the entries are already present or must be obtained as
    /// a further authenticated target. Nothing installs, verifies, or launches from a manifest whose
    /// inventory half has not run.
    pub fn parse(bytes: &[u8]) -> Result<Self, RuntimeError> {
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(RuntimeError::Manifest("manifest is too large".to_owned()));
        }
        let manifest: Self =
            parse_strict_json(bytes).map_err(|error| RuntimeError::Manifest(error.to_owned()))?;
        manifest.validate_structure()?;
        Ok(manifest)
    }

    pub fn validate_structure(&self) -> Result<(), RuntimeError> {
        if self.schema_version != RUNTIME_MANIFEST_SCHEMA_VERSION {
            return Err(RuntimeError::Manifest(format!(
                "unsupported schema version {}",
                self.schema_version
            )));
        }
        if self.release.release_sequence == 0 {
            return Err(RuntimeError::Manifest(
                "release sequence must be positive".to_owned(),
            ));
        }
        if self.release.platform != RuntimePlatform::Linux
            || self.release.architecture != RuntimeArchitecture::X86_64
        {
            return Err(RuntimeError::UnsupportedPlatform);
        }
        if self.min_retrofrontier_version.is_empty()
            || self.release.retrofrontier_runtime_version.is_empty()
            || self.release.retroarch_version.is_empty()
        {
            return Err(RuntimeError::Manifest(
                "runtime version fields must not be empty".to_owned(),
            ));
        }
        self.release.extraction.validate()?;
        self.release.inventory.validate()?;

        let mut component_ids = BTreeSet::new();
        let mut target_names = BTreeSet::new();
        let mut install_paths = BTreeSet::new();
        let mut runtime_components = 0;
        let mut runtime_install_path = None;
        for component in &self.release.components {
            if !component_ids.insert(component.id.clone()) {
                return Err(RuntimeError::Manifest(format!(
                    "duplicate component id '{}'",
                    component.id
                )));
            }
            let target_name = component.parsed_target_name()?;
            if !target_names.insert(target_name.clone()) {
                return Err(RuntimeError::Manifest(format!(
                    "duplicate target name '{}'",
                    component.target_name
                )));
            }
            if component.archive_size_bytes == 0
                || component.archive_size_bytes > self.release.extraction.max_expanded_bytes
            {
                return Err(RuntimeError::Manifest(format!(
                    "component '{}' has an unsafe archive size",
                    component.id
                )));
            }
            if component.license.trim().is_empty() {
                return Err(RuntimeError::Manifest(format!(
                    "component '{}' is missing license metadata",
                    component.id
                )));
            }
            if component
                .source_url
                .as_ref()
                .is_some_and(|url| !url.starts_with("https://") || url.contains('@'))
            {
                return Err(RuntimeError::Manifest(format!(
                    "component '{}' has an unsafe source URL",
                    component.id
                )));
            }
            if component.kind == ComponentKind::Core {
                if component.systems.is_empty() {
                    return Err(RuntimeError::Manifest(format!(
                        "core '{}' has no approved system mapping",
                        component.id
                    )));
                }
                let mut systems = BTreeSet::new();
                if component
                    .systems
                    .iter()
                    .any(|system| !systems.insert(system))
                {
                    return Err(RuntimeError::Manifest(format!(
                        "core '{}' has duplicate system mappings",
                        component.id
                    )));
                }
                if !component.install_path.starts_with("cores") {
                    return Err(RuntimeError::Manifest(format!(
                        "core '{}' must install below cores",
                        component.id
                    )));
                }
            } else if component.kind == ComponentKind::Runtime {
                runtime_components += 1;
                runtime_install_path = Some(component.install_path.clone());
                if !component.install_path.starts_with("runtime") {
                    return Err(RuntimeError::Manifest(
                        "the runtime component must install below runtime".to_owned(),
                    ));
                }
            } else if !component.install_path.starts_with("runtime") {
                return Err(RuntimeError::Manifest(format!(
                    "support component '{}' must install below runtime",
                    component.id
                )));
            }
            if component.kind == ComponentKind::Runtime
                && !matches!(
                    component.archive_format,
                    ArchiveFormat::AppImage | ArchiveFormat::SevenZip
                )
            {
                return Err(RuntimeError::Manifest(
                    "Linux runtime components must be AppImage or 7z AppDir artifacts".to_owned(),
                ));
            }
            if component.payload_filename.is_some()
                && component.archive_format != ArchiveFormat::SevenZip
            {
                return Err(RuntimeError::Manifest(format!(
                    "component '{}' declares an inner payload for a non-7z archive",
                    component.id
                )));
            }
            if install_paths.iter().any(|existing: &RelativePath| {
                existing.starts_with(component.install_path.as_str())
                    || component.install_path.starts_with(existing.as_str())
            }) {
                return Err(RuntimeError::Manifest(format!(
                    "overlapping component install path '{}'",
                    component.install_path
                )));
            }
            install_paths.insert(component.install_path.clone());
        }
        if runtime_components != 1 {
            return Err(RuntimeError::Manifest(
                "release must contain exactly one runtime component".to_owned(),
            ));
        }
        if !self.release.app_run_path.starts_with("runtime") {
            return Err(RuntimeError::Manifest(
                "AppRun must be below the managed runtime tree".to_owned(),
            ));
        }
        if !runtime_install_path
            .as_ref()
            .is_some_and(|path| self.release.app_run_path.starts_with(path.as_str()))
        {
            return Err(RuntimeError::Manifest(
                "AppRun must be below the runtime component install path".to_owned(),
            ));
        }

        // A detached inventory target shares the repository namespace with the component targets,
        // so it must be a distinct target: a name collision would make one authenticated target
        // stand in for another.
        if let Some(reference) = self.release.inventory.detached() {
            if self
                .release
                .components
                .iter()
                .any(|component| component.target_name == reference.target_name)
            {
                return Err(RuntimeError::Manifest(
                    "detached inventory target name collides with a component target".to_owned(),
                ));
            }
        }
        Ok(())
    }

    /// Validate the manifest against the installed-file inventory it declares.
    ///
    /// `entries` are the authenticated entries: either the manifest's own inline array, or the
    /// contents of the detached inventory target whose length and digest the manifest bound. This
    /// half of validation is identical for both representations, which is the point — the
    /// verification boundary below it never learns where the inventory came from.
    pub fn validate_inventory(&self, entries: &[InstalledEntry]) -> Result<(), RuntimeError> {
        if entries.len() > MAX_INVENTORY_ENTRIES {
            return Err(RuntimeError::Manifest(
                "installed-file inventory exceeds the entry limit".to_owned(),
            ));
        }
        let mut inventory_paths = BTreeMap::new();
        for entry in entries {
            if entry.path.as_str() == "release-manifest.json"
                || entry.path.as_str() == "complete.json"
                || entry.path.as_str() == "release-inventory.json"
            {
                return Err(RuntimeError::Manifest(
                    "installation metadata cannot be part of payload inventory".to_owned(),
                ));
            }
            if inventory_paths.insert(entry.path.clone(), entry).is_some() {
                return Err(RuntimeError::Manifest(format!(
                    "duplicate inventory path '{}'",
                    entry.path
                )));
            }
            match entry.entry_type {
                InstalledEntryType::File => {
                    if entry.sha256.is_none() || entry.link_target.is_some() {
                        return Err(RuntimeError::Manifest(format!(
                            "file inventory entry '{}' has invalid metadata",
                            entry.path
                        )));
                    }
                    if entry.size_bytes > self.release.extraction.max_expanded_bytes {
                        return Err(RuntimeError::Manifest(format!(
                            "file inventory entry '{}' exceeds the extraction size limit",
                            entry.path
                        )));
                    }
                    if entry.executable
                        && !entry.path.starts_with("runtime")
                        && !entry.path.starts_with("cores")
                    {
                        return Err(RuntimeError::Manifest(format!(
                            "executable inventory entry '{}' is outside managed code roots",
                            entry.path
                        )));
                    }
                }
                InstalledEntryType::Directory => {
                    if entry.size_bytes != 0
                        || entry.sha256.is_some()
                        || entry.link_target.is_some()
                        || entry.executable
                    {
                        return Err(RuntimeError::Manifest(format!(
                            "directory inventory entry '{}' has invalid metadata",
                            entry.path
                        )));
                    }
                }
                InstalledEntryType::Symlink => {
                    if entry.size_bytes != 0 || entry.sha256.is_some() || entry.executable {
                        return Err(RuntimeError::Manifest(format!(
                            "symlink inventory entry '{}' has invalid metadata",
                            entry.path
                        )));
                    }
                    if entry.link_target.is_none() {
                        return Err(RuntimeError::Manifest(format!(
                            "symlink inventory entry '{}' has no target",
                            entry.path
                        )));
                    }
                }
            }
        }

        // Resolve links only after the complete inventory has been collected; valid manifests do
        // not depend on whether a link appears before or after its target in JSON.
        for entry in entries {
            if entry.entry_type != InstalledEntryType::Symlink {
                continue;
            }
            let target = entry
                .link_target
                .as_ref()
                .expect("symlink metadata was checked above");
            let resolved = resolve_relative_link(&entry.path, target)?;
            if !inventory_paths.contains_key(&resolved) {
                return Err(RuntimeError::Manifest(format!(
                    "symlink '{}' points to an unlisted target",
                    entry.path
                )));
            }
        }

        for component in &self.release.components {
            let Some(install_root) = inventory_paths.get(&component.install_path) else {
                return Err(RuntimeError::Manifest(format!(
                    "component '{}' has no inventory directory at its install path",
                    component.id
                )));
            };
            if install_root.entry_type != InstalledEntryType::Directory {
                return Err(RuntimeError::Manifest(format!(
                    "component '{}' install path is not a directory",
                    component.id
                )));
            }
            if let Some(executable) = component.executable_relative_path.as_ref() {
                let executable_path = component.install_path.join(executable.as_str())?;
                let Some(entry) = inventory_paths.get(&executable_path) else {
                    return Err(RuntimeError::Manifest(format!(
                        "component '{}' executable is absent from the inventory",
                        component.id
                    )));
                };
                if entry.entry_type != InstalledEntryType::File || !entry.executable {
                    return Err(RuntimeError::Manifest(format!(
                        "component '{}' executable is not an executable file",
                        component.id
                    )));
                }
            }
        }

        if !inventory_paths.contains_key(&self.release.app_run_path) {
            return Err(RuntimeError::Manifest(
                "AppRun is missing from the installed inventory".to_owned(),
            ));
        }
        let app_run = inventory_paths
            .get(&self.release.app_run_path)
            .expect("checked above");
        if !matches!(
            app_run.entry_type,
            InstalledEntryType::File | InstalledEntryType::Symlink
        ) {
            return Err(RuntimeError::Manifest(
                "AppRun must be a file or approved symlink".to_owned(),
            ));
        }
        if matches!(app_run.entry_type, InstalledEntryType::File) && !app_run.executable {
            return Err(RuntimeError::Manifest(
                "AppRun must be executable".to_owned(),
            ));
        }

        for entry in entries {
            if let Some(parent) = entry.path.parent() {
                if let Some(parent_entry) = inventory_paths.get(&parent) {
                    if !matches!(parent_entry.entry_type, InstalledEntryType::Directory) {
                        return Err(RuntimeError::Manifest(format!(
                            "inventory path '{}' is below a non-directory",
                            entry.path
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn app_run_path(&self) -> &RelativePath {
        &self.release.app_run_path
    }
}

/// A Runtime Release manifest whose installed-file inventory has been authenticated and resolved.
///
/// Every boundary that verifies, permissions, extracts against, launches from, or projects
/// core-binary provenance out of an installed Runtime tree takes this type rather than a bare
/// [`RuntimeManifest`]. That is the whole safety argument for the detached representation: the
/// entries reachable through [`VerifiedRuntimeManifest::inventory`] can only have come from the
/// authenticated manifest itself or from bytes that matched the length and SHA-256 the
/// authenticated manifest bound, and there is no constructor that supplies them any other way.
///
/// It derefs to the manifest, so component, release, and compatibility metadata read exactly as
/// before.
#[derive(Debug, Clone)]
pub struct VerifiedRuntimeManifest {
    manifest: RuntimeManifest,
    inventory: Vec<InstalledEntry>,
}

impl VerifiedRuntimeManifest {
    /// Resolve a manifest whose inventory is inline.
    ///
    /// Refuses a manifest that declares a detached inventory: an inline resolution of a detached
    /// manifest would be an empty or absent inventory, and an absent inventory must never look
    /// like a valid one.
    pub fn from_inline(manifest: RuntimeManifest) -> Result<Self, RuntimeError> {
        manifest.validate_structure()?;
        let ReleaseInventory::Inline(inventory) = &manifest.release.inventory else {
            return Err(RuntimeError::Manifest(
                "manifest declares a detached inventory but none was supplied".to_owned(),
            ));
        };
        let inventory = inventory.clone();
        manifest.validate_inventory(&inventory)?;
        Ok(Self {
            manifest,
            inventory,
        })
    }

    /// Resolve a manifest that declares a detached inventory, from that target's exact bytes.
    ///
    /// `bytes` must be the authenticated inventory target's content. Length and digest are checked
    /// against the manifest's own reference here, so this refuses truncated, padded, substituted,
    /// and wrong-release documents even if a caller obtained them from somewhere careless.
    pub fn from_detached_bytes(
        manifest: RuntimeManifest,
        bytes: &[u8],
    ) -> Result<Self, RuntimeError> {
        manifest.validate_structure()?;
        let Some(reference) = manifest.release.inventory.detached() else {
            return Err(RuntimeError::Manifest(
                "a detached inventory was supplied for an inline manifest".to_owned(),
            ));
        };
        if bytes.len() as u64 != reference.size_bytes {
            return Err(RuntimeError::Integrity(
                "detached inventory length does not match the manifest reference".to_owned(),
            ));
        }
        if Sha256Digest::of(bytes) != reference.sha256 {
            return Err(RuntimeError::Integrity(
                "detached inventory SHA-256 does not match the manifest reference".to_owned(),
            ));
        }
        let document = DetachedInventoryDocument::parse(bytes)?;
        if document.manifest_id != manifest.manifest_id
            || document.release_id != manifest.release.release_id
        {
            return Err(RuntimeError::Manifest(
                "detached inventory belongs to a different release".to_owned(),
            ));
        }
        manifest.validate_inventory(&document.entries)?;
        Ok(Self {
            manifest,
            inventory: document.entries,
        })
    }

    /// Resolve either representation, chosen by the manifest's own explicit statement.
    pub fn resolve(
        manifest: RuntimeManifest,
        detached_bytes: Option<&[u8]>,
    ) -> Result<Self, RuntimeError> {
        match (manifest.release.inventory.is_detached(), detached_bytes) {
            (false, None) => Self::from_inline(manifest),
            (true, Some(bytes)) => Self::from_detached_bytes(manifest, bytes),
            (true, None) => Err(RuntimeError::Manifest(
                "manifest declares a detached inventory but none was supplied".to_owned(),
            )),
            (false, Some(_)) => Err(RuntimeError::Manifest(
                "a detached inventory was supplied for an inline manifest".to_owned(),
            )),
        }
    }

    pub fn inventory(&self) -> &[InstalledEntry] {
        &self.inventory
    }

    pub fn manifest(&self) -> &RuntimeManifest {
        &self.manifest
    }

    pub fn into_manifest(self) -> RuntimeManifest {
        self.manifest
    }
}

impl Deref for VerifiedRuntimeManifest {
    type Target = RuntimeManifest;

    fn deref(&self) -> &Self::Target {
        &self.manifest
    }
}

fn resolve_relative_link(
    link_path: &RelativePath,
    target: &SymlinkTarget,
) -> Result<RelativePath, RuntimeError> {
    let mut components = Vec::new();
    if let Some(parent) = link_path.parent() {
        components.extend(parent.as_str().split('/').map(str::to_owned));
    }
    components.extend(target.as_str().split('/').map(str::to_owned));

    let mut normalized = Vec::new();
    for component in components {
        match component.as_str() {
            "." => {}
            ".." => {
                if normalized.pop().is_none() {
                    return Err(RuntimeError::Manifest(format!(
                        "symlink '{}' escapes the installation root",
                        link_path
                    )));
                }
            }
            value => normalized.push(value.to_owned()),
        }
    }
    RelativePath::new(normalized.join("/"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivePointer {
    pub schema_version: u32,
    pub installation_id: SafeIdentifier,
    pub manifest_sha256: Sha256Digest,
}

impl ActivePointer {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.schema_version != ACTIVE_POINTER_SCHEMA_VERSION {
            return Err(RuntimeError::Pointer(
                "unsupported active pointer schema version".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteMarker {
    pub schema_version: u32,
    pub installation_id: SafeIdentifier,
    pub manifest_sha256: Sha256Digest,
}

impl CompleteMarker {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.schema_version != COMPLETE_MARKER_SCHEMA_VERSION {
            return Err(RuntimeError::InstalledTree(
                "unsupported completion marker schema version".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimePolicy {
    pub minimum_safe_release_sequence: u64,
    #[serde(default)]
    pub revoked_release_ids: Vec<SafeIdentifier>,
}

impl RuntimePolicy {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        let mut revoked = BTreeSet::new();
        for release_id in &self.revoked_release_ids {
            if !revoked.insert(release_id) {
                return Err(RuntimeError::Trust(format!(
                    "release '{}' is revoked more than once",
                    release_id
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataVersions {
    pub timestamp: u64,
    pub snapshot: u64,
    pub targets: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedReleaseRecord {
    pub release_id: SafeIdentifier,
    pub release_sequence: u64,
    pub manifest_sha256: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTrustState {
    pub schema_version: u32,
    pub metadata_versions: MetadataVersions,
    pub minimum_safe_release_sequence: u64,
    #[serde(default)]
    pub revoked_release_ids: Vec<SafeIdentifier>,
    #[serde(default)]
    pub trusted_releases: Vec<TrustedReleaseRecord>,
}

impl Default for RuntimeTrustState {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeTrustState {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            metadata_versions: MetadataVersions::default(),
            minimum_safe_release_sequence: 0,
            revoked_release_ids: Vec::new(),
            trusted_releases: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.schema_version != 1 {
            return Err(RuntimeError::Trust(
                "unsupported runtime trust-state schema version".to_owned(),
            ));
        }
        let mut revoked = BTreeSet::new();
        for release_id in &self.revoked_release_ids {
            if !revoked.insert(release_id) {
                return Err(RuntimeError::Trust(format!(
                    "release '{}' is revoked more than once",
                    release_id
                )));
            }
        }
        let mut records = BTreeSet::new();
        for release in &self.trusted_releases {
            if release.release_sequence == 0
                || !records.insert((
                    release.release_id.clone(),
                    release.release_sequence,
                    release.manifest_sha256,
                ))
            {
                return Err(RuntimeError::Trust(
                    "runtime trust state contains an invalid or duplicate release record"
                        .to_owned(),
                ));
            }
        }
        Ok(())
    }

    pub fn permits(
        &self,
        release_id: &SafeIdentifier,
        release_sequence: u64,
        manifest_sha256: Sha256Digest,
    ) -> bool {
        release_sequence >= self.minimum_safe_release_sequence
            && !self.revoked_release_ids.iter().any(|id| id == release_id)
            && self.trusted_releases.iter().any(|record| {
                &record.release_id == release_id
                    && record.release_sequence == release_sequence
                    && record.manifest_sha256 == manifest_sha256
            })
    }
}

#[derive(Debug, Clone)]
pub struct StrictJsonValue(serde_json::Value);

impl<'de> Deserialize<'de> for StrictJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StrictValueVisitor;

        impl<'de> Visitor<'de> for StrictValueVisitor {
            type Value = StrictJsonValue;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON value without duplicate object keys")
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: DeserializeError,
            {
                Ok(StrictJsonValue(serde_json::Value::Null))
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
            where
                E: DeserializeError,
            {
                Ok(StrictJsonValue(serde_json::Value::Bool(value)))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: DeserializeError,
            {
                Ok(StrictJsonValue(serde_json::Value::Number(value.into())))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: DeserializeError,
            {
                Ok(StrictJsonValue(serde_json::Value::Number(value.into())))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: DeserializeError,
            {
                let number = serde_json::Number::from_f64(value)
                    .ok_or_else(|| E::custom("invalid JSON number"))?;
                Ok(StrictJsonValue(serde_json::Value::Number(number)))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: DeserializeError,
            {
                Ok(StrictJsonValue(serde_json::Value::String(value.to_owned())))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: DeserializeError,
            {
                self.visit_str(&value)
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<StrictJsonValue>()? {
                    values.push(value.0);
                }
                Ok(StrictJsonValue(serde_json::Value::Array(values)))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON object key '{key}'"
                        )));
                    }
                    let value = map.next_value::<StrictJsonValue>()?;
                    values.insert(key, value.0);
                }
                Ok(StrictJsonValue(serde_json::Value::Object(
                    values.into_iter().collect(),
                )))
            }
        }

        deserializer.deserialize_any(StrictValueVisitor)
    }
}

pub fn parse_strict_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value =
        StrictJsonValue::deserialize(&mut deserializer).map_err(|error| error.to_string())?;
    deserializer
        .end()
        .map_err(|error| format!("trailing JSON data: {error}"))?;
    serde_json::from_value(value.0).map_err(|error| error.to_string())
}

pub fn serialize_json<T: Serialize>(value: &T) -> Result<Vec<u8>, RuntimeError> {
    serde_json::to_vec(value).map_err(|error| RuntimeError::Pointer(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        parse_strict_json, ArchiveFormat, ComponentKind, DetachedInventoryDocument,
        DetachedInventoryReference, ExtractionLimits, InstalledEntry, InstalledEntryType,
        RelativePath, ReleaseChannel, ReleaseInventory, RuntimeArchitecture, RuntimeCompatibility,
        RuntimeComponent, RuntimeError, RuntimeManifest, RuntimePlatform, RuntimeRelease,
        SafeIdentifier, Sha256Digest, VerifiedRuntimeManifest, DETACHED_INVENTORY_SCHEMA_VERSION,
        MAX_DETACHED_INVENTORY_BYTES, RUNTIME_MANIFEST_SCHEMA_VERSION,
    };

    #[test]
    fn rejects_unsafe_identifiers_and_paths() {
        assert!(SafeIdentifier::new("../runtime").is_err());
        assert!(SafeIdentifier::new("safe/id").is_err());
        assert!(RelativePath::new("../outside").is_err());
        assert!(RelativePath::new("/absolute").is_err());
        assert!(RelativePath::new("runtime\\AppRun").is_err());
    }

    #[test]
    fn parses_only_lower_or_upper_hex_with_exact_sha256_length() {
        assert!(Sha256Digest::from_hex(&"a".repeat(64)).is_ok());
        assert!(Sha256Digest::from_hex(&"A".repeat(64)).is_ok());
        assert!(Sha256Digest::from_hex(&"g".repeat(64)).is_err());
        assert!(Sha256Digest::from_hex(&"a".repeat(63)).is_err());
    }

    #[test]
    fn rejects_duplicate_json_keys_recursively() {
        let result: Result<serde_json::Value, _> = parse_strict_json(br#"{"outer":{"x":1,"x":2}}"#);
        assert!(result.is_err());
    }

    fn valid_manifest() -> RuntimeManifest {
        RuntimeManifest {
            schema_version: RUNTIME_MANIFEST_SCHEMA_VERSION,
            manifest_id: SafeIdentifier::new("manifest-1").unwrap(),
            channel: ReleaseChannel::Stable,
            min_retrofrontier_version: "0.1.0".to_owned(),
            release: RuntimeRelease {
                release_id: SafeIdentifier::new("release-1").unwrap(),
                release_sequence: 1,
                retrofrontier_runtime_version: "1".to_owned(),
                retroarch_version: "1".to_owned(),
                platform: RuntimePlatform::Linux,
                architecture: RuntimeArchitecture::X86_64,
                components: vec![RuntimeComponent {
                    id: SafeIdentifier::new("retroarch").unwrap(),
                    kind: ComponentKind::Runtime,
                    target_name: "targets/runtime.tar".to_owned(),
                    source_id: None,
                    source_url: None,
                    archive_format: ArchiveFormat::AppImage,
                    archive_size_bytes: 1,
                    sha256: Sha256Digest::from_hex(&"b".repeat(64)).unwrap(),
                    install_path: RelativePath::new("runtime/app").unwrap(),
                    expected_root: None,
                    payload_filename: None,
                    executable_relative_path: None,
                    display_version: None,
                    source_revision: None,
                    source_pinning: None,
                    license: "GPL-3.0-or-later".to_owned(),
                    systems: Vec::new(),
                }],
                app_run_path: RelativePath::new("runtime/app/AppRun").unwrap(),
                inventory: ReleaseInventory::Inline(vec![
                    InstalledEntry {
                        path: RelativePath::new("runtime").unwrap(),
                        entry_type: InstalledEntryType::Directory,
                        size_bytes: 0,
                        sha256: None,
                        executable: false,
                        link_target: None,
                    },
                    InstalledEntry {
                        path: RelativePath::new("runtime/app").unwrap(),
                        entry_type: InstalledEntryType::Directory,
                        size_bytes: 0,
                        sha256: None,
                        executable: false,
                        link_target: None,
                    },
                    InstalledEntry {
                        path: RelativePath::new("runtime/app/AppRun").unwrap(),
                        entry_type: InstalledEntryType::File,
                        size_bytes: 1,
                        sha256: Some(Sha256Digest::from_hex(&"a".repeat(64)).unwrap()),
                        executable: true,
                        link_target: None,
                    },
                ]),
                extraction: ExtractionLimits::default(),
            },
            compatibility: RuntimeCompatibility {
                retroarch_core_api: "1".to_owned(),
                save_state_policy: "isolated".to_owned(),
            },
        }
    }

    /// Parse the manifest bytes the way a client does, then resolve its inline inventory.
    fn parse_and_resolve(bytes: &[u8]) -> Result<VerifiedRuntimeManifest, RuntimeError> {
        VerifiedRuntimeManifest::from_inline(RuntimeManifest::parse(bytes)?)
    }

    #[test]
    fn accepts_a_valid_manifest() {
        let bytes = serde_json::to_vec(&valid_manifest()).unwrap();
        assert!(parse_and_resolve(&bytes).is_ok());
    }

    #[test]
    fn rejects_invalid_schema_platform_identifiers_hashes_and_paths() {
        let mut schema = valid_manifest();
        schema.schema_version += 1;
        assert!(RuntimeManifest::parse(&serde_json::to_vec(&schema).unwrap()).is_err());

        let mut platform = valid_manifest();
        platform.release.platform = RuntimePlatform::Windows;
        assert!(RuntimeManifest::parse(&serde_json::to_vec(&platform).unwrap()).is_err());

        let mut archive_format = valid_manifest();
        archive_format.release.components[0].archive_format = ArchiveFormat::Tar;
        assert!(RuntimeManifest::parse(&serde_json::to_vec(&archive_format).unwrap()).is_err());

        let mut invalid_id = serde_json::to_value(valid_manifest()).unwrap();
        invalid_id["manifest_id"] = serde_json::Value::String("../unsafe".to_owned());
        assert!(RuntimeManifest::parse(&serde_json::to_vec(&invalid_id).unwrap()).is_err());

        let mut invalid_hash = serde_json::to_value(valid_manifest()).unwrap();
        invalid_hash["release"]["components"][0]["sha256"] =
            serde_json::Value::String("z".repeat(64));
        assert!(RuntimeManifest::parse(&serde_json::to_vec(&invalid_hash).unwrap()).is_err());

        let mut unsafe_path = serde_json::to_value(valid_manifest()).unwrap();
        unsafe_path["release"]["app_run_path"] = serde_json::Value::String("../AppRun".to_owned());
        assert!(RuntimeManifest::parse(&serde_json::to_vec(&unsafe_path).unwrap()).is_err());

        let mut duplicate = valid_manifest();
        let duplicate_entry = duplicate.release.inventory.inline_entries_mut()[2].clone();
        duplicate
            .release
            .inventory
            .inline_entries_mut()
            .push(duplicate_entry);
        assert!(parse_and_resolve(&serde_json::to_vec(&duplicate).unwrap()).is_err());
    }

    // -- ADR-012 detached installed-file inventory ---------------------------------------------

    const INVENTORY_TARGET: &str = "release-1.inventory.json";

    fn inline_entries(manifest: &RuntimeManifest) -> Vec<InstalledEntry> {
        match &manifest.release.inventory {
            ReleaseInventory::Inline(entries) => entries.clone(),
            ReleaseInventory::Detached(_) => panic!("fixture is inline"),
        }
    }

    /// Build the detached counterpart of [`valid_manifest`]: the same inventory, moved into a
    /// separate document, with the manifest bound to that document's exact length and digest.
    fn detached_pair() -> (RuntimeManifest, Vec<u8>) {
        let inline = valid_manifest();
        let document = DetachedInventoryDocument {
            schema_version: DETACHED_INVENTORY_SCHEMA_VERSION,
            manifest_id: inline.manifest_id.clone(),
            release_id: inline.release.release_id.clone(),
            entries: inline_entries(&inline),
        };
        let bytes = serde_json::to_vec(&document).unwrap();
        detach_with(inline, bytes)
    }

    /// Bind `manifest` to exactly `bytes`, so only the *content* of the document is under test.
    fn detach_with(mut manifest: RuntimeManifest, bytes: Vec<u8>) -> (RuntimeManifest, Vec<u8>) {
        manifest.release.inventory = ReleaseInventory::Detached(DetachedInventoryReference {
            target_name: INVENTORY_TARGET.to_owned(),
            size_bytes: bytes.len() as u64,
            sha256: Sha256Digest::of(&bytes),
        });
        (manifest, bytes)
    }

    /// Take the manifest through real JSON, the way a client only ever sees one.
    fn reparse(manifest: &RuntimeManifest) -> RuntimeManifest {
        RuntimeManifest::parse(&serde_json::to_vec(manifest).unwrap())
            .expect("the manifest parses structurally")
    }

    /// C1 — the inline representation is unchanged on the wire.
    ///
    /// Runtime Release 001 and 002 are published, immutable, inline manifests whose SHA-256 is
    /// pinned in TUF targets metadata and in persisted client trust state. Introducing the detached
    /// representation must therefore not alter one byte of how an inline inventory serializes: it
    /// is still the bare JSON array under `inventory`, with no representation tag and no wrapper.
    #[test]
    fn an_inline_inventory_still_serializes_as_a_bare_json_array() {
        let manifest = valid_manifest();
        let value = serde_json::to_value(&manifest).unwrap();
        let inventory = &value["release"]["inventory"];
        assert!(
            inventory.is_array(),
            "an inline inventory must stay a JSON array: {inventory}"
        );
        assert_eq!(inventory.as_array().unwrap().len(), 3);

        // And it round-trips back to the same entries through the client's own parse path.
        let resolved = parse_and_resolve(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(!resolved.release.inventory.is_detached());
        assert_eq!(resolved.inventory(), inline_entries(&manifest).as_slice());
    }

    /// C1b — the manifest's authenticated field set is unchanged.
    ///
    /// Release 002 is published and immutable: its bytes are pinned by TUF targets metadata, by
    /// `active.json`, by the completion marker, and by persisted client trust state. Any added,
    /// removed, or renamed manifest field would change those bytes and make the published release
    /// unparseable or unverifiable. The detached representation is expressed entirely *inside* the
    /// existing `inventory` field, so this key set must not move.
    #[test]
    fn the_authenticated_manifest_field_set_is_unchanged() {
        fn keys(value: &serde_json::Value) -> Vec<&str> {
            let mut keys: Vec<&str> = value
                .as_object()
                .expect("a JSON object")
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort_unstable();
            keys
        }

        let value = serde_json::to_value(valid_manifest()).unwrap();
        assert_eq!(
            keys(&value),
            vec![
                "channel",
                "compatibility",
                "manifest_id",
                "min_retrofrontier_version",
                "release",
                "schema_version",
            ]
        );
        assert_eq!(
            keys(&value["release"]),
            vec![
                "app_run_path",
                "architecture",
                "components",
                "extraction",
                "inventory",
                "platform",
                "release_id",
                "release_sequence",
                "retroarch_version",
                "retrofrontier_runtime_version",
            ]
        );
        assert_eq!(RUNTIME_MANIFEST_SCHEMA_VERSION, 1);
    }

    /// C2 — the detached happy path: a manifest that references an inventory target resolves from
    /// that target's exact bytes, and the result is the same inventory an inline manifest carries.
    #[test]
    fn a_detached_manifest_resolves_from_its_exact_target_bytes() {
        let (manifest, bytes) = detached_pair();
        let parsed = reparse(&manifest);
        let reference = parsed
            .release
            .inventory
            .detached()
            .expect("the parsed manifest still declares a detached inventory");
        assert_eq!(reference.target_name, INVENTORY_TARGET);
        assert_eq!(reference.size_bytes, bytes.len() as u64);
        assert_eq!(reference.sha256, Sha256Digest::of(&bytes));

        let resolved = VerifiedRuntimeManifest::from_detached_bytes(parsed, &bytes).unwrap();
        assert_eq!(
            resolved.inventory(),
            inline_entries(&valid_manifest()).as_slice(),
            "both representations must yield the identical authenticated inventory"
        );
    }

    /// C3 — the representation is explicit. An object with no tag, the wrong tag, an unknown
    /// field, or a missing field is refused rather than being read as some default.
    #[test]
    fn a_detached_reference_must_carry_its_explicit_representation_tag() {
        let (manifest, _) = detached_pair();
        let mut value = serde_json::to_value(&manifest).unwrap();
        let original = value["release"]["inventory"].clone();
        assert_eq!(
            original["representation"],
            serde_json::json!(super::DETACHED_INVENTORY_REPRESENTATION)
        );

        for mutate in [
            // No tag at all.
            |inventory: &mut serde_json::Value| {
                inventory.as_object_mut().unwrap().remove("representation");
            },
            // A tag this client does not implement.
            |inventory: &mut serde_json::Value| {
                inventory["representation"] = serde_json::json!("detached_url");
            },
            // An extra field the schema forbids.
            |inventory: &mut serde_json::Value| {
                inventory["fetch_url"] =
                    serde_json::json!("https://example.invalid/inventory.json");
            },
            // A missing required field.
            |inventory: &mut serde_json::Value| {
                inventory.as_object_mut().unwrap().remove("sha256");
            },
        ] {
            let mut inventory = original.clone();
            mutate(&mut inventory);
            value["release"]["inventory"] = inventory;
            assert!(
                RuntimeManifest::parse(&serde_json::to_vec(&value).unwrap()).is_err(),
                "an ambiguous inventory representation must be refused"
            );
        }

        // A duplicate key inside the reference is refused by the strict JSON reader.
        let manifest_json = serde_json::to_string(&value).unwrap();
        let duplicated = manifest_json.replace(
            r#""representation":"detached_target""#,
            r#""representation":"detached_target","representation":"detached_target""#,
        );
        assert!(RuntimeManifest::parse(duplicated.as_bytes()).is_err());
    }

    /// C4 — wrong length, wrong digest, truncation, and padding all fail closed.
    #[test]
    fn detached_inventory_bytes_must_match_the_manifest_reference_exactly() {
        let (manifest, bytes) = detached_pair();

        // Truncated.
        let truncated = bytes[..bytes.len() - 1].to_vec();
        assert!(matches!(
            VerifiedRuntimeManifest::from_detached_bytes(reparse(&manifest), &truncated),
            Err(RuntimeError::Integrity(_))
        ));

        // Padded to a different length.
        let mut padded = bytes.clone();
        padded.push(b' ');
        assert!(matches!(
            VerifiedRuntimeManifest::from_detached_bytes(reparse(&manifest), &padded),
            Err(RuntimeError::Integrity(_))
        ));

        // Same length, different bytes: only the digest catches this.
        let mut substituted = bytes.clone();
        let last = substituted.len() - 1;
        substituted[last] = b' ';
        assert!(matches!(
            VerifiedRuntimeManifest::from_detached_bytes(reparse(&manifest), &substituted),
            Err(RuntimeError::Integrity(_))
        ));

        // A manifest that declares the wrong length for otherwise correct bytes is refused too.
        let mut wrong_length = manifest.clone();
        wrong_length.release.inventory = ReleaseInventory::Detached(DetachedInventoryReference {
            target_name: INVENTORY_TARGET.to_owned(),
            size_bytes: bytes.len() as u64 + 1,
            sha256: Sha256Digest::of(&bytes),
        });
        assert!(
            VerifiedRuntimeManifest::from_detached_bytes(reparse(&wrong_length), &bytes).is_err()
        );

        // …and so is one that declares the wrong digest.
        let mut wrong_digest = manifest;
        wrong_digest.release.inventory = ReleaseInventory::Detached(DetachedInventoryReference {
            target_name: INVENTORY_TARGET.to_owned(),
            size_bytes: bytes.len() as u64,
            sha256: Sha256Digest::from_hex(&"c".repeat(64)).unwrap(),
        });
        assert!(
            VerifiedRuntimeManifest::from_detached_bytes(reparse(&wrong_digest), &bytes).is_err()
        );
    }

    /// C5 — malformed, wrong-schema, and wrong-release documents are refused even when their
    /// length and digest are exactly what the manifest says.
    #[test]
    fn a_malformed_or_foreign_detached_inventory_is_refused_at_matching_length_and_digest() {
        let malformed = detach_with(valid_manifest(), b"{ not json".to_vec());
        assert!(matches!(
            VerifiedRuntimeManifest::from_detached_bytes(reparse(&malformed.0), &malformed.1),
            Err(RuntimeError::Manifest(_))
        ));

        let entries = inline_entries(&valid_manifest());
        let future_schema = serde_json::to_vec(&serde_json::json!({
            "schema_version": DETACHED_INVENTORY_SCHEMA_VERSION + 1,
            "manifest_id": "manifest-1",
            "release_id": "release-1",
            "entries": entries,
        }))
        .unwrap();
        let future = detach_with(valid_manifest(), future_schema);
        assert!(
            VerifiedRuntimeManifest::from_detached_bytes(reparse(&future.0), &future.1).is_err()
        );

        // An unknown field inside the document is refused, not ignored.
        let extra = serde_json::to_vec(&serde_json::json!({
            "schema_version": DETACHED_INVENTORY_SCHEMA_VERSION,
            "manifest_id": "manifest-1",
            "release_id": "release-1",
            "entries": entries,
            "surprise": true,
        }))
        .unwrap();
        let extra = detach_with(valid_manifest(), extra);
        assert!(VerifiedRuntimeManifest::from_detached_bytes(reparse(&extra.0), &extra.1).is_err());

        // A well-formed inventory belonging to a different release is refused: the digest already
        // prevents substitution, and the recorded owner makes the refusal explicit.
        let foreign = serde_json::to_vec(&DetachedInventoryDocument {
            schema_version: DETACHED_INVENTORY_SCHEMA_VERSION,
            manifest_id: SafeIdentifier::new("manifest-2").unwrap(),
            release_id: SafeIdentifier::new("release-2").unwrap(),
            entries,
        })
        .unwrap();
        let foreign = detach_with(valid_manifest(), foreign);
        let error = VerifiedRuntimeManifest::from_detached_bytes(reparse(&foreign.0), &foreign.1)
            .expect_err("a foreign inventory must be refused");
        assert!(
            format!("{error}").contains("different release"),
            "unexpected error: {error}"
        );
    }

    /// C6 — the two representations never substitute for one another.
    #[test]
    fn the_representations_are_never_interchangeable() {
        let (detached, bytes) = detached_pair();

        // A detached manifest resolved as if it were inline would produce no inventory at all.
        assert!(VerifiedRuntimeManifest::from_inline(reparse(&detached)).is_err());
        assert!(VerifiedRuntimeManifest::resolve(reparse(&detached), None).is_err());

        // Detached bytes offered for an inline manifest are refused rather than preferred.
        let inline = valid_manifest();
        assert!(VerifiedRuntimeManifest::from_detached_bytes(reparse(&inline), &bytes).is_err());
        assert!(VerifiedRuntimeManifest::resolve(reparse(&inline), Some(&bytes)).is_err());
    }

    /// C7 — the detached target is a target name, never a URL or a path, and never a name that
    /// already belongs to a component.
    #[test]
    fn a_detached_target_name_is_flat_safe_and_distinct_from_every_component() {
        let (manifest, _) = detached_pair();
        for name in [
            "https://example.invalid/inventory.json",
            "../inventory.json",
            "targets/inventory.json",
            "/etc/inventory.json",
            "",
        ] {
            let mut value = serde_json::to_value(&manifest).unwrap();
            value["release"]["inventory"]["target_name"] = serde_json::json!(name);
            assert!(
                RuntimeManifest::parse(&serde_json::to_vec(&value).unwrap()).is_err(),
                "'{name}' must not be accepted as an inventory target name"
            );
        }

        let mut value = serde_json::to_value(&manifest).unwrap();
        value["release"]["components"][0]["target_name"] = serde_json::json!("runtime.tar");
        value["release"]["inventory"]["target_name"] = serde_json::json!("runtime.tar");
        let error = RuntimeManifest::parse(&serde_json::to_vec(&value).unwrap())
            .expect_err("a name collision with a component target must be refused");
        assert!(
            format!("{error}").contains("collides"),
            "unexpected error: {error}"
        );
    }

    /// C8 — the detached inventory has its own explicit bound, and the manifest bound is unchanged.
    #[test]
    fn the_detached_inventory_has_an_independent_explicit_size_bound() {
        // The manifest bound is untouched by this milestone. That the detached bound exceeds it
        // is asserted at compile time beside the two constants.
        assert_eq!(super::MAX_MANIFEST_BYTES, 1024 * 1024);

        // A reference beyond the bound is refused during manifest validation, before anything is
        // fetched or read.
        let (manifest, _) = detached_pair();
        let mut value = serde_json::to_value(&manifest).unwrap();
        value["release"]["inventory"]["size_bytes"] =
            serde_json::json!(MAX_DETACHED_INVENTORY_BYTES + 1);
        let error = RuntimeManifest::parse(&serde_json::to_vec(&value).unwrap())
            .expect_err("an oversized reference must be refused");
        assert!(
            format!("{error}").contains("size limit"),
            "unexpected error: {error}"
        );

        // A zero-length reference is refused as well: an empty inventory is never a valid one.
        let mut value = serde_json::to_value(&manifest).unwrap();
        value["release"]["inventory"]["size_bytes"] = serde_json::json!(0);
        assert!(RuntimeManifest::parse(&serde_json::to_vec(&value).unwrap()).is_err());

        // And the document parser enforces the bound on its own, independently of any manifest.
        let oversized = vec![b' '; MAX_DETACHED_INVENTORY_BYTES as usize + 1];
        assert!(DetachedInventoryDocument::parse(&oversized).is_err());
    }

    /// C9 — entry-count and path bounds apply to a detached inventory too.
    #[test]
    fn inventory_entry_and_path_bounds_are_enforced() {
        let manifest = valid_manifest();
        let template = inline_entries(&manifest)[0].clone();
        let too_many: Vec<InstalledEntry> = (0..=super::MAX_INVENTORY_ENTRIES)
            .map(|index| InstalledEntry {
                path: RelativePath::new(format!("runtime/e{index}")).unwrap(),
                ..template.clone()
            })
            .collect();
        let error = manifest
            .validate_inventory(&too_many)
            .expect_err("an inventory beyond the entry limit must be refused");
        assert!(
            format!("{error}").contains("entry limit"),
            "unexpected error: {error}"
        );

        // A path beyond `RelativePath`'s bound cannot even be deserialized into an entry.
        let long_path = "a".repeat(5000);
        let document = serde_json::json!({
            "schema_version": DETACHED_INVENTORY_SCHEMA_VERSION,
            "manifest_id": "manifest-1",
            "release_id": "release-1",
            "entries": [{
                "path": long_path,
                "entry_type": "file",
                "size_bytes": 1,
                "sha256": "a".repeat(64),
                "executable": false,
                "link_target": null,
            }],
        });
        assert!(DetachedInventoryDocument::parse(&serde_json::to_vec(&document).unwrap()).is_err());
    }

    /// C10 — a detached inventory that does not describe the release's components is refused.
    ///
    /// This is the same check an inline manifest gets, which is the point: moving the inventory out
    /// of the manifest must not move it out of validation.
    #[test]
    fn a_detached_inventory_that_mismatches_the_components_is_refused() {
        let manifest = valid_manifest();
        let mut entries = inline_entries(&manifest);
        // Drop the runtime component's install-path directory.
        entries.retain(|entry| entry.path.as_str() != "runtime/app");
        let document = serde_json::to_vec(&DetachedInventoryDocument {
            schema_version: DETACHED_INVENTORY_SCHEMA_VERSION,
            manifest_id: manifest.manifest_id.clone(),
            release_id: manifest.release.release_id.clone(),
            entries,
        })
        .unwrap();
        let (manifest, bytes) = detach_with(manifest, document);
        let error = VerifiedRuntimeManifest::from_detached_bytes(reparse(&manifest), &bytes)
            .expect_err("a component with no inventory directory must be refused");
        assert!(
            format!("{error}").contains("inventory directory"),
            "unexpected error: {error}"
        );

        // AppRun missing from a detached inventory is refused for the same reason.
        let manifest = valid_manifest();
        let mut entries = inline_entries(&manifest);
        entries.retain(|entry| entry.path.as_str() != "runtime/app/AppRun");
        let document = serde_json::to_vec(&DetachedInventoryDocument {
            schema_version: DETACHED_INVENTORY_SCHEMA_VERSION,
            manifest_id: manifest.manifest_id.clone(),
            release_id: manifest.release.release_id.clone(),
            entries,
        })
        .unwrap();
        let (manifest, bytes) = detach_with(manifest, document);
        assert!(VerifiedRuntimeManifest::from_detached_bytes(reparse(&manifest), &bytes).is_err());
    }

    /// C11 — installation metadata can never be claimed as payload, `release-inventory.json`
    /// included. Otherwise an inventory entry could describe the very file that authenticates it.
    #[test]
    fn installation_metadata_filenames_are_not_valid_inventory_paths() {
        let manifest = valid_manifest();
        for reserved in [
            "release-manifest.json",
            "complete.json",
            "release-inventory.json",
        ] {
            let mut entries = inline_entries(&manifest);
            entries.push(InstalledEntry {
                path: RelativePath::new(reserved).unwrap(),
                entry_type: InstalledEntryType::File,
                size_bytes: 1,
                sha256: Some(Sha256Digest::from_hex(&"a".repeat(64)).unwrap()),
                executable: false,
                link_target: None,
            });
            assert!(
                manifest.validate_inventory(&entries).is_err(),
                "'{reserved}' must not be a payload inventory path"
            );
        }
    }
}
