use assert_cmd::Command;

#[test]
fn binary_runs_and_prints_version() {
    Command::cargo_bin("mem0")
        .unwrap()
        .assert()
        .success()
        .stdout(predicates::str::contains("mem0 v0.1.0"));
}
