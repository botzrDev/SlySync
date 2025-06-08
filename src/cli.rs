use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, warn, error};

#[derive(Parser)]
#[command(name = "synccore")]
#[command(about = "A next-generation, peer-to-peer file synchronization CLI utility")]
#[command(version = "1.0.0")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize SyncCore configuration and generate node identity
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
    
    /// Run the SyncCore engine as a background daemon
    Daemon,
}

pub async fn init() -> Result<()> {
    tracing::info!("Initializing SyncCore...");
    
    // Initialize configuration
    let config = crate::config::Config::init().await?;
    tracing::info!("Configuration initialized at: {}", config.config_path().display());
    
    // Generate and save node identity
    let identity = crate::crypto::Identity::generate()?;
    identity.save(&config.identity_path())?;
    tracing::info!("Node identity saved to: {}", config.identity_path().display());
    tracing::info!("Node ID: {}", identity.public_key_hex());
    
    println!("✅ SyncCore initialized successfully!");
    println!("Node ID: {}", identity.public_key_hex());
    
    Ok(())
}

pub async fn show_id() -> Result<()> {
    let config = crate::config::Config::load().await?;
    let identity = crate::crypto::Identity::load_or_generate(&config.identity_path())?;
    
    println!("{}", identity.public_key_hex());
    Ok(())
}

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

pub async fn generate_invitation() -> Result<()> {
    let config = crate::config::Config::load().await?;
    
    if config.sync_folders().is_empty() {
        anyhow::bail!("No folders to sync. Add a folder first with 'synccore add <path>'");
    }
    
    // Get the last added folder
    let folder = config.sync_folders().last().unwrap();
    let invitation = crate::crypto::generate_invitation_code(&folder.id)?;
    
    println!("📨 Invitation code for folder '{}' ({})", 
             folder.name.as_deref().unwrap_or("unnamed"), 
             folder.path.display());
    println!("{}", invitation);
    println!("\n💡 Share this code with peers who should have access to this folder.");
    println!("   Code expires in 24 hours for security.");
    
    Ok(())
}

pub async fn join_sync(code: String, path: PathBuf) -> Result<()> {
    tracing::info!("Joining sync with code: {} at path: {}", code, path.display());
    
    // Validate invitation code
    let folder_info = crate::crypto::validate_invitation_code(&code)?;
    
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
        println!("No folders being synchronized.");
        println!("Add a folder with: synccore add <path>");
        return Ok(());
    }
    
    println!("📂 Sync Status\n");
    
    for folder in config.sync_folders() {
        let name = folder.name.as_deref().unwrap_or("unnamed");
        println!("  {} ({})", name, folder.path.display());
        println!("    Status: Up to date"); // TODO: Implement actual status checking
        println!("    Peers: 0 connected"); // TODO: Implement peer counting
        
        if verbose {
            println!("    Files: 0"); // TODO: Implement file counting
            println!("    Size: 0 bytes"); // TODO: Implement size calculation
        }
        
        println!();
    }
    
    Ok(())
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
        println!("  - Make sure other SyncCore nodes are running on your network");
        println!("  - Use 'synccore link' to generate an invitation code");
        println!("  - Use 'synccore join <code>' to connect to a specific peer");
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
    info!("Starting SyncCore daemon...");
    
    let config = crate::config::Config::load().await?;
    let identity = crate::crypto::Identity::load_or_generate(&config.identity_path())?;
    
    println!("🚀 SyncCore daemon starting...");
    println!("Node ID: {}", identity.public_key_hex());
    println!("Listening on port: {}", config.listen_port);
    
    // Start the P2P network
    let p2p_service = std::sync::Arc::new(
        crate::p2p::P2PService::new(identity, config.clone()).await?
    );
    
    // Start file synchronization
    let mut sync_service = crate::sync::SyncService::new(config.clone()).await?;
    sync_service.set_p2p_service(p2p_service.clone());
    
    // Start sync service in background
    let sync_handle = tokio::spawn(async move {
        if let Err(e) = sync_service.run().await {
            error!("Sync service error: {}", e);
        }
    });
    
    println!("💚 SyncCore daemon is running. Press Ctrl+C to stop.");
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
    
    println!("\n🛑 SyncCore daemon stopping...");
    sync_handle.abort();
    
    Ok(())
}

// Helper function to format time ago
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
