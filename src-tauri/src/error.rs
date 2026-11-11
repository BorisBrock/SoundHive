use serde::Serialize;

/// Errors returned to the frontend. Serialized as `{ kind, message? }`.
#[derive(Debug, thiserror::Error, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppError {
    #[error("database error: {message}")]
    Database { message: String },
    #[error("not found")]
    NotFound,
    #[error("{message}")]
    Invalid { message: String },
    #[error("{message}")]
    Io { message: String },
    #[error("{message}")]
    Audio { message: String },
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::NotFound,
            e => AppError::Database { message: e.to_string() },
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Database { message: e.to_string() }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io { message: e.to_string() }
    }
}

pub type AppResult<T> = Result<T, AppError>;

pub fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid { message: message.into() }
}
