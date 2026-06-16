use clap::{Args as ClapArgs, Subcommand};
use rusqlite::Connection;

use crate::core::error::{MemError, MemResult};
use crate::store::sessions;

/// Manage sessions and their memories.
#[derive(ClapArgs, Debug)]
#[command(about = "Manage sessions and their memories")]
pub struct SessionArgs {
    #[command(subcommand)]
    pub cmd: SessionCmd,
}

#[derive(Subcommand, Debug)]
pub enum SessionCmd {
    New  (NewArgs),
    List (ListArgs),
    Show (ShowArgs),
    Close(CloseArgs),
}

#[derive(ClapArgs, Debug)]
pub struct NewArgs   { #[arg(long)] pub name: String }
#[derive(ClapArgs, Debug)]
pub struct ListArgs  {}
#[derive(ClapArgs, Debug)]
pub struct ShowArgs  { pub target: String }
#[derive(ClapArgs, Debug)]
pub struct CloseArgs { pub target: String }

fn session_json(s: &sessions::Session) -> serde_json::Value {
    serde_json::json!({
        "id": s.id.to_string(),
        "name": s.name,
        "created_at": s.created_at,
        "closed_at": s.closed_at,
    })
}

pub fn run(conn: &Connection, args: SessionArgs, json: bool) -> MemResult<()> {
    match args.cmd {
        SessionCmd::New(a) => {
            if a.name.trim().is_empty() {
                return Err(MemError::InvalidArgument("name cannot be empty".into()));
            }
            let s = sessions::new(conn, &a.name)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&session_json(&s))?);
            } else {
                println!("created session {} ({})", s.name, &s.id.to_string()[..8]);
            }
            Ok(())
        }
        SessionCmd::List(_) => {
            let all = sessions::list(conn)?;
            if json {
                let arr: Vec<_> = all.iter().map(session_json).collect();
                println!("{}", serde_json::to_string_pretty(&serde_json::json!(arr))?);
            } else if all.is_empty() {
                println!("(no sessions)");
            } else {
                for s in &all {
                    let status = if s.closed_at.is_some() { "closed" } else { "open  " };
                    println!("[{}] {} {}", &s.id.to_string()[..8], status, s.name);
                }
            }
            Ok(())
        }
        SessionCmd::Show(a) => {
            let s = sessions::get(conn, &a.target)?;
            println!("{}", serde_json::to_string_pretty(&session_json(&s))?);
            Ok(())
        }
        SessionCmd::Close(a) => {
            sessions::close(conn, &a.target)?;
            println!("closed {}", a.target);
            Ok(())
        }
    }
}
