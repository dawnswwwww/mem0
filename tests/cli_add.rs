use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn bin() -> Command { Command::cargo_bin("mem0").unwrap() }

#[test]
fn add_writes_to_specified_layer_and_returns_id() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");

    let out = bin()
        .args(["--db", db.to_str().unwrap(), "--json", "add",
               "user likes whiskey", "--to", "semantic",
               "--tag", "preference"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["lifecycle"], "semantic");
    assert!(v["id"].as_str().unwrap().len() >= 8);
    assert_eq!(v["tags"], serde_json::json!(["preference"]));
}

#[test]
fn add_to_episodic_resolves_session_name_to_id() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");

    // Seed a session via the library (the `session new` CLI is Task 24).
    {
        let conn = mem0::store::db::open(&db).unwrap();
        mem0::store::db::migrate(&conn).unwrap();
        mem0::store::sessions::new(&conn, "s1").unwrap();
    }

    let out = bin()
        .args(["--db", db.to_str().unwrap(), "--json", "add",
               "Q3 营收 120w", "--to", "episodic", "--session", "s1"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["lifecycle"], "episodic");
    assert!(v["session_id"].as_str().is_some());
}

#[test]
fn add_missing_to_flag_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    bin().args(["--db", db.to_str().unwrap(), "add", "x"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn add_with_vector_then_vsearch_recalls_it() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mem0.db").to_string_lossy().to_string();

    Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "add", "user likes whiskey", "--to", "semantic"])
        .write_stdin(r#"{"embedding":[1.0,0.0,0.0,0.0]}"#)
        .assert().success();

    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "--json", "vsearch"])
        .write_stdin(r#"{"embedding":[0.9,0.1,0.0,0.0]}"#)
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"], 1);
    assert!(v["items"][0]["content"].as_str().unwrap().contains("whiskey"));
}

#[test]
fn add_without_stdin_is_unchanged() {
    // No piped stdin ⇒ text-only add, exactly as before.
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mem0.db").to_string_lossy().to_string();
    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "add", "plain text memory", "--to", "working"])
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // No vector indexed ⇒ vsearch reports not initialized.
    let vout = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "vsearch"])
        .write_stdin(r#"{"embedding":[1.0,2.0,3.0,4.0]}"#)
        .output().unwrap();
    assert_eq!(vout.status.code(), Some(3));
}

#[test]
fn add_with_mismatched_vector_rolls_back_memory() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mem0.db").to_string_lossy().to_string();
    // seed dimension = 4
    Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "add", "seed", "--to", "semantic"])
        .write_stdin(r#"{"embedding":[1.0,2.0,3.0,4.0]}"#)
        .assert().success();
    // add with a wrong-dimension vector → upsert fails → memory must be rolled back
    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "add", "should-not-persist", "--to", "semantic"])
        .write_stdin(r#"{"embedding":[1.0,2.0,3.0]}"#)
        .output().unwrap();
    assert_eq!(out.status.code(), Some(2), "dim mismatch must exit 2");
    // the rolled-back memory must NOT appear in list
    let list = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "list", "--layer=semantic"])
        .output().unwrap();
    let list_txt = String::from_utf8_lossy(&list.stdout);
    assert!(!list_txt.contains("should-not-persist"), "rolled-back memory leaked into list: {list_txt}");
}
