//! # Local Folder Mirroring Module
//! 
//! This module provides local folder mirroring capabilities, allowing users to
//! synchronize files between two local directories without P2P networking.
//! This is useful for backing up files to different drives or maintaining
//! mirrors of important directories.
//! 
//! ## Features
//! 
//! - **Real-time Monitoring**: Watches source directory for changes
//! - **Bidirectional Sync**: Optionally sync changes in both directions
//! - **Fast Copying**: Uses efficient file operations
//! - **Conflict Resolution**: Handles file conflicts with timestamps
//! - **Incremental Updates**: Only copies changed files
//! 
//! ## Usage Example
//! 
//! ```rust,no_run
//! use slysync::mirror::MirrorService;
//! 
//! async fn start_mirror() -> anyhow::Result<()> {
//!     let mirror = MirrorService::new(
//!         "/home/user/documents".into(),
//!         "/backup/documents".into(),
//!         Some("Documents Backup".to_string())
//!     ).await?;
//!     
//!     // Run the mirror service
//!     mirror.run().await?;
//!     
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tracing::{info, warn, error, debug};

/// Configuration for a mirror operation
#[derive(Clone, Debug)]
pub struct MirrorConfig {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub name: Option<String>,
    pub bidirectional: bool,
}

/// Service that manages local folder mirroring
pub struct MirrorService {
    config: MirrorConfig,
    _watcher: RecommendedWatcher,
    file_event_rx: mpsc::Receiver<notify::Result<Event>>,
}

impl MirrorService {
    /// Create a new mirror service
    pub async fn new(source: PathBuf, destination: PathBuf, name: Option<String>) -> Result<Self> {
        info!("Setting up mirror from {} to {}", source.display(), destination.display());
        
        // Validate paths
        if !source.exists() {
            anyhow::bail!("Source path does not exist: {}", source.display());
        }
        
        if !source.is_dir() {
            anyhow::bail!("Source path is not a directory: {}", source.display());
        }
        
        // Create destination directory if it doesn't exist
        if !destination.exists() {
            tokio::fs::create_dir_all(&destination).await?;
            info!("Created destination directory: {}", destination.display());
        }
        
        let config = MirrorConfig {
            source: source.clone(),
            destination,
            name,
            bidirectional: false, // For now, only one-way mirroring
        };
        
        // Set up file system watcher
        let (file_event_tx, file_event_rx) = mpsc::channel(1000);
        
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Err(e) = file_event_tx.blocking_send(res) {
                error!("Failed to send file event: {}", e);
            }
        })?;
        
        // Watch the source directory
        watcher.watch(&source, RecursiveMode::Recursive)?;
        info!("Watching source directory: {}", source.display());
        
        let service = Self {
            config,
            _watcher: watcher,
            file_event_rx,
        };
        
        Ok(service)
    }
    
    /// Perform initial sync to copy all existing files
    pub async fn initial_sync(&self) -> Result<()> {
        info!("Performing initial sync...");
        
        self.sync_directory(&self.config.source, &self.config.destination).await?;
        
        info!("Initial sync completed");
        Ok(())
    }
    
    /// Run the mirror service (blocking)
    pub async fn run(mut self) -> Result<()> {
        // Perform initial sync
        self.initial_sync().await?;
        
        info!("Mirror service started. Watching for changes...");
        
        while let Some(event_result) = self.file_event_rx.recv().await {
            match event_result {
                Ok(event) => {
                    if let Err(e) = self.handle_file_event(event).await {
                        error!("Failed to handle file event: {}", e);
                    }
                }
                Err(e) => {
                    warn!("File watcher error: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Handle a file system event
    async fn handle_file_event(&self, event: Event) -> Result<()> {
        debug!("Received file event: {:?}", event);
        
        for path in event.paths {
            // Only process files within our source directory
            if !path.starts_with(&self.config.source) {
                continue;
            }
            
            // Calculate relative path
            let relative_path = path.strip_prefix(&self.config.source)?;
            let dest_path = self.config.destination.join(relative_path);
            
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) => {
                    self.sync_file(&path, &dest_path).await?;
                }
                EventKind::Remove(_) => {
                    self.remove_file(&dest_path).await?;
                }
                _ => {
                    // Ignore other event types
                }
            }
        }
        
        Ok(())
    }
    
    /// Sync a single file from source to destination
    async fn sync_file(&self, source_path: &Path, dest_path: &Path) -> Result<()> {
        if source_path.is_dir() {
            // Create directory if it doesn't exist
            if !dest_path.exists() {
                tokio::fs::create_dir_all(dest_path).await?;
                info!("Created directory: {}", dest_path.display());
            }
        } else if source_path.is_file() {
            // Ensure parent directory exists
            if let Some(parent) = dest_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            
            // Check if we need to copy the file
            let should_copy = if dest_path.exists() {
                let source_metadata = tokio::fs::metadata(source_path).await?;
                let dest_metadata = tokio::fs::metadata(dest_path).await?;
                
                // Compare modification times
                source_metadata.modified()? > dest_metadata.modified()?
            } else {
                true
            };
            
            if should_copy {
                tokio::fs::copy(source_path, dest_path).await?;
                info!("Copied file: {} -> {}", source_path.display(), dest_path.display());
            }
        }
        
        Ok(())
    }
    
    /// Remove a file from the destination
    async fn remove_file(&self, dest_path: &Path) -> Result<()> {
        if dest_path.exists() {
            if dest_path.is_dir() {
                tokio::fs::remove_dir_all(dest_path).await?;
                info!("Removed directory: {}", dest_path.display());
            } else {
                tokio::fs::remove_file(dest_path).await?;
                info!("Removed file: {}", dest_path.display());
            }
        }
        
        Ok(())
    }
    
    /// Recursively sync a directory
    fn sync_directory<'a>(&'a self, source_dir: &'a Path, dest_dir: &'a Path) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let mut entries = tokio::fs::read_dir(source_dir).await?;
            
            while let Some(entry) = entries.next_entry().await? {
                let source_path = entry.path();
                let file_name = source_path.file_name().unwrap();
                let dest_path = dest_dir.join(file_name);
                
                if source_path.is_dir() {
                    // Create destination directory if it doesn't exist
                    if !dest_path.exists() {
                        tokio::fs::create_dir_all(&dest_path).await?;
                    }
                    
                    // Recursively sync subdirectory
                    self.sync_directory(&source_path, &dest_path).await?;
                } else {
                    // Sync the file
                    self.sync_file(&source_path, &dest_path).await?;
                }
            }
            
            Ok(())
        })
    }
}

/// Run a one-time mirror operation (non-daemon mode)
pub async fn run_mirror_once(source: PathBuf, destination: PathBuf, name: Option<String>) -> Result<()> {
    let mirror = MirrorService::new(source, destination, name).await?;
    mirror.initial_sync().await?;
    
    println!("✅ Mirror operation completed successfully!");
    Ok(())
}
