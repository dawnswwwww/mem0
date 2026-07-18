use std::io::{IsTerminal, Read};

use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::{MemError, MemResult};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Text to embed. If omitted, reads from stdin.
    pub text: Vec<String>,

    /// Embed as a passage (`passage:` prefix) instead of a query (`query:`).
    #[arg(long)]
    pub as_passage: bool,

    /// Override the default model (multilingual-e5-small).
    #[arg(long)]
    pub model: Option<String>,
}

pub fn run(_conn: &Connection, args: Args, json: bool) -> MemResult<()> {
    // Gather text: positional args, else stdin if piped.
    let text = if !args.text.is_empty() {
        args.text.join(" ")
    } else {
        let mut stdin = std::io::stdin();
        if stdin.is_terminal() {
            return Err(MemError::InvalidArgument(
                "embed needs text: pass args or pipe text on stdin".into(),
            ));
        }
        let mut raw = String::new();
        stdin.read_to_string(&mut raw)?;
        raw.trim_end().to_string()
    };
    if text.is_empty() {
        return Err(MemError::InvalidArgument("embed text is empty".into()));
    }

    // `--json` is accepted for CLI parity; the embed command always emits a JSON
    // object, so the flag is intentionally unused here.
    let _ = json;

    #[cfg(not(feature = "embed"))]
    {
        let _ = (args.as_passage, args.model);
        return Err(MemError::EmbedFeatureNotEnabled);
    }

    #[cfg(feature = "embed")]
    {
        use crate::embed::{embed_text, ModelChoice, Role};
        let model = match args.model.as_deref() {
            Some(name) => ModelChoice::from_name(name)?,
            None => ModelChoice::DEFAULT,
        };
        let role = if args.as_passage { Role::Passage } else { Role::Query };
        let vec = embed_text(&text, role, model)?;
        let payload = serde_json::json!({
            "embedding": vec,
            "dim": vec.len(),
            "model": model.name(),
        });
        println!("{}", serde_json::to_string(&payload)?);
        Ok(())
    }
}
