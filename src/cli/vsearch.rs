use std::io::IsTerminal;

use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::{MemError, MemResult};
use crate::core::memory::Lifecycle;
use crate::output::format;
use crate::store::memories::ListFilter;
use crate::store::vectors;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Query text to embed locally (requires the `embed` feature). Mutually exclusive
    /// with a piped stdin vector; the piped vector wins if both are present.
    pub query: Option<String>,

    #[arg(long)]
    pub layer: Option<String>,
    #[arg(long)]
    pub session: Option<String>,
    #[arg(long)]
    pub limit: Option<u32>,

    /// Drop hits whose cosine distance exceeds this (lower = nearer; e.g. 0.15).
    /// Prunes the long tail of weakly-related results on short queries.
    #[arg(long)]
    pub max_distance: Option<f64>,

    /// Disable the default gap-based auto-cutoff (keep all top-N hits).
    #[arg(long)]
    pub no_cutoff: bool,

    /// Force local embedding of the query (overrides MEM0_EMBED=off).
    #[arg(long)]
    pub embed: bool,
    /// Do not embed the query text (require a piped vector instead).
    #[arg(long)]
    pub no_embed: bool,
    /// Override the default embedding model.
    #[arg(long)]
    pub model: Option<String>,
}

/// Resolve the query vector per spec §5 (piped vector > --embed > --no-embed >
/// MEM0_EMBED=off > default auto-embed).
fn resolve_query(args: &Args) -> MemResult<Vec<f32>> {
    if args.embed && args.no_embed {
        return Err(MemError::InvalidArgument(
            "conflicting --embed and --no-embed".into(),
        ));
    }

    // 1. piped stdin vector wins.
    let mut stdin = std::io::stdin();
    let piped = if stdin.is_terminal() { None } else {
        let mut raw = String::new();
        use std::io::Read;
        stdin.read_to_string(&mut raw)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() { None } else {
            let v: serde_json::Value = serde_json::from_str(trimmed)
                .map_err(|e| MemError::EmbeddingParseError(e.to_string()))?;
            let arr = v.get("embedding").and_then(|e| e.as_array())
                .ok_or_else(|| MemError::EmbeddingParseError("missing 'embedding' array".into()))?;
            Some(arr.iter().map(|x| x.as_f64().map(|f| f as f32)
                .ok_or_else(|| MemError::EmbeddingParseError("non-numeric element".into())))
                .collect::<MemResult<Vec<f32>>>()?)
        }
    };

    if piped.is_some() && args.embed {
        return Err(MemError::InvalidArgument(
            "piped vector and --embed both request a vector source".into(),
        ));
    }
    if let Some(v) = piped { return Ok(v); }

    // 2–5: embed the positional query text (if any). On the no-embedder build the
    // binding is only used to fail-fast on missing text, so mark it unused there.
    #[allow(unused_variables)]
    let text = args.query.as_deref().ok_or_else(|| MemError::EmbeddingParseError(
        "vsearch needs a query: pass text (with the embed feature) or pipe {\"embedding\":[...]}".into()
    ))?;

    #[cfg(not(feature = "embed"))]
    {
        // No embedder compiled in: a text query is unusable. (The --embed flag is
        // accepted for help-stability but cannot do work; the error is the same.)
        let _ = args.embed;
        Err(MemError::EmbedFeatureNotEnabled)
    }
    #[cfg(feature = "embed")]
    {
        if args.no_embed { return Err(MemError::InvalidArgument(
            "--no-embed given but no piped vector is available".into())); }
        if matches!(std::env::var("MEM0_EMBED"), Ok(v) if v.eq_ignore_ascii_case("off"))
            && !args.embed
        {
            return Err(MemError::InvalidArgument(
                "MEM0_EMBED=off and no piped vector; pass a vector or unset MEM0_EMBED".into()));
        }
        let model = match args.model.as_deref() {
            Some(n) => crate::embed::ModelChoice::from_name(n)?,
            None => crate::embed::ModelChoice::DEFAULT,
        };
        crate::embed::embed_text(text, crate::embed::Role::Query, model)
    }
}

pub fn run(conn: &Connection, args: Args, json: bool) -> MemResult<()> {
    let query = resolve_query(&args)?;
    let layer = args.layer.as_deref().map(str::parse::<Lifecycle>).transpose()?;
    let session = match args.session.as_deref() {
        Some(name) => Some(crate::store::sessions::get(conn, name)?.id),
        None => None,
    };
    let filter = ListFilter {
        layer, session, since_nanos: None,
        limit: args.limit.unwrap_or(20),
        max_distance: args.max_distance,
        auto_cutoff: !args.no_cutoff,
    };
    let hits = vectors::search(conn, &query, filter)?;
    if json {
        let refs: Vec<(&crate::store::memories::MemoryItem, f64)> =
            hits.iter().map(|(m, d)| (m, *d)).collect();
        println!("{}", serde_json::to_string_pretty(&format::vsearch_json(&refs))?);
    } else if hits.is_empty() {
        println!("(no matches)");
    } else {
        for (m, d) in &hits {
            println!("{}", format::vsearch_line(m, *d));
        }
    }
    Ok(())
}
