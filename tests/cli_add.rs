use assert_cmd::Command;

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
