use mem0::store::db;

#[test]
fn migrate_is_idempotent_and_creates_all_objects() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let conn = mem0::store::db::open(&path).unwrap();

    // Run twice — second call must not error or duplicate objects.
    mem0::store::db::migrate(&conn).unwrap();
    mem0::store::db::migrate(&conn).unwrap();

    // Enumerate the 8 user-facing schema objects explicitly. FTS5 shadow
    // tables (memories_fts_config/data/docsize/idx) are an implementation
    // detail and are not part of the v1 schema contract.
    let expected: Vec<&str> = vec![
        "memories",
        "idx_memories_layer_created",
        "idx_memories_session",
        "memories_fts",
        "memories_ai",
        "memories_ad",
        "memories_au",
        "sessions",
    ];
    for name in expected {
        let exists: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = ?1",
                rusqlite::params![name],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "schema object missing: {name}");
    }
}

#[test]
fn fts5_triggers_sync_on_insert() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let conn = mem0::store::db::open(&path).unwrap();
    mem0::store::db::migrate(&conn).unwrap();

    conn.execute(
        "INSERT INTO memories (id, lifecycle, content, tags, created_at, updated_at) \
         VALUES (?1, ?2, ?3, '[]', ?4, ?4)",
        rusqlite::params!["test-id-1", "semantic", "user likes whiskey", 1_000_000_000_i64],
    )
    .unwrap();

    // Search via FTS5 should find it
    let hits: i64 = conn
        .query_row(
            "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH 'whiskey'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
}

#[test]
fn open_creates_file_and_enables_wal() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let conn = db::open(&path).unwrap();

    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");

    let sync: i64 = conn
        .query_row("PRAGMA synchronous", [], |r| r.get(0))
        .unwrap();
    assert_eq!(sync, 1, "synchronous should be NORMAL");

    let fk: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fk, 1, "foreign_keys should be ON");

    let busy: i64 = conn
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .unwrap();
    assert_eq!(busy, 5000);
}

#[test]
fn open_reopens_existing_file() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let _ = db::open(&path).unwrap();
    let conn = db::open(&path).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal", "pragma must reapply on reopen");
}

#[test]
fn migrate_sets_user_version_to_latest_on_fresh_db() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let conn = mem0::store::db::open(&path).unwrap();
    mem0::store::db::migrate(&conn).unwrap();

    let v: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 4);
}

#[test]
fn sqlite_vec_is_available_after_open() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("mem0.db");
    let conn = mem0::store::db::open(&path).unwrap();
    mem0::store::db::migrate(&conn).unwrap();
    let version: String = conn
        .query_row("SELECT vec_version()", [], |r| r.get(0))
        .expect("vec_version() should exist after open");
    assert!(version.starts_with("v"), "got: {version}");
}
