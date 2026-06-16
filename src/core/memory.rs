// stub — full Lifecycle comes in Task 3
//
// Debug + Display are derived here only because `MemError` requires them for its
// `#[derive(Error, Debug)]` and the `{from} -> {to}` interpolation in
// `InvalidTransition`. Task 3 replaces this whole file with the full Lifecycle
// (Display, FromStr, ValueEnum, Serialize/Deserialize, etc.).
#[derive(Debug)]
pub enum Lifecycle {
    Working,
    Episodic,
    Semantic,
}

impl std::fmt::Display for Lifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Lifecycle::Working => "working",
            Lifecycle::Episodic => "episodic",
            Lifecycle::Semantic => "semantic",
        };
        f.write_str(s)
    }
}
