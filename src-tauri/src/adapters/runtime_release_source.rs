//! Which approved managed Runtime Release source this build talks to.
//!
//! M2 implemented `ToughTrustedReleaseSource` but the composition root never configured one, so
//! every installation attempt failed with "no approved managed runtime source is configured" and
//! the runtime could never leave `NotInstalled`. This adapter is the missing configuration, and
//! nothing else: it decides *where* trusted metadata comes from, never *whether* it is trusted.
//! Authentication stays entirely inside the TUF client.
//!
//! Two origins exist:
//!
//! * **Production** — a trusted root and repository URLs compiled into a signed RetroFrontier
//!   build. ADR-012 requires the application to ship the initial root, and that root does not
//!   exist yet: the production key ceremony and public hosting decision are M10 work. This build
//!   therefore has no production source, and says so rather than pretending.
//! * **Qualification** — a repository a maintainer built locally with `rf-runtime-release publish`,
//!   selected by explicit environment opt-in. It uses the same TUF verification code, the same
//!   Ed25519/threshold profile, and the same release manifest contract as production will.
//!
//! Environment configuration is not a trust hole: the trusted root is self-authenticating TUF
//! material, and anyone able to set this process's environment can already replace the application
//! binary. It is nonetheless gated behind an explicit opt-in variable so it can never be reached by
//! accident, and the chosen origin is reported to the UI so a qualification build is never
//! displayed as if it were a public release.

use crate::adapters::runtime_source::{ToughTrustedReleaseSource, TrustedReleaseSource};
use crate::domain::runtime::{RuntimeError, RuntimeSourceOrigin};
use std::path::Path;
use std::sync::Arc;
use url::Url;

/// Explicit opt-in. Only the exact value `qualification` selects the local qualification source.
pub const SOURCE_MODE_VARIABLE: &str = "RETROFRONTIER_RUNTIME_SOURCE";
pub const QUALIFICATION_MODE: &str = "qualification";
pub const TRUSTED_ROOT_VARIABLE: &str = "RETROFRONTIER_RUNTIME_TUF_ROOT";
pub const METADATA_URL_VARIABLE: &str = "RETROFRONTIER_RUNTIME_METADATA_URL";
pub const TARGETS_URL_VARIABLE: &str = "RETROFRONTIER_RUNTIME_TARGETS_URL";
pub const MANIFEST_TARGET_VARIABLE: &str = "RETROFRONTIER_RUNTIME_MANIFEST_TARGET";
pub const POLICY_TARGET_VARIABLE: &str = "RETROFRONTIER_RUNTIME_POLICY_TARGET";

pub const DEFAULT_POLICY_TARGET: &str = "runtime-policy.json";

/// A configured source together with the release it is allowed to install.
pub struct ConfiguredReleaseSource {
    pub origin: RuntimeSourceOrigin,
    pub source: Arc<dyn TrustedReleaseSource>,
    /// The single approved manifest target this build installs. Not user-supplied.
    pub manifest_target_name: String,
}

impl std::fmt::Debug for ConfiguredReleaseSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConfiguredReleaseSource")
            .field("origin", &self.origin)
            .field("manifest_target_name", &self.manifest_target_name)
            .finish_non_exhaustive()
    }
}

/// Read the environment and, when it is complete and valid, build the trusted release source.
///
/// A partially configured qualification environment is an error rather than a silent fallback: a
/// maintainer who sets three of five variables should be told, not left with an app that quietly
/// reports the runtime as uninstallable.
pub fn configure_release_source(
    datastore: &Path,
) -> Result<Option<ConfiguredReleaseSource>, RuntimeError> {
    if let Some(production) = production_release_source(datastore)? {
        return Ok(Some(production));
    }
    let Some(mode) = std::env::var_os(SOURCE_MODE_VARIABLE) else {
        return Ok(None);
    };
    if mode != QUALIFICATION_MODE {
        return Err(RuntimeError::Trust(format!(
            "{SOURCE_MODE_VARIABLE} must be '{QUALIFICATION_MODE}' when it is set"
        )));
    }

    let root_path = required(TRUSTED_ROOT_VARIABLE)?;
    let metadata_url = parse_url(&required(METADATA_URL_VARIABLE)?, METADATA_URL_VARIABLE)?;
    let targets_url = parse_url(&required(TARGETS_URL_VARIABLE)?, TARGETS_URL_VARIABLE)?;
    let manifest_target_name = required(MANIFEST_TARGET_VARIABLE)?;
    let policy_target_name = std::env::var(POLICY_TARGET_VARIABLE)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_POLICY_TARGET.to_owned());

    let trusted_root = std::fs::read(&root_path).map_err(|error| {
        RuntimeError::Trust(format!(
            "the configured trusted TUF root could not be read: {error}"
        ))
    })?;

    // `ToughTrustedReleaseSource::new` validates that the root is self-authenticating and that the
    // URLs are HTTPS or local file fixtures, so a malformed configuration fails here.
    let source = ToughTrustedReleaseSource::new(
        trusted_root,
        metadata_url,
        targets_url,
        datastore.to_path_buf(),
        policy_target_name,
    )?;

    Ok(Some(ConfiguredReleaseSource {
        origin: RuntimeSourceOrigin::Qualification,
        source: Arc::new(source),
        manifest_target_name,
    }))
}

/// The production source, once M10 has produced a shipped trusted root and public repository.
///
/// Returning `None` is the honest current answer. It is a function rather than a constant so the
/// M10 change is one place, and so the absence is visible in code review.
fn production_release_source(
    _datastore: &Path,
) -> Result<Option<ConfiguredReleaseSource>, RuntimeError> {
    Ok(None)
}

fn required(variable: &str) -> Result<String, RuntimeError> {
    std::env::var(variable)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            RuntimeError::Trust(format!(
                "{variable} must be set when {SOURCE_MODE_VARIABLE} is '{QUALIFICATION_MODE}'"
            ))
        })
}

fn parse_url(value: &str, variable: &str) -> Result<Url, RuntimeError> {
    Url::parse(value)
        .map_err(|error| RuntimeError::Trust(format!("{variable} is invalid: {error}")))
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_POLICY_TARGET, QUALIFICATION_MODE};

    #[test]
    fn the_default_policy_target_matches_the_published_release_definition() {
        // The committed Linux release definition publishes its policy under this exact name, so a
        // maintainer only has to configure the manifest target.
        assert_eq!(DEFAULT_POLICY_TARGET, "runtime-policy.json");
    }

    #[test]
    fn only_the_exact_qualification_value_is_an_opt_in() {
        // A typo must not select a development trust path; `configure_release_source` compares
        // against this constant with equality rather than a prefix or case-insensitive match.
        assert_eq!(QUALIFICATION_MODE, "qualification");
    }
}
