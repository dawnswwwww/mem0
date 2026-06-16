use assert_cmd::Command;

fn bin() -> Command { Command::cargo_bin("mem0").unwrap() }

#[test]
fn list_human_one_per_line_newest_first() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let db_str = db.to_str().unwrap().to_owned();

    bin().args(["--db", &db_str, "add", "alpha", "--to", "semantic"]).assert().success();
    bin().args(["--db", &db_str, "add", "beta",  "--to", "working"]).assert().success();

    let out = bin().args(["--db", &db_str, "list"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<_> = stdout.lines().filter(|l| l.contains("alpha") || l.contains("beta")).collect();
    assert!(lines[0].contains("beta"), "got: {lines:?}");
    assert!(lines[1].contains("alpha"), "got: {lines:?}");
}

#[test]
fn list_json_returns_items_and_count() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let db_str = db.to_str().unwrap().to_owned();
    bin().args(["--db", &db_str, "add", "a", "--to", "semantic"]).assert().success();
    bin().args(["--db", &db_str, "add", "b", "--to", "semantic"]).assert().success();

    let out = bin().args(["--db", &db_str, "--json", "list"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"], 2);
    assert_eq!(v["items"].as_array().unwrap().len(), 2);
}

#[test]
fn list_filter_by_layer() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let db_str = db.to_str().unwrap().to_owned();
    bin().args(["--db", &db_str, "add", "a", "--to", "semantic"]).assert().success();
    bin().args(["--db", &db_str, "add", "b", "--to", "working"]).assert().success();
    bin().args(["--db", &db_str, "add", "c", "--to", "semantic"]).assert().success();

    let out = bin().args(["--db", &db_str, "--json", "list", "--layer", "semantic"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"], 2);
}
