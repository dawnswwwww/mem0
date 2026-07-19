use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::MemResult;
use crate::core::memory::Lifecycle;
use crate::output::format;
use crate::store::memories::{self, ListFilter};

#[derive(ClapArgs, Debug)]
pub struct Args {
    pub query: Vec<String>,
    #[arg(long)] pub layer: Option<String>,
    #[arg(long)] pub session: Option<String>,
    #[arg(long)] pub limit: Option<u32>,
}

pub fn run(conn: &Connection, args: Args, json: bool) -> MemResult<()> {
    let query = args.query.join(" ");
    let layer = args.layer.as_deref().map(str::parse::<Lifecycle>).transpose()?;
    let session = match args.session.as_deref() {
        Some(name) => Some(crate::store::sessions::get(conn, name)?.id),
        None => None,
    };
    let filter = ListFilter {
        layer,
        session,
        since_nanos: None,
        limit: args.limit.unwrap_or(20),
        max_distance: None,
        auto_cutoff: false,
    };
    let hits = memories::search(conn, &query, filter)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&format::list_json(&hits))?);
    } else if hits.is_empty() {
        println!("(no matches)");
    } else {
        for m in &hits { println!("{}", format::memory_human_line(m)); }
    }
    Ok(())
}
