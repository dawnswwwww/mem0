use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn db_arg(dir: &TempDir) -> String {
    dir.path().join("mem0.db").to_string_lossy().to_string()
}

#[test]
fn vsearch_returns_ranked_hits_with_distance() {
    let dir = TempDir::new().unwrap();
    let db = db_arg(&dir);

    for (content, vec) in [
        ("close one", r#"{"embedding":[1.0,0.0,0.0,0.0]}"#),
        ("far one",   r#"{"embedding":[0.0,0.0,0.0,1.0]}"#),
    ] {
        Command::cargo_bin("mem0").unwrap()
            .args(["--db", &db, "add", content, "--to", "semantic"])
            .write_stdin(vec.as_bytes())
            .assert().success();
    }

    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "--json", "vsearch"])
        .write_stdin(r#"{"embedding":[1.0,0.0,0.0,0.0]}"#.as_bytes())
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"], 2);
    let first = &v["items"][0];
    assert!(first["content"].as_str().unwrap().contains("close one"));
    assert!(first["distance"].as_f64().unwrap() < 0.001, "nearest should be ~0 distance");
}

#[test]
fn vsearch_without_vectors_exits_3() {
    let dir = TempDir::new().unwrap();
    let db = db_arg(&dir);
    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "vsearch"])
        .write_stdin(r#"{"embedding":[1.0,2.0,3.0,4.0]}"#.as_bytes())
        .output().unwrap();
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn vsearch_dim_mismatch_exits_2() {
    let dir = TempDir::new().unwrap();
    let db = db_arg(&dir);
    Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "add", "x", "--to", "semantic"])
        .write_stdin(r#"{"embedding":[1.0,2.0,3.0,4.0]}"#.as_bytes())
        .assert().success();
    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "vsearch"])
        .write_stdin(r#"{"embedding":[1.0,2.0,3.0]}"#.as_bytes())
        .output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}
