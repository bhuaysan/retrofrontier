use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::domain::runtime::RuntimeError;

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
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PathResolution(_) => "path_unavailable",
            Self::Storage(_) => "storage_unavailable",
            Self::Database(_) => "database_unavailable",
            Self::Migration(_) => "migration_failed",
            Self::Runtime(_) => "runtime_unavailable",
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
}
