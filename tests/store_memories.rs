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

#[test]
fn search_finds_matching_content() {
    let (_tmp, conn) = fresh();
    let d1 = MemoryDraft { lifecycle: Lifecycle::Semantic, content: "user likes whiskey".into(), tags: vec![], session_id: None, source: None };
    let d2 = MemoryDraft { lifecycle: Lifecycle::Semantic, content: "user dislikes beer".into(),   tags: vec![], session_id: None, source: None };
    memories::insert(&conn, &d1).unwrap();
    memories::insert(&conn, &d2).unwrap();
    let hits = memories::search(&conn, "whiskey", ListFilter::default()).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].content.contains("whiskey"));
}

#[test]
#[ignore = "v1.1: FTS5 indexes tags_text (denormalized) instead of tags JSON. Re-enabled by Task 4 when insert() populates tags_text."]
fn search_also_matches_tags() {
    let (_tmp, conn) = fresh();
    let d = MemoryDraft { lifecycle: Lifecycle::Semantic, content: "some fact".into(), tags: vec!["whiskey".into()], session_id: None, source: None };
    memories::insert(&conn, &d).unwrap();
    let hits = memories::search(&conn, "whiskey", ListFilter::default()).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn search_filters_by_layer() {
    let (_tmp, conn) = fresh();
    let s = MemoryDraft { lifecycle: Lifecycle::Semantic, content: "whiskey".into(), tags: vec![], session_id: None, source: None };
    let w = MemoryDraft { lifecycle: Lifecycle::Working,  content: "whiskey".into(), tags: vec![], session_id: None, source: None };
    memories::insert(&conn, &s).unwrap();
    memories::insert(&conn, &w).unwrap();
    let only_sem = memories::search(&conn, "whiskey", ListFilter { layer: Some(Lifecycle::Semantic), ..Default::default() }).unwrap();
    assert_eq!(only_sem.len(), 1);
    assert_eq!(only_sem[0].lifecycle, Lifecycle::Semantic);
}

#[test]
fn search_picks_up_updates_via_trigger() {
    let (_tmp, conn) = fresh();
    let d = MemoryDraft { lifecycle: Lifecycle::Semantic, content: "old text".into(), tags: vec![], session_id: None, source: None };
    let id = memories::insert(&conn, &d).unwrap();
    // Direct UPDATE bypasses the store; the trigger should still sync FTS.
    conn.execute(
        "UPDATE memories SET content = 'brand new whiskey fact' WHERE id = ?1",
        rusqlite::params![id.to_string()],
    ).unwrap();
    let hits = memories::search(&conn, "whiskey", ListFilter::default()).unwrap();
    assert_eq!(hits.len(), 1, "au trigger should reindex");
}

#[test]
fn search_filters_by_since_nanos() {
    let (_tmp, conn) = fresh();
    let d = MemoryDraft { lifecycle: Lifecycle::Semantic, content: "whiskey".into(), tags: vec![], session_id: None, source: None };
    memories::insert(&conn, &d).unwrap();
    // since_nanos in the future: nothing matches
    let future = i64::MAX - 1;
    let hits = memories::search(&conn, "whiskey", ListFilter { since_nanos: Some(future), ..Default::default() }).unwrap();
    assert!(hits.is_empty(), "future cutoff should match nothing");
    // since_nanos in the past: 1 match
    let past = 0;
    let hits = memories::search(&conn, "whiskey", ListFilter { since_nanos: Some(past), ..Default::default() }).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn delete_removes_row_and_fts_entry() {
    let (_tmp, conn) = fresh();
    let d = MemoryDraft { lifecycle: Lifecycle::Semantic, content: "whiskey fact".into(), tags: vec![], session_id: None, source: None };
    let id = memories::insert(&conn, &d).unwrap();
    memories::delete(&conn, id).unwrap();
    let err = memories::get(&conn, id).unwrap_err();
    assert!(matches!(err, mem0::MemError::NotFound(_)));
    let hits = memories::search(&conn, "whiskey", ListFilter::default()).unwrap();
    assert!(hits.is_empty(), "fts trigger should remove");
}

#[test]
fn delete_unknown_returns_not_found() {
    let (_tmp, conn) = fresh();
    let bogus = uuid::Uuid::now_v7();
    let err = memories::delete(&conn, bogus).unwrap_err();
    assert!(matches!(err, mem0::MemError::NotFound(_)));
}

#[test]
fn set_lifecycle_legal_transition_succeeds() {
    let (_tmp, conn) = fresh();
    let d = MemoryDraft { lifecycle: Lifecycle::Working, content: "x".into(), tags: vec![], session_id: None, source: None };
    let id = memories::insert(&conn, &d).unwrap();
    let updated = memories::set_lifecycle(&conn, id, Lifecycle::Semantic).unwrap();
    assert_eq!(updated.lifecycle, Lifecycle::Semantic);
    assert!(updated.updated_at >= updated.created_at);
}

#[test]
fn set_lifecycle_illegal_transition_returns_err() {
    let (_tmp, conn) = fresh();
    let d = MemoryDraft { lifecycle: Lifecycle::Semantic, content: "x".into(), tags: vec![], session_id: None, source: None };
    let id = memories::insert(&conn, &d).unwrap();
    let err = memories::set_lifecycle(&conn, id, Lifecycle::Working).unwrap_err();
    assert!(matches!(err, mem0::MemError::InvalidTransition { .. }), "got {err:?}");
}

#[test]
fn set_lifecycle_unknown_id_returns_not_found() {
    let (_tmp, conn) = fresh();
    let bogus = uuid::Uuid::now_v7();
    let err = memories::set_lifecycle(&conn, bogus, Lifecycle::Semantic).unwrap_err();
    assert!(matches!(err, mem0::MemError::NotFound(_)));
}

#[test]
fn resolve_id_full_uuid() {
    let (_tmp, conn) = fresh();
    let d = MemoryDraft { lifecycle: Lifecycle::Semantic, content: "x".into(), tags: vec![], session_id: None, source: None };
    let id = memories::insert(&conn, &d).unwrap();
    assert_eq!(memories::resolve_id(&conn, &id.to_string()).unwrap(), id);
}

#[test]
fn resolve_id_8char_prefix() {
    let (_tmp, conn) = fresh();
    let d = MemoryDraft { lifecycle: Lifecycle::Semantic, content: "x".into(), tags: vec![], session_id: None, source: None };
    let id = memories::insert(&conn, &d).unwrap();
    let prefix = &id.to_string()[..8];
    assert_eq!(memories::resolve_id(&conn, prefix).unwrap(), id);
}

#[test]
fn resolve_id_unknown_returns_not_found() {
    let (_tmp, conn) = fresh();
    let err = memories::resolve_id(&conn, "deadbeef").unwrap_err();
    assert!(matches!(err, mem0::MemError::NotFound(_)));
}

#[test]
fn resolve_id_ambiguous_prefix_returns_invalid_id() {
    let (_tmp, conn) = fresh();
    let a = uuid::Uuid::from_u128(0x1111_1111_1111_1111_1111_1111_1111_1111);
    let b = uuid::Uuid::from_u128(0x1111_1111_2222_2222_2222_2222_2222_2222);
    for id in [a, b] {
        conn.execute(
            "INSERT INTO memories (id, lifecycle, content, tags, created_at, updated_at) VALUES (?1, 'semantic', 'x', '[]', 1, 1)",
            rusqlite::params![id.to_string()],
        ).unwrap();
    }
    let err = memories::resolve_id(&conn, "11111111").unwrap_err();
    assert!(matches!(err, mem0::MemError::InvalidId(_)), "got {err:?}");
}

#[test]
fn count_by_layer_groups_correctly() {
    let (_tmp, conn) = fresh();
    let s = MemoryDraft { lifecycle: Lifecycle::Semantic, content: "s".into(), tags: vec![], session_id: None, source: None };
    let w = MemoryDraft { lifecycle: Lifecycle::Working,  content: "w".into(), tags: vec![], session_id: None, source: None };
    let e = MemoryDraft { lifecycle: Lifecycle::Episodic, content: "e".into(), tags: vec![], session_id: None, source: None };
    memories::insert(&conn, &s).unwrap();
    memories::insert(&conn, &s).unwrap();
    memories::insert(&conn, &w).unwrap();
    memories::insert(&conn, &e).unwrap();
    let counts = memories::count_by_layer(&conn).unwrap();
    assert_eq!(counts.get(&Lifecycle::Semantic).copied().unwrap_or(0), 2);
    assert_eq!(counts.get(&Lifecycle::Working).copied().unwrap_or(0),  1);
    assert_eq!(counts.get(&Lifecycle::Episodic).copied().unwrap_or(0), 1);
}
