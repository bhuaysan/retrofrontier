//! Credential boundary for metadata providers.
//!
//! Two very different kinds of secret meet here and are deliberately kept apart:
//!
//! * **Application developer credentials** identify RetroFrontier to the provider. They come from
//!   build-time injection for releases and from the ignored local environment during development.
//!   A distributed desktop binary necessarily makes them recoverable, so this module treats them
//!   as an application credential and never as a cryptographic secret boundary.
//! * **Optional personal user credentials** belong to one individual. They are persisted only in
//!   the OS credential vault behind [`CredentialVault`], never in SQLite, settings files, or logs.
//!
//! No type in this module implements `Serialize`, and every `Debug`/`Display` implementation is
//! redacted, so a secret cannot reach IPC, tracing output, or a panic message by accident.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;
use thiserror::Error;

/// Placeholder written wherever a secret would otherwise be rendered.
pub const REDACTED: &str = "<redacted>";

/// A string whose contents must never be printed, serialized, or logged.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The only way to read the secret. Call sites should keep the borrow as short as possible.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        // Best-effort scrubbing. Filling with NUL keeps the buffer valid UTF-8, and the allocation
        // may still have been copied by the allocator, so this reduces exposure rather than
        // guaranteeing erasure.
        let bytes = unsafe { self.0.as_mut_vec() };
        bytes.fill(0);
    }
}

/// RetroFrontier's own application credentials for a provider.
#[derive(Clone, PartialEq, Eq)]
pub struct DeveloperCredentials {
    pub developer_id: SecretString,
    pub developer_password: SecretString,
}

impl fmt::Debug for DeveloperCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeveloperCredentials")
            .field("developer_id", &REDACTED)
            .field("developer_password", &REDACTED)
            .finish()
    }
}

/// One user's optional personal provider account.
#[derive(Clone, PartialEq, Eq)]
pub struct UserCredentials {
    /// The account name. Not a secret, but still personal data, so it is stored in the vault
    /// beside the password rather than in SQLite.
    pub username: String,
    pub password: SecretString,
}

impl fmt::Debug for UserCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserCredentials")
            .field("username", &REDACTED)
            .field("password", &REDACTED)
            .finish()
    }
}

/// Failure reading or writing the OS credential vault.
///
/// The variants deliberately carry no platform error: some vault errors include the raw stored
/// blob, which would turn an error log into a secret leak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CredentialVaultError {
    #[error("the operating-system credential vault is unavailable")]
    Unavailable,
    #[error("the stored credential could not be interpreted")]
    Malformed,
}

/// Persistence boundary for personal provider credentials.
pub trait CredentialVault: Send + Sync {
    fn store(
        &self,
        reference: &str,
        credentials: &UserCredentials,
    ) -> Result<(), CredentialVaultError>;

    fn load(&self, reference: &str) -> Result<Option<UserCredentials>, CredentialVaultError>;

    fn delete(&self, reference: &str) -> Result<(), CredentialVaultError>;
}

/// Vault entry payload. Username and password travel together as one opaque vault secret.
fn encode_entry(credentials: &UserCredentials) -> SecretString {
    // A single newline separator keeps the payload dependency-free and unambiguous: the username
    // is rejected on input if it contains a newline.
    SecretString::new(format!(
        "{}\n{}",
        credentials.username,
        credentials.password.expose()
    ))
}

fn decode_entry(payload: &str) -> Result<UserCredentials, CredentialVaultError> {
    let (username, password) = payload
        .split_once('\n')
        .ok_or(CredentialVaultError::Malformed)?;
    Ok(UserCredentials {
        username: username.to_owned(),
        password: SecretString::new(password),
    })
}

/// OS keychain/credential-vault implementation.
pub struct KeyringCredentialVault {
    service: String,
}

impl KeyringCredentialVault {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, reference: &str) -> Result<keyring::Entry, CredentialVaultError> {
        keyring::Entry::new(&self.service, reference).map_err(|_| CredentialVaultError::Unavailable)
    }
}

impl CredentialVault for KeyringCredentialVault {
    fn store(
        &self,
        reference: &str,
        credentials: &UserCredentials,
    ) -> Result<(), CredentialVaultError> {
        let payload = encode_entry(credentials);
        self.entry(reference)?
            .set_password(payload.expose())
            .map_err(|_| CredentialVaultError::Unavailable)
    }

    fn load(&self, reference: &str) -> Result<Option<UserCredentials>, CredentialVaultError> {
        match self.entry(reference)?.get_password() {
            Ok(payload) => decode_entry(&payload).map(Some),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(keyring::Error::BadEncoding(_)) | Err(keyring::Error::BadDataFormat(_, _)) => {
                Err(CredentialVaultError::Malformed)
            }
            Err(_) => Err(CredentialVaultError::Unavailable),
        }
    }

    fn delete(&self, reference: &str) -> Result<(), CredentialVaultError> {
        match self.entry(reference)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(CredentialVaultError::Unavailable),
        }
    }
}

/// Process-lifetime vault.
///
/// Used by tests so no suite ever needs a real OS keychain, and as the session-only fallback when
/// secure persistence is unavailable on the host.
#[derive(Default)]
pub struct InMemoryCredentialVault {
    entries: Mutex<BTreeMap<String, String>>,
    available: Mutex<bool>,
}

impl InMemoryCredentialVault {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(BTreeMap::new()),
            available: Mutex::new(true),
        }
    }

    /// Simulates a locked or missing platform vault.
    pub fn set_available(&self, available: bool) {
        *self
            .available
            .lock()
            .expect("credential vault mutex is not poisoned") = available;
    }

    /// Test helper for corrupted vault payloads.
    pub fn insert_raw(&self, reference: &str, payload: &str) {
        self.entries
            .lock()
            .expect("credential vault mutex is not poisoned")
            .insert(reference.to_owned(), payload.to_owned());
    }

    fn ensure_available(&self) -> Result<(), CredentialVaultError> {
        if *self
            .available
            .lock()
            .expect("credential vault mutex is not poisoned")
        {
            Ok(())
        } else {
            Err(CredentialVaultError::Unavailable)
        }
    }
}

impl CredentialVault for InMemoryCredentialVault {
    fn store(
        &self,
        reference: &str,
        credentials: &UserCredentials,
    ) -> Result<(), CredentialVaultError> {
        self.ensure_available()?;
        let payload = encode_entry(credentials);
        self.entries
            .lock()
            .expect("credential vault mutex is not poisoned")
            .insert(reference.to_owned(), payload.expose().to_owned());
        Ok(())
    }

    fn load(&self, reference: &str) -> Result<Option<UserCredentials>, CredentialVaultError> {
        self.ensure_available()?;
        let stored = self
            .entries
            .lock()
            .expect("credential vault mutex is not poisoned")
            .get(reference)
            .cloned();
        stored.map(|payload| decode_entry(&payload)).transpose()
    }

    fn delete(&self, reference: &str) -> Result<(), CredentialVaultError> {
        self.ensure_available()?;
        self.entries
            .lock()
            .expect("credential vault mutex is not poisoned")
            .remove(reference);
        Ok(())
    }
}

/// Environment variable names for local development credentials.
pub const DEVELOPER_ID_ENVIRONMENT_KEY: &str = "RETROFRONTIER_SCREENSCRAPER_DEV_ID";
pub const DEVELOPER_PASSWORD_ENVIRONMENT_KEY: &str = "RETROFRONTIER_SCREENSCRAPER_DEV_PASSWORD";

/// Where an application credential was found. Useful in diagnostics; carries no value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeveloperCredentialOrigin {
    /// Injected at compile time from a protected release secret.
    BuildTimeInjection,
    /// Read from the process environment, normally sourced from an ignored local `.env`.
    ProcessEnvironment,
}

/// Resolves the application developer credential for release and development builds.
///
/// Release builds receive the values through `option_env!`, which requires the build to be run
/// with the protected secret exported; nothing is committed as source or generated source.
/// Development builds fall back to the process environment.
pub fn developer_credentials_from_environment(
) -> Option<(DeveloperCredentials, DeveloperCredentialOrigin)> {
    if let (Some(id), Some(password)) = (
        option_env!("RETROFRONTIER_SCREENSCRAPER_DEV_ID"),
        option_env!("RETROFRONTIER_SCREENSCRAPER_DEV_PASSWORD"),
    ) {
        if !id.is_empty() && !password.is_empty() {
            return Some((
                DeveloperCredentials {
                    developer_id: SecretString::new(id),
                    developer_password: SecretString::new(password),
                },
                DeveloperCredentialOrigin::BuildTimeInjection,
            ));
        }
    }

    let id = std::env::var(DEVELOPER_ID_ENVIRONMENT_KEY).ok()?;
    let password = std::env::var(DEVELOPER_PASSWORD_ENVIRONMENT_KEY).ok()?;
    if id.is_empty() || password.is_empty() {
        return None;
    }
    Some((
        DeveloperCredentials {
            developer_id: SecretString::new(id),
            developer_password: SecretString::new(password),
        },
        DeveloperCredentialOrigin::ProcessEnvironment,
    ))
}

/// Loads `KEY=VALUE` pairs from an ignored local `.env` into the process environment.
///
/// Debug-only: release builds must take their credential from build-time injection, never from a
/// file next to the executable. Existing environment variables always win, and the file contents
/// are never logged.
#[cfg(debug_assertions)]
pub fn load_development_environment_file(path: &std::path::Path) {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return;
    };
    for (key, value) in parse_environment_file(&contents) {
        if std::env::var_os(&key).is_none() {
            // Safety: called once during single-threaded application setup, before any worker or
            // provider task exists.
            unsafe { std::env::set_var(&key, &value) };
        }
    }
}

#[cfg(debug_assertions)]
fn parse_environment_file(contents: &str) -> Vec<(String, String)> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim().strip_prefix("export ").unwrap_or(key.trim());
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .or_else(|| {
                    value
                        .strip_prefix('\'')
                        .and_then(|value| value.strip_suffix('\''))
                })
                .unwrap_or(value);
            (!key.is_empty()).then(|| (key.trim().to_owned(), value.to_owned()))
        })
        .collect()
}

/// Supplies credentials to a provider adapter.
///
/// Split out so the adapter never reads the environment or the vault itself, and so tests can
/// inject fake values without touching an OS keychain.
pub trait ProviderCredentialSource: Send + Sync {
    /// RetroFrontier's application credentials, or `None` when this build has none configured.
    fn developer(&self) -> Option<DeveloperCredentials>;

    /// The user's optional personal account, or `None` for guest access.
    fn user(&self) -> Option<UserCredentials>;
}

/// Fixed credential source for tests and for a build whose values never change at runtime.
pub struct StaticCredentialSource {
    developer: Option<DeveloperCredentials>,
    user: Option<UserCredentials>,
}

impl StaticCredentialSource {
    pub fn new(developer: Option<DeveloperCredentials>, user: Option<UserCredentials>) -> Self {
        Self { developer, user }
    }

    /// Developer credentials only, i.e. guest provider access.
    pub fn developer_only(developer_id: &str, developer_password: &str) -> Self {
        Self::new(
            Some(DeveloperCredentials {
                developer_id: SecretString::new(developer_id),
                developer_password: SecretString::new(developer_password),
            }),
            None,
        )
    }

    pub fn with_user(mut self, username: &str, password: &str) -> Self {
        self.user = Some(UserCredentials {
            username: username.to_owned(),
            password: SecretString::new(password),
        });
        self
    }

    pub fn without_developer() -> Self {
        Self::new(None, None)
    }
}

impl ProviderCredentialSource for StaticCredentialSource {
    fn developer(&self) -> Option<DeveloperCredentials> {
        self.developer.clone()
    }

    fn user(&self) -> Option<UserCredentials> {
        self.user.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> UserCredentials {
        UserCredentials {
            username: "example-account".to_owned(),
            password: SecretString::new("fake-user-password"),
        }
    }

    #[test]
    fn secrets_are_redacted_in_debug_and_display_output() {
        let secret = SecretString::new("fake-developer-password");

        assert_eq!(format!("{secret:?}"), REDACTED);
        assert_eq!(format!("{secret}"), REDACTED);
        assert!(!format!("{secret:?}").contains("fake-developer-password"));

        let developer = DeveloperCredentials {
            developer_id: SecretString::new("fake-developer-id"),
            developer_password: SecretString::new("fake-developer-password"),
        };
        let rendered = format!("{developer:?}");
        assert!(!rendered.contains("fake-developer-id"));
        assert!(!rendered.contains("fake-developer-password"));

        let rendered_user = format!("{:?}", credentials());
        assert!(!rendered_user.contains("fake-user-password"));
        assert!(!rendered_user.contains("example-account"));
    }

    #[test]
    fn in_memory_vault_stores_loads_and_deletes() {
        let vault = InMemoryCredentialVault::new();

        assert_eq!(vault.load("screenscraper").unwrap(), None);
        vault.store("screenscraper", &credentials()).unwrap();

        let loaded = vault.load("screenscraper").unwrap().expect("stored entry");
        assert_eq!(loaded.username, "example-account");
        assert_eq!(loaded.password.expose(), "fake-user-password");

        vault.delete("screenscraper").unwrap();
        assert_eq!(vault.load("screenscraper").unwrap(), None);
        vault.delete("screenscraper").expect("delete is idempotent");
    }

    #[test]
    fn an_unavailable_vault_reports_a_value_free_error() {
        let vault = InMemoryCredentialVault::new();
        vault.store("screenscraper", &credentials()).unwrap();
        vault.set_available(false);

        assert_eq!(
            vault.load("screenscraper"),
            Err(CredentialVaultError::Unavailable)
        );
        assert_eq!(
            vault.store("screenscraper", &credentials()),
            Err(CredentialVaultError::Unavailable)
        );
        let rendered = format!("{:?}", CredentialVaultError::Unavailable);
        assert!(!rendered.contains("fake-user-password"));
    }

    #[test]
    fn a_malformed_vault_payload_is_rejected_without_echoing_it() {
        let vault = InMemoryCredentialVault::new();
        vault.insert_raw("screenscraper", "no-separator-fake-secret");

        let error = vault
            .load("screenscraper")
            .expect_err("a payload without a separator is malformed");
        assert_eq!(error, CredentialVaultError::Malformed);
        assert!(!format!("{error}").contains("fake-secret"));
    }

    #[test]
    fn passwords_containing_separators_round_trip() {
        let vault = InMemoryCredentialVault::new();
        vault
            .store(
                "screenscraper",
                &UserCredentials {
                    username: "account".to_owned(),
                    password: SecretString::new("fake\nmulti\nline"),
                },
            )
            .unwrap();

        let loaded = vault.load("screenscraper").unwrap().expect("stored entry");
        assert_eq!(loaded.username, "account");
        assert_eq!(loaded.password.expose(), "fake\nmulti\nline");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn environment_file_parsing_ignores_comments_and_strips_quotes() {
        let parsed = parse_environment_file(
            "# comment\n\nRETROFRONTIER_SCREENSCRAPER_DEV_ID=fake-id\n\
             export RETROFRONTIER_SCREENSCRAPER_DEV_PASSWORD=\"fake-password\"\n\
             MALFORMED\n",
        );

        assert_eq!(
            parsed,
            vec![
                (
                    "RETROFRONTIER_SCREENSCRAPER_DEV_ID".to_owned(),
                    "fake-id".to_owned()
                ),
                (
                    "RETROFRONTIER_SCREENSCRAPER_DEV_PASSWORD".to_owned(),
                    "fake-password".to_owned()
                ),
            ]
        );
    }
}
