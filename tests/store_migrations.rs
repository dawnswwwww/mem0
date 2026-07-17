//! Tests for the v1 → v2 schema migration.
//!
//! These tests build a v1-shaped DB manually (using the original `V1_SCHEMA`
//! with `tags` as a JSON column and FTS5 indexing it directly) and verify
//! that `store::db::migrate()` upgrades it to v2 cleanly.

use rusqlite::Connection;
use tempfile::TempDir;

use mem0::store::db;
use mem0::store::migrations;

/// Build a fresh v1-shaped DB without going through migrate(). Used to
/// simulate a v0.1.0 user upgrading to v1.1.
fn v1_db() -> (TempDir, Connection) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let conn = db::open(&path).unwrap();
    conn.execute_batch(migrations::V1_SCHEMA).unwrap();
    // v0.1.0 did not set user_version, so leave it at 0.
    let _: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    (tmp, conn)
}

#[test]
fn v1_db_helper_has_no_tags_text_column() {
    let (_tmp, conn) = v1_db();
    let cols: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('memories')")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert!(!cols.contains(&"tags_text".to_string()),
        "v1 helper should not have tags_text; got {cols:?}");
}

#[test]
fn migrate_v2_db_picks_up_v2() {
    let (_tmp, conn) = v1_db();
    db::migrate(&conn).unwrap();

    let cols: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('memories')")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert!(cols.contains(&"tags_text".to_string()),
        "after migrate, tags_text column should exist; got {cols:?}");
}

#[test]
fn migrate_v2_backfills_tags_text() {
    let (_tmp, conn) = v1_db();
    // Seed v1 row with non-trivial tags
    conn.execute(
        "INSERT INTO memories (id, lifecycle, content, tags, created_at, updated_at) \
         VALUES ('test-1', 'semantic', 'fact', '[\"Preference\",\"WHISKEY\"]', 1, 1)",
        [],
    ).unwrap();
    db::migrate(&conn).unwrap();

    let tags_text: String = conn
        .query_row(
            "SELECT tags_text FROM memories WHERE id = 'test-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tags_text, "preference whiskey");
}

#[test]
fn migrate_v2_rebuilds_fts5_with_trigram() {
    let (_tmp, conn) = v1_db();
    conn.execute(
        "INSERT INTO memories (id, lifecycle, content, tags, created_at, updated_at) \
         VALUES ('cjk-1', 'semantic', 'user 喜欢威士忌', '[]', 1, 1)",
        [],
    ).unwrap();
    db::migrate(&conn).unwrap();

    // After migration, FTS5 must be a trigram-indexed table over (content, tags_text).
    // Trigram enables substring matching on CJK content (n-gram size = 3, so
    // queries must be at least 3 chars; we use 3-char 威士忌 here).
    let hits: i64 = conn
        .query_row(
            "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH '威士忌'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1, "trigram tokenizer should match CJK substring");
}

#[test]
fn migrate_v2_idempotent() {
    let (_tmp, conn) = v1_db();
    db::migrate(&conn).unwrap();
    db::migrate(&conn).unwrap();  // second call must not error
    let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 3);
}

#[test]
fn migrate_v2_keeps_existing_memories_intact() {
    let (_tmp, conn) = v1_db();
    conn.execute(
        "INSERT INTO memories (id, lifecycle, content, source, tags, created_at, updated_at) \
         VALUES ('keep-1', 'semantic', 'important fact', 'cli', '[\"a\"]', 1, 1)",
        [],
    ).unwrap();
    db::migrate(&conn).unwrap();

    let (content, lifecycle, source, tags): (String, String, Option<String>, String) = conn
        .query_row(
            "SELECT content, lifecycle, source, tags FROM memories WHERE id = 'keep-1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(content, "important fact");
    assert_eq!(lifecycle, "semantic");
    assert_eq!(source.as_deref(), Some("cli"));
    assert_eq!(tags, "[\"a\"]", "tags JSON must be preserved unchanged");
}

#[test]
fn migrate_v2_db_picks_up_v3_meta_table() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("mem0.db");
    let conn = mem0::store::db::open(&path).unwrap();
    mem0::store::db::migrate(&conn).unwrap();

    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, 3);

    let n: i64 = conn
        .query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='meta'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "meta table must exist after v3 migration");
}

#[test]
fn migrate_v3_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("mem0.db");
    let conn = mem0::store::db::open(&path).unwrap();
    mem0::store::db::migrate(&conn).unwrap();
    mem0::store::db::migrate(&conn).unwrap();
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, 3);
}
