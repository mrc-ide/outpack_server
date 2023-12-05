mod args;
use args::{Args, Command};

use clap::Parser;
use outpack::init::outpack_init;
use outpack::query::{parse_query, run_query};

fn main() -> anyhow::Result<()> {
    let cli = Args::parse();
    match cli.command {
        Command::Init {
            path,
            path_archive,
            use_file_store,
            require_complete_tree,
        } => {
            outpack_init(&path, path_archive, use_file_store, require_complete_tree)?;
        }

        Command::Search { root, query } => {
            let result = run_query(&root, &query)?;
            println!("{}", result);
        }

        Command::Parse { query } => {
            let result = parse_query(&query)?;
            println!("{:?}", result);
        }

        Command::StartServer { root } => {
            let server = outpack::api::api(&root)?;
            rocket::execute(server.launch())?;
        }
    }
    Ok(())
}
