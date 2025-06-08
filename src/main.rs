use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod cli;
mod config;
mod crypto;
mod p2p;
mod requests;
mod storage;
mod sync;

use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "synccore=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init => cli::init().await,
        Commands::Id => cli::show_id().await,
        Commands::Add { path, name } => cli::add_folder(path, name).await,
        Commands::Link => cli::generate_invitation().await,
        Commands::Join { code, path } => cli::join_sync(code, path).await,
        Commands::Status { verbose } => cli::show_status(verbose).await,
        Commands::Peers => cli::show_peers().await,
        Commands::Daemon => cli::run_daemon().await,
    }
}
