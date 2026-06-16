use assert_cmd::Command;

fn bin() -> Command { Command::cargo_bin("mem0").unwrap() }

#[test]
fn delete_removes_memory() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    let add = bin().args(["--db", dbs, "--json", "add", "x", "--to", "semantic"]).output().unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&add.stdout).unwrap()["id"].as_str().unwrap().to_string();
    bin().args(["--db", dbs, "delete", &id]).assert().success();
    bin().args(["--db", dbs, "show", &id]).assert().failure().code(3);
}

#[test]
fn delete_unknown_returns_exit_3() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    bin().args(["--db", dbs, "delete", "deadbeef"])
        .assert()
        .failure()
        .code(3);
}
