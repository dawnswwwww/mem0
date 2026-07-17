use std::io::{IsTerminal, Read};

use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::{MemError, MemResult};
use crate::core::memory::Lifecycle;
use crate::output::format;
use crate::store::memories::ListFilter;
use crate::store::vectors;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long)]
    pub layer: Option<String>,
    #[arg(long)]
    pub session: Option<String>,
    #[arg(long)]
    pub limit: Option<u32>,
}

/// Read the query vector from stdin (must be piped): `{"embedding":[f32,...]}`.
fn read_query_vector() -> MemResult<Vec<f32>> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Err(MemError::EmbeddingParseError(
            "vsearch requires a query vector on stdin, e.g. echo '{\"embedding\":[...]}' | mem0 vsearch".into(),
        ));
    }
    let mut raw = String::new();
    stdin.read_to_string(&mut raw)?;
    let v: serde_json::Value =
        serde_json::from_str(raw.trim()).map_err(|e| MemError::EmbeddingParseError(e.to_string()))?;
    let arr = v
        .get("embedding")
        .and_then(|e| e.as_array())
        .ok_or_else(|| MemError::EmbeddingParseError("missing 'embedding' array".into()))?;
    arr.iter()
        .map(|x| {
            x.as_f64()
                .map(|f| f as f32)
                .ok_or_else(|| MemError::EmbeddingParseError("embedding has non-numeric element".into()))
        })
        .collect()
}

pub fn run(conn: &Connection, args: Args, json: bool) -> MemResult<()> {
    let query = read_query_vector()?;
    let layer = args
        .layer
        .as_deref()
        .map(str::parse::<Lifecycle>)
        .transpose()?;
    let session = match args.session.as_deref() {
        Some(name) => Some(crate::store::sessions::get(conn, name)?.id),
        None => None,
    };
    let filter = ListFilter {
        layer,
        session,
        since_nanos: None,
        limit: args.limit.unwrap_or(20),
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
