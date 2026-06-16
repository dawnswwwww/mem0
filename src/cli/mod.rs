use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::core::error::{MemError, MemResult};

pub mod add;
pub mod compact;
pub mod delete;
pub mod list;
pub mod promote;
pub mod search;
pub mod session;
pub mod show;
pub mod stats;

#[derive(Parser, Debug)]
#[command(name = "mem0", version, about = "Layered memory for AI agents")]
pub struct Cli {
    /// Override the database path. Defaults to $XDG_DATA_HOME/mem0/mem0.db
    #[arg(long, global = true, env = "MEM0_DB")]
    pub db: Option<PathBuf>,

    /// Emit structured JSON instead of human-readable text
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Add      (crate::cli::add::Args),
    List     (crate::cli::list::Args),
    Search   (crate::cli::search::Args),
    Show     (crate::cli::show::Args),
    Promote  (crate::cli::promote::Args),
    Delete   (crate::cli::delete::Args),
    Session  (crate::cli::session::SessionArgs),
    Stats    (crate::cli::stats::Args),
    Compact  (crate::cli::compact::Args),
}

pub fn db_path(cli: &Cli) -> PathBuf {
    cli.db.clone().unwrap_or_else(|| {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("mem0").join("mem0.db")
    })
}

/// Map a MemError to a process exit code per spec §7.3.
pub fn exit_code_for(err: &MemError) -> i32 {
    match err {
        MemError::InvalidArgument(_)   => 2,
        MemError::InvalidId(_)         => 5,
        MemError::NotFound(_)          => 3,
        MemError::Storage(_)           => 4,
        MemError::InvalidTransition {..} => 2,
        _ => 1,
    }
}

pub fn run(cli: Cli) -> MemResult<()> {
    let path = db_path(&cli);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let conn = crate::store::db::open(&path)?;
    crate::store::db::migrate(&conn)?;

    match cli.command {
        Command::Add(a)     => crate::cli::add::run(&conn, a, cli.json),
        Command::List(a)    => crate::cli::list::run(&conn, a, cli.json),
        Command::Search(a)  => crate::cli::search::run(&conn, a, cli.json),
        Command::Show(a)    => crate::cli::show::run(&conn, a, cli.json),
        Command::Promote(a) => crate::cli::promote::run(&conn, a, cli.json),
        Command::Delete(a)  => crate::cli::delete::run(&conn, a, cli.json),
        Command::Session(a) => crate::cli::session::run(&conn, a, cli.json),
        Command::Stats(a)   => crate::cli::stats::run(&conn, a, cli.json),
        Command::Compact(a) => crate::cli::compact::run(&conn, a, cli.json),
    }
}
