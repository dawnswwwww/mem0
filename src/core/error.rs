use thiserror::Error;

use crate::core::memory::Lifecycle;

#[derive(Error, Debug)]
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
        let r: rusqlite::Result<()> = Err(rusqlite::Error::InvalidQuery);
        let e: MemError = r.unwrap_err().into();
        assert!(matches!(e, MemError::Storage(_)));
    }
}
