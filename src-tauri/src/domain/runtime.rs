use serde::de::{DeserializeOwned, Error as DeserializeError, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use thiserror::Error;

pub const RUNTIME_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const ACTIVE_POINTER_SCHEMA_VERSION: u32 = 1;
pub const COMPLETE_MARKER_SCHEMA_VERSION: u32 = 1;
pub const MANAGED_PROCESS_RECORD_SCHEMA_VERSION: u32 = 2;
pub const MAX_ACTIVE_POINTER_BYTES: u64 = 4 * 1024;
pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub const MAX_TRUST_STATE_BYTES: u64 = 1024 * 1024;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedProcessRecord {
    pub schema_version: u32,
    pub phase: ManagedProcessPhase,
    pub pid: u32,
    pub process_start_time_ticks: u64,
    pub boot_id: String,
    pub installation_id: SafeIdentifier,
    pub expected_apprun_path: String,
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
        if self.pid == 0
            || self.process_start_time_ticks == 0
            || self.boot_id.trim().is_empty()
            || self.expected_apprun_path.is_empty()
            || !std::path::Path::new(&self.expected_apprun_path).is_absolute()
            || self
                .expected_executable_path
                .as_deref()
                .is_some_and(|path| path.is_empty() || !std::path::Path::new(path).is_absolute())
        {
            return Err(RuntimeError::GameActive);
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledEntry {
    pub path: RelativePath,
    pub entry_type: InstalledEntryType,
    pub size_bytes: u64,
    pub sha256: Option<Sha256Digest>,
    pub executable: bool,
    pub link_target: Option<SymlinkTarget>,
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
    #[serde(default)]
    pub inventory: Vec<InstalledEntry>,
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
    pub fn parse(bytes: &[u8]) -> Result<Self, RuntimeError> {
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(RuntimeError::Manifest("manifest is too large".to_owned()));
        }
        let manifest: Self =
            parse_strict_json(bytes).map_err(|error| RuntimeError::Manifest(error.to_owned()))?;
        manifest.validate_for_linux_x86_64()?;
        Ok(manifest)
    }

    pub fn validate_for_linux_x86_64(&self) -> Result<(), RuntimeError> {
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

        let mut inventory_paths = BTreeMap::new();
        for entry in &self.release.inventory {
            if entry.path.as_str() == "release-manifest.json"
                || entry.path.as_str() == "complete.json"
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
        for entry in &self.release.inventory {
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

        for entry in &self.release.inventory {
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
        parse_strict_json, ArchiveFormat, ComponentKind, ExtractionLimits, InstalledEntry,
        InstalledEntryType, RelativePath, ReleaseChannel, RuntimeArchitecture,
        RuntimeCompatibility, RuntimeComponent, RuntimeManifest, RuntimePlatform, RuntimeRelease,
        SafeIdentifier, Sha256Digest, RUNTIME_MANIFEST_SCHEMA_VERSION,
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
                inventory: vec![
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
                ],
                extraction: ExtractionLimits::default(),
            },
            compatibility: RuntimeCompatibility {
                retroarch_core_api: "1".to_owned(),
                save_state_policy: "isolated".to_owned(),
            },
        }
    }

    #[test]
    fn accepts_a_valid_manifest() {
        let bytes = serde_json::to_vec(&valid_manifest()).unwrap();
        assert!(RuntimeManifest::parse(&bytes).is_ok());
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
        let duplicate_entry = duplicate.release.inventory[2].clone();
        duplicate.release.inventory.push(duplicate_entry);
        assert!(RuntimeManifest::parse(&serde_json::to_vec(&duplicate).unwrap()).is_err());
    }
}
