use rusqlite::Connection;
use crate::core::MemResult;

/// Manage sessions and their memories.
#[derive(clap::Args, Debug)]
#[command(about = "Manage sessions and their memories")]
pub struct SessionArgs {
    #[command(subcommand)]
    pub cmd: SessionCmd,
}

#[derive(clap::Subcommand, Debug)]
pub enum SessionCmd {
    New  (NewArgs),
    List (ListArgs),
    Show (ShowArgs),
    Close(CloseArgs),
}

#[derive(clap::Args, Debug)]
pub struct NewArgs   { #[arg(long)] pub name: String }
#[derive(clap::Args, Debug)]
pub struct ListArgs  {}
#[derive(clap::Args, Debug)]
pub struct ShowArgs  { pub target: String }
#[derive(clap::Args, Debug)]
pub struct CloseArgs { pub target: String }

pub fn run(_conn: &Connection, _args: SessionArgs, _json: bool) -> MemResult<()> { unimplemented!("mem0 session — Task 24") }
