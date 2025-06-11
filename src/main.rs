//! # SlySync CLI Application
//!
//! Main entry point for the SlySync peer-to-peer file synchronization utility.
//! This application provides a command-line interface for managing file synchronization
//! across multiple devices without requiring central servers.

use anyhow::Result;
use clap::Parser;
use colored::*;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod bandwidth;
mod cli;
mod config;
mod crypto;
mod debounce;
mod mirror;
mod p2p;
mod requests;
mod storage;
mod sync;
mod watcher;

use cli::{Cli, Commands};

/// Main entry point for the SlySync application.
/// 
/// This function sets up logging, parses command line arguments, and dispatches
/// to the appropriate command handler based on user input.
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "slysync=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    if let Err(e) = match cli.command {
        Commands::Init => cli::init().await,
        Commands::Id => cli::show_id().await,
        Commands::Add { path, name } => cli::add_folder(path, name).await,
        Commands::Link => cli::generate_invitation().await,
        Commands::Join { code, path } => cli::join_sync(code, path).await,
        Commands::Status { verbose } => cli::show_status(verbose).await,
        Commands::Peers => cli::show_peers().await,
        Commands::Daemon => cli::run_daemon().await,
        Commands::Mirror { source, destination, name, daemon } => {
            cli::setup_mirror(source, destination, name, daemon).await
        },
    } {
        eprintln!("{} {}\n{}", "Error:".red().bold(), e.to_string().red(), "Tip: Run with --help for usage information.".yellow());
        std::process::exit(1);
    }
    Ok(())
}
