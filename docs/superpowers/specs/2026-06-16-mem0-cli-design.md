# mem0 CLI Design

**Date:** 2026-06-16
**Status:** Approved (pending spec review)
**Scope:** v1 — local CLI only

## 1. Goal

Provide a local-first CLI that gives an AI agent (or its human operator) a **layered memory store** with three lifecycle tiers: `working`, `episodic`, `semantic`. v1 ships with explicit writes, keyword retrieval, and no daemon.

## 2. Non-Goals (v1)

- LLM-driven extraction / consolidation / dedup
- Embedding-based or hybrid semantic retrieval
- Background daemon, IPC, multi-endpoint sync
- Time-decay, importance scoring, ranking beyond FTS5 BM25
- Encryption-at-rest, cloud backup
- TUI / REPL / interactive mode

The fields `accessed_at` and `tags` are persisted in v1 but **not consumed by v1 logic**; they exist to keep v1.1 migrations cheap.

## 3. Architecture

```
┌──────────────────────────────────────────────────┐
│  CLI layer  (clap derive)                         │
│  add / list / show / search / promote / session  │
└────────────────┬─────────────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────────────┐
│  Core domain layer  (pure Rust, no IO)            │
│   • MemoryItem, Lifecycle, SessionId             │
│   • Error model, validation                       │
└────────────────┬─────────────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────────────┐
│  Store layer  (rusqlite + FTS5)                   │
│   • Connection, migrations, WAL mode              │
│   • memories table + FTS5 virtual table           │
│   • Default path: $XDG_DATA_HOME/mem0/mem0.db     │
└──────────────────────────────────────────────────┘
```

**Runtime model:** stateless CLI. Each `mem0` invocation opens SQLite, performs its operation, closes. SQLite serial-write + WAL is sufficient for an agent's sequential CLI usage. No daemon, no socket, no lockfile in v1.

**Layering axis:** a single `lifecycle` column. Three layers share one schema; layer-specific extensions come in v1.1+ if any layer's access pattern diverges materially.

## 4. Data Model

```sql
CREATE TABLE memories (
  id          TEXT PRIMARY KEY,        -- UUIDv7
  lifecycle   TEXT NOT NULL,           -- 'working' | 'episodic' | 'semantic'
  content     TEXT NOT NULL,
  source      TEXT,                    -- 'cli' | 'agent' | 'api'
  session_id  TEXT,                    -- nullable; meaningful for working/episodic
  tags        TEXT NOT NULL DEFAULT '[]',  -- JSON array of strings
  created_at  INTEGER NOT NULL,        -- unix nanoseconds
  updated_at  INTEGER NOT NULL,        -- unix nanoseconds
  accessed_at INTEGER                  -- unix nanoseconds; reserved, v1 unused
);

CREATE INDEX idx_memories_layer_created
  ON memories(lifecycle, created_at DESC);
CREATE INDEX idx_memories_session
  ON memories(session_id) WHERE session_id IS NOT NULL;

CREATE VIRTUAL TABLE memories_fts
  USING fts5(content, tags, content='memories', content_rowid='rowid');

-- triggers keep FTS in sync with the base table on INSERT/UPDATE/DELETE
CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
  INSERT INTO memories_fts(rowid, content, tags) VALUES (new.rowid, new.content, new.tags);
END;
CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, content, tags) VALUES('delete', old.rowid, old.content, old.tags);
END;
CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, content, tags) VALUES('delete', old.rowid, old.content, old.tags);
  INSERT INTO memories_fts(rowid, content, tags) VALUES (new.rowid, new.content, new.tags);
END;
```

**ID strategy:** UUIDv7 (time-ordered) for primary key. v1 accepts full UUIDs and **8-character prefixes** in any user-facing position (e.g. `mem0 show <id>`); prefix collisions abort the command with a non-zero exit code.

**FTS5 strategy:** `content='memories'` external-content mode — base table is the source of truth, FTS holds only the inverted index. Three triggers keep them aligned. Tokenizer default is `unicode61` with `remove_diacritics 2`; Chinese-quality tokenization is an open question (see §10).

**Sessions:** stored as a separate table (see §5) and referenced by `memories.session_id`. The session row holds metadata; memory rows are the contents.

## 5. Sessions (Episodic Container)

```sql
CREATE TABLE sessions (
  id          TEXT PRIMARY KEY,        -- UUIDv7
  name        TEXT NOT NULL UNIQUE,    -- human-friendly, e.g. 'standup-0616'
  created_at  INTEGER NOT NULL,
  closed_at   INTEGER                  -- nullable; non-null means session is closed
);
```

`mem0 session new --name=...` creates a row; `mem0 session close <sid>` sets `closed_at`. Episodes written with `--session=<name>` resolve the name to the session id at write time.

## 6. Lifecycle Semantics

| Layer | Purpose | Session-bound | Tag use | Retention |
|---|---|---|---|---|
| `working` | Current task, in-flight context | optional | optional | manual |
| `episodic` | Time-ordered events within a session | yes | optional | manual |
| `semantic` | Consolidated facts / knowledge | no | encouraged | manual |

**Transition rules** (enforced in `core::memory::Lifecycle::can_transition_to`):

- `working → episodic`: allowed, requires `--session=<name>` at command time
- `working → semantic`: allowed
- `episodic → semantic`: allowed (the "consolidate" operation)
- `episodic → working`: disallowed (cannot re-open a closed time window)
- `semantic → working | episodic`: disallowed (semantic is the most stable layer)
- Same-layer transitions: no-op (returns 0 in v1; reserved for future use)

The CLI exposes only `promote` (with an explicit `--to=<target>`). There is no `demote` and no `archive` in v1. To fix a misfiled memory, the user runs `mem0 delete <id>` and re-`add`s it. The transition table is the single source of truth — both the `promote` command and any future program-initiated moves consult `can_transition_to`.

## 7. CLI Surface

### 7.1 Global flags

- `--json` — all output as structured JSON
- `--db <path>` — override the data directory (default: `$XDG_DATA_HOME/mem0/mem0.db` or platform fallback)

The human renderer detects whether stdout is a TTY: if not, it emits plain text without ANSI escapes. v1 ships no color output and adds no color dependency; a future version may add color under a feature flag.

### 7.2 Commands

```text
# writes
mem0 add <CONTENT> --to=<working|episodic|semantic> [--tag=<tag>]... [--session=<name>]
mem0 promote <id> [--to=<working|episodic|semantic>] [--session=<name>]   # default --to=semantic
mem0 delete <id>

# reads
mem0 list   [--layer=<l>] [--session=<name>] [--limit=<n>] [--since=<duration>]
mem0 show   <id>
mem0 search <QUERY>  [--layer=<l>] [--session=<name>] [--limit=<n>]

# sessions
mem0 session new   --name=<name>
mem0 session list
mem0 session show  <sid|name>
mem0 session close <sid|name>

# maintenance
mem0 stats
mem0 compact        # v1: runs VACUUM; reserved hook for future cleanup
```

`<duration>` accepts `30s`, `15m`, `2h`, `7d`. `<id>` accepts full UUID or 8-char prefix.

### 7.3 Output

- **Human mode (default):** one memory per line for `list`; pretty JSON for `show`; tabular for `session list`.
- **JSON mode:** stable schema. `list` returns `{"items": [...], "count": N}`; `add` returns `{"id": "...", "lifecycle": "...", "created_at": N}`; errors return `{"error": {"code": "NotFound", "message": "..."}}`.
- **Exit codes:** 0 success, 1 generic, 2 invalid usage, 3 not found, 4 storage error, 5 invalid id.

## 8. Module Layout

```
src/
├── main.rs           # clap entrypoint
├── lib.rs            # public API surface (so v1.1+ can add another binary that reuses it)
├── cli/
│   ├── mod.rs        # clap derive App + subcommand enum
│   ├── add.rs
│   ├── list.rs
│   ├── search.rs
│   ├── show.rs
│   ├── promote.rs
│   ├── delete.rs
│   └── session.rs
├── core/
│   ├── memory.rs     # MemoryItem, Lifecycle enum, transition rules
│   ├── ids.rs        # UUIDv7 wrapper, SessionId newtype, id-prefix parsing
│   └── error.rs      # thiserror-based MemError
├── store/
│   ├── db.rs         # open(), migrations, WAL pragmas
│   ├── memories.rs   # CRUD + FTS5-aware search
│   └── sessions.rs
└── output/
    └── format.rs     # human / json rendering
```

**Dependency direction:** `cli` → `store` + `core`; `store` → `core`; `core` has no project-internal dependencies. This keeps `core` testable without SQLite and lets a future HTTP/RPC binary reuse `store` unchanged.

## 9. Dependencies (v1)

```toml
[dependencies]
clap        = { version = "4", features = ["derive"] }
rusqlite    = { version = "0.32", features = ["bundled"] }
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
uuid        = { version = "1", features = ["v7", "serde"] }
chrono      = { version = "0.4", default-features = false, features = ["clock"] }
thiserror   = "1"
dirs        = "5"

[dev-dependencies]
assert_cmd  = "2"
predicates  = "3"
tempfile    = "3"
```

**Explicitly not used in v1:** `tokio`, `reqwest`, `sqlx`, `embeddings` crates, `candle`, any LLM SDK. The bundled `rusqlite` feature means we ship our own SQLite so the binary works on machines without a system `libsqlite3`.

## 10. Open Questions (v1 non-blocking)

1. **Chinese tokenization for FTS5.** Default `unicode61` is weak on CJK. Options: `tokenize='trigram'` (good recall, bigger index), or `unicode61` + require tags / N-grams supplied by the caller. v1 ships with the default `unicode61` and documents the limitation; a v1.1 PR may switch to `trigram` after benchmarking.
2. **Soft delete vs hard delete.** v1 hard-deletes via `mem0 delete`. If users complain, v1.1 introduces an `archived` lifecycle value and a soft-delete command.
3. **Configuration surface.** v1 honors `MEM0_DB` env var and `--db` flag only. A `mem0 config` subcommand is deferred.
4. **Multi-process safety.** v1 assumes one writer. Concurrent writers from forked agents are unsupported; if observed, v1.1 adds a per-DB lockfile.

## 11. Testing Strategy

- **Unit tests** on `core::*` (transition rules, id parsing, lifecycle validation) — target 100% line coverage.
- **Store integration tests** in `tests/store_*.rs` using `tempfile::tempdir()` + a real SQLite file. Cover: schema migration idempotency, FTS5 sync via triggers, search ranking sanity, session binding.
- **CLI end-to-end tests** in `tests/cli_*.rs` using `assert_cmd`. Cover: each subcommand's exit code, `--json` schema stability, `--db` override, error mapping (NotFound → exit 3, etc.).
- **Property test (light):** for any sequence of `add` / `promote` / `search`, the FTS index returns at least the items just added (smoke test for trigger correctness).

## 12. Definition of Done (v1)

- All §7 commands implemented and behind `assert_cmd` tests
- `cargo clippy --all-targets -- -D warnings` clean
- `cargo test` green; coverage of `core` ≥ 90%
- README with install / quickstart / lifecycle explanation
- The four open questions in §10 either resolved or explicitly deferred to v1.1
