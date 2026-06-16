use rusqlite::Connection;
use crate::core::MemResult;

#[derive(clap::Args, Debug)]
pub struct Args {
    #[arg(long)] pub layer: Option<String>,
    #[arg(long)] pub session: Option<String>,
    #[arg(long)] pub limit: Option<u32>,
    #[arg(long)] pub since: Option<String>,
}

pub fn run(_conn: &Connection, _args: Args, _json: bool) -> MemResult<()> { unimplemented!("mem0 list — Task 19") }
