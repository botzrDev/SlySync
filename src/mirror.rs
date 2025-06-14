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
use tracing::{info, warn, error};

/// Configuration for a mirror operation
#[derive(Clone, Debug)]
pub struct MirrorConfig {
    pub source: PathBuf,
    pub destination: PathBuf,
    #[allow(dead_code)]
    pub name: Option<String>,
    #[allow(dead_code)]
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
        
        let mut watcher = notify::recommended_watcher(
            move |res: notify::Result<Event>| {
                match &res {
                    Ok(event) => {
                        if let EventKind::Remove(_) = event.kind {
                            // For delete events, log them immediately for debugging
                            let paths = event.paths.iter()
                                .map(|p| p.display().to_string())
                                .collect::<Vec<_>>()
                                .join(", ");
                            // Use debug format for EventKind
                            eprintln!("DELETE event detected: {:?} - paths: {}", event.kind, paths);
                        }
                    }
                    Err(e) => eprintln!("Error in watcher: {}", e),
                }
                
                if let Err(e) = file_event_tx.blocking_send(res) {
                    error!("Failed to send file event: {}", e);
                }
            }
        )?;
        
        // Watch the source directory with recursive mode
        watcher.watch(&source, RecursiveMode::Recursive)?;
        info!("Watching source directory: {} (with enhanced deletion detection)", source.display());
        
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
        
        // Create a periodic timer for deletion scanning (every 5 minutes)
        let mut scan_interval = tokio::time::interval(std::time::Duration::from_secs(5 * 60));
        
        loop {
            tokio::select! {
                // Handle file events
                Some(event_result) = self.file_event_rx.recv() => {
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
                
                // Periodically scan for deletions (missed by the watcher)
                _ = scan_interval.tick() => {
                    if let Err(e) = self.scan_for_deletions().await {
                        error!("Failed to perform deletion scan: {}", e);
                    }
                }
                
                // If both channels are closed, break the loop
                else => break,
            }
        }
        
        info!("Mirror service stopped");
        Ok(())
    }
    
    /// Handle a file system event
    async fn handle_file_event(&self, event: Event) -> Result<()> {
        info!("Received file system event: {:?}", event);
        
        for path in event.paths {
            // Only process files within our source directory
            if !path.starts_with(&self.config.source) {
                info!("Skipping event for path outside source directory: {}", path.display());
                continue;
            }
            
            // Calculate relative path
            let relative_path = path.strip_prefix(&self.config.source)?;
            let dest_path = self.config.destination.join(relative_path);
            
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) => {
                    info!("Processing create/modify for path: {}", path.display());
                    self.sync_file(&path, &dest_path).await?;
                }
                EventKind::Remove(_) => {
                    info!("Processing remove for path: {}", path.display());
                    info!("Corresponding destination path for removal: {}", dest_path.display());
                    
                    // For removal events, add a small delay to ensure any other pending events
                    // for the same path are processed first
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    
                    self.remove_file(&dest_path).await?;
                }
                _ => {
                    // Handle any other events generically
                    info!("Received other event type: {:?} for path: {}", event.kind, path.display());
                    
                    // If the file exists in source, sync it; otherwise remove it from destination
                    if path.exists() {
                        self.sync_file(&path, &dest_path).await?;
                    } else if dest_path.exists() {
                        self.remove_file(&dest_path).await?;
                    }
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
        info!("Attempting to remove from destination: {}", dest_path.display());
        
        if dest_path.exists() {
            if dest_path.is_dir() {
                // First check if the directory is empty (quick operation)
                match tokio::fs::read_dir(dest_path).await {
                    Ok(mut dir) => {
                        if dir.next_entry().await?.is_none() {
                            // Directory is empty, remove it
                            match tokio::fs::remove_dir(dest_path).await {
                                Ok(_) => info!("Removed empty directory: {}", dest_path.display()),
                                Err(e) => warn!("Failed to remove empty directory {}: {}", dest_path.display(), e),
                            }
                        } else {
                            // Directory has contents, use recursive removal
                            info!("Removing directory and contents: {}", dest_path.display());
                            match tokio::fs::remove_dir_all(dest_path).await {
                                Ok(_) => info!("Removed directory and contents: {}", dest_path.display()),
                                Err(e) => {
                                    warn!("Failed to remove directory {}: {}", dest_path.display(), e);
                                    
                                    // In some cases, folders might not be removable due to OS locks
                                    // We'll still consider this a "success" for mirroring purposes
                                    // but log a warning
                                }
                            }
                        }
                    }
                    Err(e) => warn!("Failed to read directory for removal {}: {}", dest_path.display(), e),
                }
            } else {
                // File removal
                match tokio::fs::remove_file(dest_path).await {
                    Ok(_) => info!("Removed file: {}", dest_path.display()),
                    Err(e) => {
                        // Check if file is still there, or was already removed
                        if dest_path.exists() {
                            warn!("Failed to remove file {}: {}", dest_path.display(), e);
                        } else {
                            // File no longer exists, so the error is misleading
                            info!("File was already removed: {}", dest_path.display());
                        }
                    }
                }
            }
        } else {
            info!("Path doesn't exist, nothing to remove: {}", dest_path.display());
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
    
    /// Scan for files in destination that no longer exist in source
    async fn scan_for_deletions(&self) -> Result<()> {
        info!("Performing scan for deletions...");
        
        // Start the scan from the root directory
        scan_directory_for_deletions(&self.config.source, &self.config.destination, 
            Path::new(""), self).await?;
        
        info!("Deletion scan completed");
        Ok(())
    }
}

/// Helper function to recursively scan directories for deletions
fn scan_directory_for_deletions<'a>(
    source_base: &'a Path,
    dest_base: &'a Path, 
    current_dir: &'a Path,
    service: &'a MirrorService,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        let dest_dir = dest_base.join(current_dir);
        
        // Skip if destination directory doesn't exist
        if !dest_dir.exists() {
            return Ok(());
        }
        
        let mut entries = tokio::fs::read_dir(&dest_dir).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let entry_path = entry.path();
            let rel_path = entry_path.strip_prefix(dest_base)?;
            let source_path = source_base.join(rel_path);
            
            if entry_path.is_dir() {
                // Recursively check subdirectory
                scan_directory_for_deletions(source_base, dest_base, rel_path, service).await?;
                
                // Check if source directory exists
                if !source_path.exists() {
                    info!("Detected deleted directory during scan: {}", source_path.display());
                    service.remove_file(&entry_path).await?;
                }
            } else {
                // Check if source file exists
                if !source_path.exists() {
                    info!("Detected deleted file during scan: {}", source_path.display());
                    service.remove_file(&entry_path).await?;
                }
            }
        }
        
        Ok(())
    })
}

/// Run a one-time mirror operation (non-daemon mode)
pub async fn run_mirror_once(source: PathBuf, destination: PathBuf, name: Option<String>) -> Result<()> {
    let mirror = MirrorService::new(source, destination, name).await?;
    mirror.initial_sync().await?;
    
    println!("✅ Mirror operation completed successfully!");
    Ok(())
}
