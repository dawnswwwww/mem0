use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn bin() -> Command { Command::cargo_bin("mem0").unwrap() }

#[test]
fn add_writes_to_specified_layer_and_returns_id() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");

    let out = bin()
        .args(["--db", db.to_str().unwrap(), "--json", "add",
               "user likes whiskey", "--to", "semantic",
               "--tag", "preference", "--no-embed"])
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
               "Q3 营收 120w", "--to", "episodic", "--session", "s1",
               "--no-embed"])
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

#[test]
fn add_with_vector_then_vsearch_recalls_it() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mem0.db").to_string_lossy().to_string();

    Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "add", "user likes whiskey", "--to", "semantic"])
        .write_stdin(r#"{"embedding":[1.0,0.0,0.0,0.0]}"#)
        .assert().success();

    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "--json", "vsearch"])
        .write_stdin(r#"{"embedding":[0.9,0.1,0.0,0.0]}"#)
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"], 1);
    assert!(v["items"][0]["content"].as_str().unwrap().contains("whiskey"));
}

#[test]
fn add_without_stdin_is_unchanged() {
    // No piped stdin + --no-embed ⇒ text-only add, exactly as in v1.2.
    // (Under the `embed` feature the default became auto-embed; pinning --no-embed
    //  keeps this v1.2 regression test network-free on both builds.)
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mem0.db").to_string_lossy().to_string();
    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "add", "plain text memory", "--to", "working", "--no-embed"])
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // No vector indexed ⇒ vsearch reports not initialized.
    let vout = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "vsearch"])
        .write_stdin(r#"{"embedding":[1.0,2.0,3.0,4.0]}"#)
        .output().unwrap();
    assert_eq!(vout.status.code(), Some(3));
}

#[test]
fn add_with_mismatched_vector_rolls_back_memory() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mem0.db").to_string_lossy().to_string();
    // seed dimension = 4
    Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "add", "seed", "--to", "semantic"])
        .write_stdin(r#"{"embedding":[1.0,2.0,3.0,4.0]}"#)
        .assert().success();
    // add with a wrong-dimension vector → upsert fails → memory must be rolled back
    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "add", "should-not-persist", "--to", "semantic"])
        .write_stdin(r#"{"embedding":[1.0,2.0,3.0]}"#)
        .output().unwrap();
    assert_eq!(out.status.code(), Some(2), "dim mismatch must exit 2");
    // the rolled-back memory must NOT appear in list
    let list = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "list", "--layer=semantic"])
        .output().unwrap();
    let list_txt = String::from_utf8_lossy(&list.stdout);
    assert!(!list_txt.contains("should-not-persist"), "rolled-back memory leaked into list: {list_txt}");
}

// --- spec §5 vector-source precedence (Task 8) ---
//
// These tests do NOT require the `embed` feature (they assert text-only /
// error behaviour and flag-conflict parsing). They run on the default build.

mod embed_precedence {
    use super::bin;
    use tempfile::TempDir;

    fn tmp_db() -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        (dir, db)
    }

    #[test]
    fn embed_and_no_embed_conflict_exits_2() {
        let (_d, db) = tmp_db();
        let out = bin()
            .args(["--db", &db, "add", "x", "--to", "semantic", "--embed", "--no-embed"])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn piped_vector_with_embed_flag_conflicts_exits_2() {
        let (_d, db) = tmp_db();
        let out = bin()
            .args(["--db", &db, "add", "x", "--to", "semantic", "--embed"])
            .write_stdin(r#"{"embedding":[1.0,2.0,3.0,4.0]}"#)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    #[cfg(not(feature = "embed"))]
    fn embed_without_feature_exits_2() {
        // Default build only: --embed without the feature compiled in must exit 2
        // with EmbedFeatureNotEnabled. (Under the feature, --embed auto-embeds.)
        let (_d, db) = tmp_db();
        let out = bin()
            .args(["--db", &db, "add", "x", "--to", "semantic", "--embed"])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    #[cfg(not(feature = "embed"))]
    fn plain_add_stays_text_only_without_feature() {
        // v1.2 regression guard (default build only): with no flags and no piped
        // stdin, add is text-only. Under the `embed` feature the default becomes
        // auto-embed, so this test is compiled out there.
        let (_d, db) = tmp_db();
        let out = bin()
            .args(["--db", &db, "add", "plain text memory", "--to", "working"])
            .output()
            .unwrap();
        assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
        let vout = bin()
            .args(["--db", &db, "vsearch"])
            .write_stdin(r#"{"embedding":[1.0,2.0,3.0,4.0]}"#)
            .output()
            .unwrap();
        assert_eq!(vout.status.code(), Some(3), "expected vector index not initialized");
    }
}

#[cfg(feature = "embed")]
mod autoembed {
    use super::bin;
    use tempfile::TempDir;

    #[test]
    #[ignore] // network: downloads model on first run (HuggingFace firewalled in CI)
    fn add_autoembed_then_vsearch_recalls() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("mem0.db").to_string_lossy().to_string();

        bin().args(["--db", &db, "add", "the user prefers single malt whiskey", "--to", "semantic"])
            .assert().success();
        bin().args(["--db", &db, "add", "unrelated note about the weather", "--to", "semantic"])
            .assert().success();

        // Compute a query vector via the embed subcommand (Query role, default model).
        let q = bin().args(["embed", "what does the user drink"]).output().unwrap();
        assert!(q.status.success(), "stderr: {}", String::from_utf8_lossy(&q.stderr));
        let qvec = q.stdout.clone();

        // Feed the query vector to vsearch via stdin.
        let out = bin()
            .args(["--db", &db, "vsearch", "--layer=semantic", "--limit=5"])
            .write_stdin(qvec)
            .output()
            .unwrap();
        assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("whiskey"), "top hit should be the whiskey memory: {s}");
    }
}
