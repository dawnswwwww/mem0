use clap::Parser;
use mem0::cli::{run, Cli};

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(mem0::cli::exit_code_for(&e));
        }
    }
}
