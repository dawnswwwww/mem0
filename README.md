# mem0

A local-first CLI that gives an AI agent a layered memory store with three
lifecycle tiers: **working**, **episodic**, **semantic**.

## Install

```bash
cargo install --path .
```

## Quickstart

```bash
# write
mem0 add "user likes whiskey" --to=semantic --tag=preference
mem0 session new --name=standup-0616
mem0 add "Q3 revenue 1.2M" --to=episodic --session=standup-0616
mem0 add "current task: fix login bug" --to=working

# read
mem0 list --layer=episodic --since=1d
mem0 search "whiskey" --layer=semantic
mem0 show <id-or-8char-prefix>

# lifecycle
mem0 promote <id>                  # working|episodic -> semantic
mem0 delete  <id>

# maintenance
mem0 stats
mem0 compact
```

## Layers

| Layer | Purpose | Example |
|---|---|---|
| `working` | Current task, in-flight context | "currently debugging login" |
| `episodic` | Time-ordered events within a session | "Q3 revenue 1.2M (from standup)" |
| `semantic` | Consolidated knowledge | "user likes whiskey" |

Transitions (enforced by `core::memory::Lifecycle::can_transition_to`):

- `working -> episodic` (requires a session)
- `working -> semantic`
- `episodic -> semantic`

Anything else is rejected.

## Storage

A single SQLite file at `$XDG_DATA_HOME/mem0/mem0.db` (override with `--db <path>`
or `MEM0_DB` env var). Search is FTS5 over `content` and `tags`.

## Exit codes

| code | meaning |
|---|---|
| 0 | success |
| 1 | generic error |
| 2 | invalid usage / invalid transition |
| 3 | not found |
| 4 | storage error |
| 5 | invalid id |

## v1 limits

- No LLM extraction, no embedding, no daemon
- FTS5 default tokenization (unicode61) — weak on CJK; use tags for fine recall
- Hard delete only

## License

MIT OR Apache-2.0