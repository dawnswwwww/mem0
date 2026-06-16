use std::fmt;
use std::str::FromStr;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::core::error::MemError;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Lifecycle {
    Working,
    Episodic,
    Semantic,
}

impl Lifecycle {
    pub const ALL: [Lifecycle; 3] = [Lifecycle::Working, Lifecycle::Episodic, Lifecycle::Semantic];
}

impl fmt::Display for Lifecycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Lifecycle::Working  => "working",
            Lifecycle::Episodic => "episodic",
            Lifecycle::Semantic => "semantic",
        };
        f.write_str(s)
    }
}

impl FromStr for Lifecycle {
    type Err = MemError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "working"  => Ok(Lifecycle::Working),
            "episodic" => Ok(Lifecycle::Episodic),
            "semantic" => Ok(Lifecycle::Semantic),
            other => Err(MemError::InvalidArgument(format!("unknown lifecycle: {other}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known_values() {
        assert_eq!("working".parse::<Lifecycle>().unwrap(),  Lifecycle::Working);
        assert_eq!("episodic".parse::<Lifecycle>().unwrap(), Lifecycle::Episodic);
        assert_eq!("semantic".parse::<Lifecycle>().unwrap(), Lifecycle::Semantic);
    }

    #[test]
    fn parse_unknown_returns_err() {
        assert!("bogus".parse::<Lifecycle>().is_err());
    }

    #[test]
    fn parse_is_case_sensitive() {
        assert!("Working".parse::<Lifecycle>().is_err());
    }

    #[test]
    fn display_roundtrips_with_parse() {
        for l in [Lifecycle::Working, Lifecycle::Episodic, Lifecycle::Semantic] {
            assert_eq!(l.to_string().parse::<Lifecycle>().unwrap(), l);
        }
    }
}