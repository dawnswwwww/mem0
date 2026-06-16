use mem0::core::Lifecycle;
use mem0::store::memories::{self, MemoryDraft};
use mem0::store::db;
use rusqlite::Connection;
use tempfile::TempDir;

fn fresh() -> (TempDir, Connection) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let conn = db::open(&path).unwrap();
    db::migrate(&conn).unwrap();
    (tmp, conn)
}

#[test]
fn insert_then_get_round_trip() {
    let (_tmp, conn) = fresh();

    let draft = MemoryDraft {
        lifecycle:  Lifecycle::Semantic,
        content:    "user likes whiskey".into(),
        tags:       vec!["preference".into()],
        session_id: None,
        source:     Some("cli".into()),
    };
    let id = memories::insert(&conn, &draft).unwrap();

    let got = memories::get(&conn, id).unwrap();
    assert_eq!(got.lifecycle, Lifecycle::Semantic);
    assert_eq!(got.content, "user likes whiskey");
    assert_eq!(got.tags, vec!["preference".to_string()]);
    assert_eq!(got.session_id, None);
    assert_eq!(got.source.as_deref(), Some("cli"));
    assert!(got.created_at > 0);
}

#[test]
fn insert_assigns_unique_ids() {
    let (_tmp, conn) = fresh();
    let draft = MemoryDraft {
        lifecycle:  Lifecycle::Working,
        content:    "x".into(),
        tags:       vec![],
        session_id: None,
        source:     None,
    };
    let a = memories::insert(&conn, &draft).unwrap();
    let b = memories::insert(&conn, &draft).unwrap();
    assert_ne!(a, b);
}

#[test]
fn insert_sets_timestamps_and_accessed_at_is_null() {
    let (_tmp, conn) = fresh();
    let draft = MemoryDraft {
        lifecycle:  Lifecycle::Semantic,
        content:    "x".into(),
        tags:       vec![],
        session_id: None,
        source:     None,
    };
    let id = memories::insert(&conn, &draft).unwrap();
    let got = memories::get(&conn, id).unwrap();
    assert_eq!(got.created_at, got.updated_at);
    assert!(got.accessed_at.is_none());
}
