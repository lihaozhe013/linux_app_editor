//! Error type shared by all Tauri commands. Serialized as
//! `{ code, message }` so the frontend can display it without parsing.

use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(
        "cannot determine the applications directory: XDG_DATA_HOME is unset or relative and HOME is unset"
    )]
    HomeNotFound,
    #[error("invalid desktop filename: {0}")]
    InvalidFilename(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("file already exists: {0}")]
    AlreadyExists(PathBuf),
}

impl AppError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        AppError::Io {
            path: path.into(),
            source,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            AppError::Io { source, .. } => match source.kind() {
                std::io::ErrorKind::PermissionDenied => "permission-denied",
                std::io::ErrorKind::NotFound => "not-found",
                _ => "io-error",
            },
            AppError::HomeNotFound => "home-not-found",
            AppError::InvalidFilename(_) => "invalid-filename",
            AppError::Validation(_) => "validation-failed",
            AppError::AlreadyExists(_) => "already-exists",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
