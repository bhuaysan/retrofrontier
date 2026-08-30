use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::domain::runtime::RuntimeError;
use crate::services::bios::BiosError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("application data path could not be resolved")]
    PathResolution(String),

    #[error("application data directory could not be prepared")]
    Storage(#[source] std::io::Error),

    #[error("local database is unavailable")]
    Database(#[source] sqlx::Error),

    #[error("local database migrations could not be applied")]
    Migration(#[source] sqlx::migrate::MigrateError),

    #[error("managed runtime is unavailable")]
    Runtime(#[source] RuntimeError),

    #[error("the system catalog is invalid: {0}")]
    Catalog(String),

    #[error("BIOS discovery is unavailable")]
    Bios(#[source] BiosError),

    #[error("BIOS path overrides are available only in development builds")]
    BiosOverrideNotAllowed,

    #[error("local game library is unavailable: {0}")]
    Library(String),

    #[error("the content-root path is invalid or unsafe")]
    ContentRootInvalidPath,

    #[error("the content root is unavailable")]
    ContentRootUnavailable,

    #[error("the content-root path is not a directory")]
    ContentRootNotDirectory,

    #[error("the content root overlaps another enabled root")]
    ContentRootOverlap,

    #[error("the content-root operation is invalid")]
    ContentRootInvalidOperation,

    #[error("game metadata is unavailable: {0}")]
    Metadata(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PathResolution(_) => "path_unavailable",
            Self::Storage(_) => "storage_unavailable",
            Self::Database(_) => "database_unavailable",
            Self::Migration(_) => "migration_failed",
            Self::Runtime(_) => "runtime_unavailable",
            Self::Catalog(_) => "catalog_invalid",
            Self::Bios(_) => "bios_unavailable",
            Self::BiosOverrideNotAllowed => "bios_override_disabled",
            Self::Library(_) => "library_unavailable",
            Self::ContentRootInvalidPath => "content_root_invalid_path",
            Self::ContentRootUnavailable => "content_root_unavailable",
            Self::ContentRootNotDirectory => "content_root_not_directory",
            Self::ContentRootOverlap => "content_root_overlap",
            Self::ContentRootInvalidOperation => "content_root_invalid_operation",
            Self::Metadata(_) => "metadata_unavailable",
        }
    }

    pub fn user_message(&self) -> &'static str {
        match self {
            Self::PathResolution(_) => {
                "RetroFrontier could not locate its application data directory."
            }
            Self::Storage(_) => "RetroFrontier could not prepare its application data directory.",
            Self::Database(_) => "RetroFrontier could not access its local database.",
            Self::Migration(_) => "RetroFrontier could not prepare its local storage.",
            Self::Runtime(_) => "RetroFrontier could not prepare its managed runtime.",
            Self::Catalog(_) => "RetroFrontier could not load its supported-system catalog.",
            Self::Bios(_) => "RetroFrontier could not inspect the BIOS folder.",
            Self::BiosOverrideNotAllowed => {
                "BIOS path overrides are available only in development builds."
            }
            Self::Library(_) => "RetroFrontier could not access the local game library.",
            Self::ContentRootInvalidPath => {
                "That content-root path is invalid or uses an unsafe path form."
            }
            Self::ContentRootUnavailable => "That content root is currently unavailable.",
            Self::ContentRootNotDirectory => "That content-root path is not a directory.",
            Self::ContentRootOverlap => "That content root overlaps another enabled content root.",
            Self::ContentRootInvalidOperation => {
                "That content-root operation is not valid for the selected root."
            }
            Self::Metadata(_) => "RetroFrontier could not access local game metadata.",
        }
    }

    pub fn log(&self) {
        if let Some(source) = std::error::Error::source(self) {
            tracing::error!(code = self.code(), error = %self, cause = %source, "application error");
        } else {
            tracing::error!(code = self.code(), error = %self, "application error");
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut error = serializer.serialize_struct("IpcError", 2)?;
        error.serialize_field("code", self.code())?;
        error.serialize_field("message", self.user_message())?;
        error.end()
    }
}

#[cfg(test)]
mod tests {
    use super::AppError;
    use crate::domain::runtime::RuntimeError;
    use crate::services::bios::BiosError;
    use sqlx::migrate::MigrateError;
    use std::path::PathBuf;

    #[test]
    fn serializes_a_safe_ui_error_without_internal_details() {
        let serialized = serde_json::to_value(AppError::Database(sqlx::Error::RowNotFound))
            .expect("application errors should serialize");

        assert_eq!(serialized["code"], "database_unavailable");
        assert_eq!(
            serialized["message"],
            "RetroFrontier could not access its local database."
        );
        assert!(!serialized.to_string().contains("RowNotFound"));
    }

    #[test]
    fn content_root_errors_use_stable_safe_codes_and_messages() {
        for (error, code) in [
            (
                AppError::ContentRootInvalidPath,
                "content_root_invalid_path",
            ),
            (AppError::ContentRootUnavailable, "content_root_unavailable"),
            (
                AppError::ContentRootNotDirectory,
                "content_root_not_directory",
            ),
            (AppError::ContentRootOverlap, "content_root_overlap"),
            (
                AppError::ContentRootInvalidOperation,
                "content_root_invalid_operation",
            ),
        ] {
            let serialized = serde_json::to_value(error).expect("root errors should serialize");
            assert_eq!(serialized["code"], code);
            assert!(serialized["message"]
                .as_str()
                .is_some_and(|message| { !message.contains("sql") && !message.contains("/tmp") }));
        }
    }

    #[test]
    fn pins_the_complete_backend_ipc_error_code_set() {
        let errors = [
            AppError::PathResolution(String::new()),
            AppError::Storage(std::io::Error::other("fixture")),
            AppError::Database(sqlx::Error::RowNotFound),
            AppError::Migration(MigrateError::VersionMissing(1)),
            AppError::Runtime(RuntimeError::UnsupportedPlatform),
            AppError::Catalog(String::new()),
            AppError::Bios(BiosError::UnsafeRoot {
                path: PathBuf::from("/fixture"),
            }),
            AppError::BiosOverrideNotAllowed,
            AppError::Library(String::new()),
            AppError::ContentRootInvalidPath,
            AppError::ContentRootUnavailable,
            AppError::ContentRootNotDirectory,
            AppError::ContentRootOverlap,
            AppError::ContentRootInvalidOperation,
            AppError::Metadata(String::new()),
        ];
        let codes: Vec<_> = errors.iter().map(AppError::code).collect();

        assert_eq!(
            codes,
            vec![
                "path_unavailable",
                "storage_unavailable",
                "database_unavailable",
                "migration_failed",
                "runtime_unavailable",
                "catalog_invalid",
                "bios_unavailable",
                "bios_override_disabled",
                "library_unavailable",
                "content_root_invalid_path",
                "content_root_unavailable",
                "content_root_not_directory",
                "content_root_overlap",
                "content_root_invalid_operation",
                "metadata_unavailable",
            ]
        );
    }
}
