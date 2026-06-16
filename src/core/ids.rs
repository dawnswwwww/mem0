use uuid::Uuid;

use crate::core::error::{MemError, MemResult};

pub fn new_v7() -> Uuid {
    Uuid::now_v7()
}

/// Parses a *full* UUID string. Prefix-based lookup is a store-level
/// concern (it needs the DB to disambiguate).
pub fn parse(s: &str) -> MemResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| MemError::InvalidId(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_v7_is_uuid_v7() {
        let u = new_v7();
        assert_eq!(u.get_version_num(), 7);
    }

    #[test]
    fn parse_full_uuid() {
        let u = new_v7();
        let s = u.to_string();
        let parsed = parse(&s).unwrap();
        assert_eq!(parsed, u);
    }

    #[test]
    fn parse_rejects_non_uuid() {
        assert!(parse("not-a-uuid").is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn parse_rejects_short_input() {
        // anything shorter than full UUID (36 chars w/ dashes, 32 without) is invalid as a full id
        assert!(parse("abcdef12").is_err());
    }
}
