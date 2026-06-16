use mem0::core::Lifecycle;
use mem0::store::memories::{self, ListFilter, MemoryDraft};
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

#[test]
fn list_returns_recent_first() {
    let (_tmp, conn) = fresh();
    let draft = MemoryDraft {
        lifecycle:  Lifecycle::Semantic,
        content:    "x".into(),
        tags:       vec![],
        session_id: None,
        source:     None,
    };
    let a = memories::insert(&conn, &draft).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let b = memories::insert(&conn, &draft).unwrap();
    let items = memories::list(&conn, ListFilter::default()).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, b, "newest first");
    assert_eq!(items[1].id, a);
}

#[test]
fn list_filters_by_layer() {
    let (_tmp, conn) = fresh();
    let s = MemoryDraft { lifecycle: Lifecycle::Semantic, content: "s".into(), tags: vec![], session_id: None, source: None };
    let w = MemoryDraft { lifecycle: Lifecycle::Working,  content: "w".into(), tags: vec![], session_id: None, source: None };
    memories::insert(&conn, &s).unwrap();
    memories::insert(&conn, &w).unwrap();
    memories::insert(&conn, &s).unwrap();
    let only_semantic = memories::list(&conn, ListFilter { layer: Some(Lifecycle::Semantic), ..Default::default() }).unwrap();
    assert_eq!(only_semantic.len(), 2);
    assert!(only_semantic.iter().all(|m| m.lifecycle == Lifecycle::Semantic));
}

#[test]
fn list_respects_limit() {
    let (_tmp, conn) = fresh();
    let d = MemoryDraft { lifecycle: Lifecycle::Semantic, content: "x".into(), tags: vec![], session_id: None, source: None };
    for _ in 0..5 { memories::insert(&conn, &d).unwrap(); }
    let items = memories::list(&conn, ListFilter { limit: 3, ..Default::default() }).unwrap();
    assert_eq!(items.len(), 3);
}
