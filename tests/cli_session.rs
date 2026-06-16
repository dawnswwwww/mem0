use assert_cmd::Command;

fn bin() -> Command { Command::cargo_bin("mem0").unwrap() }

#[test]
fn session_new_creates_and_list_returns_it() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    bin().args(["--db", dbs, "session", "new", "--name", "s1"]).assert().success();
    bin().args(["--db", dbs, "session", "new", "--name", "s2"]).assert().success();
    let out = bin().args(["--db", dbs, "--json", "session", "list"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    let names: Vec<_> = arr.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"s1"));
    assert!(names.contains(&"s2"));
}

#[test]
fn session_show_resolves_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    bin().args(["--db", dbs, "session", "new", "--name", "alpha"]).assert().success();
    let out = bin().args(["--db", dbs, "--json", "session", "show", "alpha"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["name"], "alpha");
}

#[test]
fn session_close_marks_session_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    bin().args(["--db", dbs, "session", "new", "--name", "x"]).assert().success();
    bin().args(["--db", dbs, "session", "close", "x"]).assert().success();
    let out = bin().args(["--db", dbs, "--json", "session", "show", "x"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["closed_at"].is_number());
}

#[test]
fn session_new_duplicate_name_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    bin().args(["--db", dbs, "session", "new", "--name", "dup"]).assert().success();
    bin().args(["--db", dbs, "session", "new", "--name", "dup"]).assert().failure();
}
