#[derive(clap::Parser, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Initialize a new outpack repository
    Init {
        path: String,

        /// Path to the archive in which packets are stored.
        #[arg(long)]
        path_archive: Option<String>,

        /// Store packets in a content-addressed store.
        #[arg(long)]
        use_file_store: bool,

        /// Require a complete tree.
        #[arg(long)]
        require_complete_tree: bool,
    },

    /// Search for a packet in a repository
    Search {
        #[arg(short, long)]
        root: String,
        query: String,
    },

    /// Parse an outpack query, without evaluating it
    Parse { query: String },

    /// Start the outpack API server
    StartServer {
        #[arg(short, long)]
        root: String,
    },
}
