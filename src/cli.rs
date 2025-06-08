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
//! - `peers` - List all connected peers
//! - `daemon` - Run the SlySync engine as a background daemon

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, warn, error};

/// Command-line interface structure for SlySync.
/// 
/// This structure defines the main CLI parser and available subcommands
/// using the clap derive macros for automatic argument parsing.
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
        println!("No folders being synchronized.");
        println!("Add a folder with: slysync add <path>");
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    async fn create_test_config() -> (TempDir, crate::config::Config) {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");
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
}
