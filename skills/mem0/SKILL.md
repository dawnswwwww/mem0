---
name: mem0
description: 'How to use mem0 — a local-first CLI giving an AI agent a three-tier memory store (working / episodic / semantic) backed by SQLite + FTS5. Use whenever the agent should persist or recall durable knowledge across sessions: at the start of a task to recall prior work, when the user says "remember/save/note this", "记一下", "上次", "where were we", before context is compacted or a session ends, or before a non-trivial decision worth auditing. Do not use for ephemeral lookups or info already in context.'
---

# mem0 — Layered memory for AI agents

> Local-first CLI that gives an AI agent a three-tier memory store:
> working (in-flight scratchpad), episodic (time-stamped events), semantic
> (consolidated knowledge). Stores in SQLite + FTS5 at
> `$XDG_DATA_HOME/mem0/mem0.db`.

## When to invoke this skill

Invoke when **any** of these triggers fires:

- **Starting a new task session**: agent needs to recall what was done last time.
- **User says**: "remember this", "save this", "note that", "where were we", "上次", "记一下".
- **End of a long task**: before context is about to be compacted or session ends.
- **About to make a non-trivial decision**: capture the *why* for future audit.
- **Resuming work** on a project the agent has touched before.

Do **not** invoke when:

- The information is already in your current context.
- A single-shot, ephemeral lookup ("what's 2+2").
- The user is asking about a totally unrelated topic.

## Mental model

Three lifecycle tiers, enforced by `core::memory::Lifecycle::can_transition_to`:

| Layer | Purpose | When to write | When to read | Promotes to |
|---|---|---|---|---|
| `working` | Current task scratchpad | "Now I'm doing X" | At session start | `semantic` (fact) or `episodic` (event) |
| `episodic` | Time-stamped events, session-grouped | "We decided Y because Z" | When reviewing history | `semantic` (consolidated fact) |
| `semantic` | Consolidated knowledge | "User prefers A; project uses B" | At session start, before planning | (terminal) |

**Transition rules** (mem0 will reject invalid moves):

- `working` → `episodic` (requires `--session=<name>`)
- `working` → `semantic`
- `episodic` → `semantic`
- All other transitions: rejected with exit code 2

## Terminology

These words overlap in casual English; in mem0 they mean specific things. Use this table to pick the right word (and therefore the right layer):

| Term | Meaning | mem0 layer | CLI flag |
|---|---|---|---|
| **memory** (mem) | Any stored entry — the storage primitive. Three flavors below. | all three | (any `--to=`) |
| **durable knowledge** (持久知识) | A fact/principle that should outlive the task. Survives sessions. | **`semantic` only** | `--to=semantic` |
| **scratch** | In-flight task state. Throwaway once task ends. | `working` only | `--to=working` |
| **event log** (审计记录) | A time-stamped decision or event, grouped by session. | `episodic` only | `--to=episodic` |
| **fact** (事实) | A single piece of durable knowledge. One row in `semantic`. | `semantic` only | `--to=semantic` |

**Quick test:** if a word you want to use doesn't appear above, translate it:

- "I should remember this for later" → ask: is it durable knowledge (`semantic`), or scratch (`working`), or an event (`episodic`)?
- "I should remember this fact" → `semantic`
- "I'll write a note about this" → usually `working` (notes are scratch) or `semantic` if the note IS the durable knowledge
- "log this decision" → `episodic` with `--session=<name>`

**Rule of thumb:** every `mem0 add` should match exactly ONE row above. If you can't decide, you're probably trying to store scratch as durable knowledge or vice versa — re-read the **Anti-patterns** section.

## Quick start — 5 core commands

```bash
# 1. Write (specify layer; tags and session are optional)
mem0 add "<content>" --to=working|episodic|semantic [--tag=<tag>]... [--session=<name>]

# 2. List by layer (no `--tag` filter in v1; tag-aware list is planned for a future release)
mem0 list --layer=working [--limit=N]
mem0 list --layer=semantic [--session=<name>] [--since=1d]

# 3. Search (FTS5 keyword + trigram; CJK supported since v1.1.0; no `--tag` filter in v1)
mem0 search "<query>" [--layer=semantic] [--session=<name>] [--limit=N]

# 4. Promote (working|episodic → semantic)
mem0 promote <id>           # default target = semantic
mem0 promote <id> --to=episodic --session=<name>   # working → episodic

# 5. Delete
mem0 delete <id>
```

Auxiliary: `mem0 show <id-or-8char-prefix>`, `mem0 session new|list|close`, `mem0 stats`, `mem0 compact`.

`<id>` accepts full UUID or 8-char prefix. If ambiguous, exit 5 (use more chars).

## Session protocol

### Beginning of session — recall

```bash
# What was I working on last time?
mem0 list --layer=working

# What does the user prefer / what does the project use?
mem0 list --layer=semantic

# Is there an in-progress session I should know about?
mem0 session list
```

If empty: probably a fresh start. Don't fabricate context. Ask the user what they want to do.

### Periodic recall test (every few days or after major changes)

Run a 5-question recall test to verify mem0 is actually serving you. Hit rate < 4/5 means a coverage gap.

```bash
# 5 questions to ask. Use as-is, or replace with your own project's anchors.
mem0 search "binary install location"      # Q1: setup anchor
mem0 search "main architectural decision"  # Q2: design rationale
mem0 search "<biggest gotcha known>"      # Q3: pain points
mem0 search "<feature deferred>"           # Q4: roadmap
mem0 search "<user preference known>"      # Q5: about the human

# If any miss, add it now — the test result IS the gap signal:
mem0 add "<missing info>" --to=semantic --tag=...

# Spot-check recall shape with --json | jq:
mem0 list --layer=semantic --json | jq '.items | length, ([.items[].tags[]] | flatten | unique | length)'
# → first number = total memories; second = unique tag vocabulary size
```

If hit rate is consistently < 4/5, your mem0 isn't pulling its weight — either (a) you're not writing enough, (b) you're writing without thinking about future recall (see Pattern 6), or (c) the system is fine and the recall questions are wrong.

### During work — capture

```bash
# Mid-task discovery worth keeping
mem0 add "tried X, didn't work because Y" --to=working

# Decision worth auditing
mem0 add "chose Z over W because ... " --to=episodic --session=<task-name>

# Persistent fact about the user / project
mem0 add "user prefers 4-space Rust indents" --to=semantic --tag=preference
```

### End of session — consolidate

```bash
# 1. See what's still in working
mem0 list --layer=working

# 2. For each: keep as working, promote to semantic, or delete
mem0 promote <id>           # if it's a durable fact
mem0 delete  <id>           # if it's scratch / no longer relevant

# 3. Close any open episodic session
mem0 session close <name>
```

**Never end a session with un-promoted working memories** — they persist forever and pollute future recalls.

## Patterns (with examples)

### Pattern 1: Persist a user preference

```bash
mem0 add "user prefers dark mode in all TUI apps" \
  --to=semantic --tag=preference --tag=ui
```

### Pattern 2: Capture a decision mid-task

```bash
# First, create the session (or use an existing one)
mem0 session new --name=auth-refactor

# Then log the decision
mem0 add "chose jsonwebtoken over simple-jwt: latter unmaintained, former has 50M+ downloads" \
  --to=episodic --session=auth-refactor
```

### Pattern 3: Resume after days

```bash
# What was I doing?
mem0 list --layer=working
# → [01abc123] working  正在重构 auth 模块
# → [01def456] working  JWT 中间件测试还差 2 个

# What's the project context? (search for keywords; v1 has no --tag filter)
mem0 search "auth" --layer=semantic
# → [02aaa111] semantic  user uses axum 0.7; prefers sqlx over diesel
```

### Pattern 4: Audit a multi-step decision

```bash
mem0 session list
# → [s1] open   auth-refactor
# → [s2] closed standup-0616
# → [s3] closed schema-v1.1-design

mem0 search "chose postgres" --layer=episodic --session=schema-v1.1-design
```

### Pattern 5: Tag-aware listing via `--json` (workaround for v1)

```bash
# v1 doesn't support `--tag` filter on `list` or `search`.
# Workaround: pipe `--json` through `jq` and filter client-side.
mem0 list --layer=semantic --json | jq '.items[] | select(.tags | index("preference"))'
```

### Pattern 6: Write for future recall — include keyword variants

Recall in v1 is FTS5 keyword match only (no semantic search until v1.2+). Future-you (or another agent) will query with **paraphrased** words, not the ones you wrote. Counter by including multiple keyword variants in the content.

```bash
# ❌ brittle — only matches if query uses exact phrasing
mem0 add "user prefers 飞书 Base" --to=semantic

# ✅ resilient — matches direct keyword AND likely paraphrases
mem0 add "user prefers 飞书 Base (lark-base / Base table) for record-keeping" --to=semantic

# Same rule for technical terms:
# ❌ "uses hnsw index for vector search"
# ✅ "uses HNSW index (Hierarchical Navigable Small World) for vector search"
```

Rule of thumb: when writing, ask "if a different agent reads this in 3 months with only the topic, what 3-5 words might they search for?" — include them.

## Vector search (`vsearch`) — semantic recall

`vsearch` does cosine nearest-neighbour over memories that have a stored vector.
mem0 never computes embeddings — **the caller does**. Compute an embedding for the
text you want to find (and for memories when storing them), then pipe the vector as
`{"embedding":[...]}` on stdin.

```bash
# store a memory WITH a vector (vector is optional; omit to store text-only)
echo '{"embedding":[...your embedding...]}' | mem0 add "user prefers dark mode" --to=semantic

# semantic search: pipe the query vector
echo '{"embedding":[...query embedding...]}' | mem0 vsearch --layer=semantic --limit=10
```

Rules:
- The dimension is fixed by the **first** vector mem0 sees; all later vectors (add or
  vsearch) must match it, else exit code 2.
- `search` (FTS5 keywords) and `vsearch` (vector) are independent — use either or both.
- Memories added without a vector never appear in `vsearch` (they still appear in
  `search`/`list`). To make a memory vector-searchable, the embedding must be supplied
  at `add` time.
- Changing embedding model / dimension is manual: clear `memories_vec` and
  `meta.embedding_dim`, then re-add with the new model's vectors.

## Anti-patterns

- ❌ **Storing transient info in `semantic`**: "currently reading X" should be `working`, not `semantic`. Semantic is for facts that should outlive the task.
- ❌ **Storing durable facts in `working`**: "user prefers dark mode" should be `semantic`. Working memory gets promoted/deleted; facts shouldn't get caught in that churn.
- ❌ **Forgetting to promote at end of session**: leaves orphaned working memory that pollutes future recalls. Always end with `mem0 list --layer=working` and decide each.
- ❌ **Using `mem0` for ephemeral context**: if you need the value 30 seconds from now, it's already in your context. Don't store it.
- ❌ **Treating `mem0` as a general-purpose vector DB**: `vsearch` does semantic recall of stored memories via caller-supplied embeddings — it is not a store for arbitrary vectors unrelated to memories.
- ❌ **Assuming `--tag` filter exists on `list` / `search`**: v1 doesn't have it. Use the `--json | jq` workaround (Pattern 5) or rely on session/layer filters + FTS5 keyword recall.

## Cheatsheet

```bash
# Save durable knowledge
mem0 add "..." --to=semantic [--tag=X]

# Save current scratch
mem0 add "..." --to=working

# Save audit-worthy decision
mem0 add "..." --to=episodic --session=<name>

# Recall at session start
mem0 list --layer=working
mem0 list --layer=semantic

# Search by keyword (no --tag in v1)
mem0 search "..." [--layer=...] [--session=...]

# Tag-filter via JSON
mem0 list --layer=semantic --json | jq '.items[] | select(.tags | index("X"))'

# End-of-session cleanup
mem0 list --layer=working  # decide each
mem0 promote <id>          # or delete
```

## Embedding (built-in, opt-in `embed` feature)

When mem0 is built with the `embed` feature (`cargo build --features embed`),
`add` and `vsearch` embed text locally on CPU — no external embedder needed.

- `mem0 add "user likes whiskey" --to=semantic`   # auto-embeds (passage)
- `mem0 vsearch "drink preferences" --layer=semantic`  # auto-embeds (query)
- `mem0 embed "any text"`                          # prints {"embedding":...,"dim":384,"model":...}

Opt out of auto-embedding:
- `--no-embed` on a single command (text-only `add`).
- `MEM0_EMBED=off` environment variable (disables auto-embed globally for that shell).

Low-level path (still works, highest precedence — use this with an external embedder):
- `echo '{"embedding":[...]}' | mem0 add "x" --to=semantic`
- `echo '{"embedding":[...]}' | mem0 vsearch --layer=semantic`
- `my-embed "q" | mem0 vsearch`  (a piped vector always wins over auto-embed)

Model: default `multilingual-e5-small` (384-dim); override with `--model`
(`all-MiniLM-L6-v2`, `bge-small-en-v1.5`, `bge-small-zh-v1.5`, `nomic-embed-text-v1.5`).

Model location (searched in order): `$MEM0_EMBED_MODEL_DIR` → `<exe_dir>/models/` →
`<cache_dir>/mem0/fastembed/` → lazy download. Release builds ship the model beside
the binary so the common path is offline.

## Notes

- Binary lives at `~/.cargo/bin/mem0` (after `cargo install --path /path/to/mem0`).
- DB default: `$XDG_DATA_HOME/mem0/mem0.db` (override with `--db <path>` or `MEM0_DB` env var).
- Exit codes: 0 success · 2 invalid · 3 not found · 5 ambiguous/invalid id.
- All output supports `--json` for machine-readable.
