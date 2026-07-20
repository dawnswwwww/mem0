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

use mem0::embed::{Role, apply_prefix};

#[test]
fn prefix_is_asymmetric() {
    assert_eq!(apply_prefix("hello", Role::Passage), "passage: hello");
    assert_eq!(apply_prefix("hello", Role::Query),   "query: hello");
}

#[test]
fn prefix_trims_only_leading_whitespace_of_input_not_added() {
    // input is taken verbatim after the prefix; prefix is exactly "passage: " / "query: "
    assert_eq!(apply_prefix("  spaced", Role::Query), "query:   spaced");
}

use mem0::embed::store::{resolve, SearchRoots, hf_cache_subdir};

#[test]
fn hf_cache_subdir_transform() {
    // Default model repo is intfloat/multilingual-e5-small (spike-verified).
    assert_eq!(hf_cache_subdir("intfloat/multilingual-e5-small"),
               "models--intfloat--multilingual-e5-small");
}

#[test]
fn resolve_returns_first_root_with_the_model_subdir() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let subdir = hf_cache_subdir(ModelChoice::DEFAULT.repo());
    // 'b' contains the model's cache subdir; 'a' does not.
    std::fs::create_dir_all(b.path().join(&subdir)).unwrap();
    let roots = SearchRoots { roots: vec![a.path().to_path_buf(), b.path().to_path_buf()] };
    // resolve returns the ROOT (passed to with_cache_dir), not the model subdir.
    assert_eq!(resolve(ModelChoice::DEFAULT, &roots), Some(b.path().to_path_buf()));
}

#[test]
fn resolve_none_when_no_root_has_the_subdir() {
    let a = tempfile::tempdir().unwrap();
    let roots = SearchRoots { roots: vec![a.path().to_path_buf()] };
    assert_eq!(resolve(ModelChoice::DEFAULT, &roots), None);
}
