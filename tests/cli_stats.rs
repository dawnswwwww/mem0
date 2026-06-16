use assert_cmd::Command;

fn bin() -> Command { Command::cargo_bin("mem0").unwrap() }

#[test]
fn stats_reports_counts_per_layer() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    bin().args(["--db", dbs, "add", "a", "--to", "semantic"]).assert().success();
    bin().args(["--db", dbs, "add", "b", "--to", "semantic"]).assert().success();
    bin().args(["--db", dbs, "add", "c", "--to", "working"]).assert().success();
    let out = bin().args(["--db", dbs, "--json", "stats"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["semantic"], 2);
    assert_eq!(v["working"],  1);
    assert_eq!(v["episodic"], 0);
}

#[test]
fn compact_succeeds_even_on_empty_db() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let dbs = db.to_str().unwrap();
    bin().args(["--db", dbs, "compact"]).assert().success();
}
