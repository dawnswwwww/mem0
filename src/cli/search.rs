use rusqlite::Connection;
use crate::core::MemResult;

#[derive(clap::Args, Debug)]
pub struct Args {
    pub query: Vec<String>,
    #[arg(long)] pub layer: Option<String>,
    #[arg(long)] pub session: Option<String>,
    #[arg(long)] pub limit: Option<u32>,
}

pub fn run(_conn: &Connection, _args: Args, _json: bool) -> MemResult<()> { unimplemented!("mem0 search — Task 21") }
