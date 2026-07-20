use assert_cmd::Command;

fn bin() -> Command {
    // MEM0_EMBED=off keeps setup `add` calls from auto-embedding (network)
    // under --features embed; embed behaviour is covered elsewhere.
    let mut cmd = Command::cargo_bin("mem0").unwrap();
    cmd.env("MEM0_EMBED", "off");
    cmd
}

#[test]
fn show_pretty_prints_one_memory() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");

    let add = bin().args(["--db", db.to_str().unwrap(), "--json", "add", "hello", "--to", "semantic"]).output().unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&add.stdout).unwrap()["id"].as_str().unwrap().to_string();

    let out = bin().args(["--db", db.to_str().unwrap(), "show", &id]).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hello"));
    assert!(stdout.contains(&id));
}

#[test]
fn show_accepts_8char_prefix() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    let add = bin().args(["--db", db.to_str().unwrap(), "--json", "add", "x", "--to", "semantic"]).output().unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&add.stdout).unwrap()["id"].as_str().unwrap().to_string();
    let prefix = &id[..8];
    let out = bin().args(["--db", db.to_str().unwrap(), "--json", "show", prefix]).output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["id"], id);
}

#[test]
fn show_unknown_returns_exit_3() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    bin().args(["--db", db.to_str().unwrap(), "show", "deadbeef"])
        .assert()
        .failure()
        .code(3);
}
