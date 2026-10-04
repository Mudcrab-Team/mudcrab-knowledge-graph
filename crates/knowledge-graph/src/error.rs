//! Stable error codes for CLI and query callers.
use serde::Serialize;
#[derive(Debug, thiserror::Error, Serialize)]
#[error("{code}: {message}")]
pub struct Error {
    pub code: String,
    pub message: String,
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    /// Construct a stable, path-free diagnostic.
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    /// Map error classes to the documented process status.
    pub fn exit_code(&self) -> i32 {
        match self.code.as_str() {
            "MISSING_INPUT" | "UNAVAILABLE" => 3,
            "POLICY" => 4,
            "EXTRACTION_FAILED" => 5,
            "INTEGRITY" | "IDENTITY_COLLISION" | "STORE" => 6,
            "BUDGET_TOO_SMALL" | "TIMEOUT" => 7,
            _ => 2,
        }
    }
}
impl From<rusqlite::Error> for Error {
    fn from(_: rusqlite::Error) -> Self {
        Self::new("STORE", "database operation failed")
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::new(
            "INVALID_INPUT",
            format!("invalid JSON at line {}, column {}", e.line(), e.column()),
        )
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::new(
            "MISSING_INPUT",
            format!("file operation failed: {:?}", e.kind()),
        )
    }
}
