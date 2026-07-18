#![cfg(feature = "embed")]

use mem0::embed::model::ModelChoice;

#[test]
fn default_is_multilingual_e5_small_384() {
    let m = ModelChoice::DEFAULT;
    assert_eq!(m.name(), "multilingual-e5-small");
    assert_eq!(m.dim(), 384);
}

#[test]
fn name_roundtrip_for_known_models() {
    for name in [
        "multilingual-e5-small",
        "all-MiniLM-L6-v2",
        "bge-small-en-v1.5",
        "bge-small-zh-v1.5",
        "nomic-embed-text-v1.5",
    ] {
        let m = ModelChoice::from_name(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(m.name(), name);
    }
}

#[test]
fn unknown_model_errors() {
    assert!(ModelChoice::from_name("gpt-4").is_err());
}
