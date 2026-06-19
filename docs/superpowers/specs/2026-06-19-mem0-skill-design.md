# mem0 Skill Design

**Date:** 2026-06-19
**Status:** Approved (pending spec review)
**Scope:** A Claude skill (`SKILL.md`) that teaches AI agents how to use the mem0 CLI for cross-session task state. Companion artifact to the v0.1.0/v1.1.0 binary; no code changes to mem0 itself.

## 1. Goal

Write a user-level Claude skill that makes mem0 actionable for AI agents without reading any other documentation. The skill is the **onboarding contract**: when invoked, an agent should know (a) when to think of mem0, (b) what mental model to use, (c) which 5-7 commands cover 90% of usage, (d) the right patterns to follow, (e) the anti-patterns to avoid. The skill is consumed by future AI agents; the immediate user value is dogfooding mem0 for the user's own work.

## 2. Non-Goals

- zvec-rust / embedding integration (deferred to v1.2+)
- Soft delete (`archived` lifecycle) — deferred
- `mem0 config` subcommand — deferred
- Multi-process safety / lockfile — deferred
- An MCP server wrapper around mem0 (no current request)
- A skill for non-Claude agents (the format is Claude-specific)

## 3. Architecture

Two locations for the skill content, kept byte-identical:

```
~/.claude/skills/mem0/SKILL.md                       # user-level (primary)
/Users/ringconn/workspace/projects/mem0/.claude/skills/mem0/SKILL.md  # project-level copy (committed to git as documentation)
```

The project-level copy is a **documentation artifact**, not a runtime artifact — it ships in the mem0 repository so future contributors see what an effective user-facing skill looks like and so the `mem0` repository can be dogfooded by anyone who clones it.

**Maintenance note:** byte-identity between the two files is maintained **manually** in v1. After updating the user-level copy, the user is expected to also update the project copy and commit. If divergence becomes a recurring problem, v1.1+ could introduce a `make sync-skill` target that copies user-level → project-level.

The skill is loaded by Claude when the user invokes the `/mem0` slash-command (or when the agent's system prompt matches the skill's trigger conditions). The content of `SKILL.md` becomes part of the agent's working context until the conversation ends or the user revokes the skill.

## 4. SKILL.md Content Structure

```
# Header + summary (5 lines)
  → Tool name, what it does, where data lives

# When to invoke this skill (15 lines)
  → 5 trigger conditions + 3 anti-triggers

# Mental model (25 lines)
  → 3 lifecycle layers as a comparison table
  → Transition rules as a list

# Quick start — 5 core commands (30 lines)
  → One block per command (signature + minimal example)

# Session protocol (60 lines)
  → Beginning-of-session recall (3 commands)
  → During-work capture (3 command patterns)
  → End-of-session consolidation (3-step checklist)

# Patterns (40 lines)
  → 4 worked examples with realistic content

# Anti-patterns (20 lines)
  → 5 "don't do this" warnings

# Cheatsheet (10 lines)
  → One-line-per-command summary for fast reference

# Notes (10 lines)
  → Binary path, DB path, env vars, exit codes, --json
```

Total: ~215 lines. Read time: 1-2 minutes. No external references required (no links to spec/plan/README — fully self-contained).

## 5. Trigger Conditions

The skill's "When to invoke" section uses these 5 triggers:

1. **Starting a new task session**: agent needs to recall what was done last time.
2. **User says**: "remember this", "save this", "note that", "where were we", "上次", "记一下".
3. **End of a long task**: before context is about to be compacted or session ends.
4. **About to make a non-trivial decision**: capture the *why* for future audit.
5. **Resuming work** on a project the agent has touched before.

Anti-triggers (do NOT invoke):
- The information is already in current context.
- A single-shot, ephemeral lookup ("what's 2+2").
- The user is asking about a totally unrelated topic.

## 6. Core Commands (5 commands, the 90% case)

```bash
# 1. Write — must specify --to
mem0 add "<content>" --to=working|episodic|semantic [--tag=<tag>]... [--session=<name>]

# 2. List by layer
mem0 list --layer=working [--limit=N]
mem0 list --layer=semantic [--tag=<tag>]

# 3. Search (FTS5 keyword + trigram; CJK supported since v1.1.0)
mem0 search "<query>" [--layer=semantic] [--tag=<tag>] [--limit=N]

# 4. Promote (working|episodic → semantic)
mem0 promote <id>           # default target = semantic
mem0 promote <id> --to=episodic --session=<name>

# 5. Delete
mem0 delete <id>
```

Auxiliary commands are listed briefly in the cheatsheet but not explained: `show`, `session new|list|close`, `stats`, `compact`.

## 7. Session Protocol

The skill prescribes a 3-phase protocol agents should follow:

| Phase | Commands | Purpose |
|---|---|---|
| Beginning | `list --layer=working`, `list --layer=semantic`, `session list` | Recall prior context |
| During work | `add --to=working` (scratch), `add --to=episodic --session=...` (decisions), `add --to=semantic --tag=...` (facts) | Capture as you go |
| End | `list --layer=working` → decide each → `promote <id>` or `delete <id>`; then `session close <name>` | Consolidate before context loss |

The "end of session" phase is **non-optional**: the skill explicitly says "Never end a session with un-promoted working memories — they persist forever and pollute future recalls."

## 8. Patterns (4 worked examples)

1. **Persist a user preference**: `add "user prefers dark mode in all TUI apps" --to=semantic --tag=preference --tag=ui`
2. **Capture a decision mid-task**: `session new --name=auth-refactor` → `add "chose jsonwebtoken over simple-jwt: latter unmaintained, former has 50M+ downloads" --to=episodic --session=auth-refactor`
3. **Resume after days**: `list --layer=working` + `search "auth" --layer=semantic --tag=project`
4. **Audit a multi-step decision**: `session list` → `search "chose postgres" --layer=episodic --session=schema-v1.1-design`

Each example uses realistic content (not "foo" / "bar"), so agents can pattern-match from concrete to concrete.

## 9. Anti-Patterns

The skill enumerates 5 anti-patterns to prevent:

1. Storing transient info in `semantic` (should be `working`)
2. Storing durable facts in `working` (should be `semantic`)
3. Forgetting to promote at end of session (leaves orphaned working memory)
4. Using `mem0` for ephemeral context (already in current turn)
5. Treating `mem0` as a vector DB (it's keyword + tag search; semantic recall is v1.2+)

## 10. Module-Level Changes

| File | Change |
|---|---|
| `~/.claude/skills/mem0/SKILL.md` | New, user-level skill content (~215 lines) |
| `/Users/ringconn/workspace/projects/mem0/.claude/skills/mem0/SKILL.md` | New, project-level copy (byte-identical to user-level) |

No code changes to mem0 itself. The skill is purely a documentation artifact.

## 11. Pre-implementation: Install the Binary

Before the skill is useful, mem0 must be installed locally:

```bash
cd /Users/ringconn/workspace/projects/mem0
cargo install --path .
~/.cargo/bin/mem0 --version    # → mem0 0.1.0
~/.cargo/bin/mem0 --help       # → usage with all 9 subcommands
```

This is part of Task 1 in the implementation plan; the skill's "Notes" section references the install path `~/.cargo/bin/mem0` and assumes the binary is there.

## 12. Validation Strategy

| Check | How |
|---|---|
| Skill content is correct | Manual review by reading `SKILL.md` end-to-end (no test framework for skills in v1) |
| Skill triggers appropriately | Manual: invoke `/mem0` in a fresh session, observe the agent's first action |
| Skill content is byte-identical across locations | `diff ~/.claude/skills/mem0/SKILL.md /Users/ringconn/workspace/projects/mem0/.claude/skills/mem0/SKILL.md` returns empty |
| Commands in the skill work | Each command in the skill is exercised against a real `~/.cargo/bin/mem0` install; exit codes and outputs verified |
| Anti-patterns are correct | Manual: read each anti-pattern; confirm it maps to a real failure mode in the v1.1.0 binary |

There is no automated test framework for Claude skills in v1; validation is by manual exercise.

## 13. Out-of-Scope (deferred)

- **A second SKILL.md for advanced features** (vector search, soft delete) — written when v1.2 lands.
- **Per-domain skills** (e.g. `mem0-rust-usage` for Rust-specific patterns) — only if dogfooding reveals the core skill isn't specific enough.
- **A README in the skill directory** explaining how to maintain it — added only if maintenance becomes a question.
- **Versioning of the skill** — the project copy in `mem0/.claude/skills/` is updated per release; the user-level copy is the user's responsibility.

## 14. Definition of Done

- `cargo install --path .` succeeds; `mem0 --version` prints `mem0 0.1.0`.
- `~/.claude/skills/mem0/SKILL.md` exists, ~215 lines, matches §4 structure.
- `/Users/ringconn/workspace/projects/mem0/.claude/skills/mem0/SKILL.md` is byte-identical to user-level copy.
- `diff` between the two locations returns empty.
- Project copy is committed to git on `main`.
- All 5 commands in the skill have been exercised at least once against the installed binary.
- Dogfooding: at least one piece of information from this design session has been written to mem0 via the CLI as proof of end-to-end usability.