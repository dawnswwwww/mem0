use assert_cmd::Command;

fn bin() -> Command { Command::cargo_bin("mem0").unwrap() }

#[test]
fn full_lifecycle_add_list_promote_show_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();

    bin().args(["--db", dbs, "session", "new", "--name", "standup"]).assert().success();
    let add = bin().args(["--db", dbs, "--json", "add", "Q3 revenue 1.2M", "--to", "episodic", "--session", "standup"]).output().unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&add.stdout).unwrap()["id"].as_str().unwrap().to_string();

    let list = bin().args(["--db", dbs, "--json", "list", "--layer", "episodic", "--session", "standup"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(v["count"], 1);

    bin().args(["--db", dbs, "promote", &id]).assert().success();
    let show = bin().args(["--db", dbs, "--json", "show", &id]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(v["lifecycle"], "semantic");

    bin().args(["--db", dbs, "delete", &id]).assert().success();
    bin().args(["--db", dbs, "show", &id]).assert().failure().code(3);
}

#[test]
fn search_uses_fts_to_find_promoted_memory() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    bin().args(["--db", dbs, "add", "user likes whiskey", "--to", "working"]).assert().success();
    let add = bin().args(["--db", dbs, "--json", "add", "user likes wine", "--to", "working"]).output().unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&add.stdout).unwrap()["id"].as_str().unwrap().to_string();
    bin().args(["--db", dbs, "promote", &id]).assert().success();

    let out = bin().args(["--db", dbs, "--json", "search", "whiskey"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"], 1);
}

#[test]
fn error_mapping_consistent() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    // NotFound => exit 3
    let out = bin().args(["--db", dbs, "show", "deadbeef"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    // InvalidId (ambiguous prefix) => exit 5
    let conn = mem0::store::db::open(&db).unwrap();
    mem0::store::db::migrate(&conn).unwrap();
    let a = uuid::Uuid::from_u128(0x1111_1111_1111_1111_1111_1111_1111_1111);
    let b = uuid::Uuid::from_u128(0x1111_1111_2222_2222_2222_2222_2222_2222);
    for id in [a, b] {
        conn.execute(
            "INSERT INTO memories (id, lifecycle, content, tags, created_at, updated_at) VALUES (?1, 'semantic', 'x', '[]', 1, 1)",
            rusqlite::params![id.to_string()],
        ).unwrap();
    }
    drop(conn);
    let out = bin().args(["--db", dbs, "show", "11111111"]).output().unwrap();
    assert_eq!(out.status.code(), Some(5), "InvalidId should map to exit 5");
}
