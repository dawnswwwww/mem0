use thiserror::Error;

use crate::core::memory::Lifecycle;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum MemError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid id: {0}")]
    InvalidId(String),

    #[error("invalid lifecycle transition: {from} -> {to}")]
    InvalidTransition { from: Lifecycle, to: Lifecycle },

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("storage error: {0}")]
    Storage(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("embedding dimension mismatch: expected {expected}, got {got}")]
    EmbeddingDimMismatch { expected: usize, got: usize },

    #[error("embedding parse error: {0}")]
    EmbeddingParseError(String),

    #[error("vector index not initialized: add a memory with a vector first")]
    VectorNotInitialized,
}

pub type MemResult<T> = std::result::Result<T, MemError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_displays_message() {
        let e = MemError::NotFound("abc12345".into());
        assert_eq!(e.to_string(), "not found: abc12345");
    }

    #[test]
    fn invalid_transition_carries_from_and_to() {
        let e = MemError::InvalidTransition {
            from: Lifecycle::Semantic,
            to: Lifecycle::Working,
        };
        let s = e.to_string();
        assert!(s.contains("semantic"), "msg: {s}");
        assert!(s.contains("working"),  "msg: {s}");
    }

    #[test]
    fn from_rusqlite_error() {
        let e: MemError = rusqlite::Error::InvalidQuery.into();
        assert!(matches!(e, MemError::Storage(_)));
    }

    #[test]
    fn from_io_error() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "x");
        let e: MemError = io.into();
        assert!(matches!(e, MemError::Io(_)));
    }

    #[test]
    fn from_json_error() {
        let je = serde_json::from_str::<i32>("not json").unwrap_err();
        let e: MemError = je.into();
        assert!(matches!(e, MemError::Json(_)));
    }
}
