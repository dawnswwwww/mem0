use std::io::{IsTerminal, Read};

use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::{MemError, MemResult};
use crate::core::memory::Lifecycle;
use crate::output::format;
use crate::store::memories::{self, MemoryDraft};
use crate::store::vectors;

/// If stdin is piped, parse `{"embedding":[...]}`. If stdin is a terminal (no pipe),
/// return `Ok(None)` so text-only `add` is unchanged. A piped-but-invalid payload is
/// an error.
fn maybe_read_vector() -> MemResult<Option<Vec<f32>>> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Ok(None);
    }
    let mut raw = String::new();
    stdin.read_to_string(&mut raw)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let v: serde_json::Value =
        serde_json::from_str(trimmed).map_err(|e| MemError::EmbeddingParseError(e.to_string()))?;
    let arr = v
        .get("embedding")
        .and_then(|e| e.as_array())
        .ok_or_else(|| MemError::EmbeddingParseError("missing 'embedding' array".into()))?;
    let out: Vec<f32> = arr
        .iter()
        .map(|x| {
            x.as_f64()
                .map(|f| f as f32)
                .ok_or_else(|| MemError::EmbeddingParseError("embedding has non-numeric element".into()))
        })
        .collect::<MemResult<_>>()?;
    Ok(Some(out))
}

#[derive(ClapArgs, Debug)]
pub struct Args {
    pub content: Vec<String>,
    #[arg(long, value_enum)]
    pub to: Lifecycle,
    #[arg(long)]
    pub tag: Vec<String>,
    #[arg(long)]
    pub session: Option<String>,

    /// Force local embedding for this memory (overrides MEM0_EMBED=off).
    #[arg(long)]
    pub embed: bool,

    /// Store text only, do not embed (overrides auto-embed default).
    #[arg(long)]
    pub no_embed: bool,

    /// Override the default embedding model.
    #[arg(long)]
    pub model: Option<String>,

    /// Force a literal duplicate even if identical content exists in scope
    /// (default is to dedup: touch + merge tags on the existing row).
    #[arg(long)]
    pub no_dedup: bool,
}

pub fn run(conn: &Connection, args: Args, json: bool) -> MemResult<()> {
    let content = args.content.join(" ");
    if content.is_empty() {
        return Err(MemError::InvalidArgument("content cannot be empty".into()));
    }

    // Resolve session name -> id (only meaningful for episodic; but allow for working too).
    let session_id = match args.session.as_deref() {
        Some(name) => Some(crate::store::sessions::get(conn, name)?.id),
        None => None,
    };

    // working -> episodic requires a session
    if args.to == Lifecycle::Episodic && session_id.is_none() {
        return Err(MemError::InvalidArgument(
            "--to=episodic requires --session=<name>".into(),
        ));
    }

    // --- vector-source precedence (spec §5) ---
    // Resolve the vector FIRST (outside the transaction) so we don't hold BEGIN
    // across stdin IO / model inference. Order (highest priority first):
    //   1. piped stdin vector
    //   2. --embed                        (auto-embed, feature on)
    //   3. --no-embed                     (text only)
    //   4. MEM0_EMBED=off (case-insens.)  (text only)
    //   5. default                        (auto-embed, feature on)
    if args.embed && args.no_embed {
        return Err(MemError::InvalidArgument(
            "conflicting --embed and --no-embed".into(),
        ));
    }
    #[cfg(not(feature = "embed"))]
    if args.embed {
        return Err(MemError::EmbedFeatureNotEnabled);
    }

    // --- vector-source precedence (spec §5): piped vector wins, else auto-embed ---
    let piped = maybe_read_vector()?;
    if piped.is_some() && args.embed {
        return Err(MemError::InvalidArgument(
            "piped vector and --embed both request a vector source".into(),
        ));
    }
    // Embed only when there is no piped vector and policy allows (§5 rules 2–5).
    #[cfg(feature = "embed")]
    let auto: Option<Vec<f32>> = if piped.is_none() && should_embed(args.embed, args.no_embed) {
        let model = match args.model.as_deref() {
            Some(n) => crate::embed::ModelChoice::from_name(n)?,
            None => crate::embed::ModelChoice::DEFAULT,
        };
        Some(crate::embed::embed_text(&content, crate::embed::Role::Passage, model)?)
    } else {
        None
    };
    #[cfg(not(feature = "embed"))]
    let auto: Option<Vec<f32>> = None;
    let vec_opt: Option<Vec<f32>> = piped.or(auto);

    let draft = MemoryDraft {
        lifecycle:  args.to,
        content,
        tags:       args.tag,
        session_id,
        source:     Some("cli".into()),
    };

    conn.execute_batch("BEGIN")?;
    let result = (|| -> MemResult<_> {
        let (id, action) = memories::store(conn, &draft, !args.no_dedup)?;
        let item = memories::get(conn, id)?;
        if let Some(vec) = &vec_opt {
            // Look up rowid by id (works for both Inserted and Touched — Touched
            // did no INSERT, so last_insert_rowid() would be wrong).
            let rowid: i64 = conn.query_row(
                "SELECT rowid FROM memories WHERE id = ?1",
                rusqlite::params![id.to_string()],
                |r| r.get(0),
            )?;
            vectors::upsert(conn, rowid, vec)?;
        }
        Ok((item, action))
    })();
    let (item, action) = match result {
        Ok(v) => {
            conn.execute_batch("COMMIT")?;
            v
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    };

    if json {
        let mut v = format::memory_json(&item);
        v["action"] = match action {
            memories::StoreAction::Inserted => "stored",
            memories::StoreAction::Touched  => "touched",
        }.into();
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        let (verb, suffix) = match action {
            memories::StoreAction::Inserted => ("stored", ""),
            memories::StoreAction::Touched  => ("touched", " (dedup)"),
        };
        println!("{} {} as {}{}", verb, &item.id.to_string()[..8], item.lifecycle, suffix);
    }
    Ok(())
}

/// spec §5 rules 2–5 (feature on): embed > no-embed > MEM0_EMBED=off > default-on.
#[cfg(feature = "embed")]
fn should_embed(embed: bool, no_embed: bool) -> bool {
    if embed { return true; }
    if no_embed { return false; }
    !matches!(std::env::var("MEM0_EMBED"), Ok(v) if v.eq_ignore_ascii_case("off"))
}
