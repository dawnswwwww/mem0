pub const V1_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE,
  created_at  INTEGER NOT NULL,
  closed_at   INTEGER
);

CREATE TABLE IF NOT EXISTS memories (
  id          TEXT PRIMARY KEY,
  lifecycle   TEXT NOT NULL CHECK (lifecycle IN ('working','episodic','semantic')),
  content     TEXT NOT NULL,
  source      TEXT,
  session_id  TEXT REFERENCES sessions(id) ON DELETE SET NULL,
  tags        TEXT NOT NULL DEFAULT '[]',
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  accessed_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_memories_layer_created
  ON memories(lifecycle, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_memories_session
  ON memories(session_id) WHERE session_id IS NOT NULL;

CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts
  USING fts5(content, tags, content='memories', content_rowid='rowid');

CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
  INSERT INTO memories_fts(rowid, content, tags) VALUES (new.rowid, new.content, new.tags);
END;
CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, content, tags) VALUES('delete', old.rowid, old.content, old.tags);
END;
CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, content, tags) VALUES('delete', old.rowid, old.content, old.tags);
  INSERT INTO memories_fts(rowid, content, tags) VALUES (new.rowid, new.content, new.tags);
END;
"#;
