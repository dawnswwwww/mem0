use assert_cmd::Command;

fn bin() -> Command { Command::cargo_bin("mem0").unwrap() }

#[test]
fn search_finds_matching_content() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    bin().args(["--db", dbs, "add", "user likes whiskey", "--to", "semantic"]).assert().success();
    bin().args(["--db", dbs, "add", "user dislikes beer", "--to", "semantic"]).assert().success();
    bin().args(["--db", dbs, "add", "weather is sunny", "--to", "working"]).assert().success();
    let out = bin().args(["--db", dbs, "--json", "search", "whiskey"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"], 1);
    assert!(v["items"][0]["content"].as_str().unwrap().contains("whiskey"));
}

#[test]
fn search_no_match_returns_empty_list() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    bin().args(["--db", dbs, "add", "x", "--to", "semantic"]).assert().success();
    let out = bin().args(["--db", dbs, "--json", "search", "zzznothing"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"], 0);
}

#[test]
fn search_filters_by_layer() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    bin().args(["--db", dbs, "add", "alpha", "--to", "semantic"]).assert().success();
    bin().args(["--db", dbs, "add", "alpha2", "--to", "working"]).assert().success();
    let out = bin().args(["--db", dbs, "--json", "search", "alpha", "--layer", "semantic"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"], 1);
}
