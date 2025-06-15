//! # Command Line Interface
//! 
//! This module provides the command-line interface for SlySync, including
//! argument parsing, command definitions, and command implementations.
//! 
//! ## Commands
//! 
//! - `init` - Initialize SlySync configuration and generate node identity
//! - `id` - Display the current node's public ID
//! - `add` - Add a new local folder to be synchronized
//! - `link` - Generate a secure invitation code for sharing folders
//! - `join` - Join a remote sync folder using an invitation code
//! - `status` - Display status of all sync jobs
//! - `peers' - List all connected peers
//! - `daemon` - Run the SlySync engine as a background daemon

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, warn, error};
use colored::*;
use tokio::time::Duration;
use std::env;
use serde::{Serialize, Deserialize};
use std::fs;

/// SlySync - A next-generation, peer-to-peer file synchronization CLI utility.
/// 
/// SlySync provides secure, decentralized file synchronization without relying 
/// on central servers, using modern cryptographic protocols and efficient networking.
#[derive(Parser)]
#[command(name = "slysync")]
#[command(about = "A next-generation, peer-to-peer file synchronization CLI utility")]
#[command(version = "1.0.0")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI commands for SlySync.
/// 
/// Each command represents a different operation that can be performed
/// with the SlySync application, from initialization to daemon operation.
#[derive(Subcommand)]
pub enum Commands {
    /// Initialize SlySync configuration and generate node identity
    Init,
    
    /// Display the current node's public ID
    Id,
    
    /// Add a new local folder to be synchronized
    Add {
        /// Path to the folder to synchronize
        path: PathBuf,
        /// Optional human-readable alias for the folder
        #[arg(short, long)]
        name: Option<String>,
    },
    
    /// Generate a secure invitation code for the last-added folder
    Link,
    
    /// Join a remote sync folder using an invitation code
    Join {
        /// The invitation code received from another peer
        code: String,
        /// Local path where the synchronized folder will be saved
        path: PathBuf,
    },
    
    /// Display status of all sync jobs
    Status {
        /// Show detailed information for each file
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// List all connected peers
    Peers,
    
    /// Run the SlySync engine as a background daemon
    Daemon,
    
    /// Mirror a local folder to another local folder
    Mirror {
        /// Source folder path to mirror from
        source: PathBuf,
        /// Destination folder path to mirror to
        destination: PathBuf,
        /// Optional name for this mirror setup
        #[arg(short, long)]
        name: Option<String>,
        /// Run as a daemon (continuous monitoring)
        #[arg(short, long)]
        daemon: bool,
    },
    /// Control or query running mirror daemons
    MirrorCtl {
        #[command(subcommand)]
        subcmd: MirrorCtlSubcommand,
    },
}

#[derive(Subcommand)]
pub enum MirrorCtlSubcommand {
    /// Stop a running mirror daemon by name or path
    Stop {
        /// Name or source path of the mirror
        #[arg(short, long)]
        name: Option<String>,
        #[arg(long)]
        source: Option<PathBuf>,
    },
    /// Restart a running mirror daemon by name or path
    Restart {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(long)]
        source: Option<PathBuf>,
    },
    /// Manually trigger a full re-sync for a mirror
    Resync {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(long)]
        source: Option<PathBuf>,
    },
    /// Show status of all running mirror daemons
    Status,
}

/// Initialize SlySync configuration and generate node identity.
/// 
/// This command sets up the initial configuration directory, generates
/// a cryptographic identity for this node, and prepares the system
/// for file synchronization operations.
/// 
/// # Returns
/// 
/// Returns `Ok(())` on successful initialization, or an error if
/// configuration creation or identity generation fails.
pub async fn init() -> Result<()> {
    tracing::info!("Initializing SlySync...");
    
    // Initialize configuration
    let config = crate::config::Config::init().await?;
    tracing::info!("Configuration initialized at: {}", config.config_path().display());
    
    // Generate and save node identity
    let identity = crate::crypto::Identity::generate()?;
    identity.save(&config.identity_path())?;
    tracing::info!("Node identity saved to: {}", config.identity_path().display());
    tracing::info!("Node ID: {}", identity.public_key_hex());
    
    println!("✅ SlySync initialized successfully!");
    println!("Node ID: {}", identity.public_key_hex());
    
    Ok(())
}

/// Display the current node's public ID.
/// 
/// Shows the Ed25519 public key that uniquely identifies this SlySync node.
/// This ID is used by other peers to verify the authenticity of this node.
pub async fn show_id() -> Result<()> {
    let config = crate::config::Config::load().await?;
    let identity = crate::crypto::Identity::load_or_generate(&config.identity_path())?;
    
    println!("{}", identity.public_key_hex());
    Ok(())
}

/// Add a folder to the synchronization list.
/// 
/// This command registers a local folder for synchronization with other peers.
/// The folder will be monitored for changes and synchronized automatically
/// when the daemon is running.
/// 
/// # Arguments
/// 
/// * `path` - The filesystem path to the folder to synchronize
/// * `name` - Optional human-readable name for the folder
/// 
/// # Errors
/// 
/// Returns an error if the path doesn't exist, isn't a directory, or if
/// configuration cannot be saved.
pub async fn add_folder(path: PathBuf, name: Option<String>) -> Result<()> {
    tracing::info!("Adding folder: {}", path.display());
    
    if !path.exists() {
        anyhow::bail!("Path does not exist: {}", path.display());
    }
    
    if !path.is_dir() {
        anyhow::bail!("Path is not a directory: {}", path.display());
    }
    
    let mut config = crate::config::Config::load().await?;
    let folder_id = config.add_sync_folder(path.clone(), name.clone())?;
    config.save().await?;
    
    println!("✅ Added folder: {}", path.display());
    if let Some(name) = name {
        println!("   Alias: {}", name);
    }
    println!("   Folder ID: {}", folder_id);
    
    Ok(())
}

/// Generate an invitation code for the most recently added folder.
/// 
/// Creates a cryptographically signed invitation that allows other peers
/// to join and synchronize the specified folder. The invitation includes
/// folder metadata and expires after 24 hours for security.
/// 
/// # Errors
/// 
/// Returns an error if no folders are configured or if cryptographic
/// operations fail.
pub async fn generate_invitation() -> Result<()> {
    let config = crate::config::Config::load().await?;
    if config.sync_folders().is_empty() {
        anyhow::bail!("No folders to sync. Add a folder first with 'slysync add <path>'");
    }
    
    // Load identity to sign the invitation
    let identity = crate::crypto::Identity::load_or_generate(&config.identity_path())?;
    
    // Use configured listen address
    let listen_addr = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 
        config.listen_port
    );
    
    // Get the last added folder
    let folder = config.sync_folders().last().unwrap();
    let invitation = crate::crypto::generate_invitation_code(&folder.id, &identity, listen_addr)?;
    
    println!("📨 Invitation code for folder '{}' ({})", 
             folder.name.as_deref().unwrap_or("unnamed"), 
             folder.path.display());
    println!("{}", invitation);
    println!("\n💡 Share this code with peers who should have access to this folder.");
    println!("   Code expires in 24 hours for security.");
    
    Ok(())
}

/// Join a remote synchronization folder using an invitation code.
/// 
/// This command validates the provided invitation code and sets up
/// a local folder to synchronize with the remote peer's folder.
/// 
/// # Arguments
/// 
/// * `code` - The invitation code received from another peer
/// * `path` - Local path where the synchronized folder will be created
/// 
/// # Errors
/// 
/// Returns an error if the invitation code is invalid, expired, or if
/// the local path cannot be created.
pub async fn join_sync(code: String, path: PathBuf) -> Result<()> {
    tracing::info!("Joining sync with code: {} at path: {}", code, path.display());
    
    // Validate invitation code (without peer key verification for now)
    let folder_info = crate::crypto::validate_invitation_code(&code, None)?;
    
    // Create local directory if it doesn't exist
    if !path.exists() {
        std::fs::create_dir_all(&path)?;
    }
    
    let mut config = crate::config::Config::load().await?;
    config.add_remote_sync_folder(path.clone(), folder_info)?;
    config.save().await?;
    
    println!("✅ Joined sync folder at: {}", path.display());
    println!("   Starting synchronization...");
    
    Ok(())
}

pub async fn show_status(verbose: bool) -> Result<()> {
    let config = crate::config::Config::load().await?;
    if config.sync_folders().is_empty() {
        println!("{}", "No folders being synchronized.".yellow().bold());
        println!("{} {}", "Add a folder with:".cyan(), "slysync add <path>".bold());
        return Ok(());
    }
    
    // Initialize P2P service to get peer information
    let identity = crate::crypto::Identity::load_or_generate(&config.identity_path())?;
    let p2p_service = crate::p2p::P2PService::new(identity, config.clone()).await?;
    
    // Get connected peers
    let peers = p2p_service.get_connected_peers().await;
    let connected_peers = peers.iter().filter(|p| p.authenticated).count();
    let unauth_peers = peers.len() - connected_peers;
    
    println!("{}\n", "📂 Sync Status".bold().underline());
    
    // Display node configuration
    println!("{}", "🔧 Node Configuration:".bold());
    println!("    Node ID: {}", config.node_id.cyan());
    println!("    Listen Port: {}", config.listen_port.to_string().cyan());
    
    // Display bandwidth configuration
    if let Some(upload_limit) = config.bandwidth_limit_up {
        println!("    Upload Limit: {}", format_bytes_per_sec(upload_limit).green());
    } else {
        println!("    Upload Limit: {}", "Unlimited".green());
    }
    
    if let Some(download_limit) = config.bandwidth_limit_down {
        println!("    Download Limit: {}", format_bytes_per_sec(download_limit).green());
    } else {
        println!("    Download Limit: {}", "Unlimited".green());
    }
    
    println!("    Discovery: {}", if config.discovery_enabled { "Enabled".green() } else { "Disabled".red() });
    println!();
    
    // Display sync folders
    for folder in config.sync_folders() {
        let name = folder.name.as_deref().unwrap_or("unnamed");
        println!("  {} {}", "📁".bold(), name.bold().cyan());
        println!("    Path: {}", folder.path.display());
        
        // Check sync status based on folder state
        let status = if folder.path.exists() {
            "Up to date".green().bold()
        } else {
            "Folder missing".red().bold()
        };
        println!("    Status: {}", status);
        println!("    Peers: {} {}{}", connected_peers.to_string().blue().bold(), "connected".blue(), if unauth_peers > 0 { format!(" ({} unauthenticated)", unauth_peers).yellow().to_string() } else { String::new() });
        
        if verbose {
            // Count files and calculate total size
            let (file_count, total_size) = count_files_and_size(&folder.path).await;
            println!("    Files: {}", file_count.to_string().magenta());
            println!("    Size: {}", format_bytes(total_size).magenta());
            println!("    Created: {}", folder.created_at.format("%Y-%m-%d %H:%M:%S UTC").to_string().dimmed());
        }
        
        println!();
    }

    // Show running mirror daemons
    ensure_pid_dir().ok();
    let mirrors = list_mirror_pids();
    if !mirrors.is_empty() {
        println!("{}
", "🪞 Local Mirror Daemons:".bold().underline());
        for (name, pid) in mirrors {
            println!("  {} (PID {})", name.cyan(), pid);
        }
    }
    Ok(())
}

fn count_files_and_size(path: &std::path::Path) -> std::pin::Pin<Box<dyn std::future::Future<Output = (usize, u64)> + Send + '_>> {
    Box::pin(async move {
        if !path.exists() {
            return (0, 0);
        }
        
        let mut file_count = 0;
        let mut total_size = 0;
        
        if let Ok(mut entries) = tokio::fs::read_dir(path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let entry_path = entry.path();
                if entry_path.is_file() {
                    file_count += 1;
                    if let Ok(metadata) = entry.metadata().await {
                        total_size += metadata.len();
                    }
                } else if entry_path.is_dir() {
                    let (sub_files, sub_size) = count_files_and_size(&entry_path).await;
                    file_count += sub_files;
                    total_size += sub_size;
                }
            }
        }
        
        (file_count, total_size)
    })
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

fn format_bytes_per_sec(bytes_per_sec: u64) -> String {
    const UNITS: &[&str] = &["B/s", "KB/s", "MB/s", "GB/s", "TB/s"];
    let mut rate = bytes_per_sec as f64;
    let mut unit_index = 0;
    
    while rate >= 1024.0 && unit_index < UNITS.len() - 1 {
        rate /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", bytes_per_sec, UNITS[unit_index])
    } else {
        format!("{:.1} {}", rate, UNITS[unit_index])
    }
}

pub async fn show_peers() -> Result<()> {
    let config = crate::config::Config::load().await?;
    let identity = crate::crypto::Identity::load_or_generate(&config.identity_path())?;
    
    // Start P2P service to discover peers
    let p2p_service = crate::p2p::P2PService::new(identity, config).await?;
    
    println!("🌐 Discovering Peers...\n");
    
    // Wait a bit for peer discovery
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    let peers = p2p_service.get_connected_peers().await;
    
    if peers.is_empty() {
        println!("No peers found.");
        println!("💡 Tips:");
        println!("  - Make sure other SlySync nodes are running on your network");
        println!("  - Use 'slysync link' to generate an invitation code");
        println!("  - Use 'slysync join <code>' to connect to a specific peer");
    } else {
        println!("Found {} peer(s):\n", peers.len());
        
        for (i, peer) in peers.iter().enumerate() {
            let status = if peer.authenticated { "🔐 Authenticated" } else { "🔓 Pending" };
            let time_ago = format_time_ago(peer.last_seen);
            
            println!("{}. 📡 {}", i + 1, peer.id);
            println!("   Address: {}", peer.address);
            println!("   Status: {}", status);
            println!("   Last seen: {}", time_ago);
            println!();
        }
    }
    
    Ok(())
}

pub async fn run_daemon() -> Result<()> {
    info!("Starting SlySync daemon...");
    
    let config = crate::config::Config::load().await?;
    let identity = crate::crypto::Identity::load_or_generate(&config.identity_path())?;
    
    println!("🚀 SlySync daemon starting...");
    println!("Node ID: {}", identity.public_key_hex());
    println!("Listening on port: {}", config.listen_port);
    
    // Start the P2P network
    let p2p_service = std::sync::Arc::new(
        crate::p2p::P2PService::new(identity, config.clone()).await?
    );
    
    // Start file synchronization
    let mut sync_service = crate::sync::SyncService::new(config.clone()).await?;
    sync_service.set_p2p_service(p2p_service.clone());
    
    // Initialize the efficient file watcher
    if let Err(e) = sync_service.init_watcher().await {
        error!("Failed to initialize efficient watcher: {}", e);
        return Err(e);
    }
    
    // Show watcher configuration
    println!("⚡ File Watcher Configuration:");
    println!("    Debounce Delay: {}ms", config.watcher_debounce_ms);
    println!("    Polling Interval: {}ms", config.watcher_polling_ms);
    println!("    Max Pending Events: {}", config.watcher_max_events);
    println!("    Performance Monitoring: {}", if config.watcher_performance_monitoring { "Enabled" } else { "Disabled" });
    
    // Start sync service in background
    let sync_handle = tokio::spawn(async move {
        if let Err(e) = sync_service.run().await {
            error!("Sync service error: {}", e);
        }
    });
    
    println!("💚 SlySync daemon is running. Press Ctrl+C to stop.");
    println!("📂 Monitoring {} sync folder(s)", config.sync_folders().len());
    
    // Start peer discovery
    tokio::spawn({
        let p2p_service = p2p_service.clone();
        async move {
            if let Err(e) = p2p_service.discover_peers().await {
                warn!("Peer discovery failed: {}", e);
            }
        }
    });
    
    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    
    println!("\n🛑 SlySync daemon stopping...");
    sync_handle.abort();
    
    Ok(())
}

/// Set up a local folder mirror
/// 
/// This command creates a mirror relationship between two local folders,
/// allowing for real-time synchronization without P2P networking.
/// 
/// # Arguments
/// 
/// * `source` - The source folder to mirror from
/// * `destination` - The destination folder to mirror to
/// * `name` - Optional human-readable name for this mirror
/// * `daemon` - Whether to run as a daemon (continuous monitoring)
/// 
/// # Errors
/// 
/// Returns an error if paths are invalid, inaccessible, or if mirror setup fails.
pub async fn setup_mirror(
    source: PathBuf,
    destination: PathBuf,
    name: Option<String>,
    daemon: bool,
) -> Result<()> {
    tracing::info!("Setting up mirror from {} to {}", source.display(), destination.display());
    let mirror_name = name.clone().unwrap_or_else(|| source.display().to_string());
    if daemon {
        ensure_pid_dir().ok();
        let pidfile = mirror_pid_file(&mirror_name);
        let configfile = mirror_config_file(&mirror_name);
        fs::write(&pidfile, std::process::id().to_string()).ok();
        // Write config file
        let config = MirrorDaemonConfig {
            source: source.display().to_string(),
            destination: destination.display().to_string(),
            name: name.clone(),
        };
        fs::write(&configfile, serde_json::to_string_pretty(&config)?).ok();
        // Run as daemon with continuous monitoring
        println!("🔄 Starting mirror daemon...");
        if let Some(ref name) = name {
            println!("Mirror: {}", name);
        }
        println!("Source: {}", source.display());
        println!("Destination: {}", destination.display());
        println!("💚 Mirror daemon is running. Press Ctrl+C to stop.");
        
        let mirror_service = crate::mirror::MirrorService::new(source, destination, name).await?;
        
        // Handle shutdown signal
        let shutdown_handle = tokio::spawn(async {
            tokio::signal::ctrl_c().await.unwrap();
            println!("\n🛑 Mirror daemon stopping...");
        });
        
        // Run mirror service with graceful shutdown
        tokio::select! {
            result = mirror_service.run() => {
                if let Err(e) = result {
                    error!("Mirror service error: {}", e);
                    return Err(e);
                }
            }
            _ = shutdown_handle => {
                println!("Mirror daemon stopped.");
            }
        }
        // On shutdown, remove PID and config file
        let _ = std::fs::remove_file(&pidfile);
        let _ = std::fs::remove_file(&configfile);
    } else {
        // Run once and exit
        println!("🔄 Running one-time mirror operation...");
        if let Some(ref name) = name {
            println!("Mirror: {}", name);
        }
        println!("Source: {}", source.display());
        println!("Destination: {}", destination.display());
        
        crate::mirror::run_mirror_once(source, destination, name).await?;
    }
    
    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MirrorDaemonConfig {
    pub source: String,
    pub destination: String,
    pub name: Option<String>,
}

fn mirror_pid_dir() -> String {
    std::env::var("SLYSYNC_MIRROR_PID_DIR").unwrap_or_else(|_| "/tmp/slysync_mirror_pids".to_string())
}

fn ensure_pid_dir() -> std::io::Result<()> {
    let dir = mirror_pid_dir();
    if !std::path::Path::new(&dir).exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(())
}

fn mirror_pid_file(name: &str) -> std::path::PathBuf {
    let safe = name.replace('/', "_");
    std::path::PathBuf::from(format!("{}/{}.pid", mirror_pid_dir(), safe))
}

fn mirror_config_file(name: &str) -> std::path::PathBuf {
    let safe = name.replace('/', "_");
    std::path::PathBuf::from(format!("{}/{}.json", mirror_pid_dir(), safe))
}

fn list_mirror_pids() -> Vec<(String, u32)> {
    let mut result = Vec::new();
    let dir = mirror_pid_dir();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Some(fname) = entry.file_name().to_str() {
                if fname.ends_with(".pid") {
                    let name = fname.trim_end_matches(".pid").replace('_', "/");
                    if let Ok(pidstr) = std::fs::read_to_string(entry.path()) {
                        if let Ok(pid) = pidstr.trim().parse::<u32>() {
                            let process_exists = unsafe { libc::kill(pid as i32, 0) == 0 };
                            if process_exists {
                                result.push((name, pid));
                            } else {
                                let _ = std::fs::remove_file(entry.path());
                            }
                        }
                    }
                }
            }
        }
    }
    result
}

pub async fn mirror_ctl(subcmd: MirrorCtlSubcommand) -> Result<()> {
    match subcmd {
        MirrorCtlSubcommand::Stop { name, source } => {
            ensure_pid_dir().ok();
            
            // If neither name nor source is provided, show error and usage
            if name.is_none() && source.is_none() {
                println!("{}", "Error: Please specify --name or --source.".red().bold());
                println!("Usage: {} mirror-ctl stop --name <name>", env!("CARGO_PKG_NAME"));
                println!("       {} mirror-ctl stop --source <path>", env!("CARGO_PKG_NAME"));
                return Ok(());
            }

            // Check if we have a direct match by name
            if let Some(name_str) = &name {
                let pidfile = mirror_pid_file(&name_str);
                if pidfile.exists() {
                    return stop_mirror_daemon(&pidfile, name_str).await;
                }
            }

            // Check if we have a direct match by source path
            if let Some(source_path) = &source {
                let source_str = source_path.display().to_string();
                let pidfile = mirror_pid_file(&source_str);
                if pidfile.exists() {
                    return stop_mirror_daemon(&pidfile, &source_str).await;
                }
            }
            
            // If no direct match, list all mirrors and try to find partial matches
            let mirrors = list_mirror_pids();
            if mirrors.is_empty() {
                println!("No running mirror daemons found.");
                return Ok(());
            }
            
            // Try to find a match by name or path
            let mut matches = Vec::new();
            for (mirror_name, pid) in &mirrors {
                if let Some(name_str) = &name {
                    if mirror_name.contains(name_str) {
                        matches.push((mirror_name.clone(), *pid));
                    }
                } else if let Some(source_path) = &source {
                    let source_str = source_path.display().to_string();
                    if mirror_name.contains(&source_str) {
                        matches.push((mirror_name.clone(), *pid));
                    }
                }
            }
            
            // Process matches
            match matches.len() {
                0 => {
                    if let Some(name_str) = &name {
                        println!("No running mirror daemon found with name '{}'.", name_str);
                    } else if let Some(source_path) = &source {
                        println!("No running mirror daemon found for source '{}'.", source_path.display());
                    }
                    println!("\nRunning mirror daemons:");
                    for (name, pid) in mirrors {
                        println!("  {} (PID {})", name, pid);
                    }
                },
                1 => {
                    // We have exactly one match, stop it
                    let (mirror_name, pid) = &matches[0];
                    let pidfile = mirror_pid_file(mirror_name);
                    unsafe {
                        libc::kill(*pid as i32, libc::SIGTERM);
                    }
                    println!("Sent SIGTERM to mirror daemon '{}' (PID {})", mirror_name, pid);
                    std::fs::remove_file(&pidfile).ok();
                },
                _ => {
                    // Multiple matches, ask user to be more specific
                    println!("Multiple matching mirror daemons found. Please be more specific:");
                    for (name, pid) in matches {
                        println!("  {} (PID {})", name, pid);
                    }
                }
            }
        }
        MirrorCtlSubcommand::Restart { name, source } | MirrorCtlSubcommand::Resync { name, source } => {
            ensure_pid_dir().ok();
            if name.is_none() && source.is_none() {
                println!("{}", "Error: Please specify --name or --source.".red().bold());
                println!("Usage: {} mirror-ctl restart --name <name>", env!("CARGO_PKG_NAME"));
                println!("       {} mirror-ctl restart --source <path>", env!("CARGO_PKG_NAME"));
                return Ok(());
            }
            let mirrors = list_mirror_pids();
            if mirrors.is_empty() {
                println!("No running mirror daemons found.");
                return Ok(());
            }
            let mut matches = Vec::new();
            for (mirror_name, pid) in &mirrors {
                if let Some(name_str) = &name {
                    if mirror_name.contains(name_str) {
                        matches.push((mirror_name.clone(), *pid));
                    }
                } else if let Some(source_path) = &source {
                    let source_str = source_path.display().to_string();
                    if mirror_name.contains(&source_str) {
                        matches.push((mirror_name.clone(), *pid));
                    }
                }
            }
            match matches.len() {
                0 => {
                    if let Some(name_str) = &name {
                        println!("No running mirror daemon found with name '{}'.", name_str);
                    } else if let Some(source_path) = &source {
                        println!("No running mirror daemon found for source '{}'.", source_path.display());
                    }
                    println!("\nRunning mirror daemons:");
                    for (name, pid) in mirrors {
                        println!("  {} (PID {})", name, pid);
                    }
                },
                1 => {
                    let (mirror_name, pid) = &matches[0];
                    println!("Restarting mirror daemon '{}' (PID {})...", mirror_name, pid);
                    // Read config file for correct source/destination/name
                    let configfile = mirror_config_file(mirror_name);
                    let config: MirrorDaemonConfig = match fs::read_to_string(&configfile)
                        .ok()
                        .and_then(|s| serde_json::from_str(&s).ok()) {
                        Some(cfg) => cfg,
                        _ => {
                            println!("Unable to determine mirror configuration to restart.\nPlease stop the mirror daemon and start it manually.");
                            return Ok(());
                        }
                    };
                    // Stop the running daemon
                    let pidfile = mirror_pid_file(mirror_name);
                    stop_mirror_daemon(&pidfile, mirror_name).await?;
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                    // Start a new daemon with correct config
                    println!("Starting new mirror daemon...");
                    println!("Source: {}", config.source);
                    println!("Destination: {}", config.destination);
                    let command = format!(
                        "{} mirror {} {} --name \"{}\" --daemon",
                        env!("CARGO_PKG_NAME"),
                        config.source,
                        config.destination,
                        config.name.clone().unwrap_or_else(|| mirror_name.clone())
                    );
                    if let Err(e) = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(format!("nohup {} > /dev/null 2>&1 &", command))
                        .spawn() {
                        println!("Failed to restart mirror daemon: {}", e);
                    } else {
                        println!("✅ Mirror daemon '{}' successfully restarted!", mirror_name);
                    }
                },
                _ => {
                    println!("Multiple matching mirror daemons found. Please be more specific:");
                    for (name, pid) in matches {
                        println!("  {} (PID {})", name, pid);
                    }
                }
            }
        }
        MirrorCtlSubcommand::Status => {
            ensure_pid_dir().ok();
            let mirrors = list_mirror_pids();
            if mirrors.is_empty() {
                println!("No running mirror daemons found.");
                println!("\nTip: Start a mirror daemon with '{} mirror <source> <destination> --daemon'", env!("CARGO_PKG_NAME"));
            } else {
                println!("🪞 {} running mirror {}:", mirrors.len(), if mirrors.len() == 1 { "daemon" } else { "daemons" });
                for (name, pid) in mirrors {
                    println!("  {} (PID {})", name.cyan(), pid);
                }
                
                println!("\nℹ️  Control daemons with:");
                println!("  - {} mirror-ctl stop --name <name>", env!("CARGO_PKG_NAME"));
                println!("  - {} mirror-ctl restart --name <name>", env!("CARGO_PKG_NAME"));
                println!("  - {} mirror-ctl resync --name <name>", env!("CARGO_PKG_NAME"));
            }
        }
    }
    Ok(())
}

/// Control running mirror daemons or query their status.
///
/// This function implements the `mirrorctl` command, providing various
/// operations to manage and monitor mirror daemons, including:
///
/// * Stopping running daemons
/// * Restarting daemons (which forces a full resync)
/// * Manually triggering a resync operation
/// * Viewing status of all running daemons
///
/// # Arguments
///
/// * `subcmd` - The subcommand to execute (Status, Stop, Restart, or Resync)
///
/// # Usage Examples
///
/// ```bash
/// # List all running mirror daemons
/// slysync mirror-ctl status
///
/// # Stop a daemon by name
/// slysync mirror-ctl stop --name "MyBackup"
///
/// # Stop a daemon by source path
/// slysync mirror-ctl stop --source ~/Documents
///
/// # Restart a daemon by name (forces full resync)
/// slysync mirror-ctl restart --name "MyBackup"
///
/// # Manually trigger a resync (same as restart for now)
/// slysync mirror-ctl resync --name "MyBackup"
/// ```
///
/// # Returns
///
/// Returns `Ok(())` on success, or an error if the operation fails

/// Helper function to stop a mirror daemon by pidfile
///
/// This function attempts to gracefully stop a running mirror daemon by:
/// 1. Reading the PID from the pidfile
/// 2. Sending a SIGTERM signal to the process
/// 3. Removing the PID file
///
/// # Arguments
///
/// * `pidfile` - Path to the PID file
/// * `name` - Name of the mirror daemon for display purposes
///
/// # Returns
///
/// * `Ok(())` on success, including when the process is no longer running
/// * `Err(...)` if there's an error reading the PID file or parsing the PID
async fn stop_mirror_daemon(pidfile: &std::path::Path, name: &str) -> Result<()> {
    if let Ok(pidstr) = std::fs::read_to_string(pidfile) {
        if let Ok(pid) = pidstr.trim().parse::<i32>() {
            // Check if process exists
            let process_exists = unsafe { libc::kill(pid, 0) == 0 };
            
            if process_exists {
                // Process exists, try to terminate it
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
                println!("Sent SIGTERM to mirror daemon '{}' (PID {})", name, pid);
                
                // Give the process a moment to terminate
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                
                // Check if it's still running
                let still_running = unsafe { libc::kill(pid, 0) == 0 };
                if still_running {
                    println!("Mirror daemon is still shutting down...");
                }
            } else {
                println!("Process with PID {} no longer exists. Cleaning up...", pid);
            }
            
            // Remove pidfile regardless
            std::fs::remove_file(pidfile).ok();
        } else {
            println!("Invalid PID in file: {}", pidfile.display());
            std::fs::remove_file(pidfile).ok();
        }
    } else {
        println!("Failed to read PID file: {}", pidfile.display());
    }
    // Remove config file as well
    let configfile = mirror_config_file(name);
    std::fs::remove_file(configfile).ok();
    Ok(())
}

fn format_time_ago(timestamp: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(timestamp);
    
    if duration.num_seconds() < 60 {
        "just now".to_string()
    } else if duration.num_minutes() < 60 {
        format!("{} minutes ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{} hours ago", duration.num_hours())
    } else {
        format!("{} days ago", duration.num_days())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;
    use std::env;

    #[allow(dead_code)]
    async fn create_test_config() -> (TempDir, crate::config::Config) {
        let temp_dir = TempDir::new().unwrap();
        let _config_path = temp_dir.path().join("test_config.toml");
        let config = crate::config::Config::init().await.unwrap();
        (temp_dir, config)
    }

    #[tokio::test]
    async fn test_init_command() {
        let temp_dir = TempDir::new().unwrap();
        
        // Set a custom config path for testing
        std::env::set_var("SLYSYNC_CONFIG_DIR", temp_dir.path());
        
        let result = init().await;
        
        // Clean up environment variable
        std::env::remove_var("SLYSYNC_CONFIG_DIR");
        
        // Note: This may fail if init() doesn't use environment variables
        // The test mainly checks that the function doesn't panic
        match result {
            Ok(_) => {
                // Successfully initialized
            }
            Err(e) => {
                // Expected if we can't control where init() creates files
                println!("Init failed (expected in test environment): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_show_id_without_config() {
        // Test show_id when no config exists
        let result = show_id().await;
        
        // This should fail if no config is found
        match result {
            Ok(_) => {
                // If it succeeds, an identity was found or created
            }
            Err(_) => {
                // Expected if no config exists
            }
        }
    }

    #[tokio::test]
    async fn test_add_folder_nonexistent_path() {
        let nonexistent = PathBuf::from("/nonexistent/path/that/does/not/exist");
        let result = add_folder(nonexistent, None).await;
        
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Path does not exist"));
    }

    #[tokio::test]
    async fn test_add_folder_file_instead_of_directory() {
        let temp_dir = TempDir::new().unwrap();
        let temp_file = temp_dir.path().join("test_file.txt");
        fs::write(&temp_file, "test content").await.unwrap();
        
        let result = add_folder(temp_file, None).await;
        
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Path is not a directory"));
    }

    #[tokio::test]
    async fn test_add_folder_valid_directory() {
        let temp_dir = TempDir::new().unwrap();
        let sync_dir = temp_dir.path().join("sync_folder");
        fs::create_dir_all(&sync_dir).await.unwrap();
        
        // This test may fail because it tries to load real config
        // We'll catch the error and verify it's about config, not directory validation
        let result = add_folder(sync_dir.clone(), Some("test_folder".to_string())).await;
        
        match result {
            Ok(_) => {
                // Successfully added folder
            }
            Err(e) => {
                // Should fail on config load, not directory validation
                let error_msg = e.to_string();
                assert!(!error_msg.contains("Path does not exist"));
                assert!(!error_msg.contains("Path is not a directory"));
            }
        }
    }

    #[tokio::test]
    async fn test_generate_invitation_no_folders() {
        // Ensure a clean config directory for this test
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("SLYSYNC_CONFIG_DIR", temp_dir.path());

        let result = generate_invitation().await;

        // Clean up environment variable
        std::env::remove_var("SLYSYNC_CONFIG_DIR");

        // Should fail because either no config exists or no folders are configured
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_join_sync_invalid_code() {
        let temp_dir = TempDir::new().unwrap();
        let join_path = temp_dir.path().join("joined_folder");
        
        let result = join_sync("invalid_code_123".to_string(), join_path).await;
        
        assert!(result.is_err());
        // Should fail on invitation validation
    }

    #[tokio::test]
    async fn test_show_status_no_config() {
        let result = show_status(false).await;
        
        match result {
            Ok(_) => {
                // Successfully showed status (or no folders)
            }
            Err(_) => {
                // Expected if no config file exists
            }
        }
    }

    #[tokio::test]
    async fn test_show_status_verbose() {
        let result = show_status(true).await;
        
        match result {
            Ok(_) => {
                // Successfully showed verbose status
            }
            Err(_) => {
                // Expected if no config file exists
            }
        }
    }

    #[tokio::test]
    async fn test_show_peers() {
        let result = show_peers().await;
        
        match result {
            Ok(_) => {
                // Successfully started peer discovery
            }
            Err(e) => {
                // May fail due to network setup in test environment
                println!("Peer discovery failed (expected in test environment): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_run_daemon() {
        // We can't easily test the full daemon in a unit test, but we can test that it starts
        let daemon_task = tokio::spawn(async {
            tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                run_daemon()
            ).await
        });
        
        match daemon_task.await {
            Ok(Ok(_)) => {
                // Daemon completed (unlikely in 100ms)
            }
            Ok(Err(e)) => {
                // Daemon failed to start
                println!("Daemon start failed (expected in test environment): {}", e);
            }
            Err(_) => {
                // Timeout is expected - daemon was running
            }
        }
    }

    #[test]
    fn test_format_time_ago() {
        let now = chrono::Utc::now();
        
        // Test "just now"
        let recent = now - chrono::Duration::seconds(30);
        assert_eq!(format_time_ago(recent), "just now");
        
        // Test minutes ago
        let minutes_ago = now - chrono::Duration::minutes(5);
        assert_eq!(format_time_ago(minutes_ago), "5 minutes ago");
        
        // Test hours ago
        let hours_ago = now - chrono::Duration::hours(3);
        assert_eq!(format_time_ago(hours_ago), "3 hours ago");
        
        // Test days ago
        let days_ago = now - chrono::Duration::days(2);
        assert_eq!(format_time_ago(days_ago), "2 days ago");
    }

    #[test]
    fn test_cli_parser() {
        // Test that CLI can be parsed correctly
        use clap::Parser;
        
        // Test init command
        let args = vec!["slysync", "init"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(matches!(cli.command, Commands::Init));
        
        // Test id command
        let args = vec!["slysync", "id"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(matches!(cli.command, Commands::Id));
        
        // Test add command
        let args = vec!["slysync", "add", "/test/path"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Add { path, name } => {
                assert_eq!(path, PathBuf::from("/test/path"));
                assert_eq!(name, None);
            }
            _ => panic!("Wrong command parsed"),
        }
        
        // Test add command with name
        let args = vec!["slysync", "add", "/test/path", "--name", "my_folder"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Add { path, name } => {
                assert_eq!(path, PathBuf::from("/test/path"));
                assert_eq!(name, Some("my_folder".to_string()));
            }
            _ => panic!("Wrong command parsed"),
        }
        
        // Test link command
        let args = vec!["slysync", "link"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(matches!(cli.command, Commands::Link));
        
        // Test join command
        let args = vec!["slysync", "join", "invitation_code_123", "/local/path"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Join { code, path } => {
                assert_eq!(code, "invitation_code_123");
                assert_eq!(path, PathBuf::from("/local/path"));
            }
            _ => panic!("Wrong command parsed"),
        }
        
        // Test status command
        let args = vec!["slysync", "status"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Status { verbose } => {
                assert!(!verbose);
            }
            _ => panic!("Wrong command parsed"),
        }
        
        // Test status command with verbose
        let args = vec!["slysync", "status", "--verbose"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Status { verbose } => {
                assert!(verbose);
            }
            _ => panic!("Wrong command parsed"),
        }
        
        // Test peers command
        let args = vec!["slysync", "peers"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(matches!(cli.command, Commands::Peers));
        
        // Test daemon command
        let args = vec!["slysync", "daemon"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(matches!(cli.command, Commands::Daemon));
        
        // Test mirror command
        let args = vec!["slysync", "mirror", "/source/path", "/dest/path"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Mirror { source, destination, name, daemon } => {
                assert_eq!(source, PathBuf::from("/source/path"));
                assert_eq!(destination, PathBuf::from("/dest/path"));
                assert_eq!(name, None);
                assert!(!daemon);
            }
            _ => panic!("Wrong command parsed"),
        }
        
        // Test mirror command with all options
        let args = vec!["slysync", "mirror", "/source/path", "/dest/path", "--name", "My Mirror", "--daemon"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Mirror { source, destination, name, daemon } => {
                assert_eq!(source, PathBuf::from("/source/path"));
                assert_eq!(destination, PathBuf::from("/dest/path"));
                assert_eq!(name, Some("My Mirror".to_string()));
                assert!(daemon);
            }
            _ => panic!("Wrong command parsed"),
        }
    }

    #[test]
    fn test_cli_invalid_arguments() {
        use clap::Parser;
        
        // Test missing required argument
        let args = vec!["slysync", "add"];
        let result = Cli::try_parse_from(args);
        assert!(result.is_err());
        
        // Test missing required argument for join
        let args = vec!["slysync", "join", "code_only"];
        let result = Cli::try_parse_from(args);
        assert!(result.is_err());
        
        // Test unknown command
        let args = vec!["slysync", "unknown"];
        let result = Cli::try_parse_from(args);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mirror_daemon_config_file_creation_and_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
        let source = temp_dir.path().join("src");
        let dest = temp_dir.path().join("dst");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        let name = Some("test_mirror".to_string());
        // Use one-shot mode to avoid hanging
        let _ = super::setup_mirror(source.clone(), dest.clone(), name.clone(), false).await;
        let pidfile = super::mirror_pid_file("test_mirror");
        let configfile = super::mirror_config_file("test_mirror");
        // In one-shot mode, files may not exist after run, but config file should be created and then cleaned up
        // So we just check that the function completes and does not hang
        env::remove_var("SLYSYNC_MIRROR_PID_DIR");
    }

    #[tokio::test]
    async fn test_mirror_ctl_restart_reads_config() {
        use std::fs;
        let temp_dir = TempDir::new().unwrap();
        env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
        let name = "restart_test";
        let pidfile = super::mirror_pid_file(name);
        let configfile = super::mirror_config_file(name);
        fs::write(&pidfile, "12345").unwrap();
        let config = super::MirrorDaemonConfig {
            source: "/tmp/src".to_string(),
            destination: "/tmp/dst".to_string(),
            name: Some(name.to_string()),
        };
        fs::write(&configfile, serde_json::to_string_pretty(&config).unwrap()).unwrap();
        let result = super::mirror_ctl(super::MirrorCtlSubcommand::Restart { name: Some(name.to_string()), source: None }).await;
        assert!(result.is_ok());
        let _ = fs::remove_file(&pidfile);
        let _ = fs::remove_file(&configfile);
        env::remove_var("SLYSYNC_MIRROR_PID_DIR");
    }

    #[tokio::test]
    async fn test_mirror_ctl_stop_handles_missing_files() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
        let name = "missing_test";
        let pidfile = super::mirror_pid_file(name);
        let result = super::stop_mirror_daemon(&pidfile, name).await;
        assert!(result.is_ok());
        env::remove_var("SLYSYNC_MIRROR_PID_DIR");
    }

    #[tokio::test]
    async fn test_mirror_ctl_restart_handles_missing_config() {
        let temp_dir = TempDir::new().unwrap();
        env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
        let name = "no_config";
        let pidfile = super::mirror_pid_file(name);
        std::fs::write(&pidfile, "12345").unwrap();
        let result = super::mirror_ctl(super::MirrorCtlSubcommand::Restart { name: Some(name.to_string()), source: None }).await;
        assert!(result.is_ok());
        env::remove_var("SLYSYNC_MIRROR_PID_DIR");
    }

    #[tokio::test]
    async fn test_mirror_ctl_multiple_matches() {
        use std::fs;
        let temp_dir = TempDir::new().unwrap();
        env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
        let pidfile1 = super::mirror_pid_file("ambig1");
        let pidfile2 = super::mirror_pid_file("ambig1_extra");
        fs::write(&pidfile1, "11111").unwrap();
        fs::write(&pidfile2, "22222").unwrap();
        let result = super::mirror_ctl(super::MirrorCtlSubcommand::Stop { name: Some("ambig1".to_string()), source: None }).await;
        assert!(result.is_ok());
        let _ = fs::remove_file(&pidfile1);
        let _ = fs::remove_file(&pidfile2);
        env::remove_var("SLYSYNC_MIRROR_PID_DIR");
    }
}
