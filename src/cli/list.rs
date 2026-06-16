use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::MemResult;
use crate::core::memory::Lifecycle;
use crate::output::format;
use crate::store::memories::{self, ListFilter};

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long)] pub layer: Option<String>,
    #[arg(long)] pub session: Option<String>,
    #[arg(long)] pub limit: Option<u32>,
    #[arg(long)] pub since: Option<String>,
}

fn parse_duration_to_nanos(s: &str) -> Option<i64> {
    let (num, unit) = s.split_at(s.len().saturating_sub(1));
    let n: i64 = num.parse().ok()?;
    let mul: i64 = match unit {
        "s" => 1_000_000_000,
        "m" => 60 * 1_000_000_000,
        "h" => 3600 * 1_000_000_000,
        "d" => 86400 * 1_000_000_000,
        _   => return None,
    };
    Some(n * mul)
}

pub fn run(conn: &Connection, args: Args, json: bool) -> MemResult<()> {
    let layer = args.layer.as_deref().map(str::parse::<Lifecycle>).transpose()?;
    let session = match args.session.as_deref() {
        Some(name) => Some(crate::store::sessions::get(conn, name)?.id),
        None => None,
    };
    let since_nanos = args.since.as_deref().and_then(parse_duration_to_nanos).map(|d| {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as i64 - d
    });
    let filter = ListFilter {
        layer,
        session,
        since_nanos,
        limit: args.limit.unwrap_or(20),
    };
    let items = memories::list(conn, filter)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&format::list_json(&items))?);
    } else if items.is_empty() {
        println!("(no memories)");
    } else {
        for m in &items { println!("{}", format::memory_human_line(m)); }
    }
    Ok(())
}
