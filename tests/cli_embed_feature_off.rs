// Only meaningful on the default (no-`embed`) build: under `--features embed`
// `mem0 embed` actually runs the embedder, so the exit-2 / "not compiled in"
// assertions below would hang (firewalled HF) or fail. Gate the whole file.
#![cfg(not(feature = "embed"))]

// Compiles WITHOUT --features embed; the subcommand exists but errors.
//
// assert_cmd's `unwrap_err()` returns an `OutputError`; the underlying exit code
// is reached via `.as_output().status.code()`. The repo's other CLI tests use
// `.output().unwrap()` + `status.code()`, so we mirror that form here.
//
// Note: clap itself rejects unknown subcommands with exit code 2, so an exit-2
// assertion alone cannot distinguish "clap rejected `embed`" from "our code
// returned `EmbedFeatureNotEnabled`". We strengthen the guard with two
// additional assertions:
//   1. `mem0 --help` lists `embed` (proves the subcommand is wired in).
//   2. The feature-off run prints the `EmbedFeatureNotEnabled` message, which
//      only our code path emits (clap's unknown-subcommand text is different).
use assert_cmd::Command;

#[test]
fn embed_subcommand_is_listed_in_help() {
    let out = Command::cargo_bin("mem0")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(
        help.contains("embed"),
        "`embed` subcommand missing from --help output:\n{help}"
    );
}

#[test]
fn embed_without_feature_exits_2_with_feature_message() {
    let out = Command::cargo_bin("mem0")
        .unwrap()
        .args(["embed", "hello"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2 (EmbedFeatureNotEnabled); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Distinguish our error path from clap's "unrecognized subcommand" (also
    // exit 2). Our message is "embedding support is not compiled in ...".
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not compiled in"),
        "expected EmbedFeatureNotEnabled message on stderr, got: {stderr}"
    );
}
