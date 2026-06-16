use assert_cmd::Command;

fn bin() -> Command { Command::cargo_bin("mem0").unwrap() }

fn add_get_id(db: &std::path::Path, content: &str, to: &str) -> String {
    let out = bin().args(["--db", db.to_str().unwrap(), "--json", "add", content, "--to", to])
        .output().unwrap();
    assert!(out.status.success());
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["id"].as_str().unwrap().to_string()
}

#[test]
fn promote_working_to_semantic_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    let id = add_get_id(&db, "draft", "working");
    bin().args(["--db", dbs, "promote", &id]).assert().success();
    let show = bin().args(["--db", dbs, "--json", "show", &id]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(v["lifecycle"], "semantic");
}

#[test]
fn promote_working_to_episodic_requires_session() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    let id = add_get_id(&db, "draft", "working");
    // Use library API to create session (CLI is still unimplemented at this point).
    {
        let path = db.clone();
        let conn = mem0::store::db::open(&path).unwrap();
        mem0::store::db::migrate(&conn).unwrap();
        mem0::store::sessions::new(&conn, "s").unwrap();
    }
    bin().args(["--db", dbs, "promote", &id, "--to", "episodic", "--session", "s"]).assert().success();
    let show = bin().args(["--db", dbs, "--json", "show", &id]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(v["lifecycle"], "episodic");
}

#[test]
fn promote_semantic_to_working_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    let id = add_get_id(&db, "fact", "semantic");
    bin().args(["--db", dbs, "promote", &id, "--to", "working"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn promote_unknown_id_fails_with_exit_3() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    bin().args(["--db", dbs, "promote", "deadbeef"])
        .assert()
        .failure()
        .code(3);
}
