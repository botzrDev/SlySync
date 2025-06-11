//! # File Synchronization Module
//! 
//! This module provides the core file synchronization engine for SlySync.
//! It monitors file system changes, manages file chunks, coordinates with
//! the P2P service for data transfer, and reconstructs files from chunks.
//! 
//! ## Features
//! 
//! - **File System Monitoring**: Real-time monitoring using the `notify` crate
//! - **Chunk-based Synchronization**: Files are split into chunks for efficient transfer
//! - **Deduplication**: Identical chunks are only stored once across all files
//! - **Peer Coordination**: Coordinates with the P2P service for chunk requests and broadcasts
//! - **File Reconstruction**: Automatically reconstructs files when all chunks are available
//! - **Manifest Management**: Tracks file metadata and chunk availability
//! 
//! ## Architecture
//! 
//! The sync service operates as follows:
//! 1. Monitors configured directories for file changes
//! 2. When files change, splits them into chunks and stores locally
//! 3. Broadcasts file updates to connected peers
//! 4. Receives file updates from peers and requests missing chunks
//! 5. Reconstructs files when all chunks are available
//! 
//! ## Usage Example
//! 
//! ```rust,no_run
//! use slysync::sync::SyncService;
//! use slysync::config::Config;
//! 
//! async fn start_sync() -> anyhow::Result<()> {
//!     let config = Config::init().await?;
//!     let mut sync_service = SyncService::new(config).await?;
//!     
//!     // Run the sync service
//!     sync_service.run().await?;
//!     
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use notify::Event;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn, error, debug};
use crate::storage::{ChunkStore, FileManifest};
use crate::requests::RequestManager;
use crate::watcher::EfficientWatcher;
use crate::debounce::{DebouncedFileEvent, FileChangeType};
use indicatif::{ProgressBar, ProgressStyle};

pub struct SyncService {
    config: crate::config::Config,
    efficient_watcher: Option<EfficientWatcher<ProcessorClosure>>,
    p2p_service: Option<Arc<crate::p2p::P2PService>>,
    chunk_store: Arc<ChunkStore>,
    request_manager: Arc<RequestManager>,
    file_manifests: Arc<tokio::sync::RwLock<std::collections::HashMap<String, FileManifest>>>,
    debounce_enabled: bool,
}

// Type alias for the closure that processes file events
type ProcessorClosure = Box<dyn Fn(Vec<DebouncedFileEvent>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> + Send + Sync>;

impl SyncService {
    pub async fn new(config: crate::config::Config) -> Result<Self> {
        info!("Starting synchronization service...");
        
        // Initialize storage
        let storage_dir = config.data_dir()?.join("storage");
        let chunk_store = Arc::new(ChunkStore::new(storage_dir).await?);
        
        // Initialize request manager
        let request_manager = Arc::new(RequestManager::new());
        
        // Initialize file manifest tracking
        let file_manifests = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));
        
        // Create the service first without the watcher
        let service = Self {
            config: config.clone(),
            efficient_watcher: None,
            p2p_service: None,
            chunk_store,
            request_manager,
            file_manifests,
            debounce_enabled: false,
        };
        
        // Scan existing files and build manifests
        service.scan_existing_files().await?;
        
        Ok(service)
    }
    
    /// Initialize the efficient file system watcher
    pub async fn init_watcher(&mut self) -> Result<()> {
        let watcher_config = self.config.to_watcher_config();
        
        // Clone references for the processor closure
        let chunk_store = self.chunk_store.clone();
        let file_manifests = self.file_manifests.clone();
        let p2p_service_ref = self.p2p_service.clone();
        let config = self.config.clone();
        
        // Create the file event processor
        let processor: ProcessorClosure = Box::new(move |events: Vec<DebouncedFileEvent>| {
            let chunk_store = chunk_store.clone();
            let file_manifests = file_manifests.clone();
            let p2p_service = p2p_service_ref.clone();
            let config = config.clone();
            
            Box::pin(async move {
                for event in events {
                    if let Err(e) = Self::process_debounced_event(
                        &event,
                        &chunk_store,
                        &file_manifests,
                        &p2p_service,
                        &config,
                    ).await {
                        warn!("Failed to process debounced event for {}: {}", event.path.display(), e);
                    }
                }
                Ok(())
            })
        });
        
        // Create the efficient watcher
        let mut watcher = EfficientWatcher::new(watcher_config, processor).await?;
        
        // Watch all configured sync folders
        for folder in self.config.sync_folders() {
            info!("Adding path to efficient watcher: {}", folder.path.display());
            watcher.add_path(&folder.path, true).await?;
        }
        
        self.efficient_watcher = Some(watcher);
        info!("Efficient file system watcher initialized");
        
        Ok(())
    }
    
    /// Process a debounced file event (static method for use in closure)
    async fn process_debounced_event(
        event: &DebouncedFileEvent,
        chunk_store: &Arc<ChunkStore>,
        file_manifests: &Arc<tokio::sync::RwLock<std::collections::HashMap<String, FileManifest>>>,
        p2p_service: &Option<Arc<crate::p2p::P2PService>>,
        config: &crate::config::Config,
    ) -> Result<()> {
        info!("Processing debounced event: {:?} for {}", event.change_type, event.path.display());
        
        match event.change_type {
            FileChangeType::Created | FileChangeType::Modified => {
                if event.path.is_file() {
                    Self::process_file_static(&event.path, chunk_store, file_manifests, p2p_service, config).await?;
                }
            }
            FileChangeType::Removed => {
                Self::handle_file_removed_static(&event.path, chunk_store, file_manifests, p2p_service, config).await?;
            }
        }
        
        Ok(())
    }

    pub fn set_p2p_service(&mut self, p2p_service: Arc<crate::p2p::P2PService>) {
        self.p2p_service = Some(p2p_service);
    }

    /// Shutdown the sync service and clean up resources
    #[allow(dead_code)]
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down sync service...");
        
        if let Some(mut watcher) = self.efficient_watcher.take() {
            watcher.shutdown().await?;
        }
        
        info!("Sync service shutdown complete");
        Ok(())
    }

    /// Enable optimized file change debouncing
    #[allow(dead_code)] // Add this
    pub fn enable_debouncing(&mut self) {
        self.debounce_enabled = true;
        info!("File change debouncing enabled");
    }
    
    #[allow(dead_code)]
    pub fn get_chunk_store(&self) -> Arc<ChunkStore> {
        self.chunk_store.clone()
    }
    
    #[allow(dead_code)]
    pub fn get_request_manager(&self) -> Arc<RequestManager> {
        self.request_manager.clone()
    }
    
    async fn scan_existing_files(&self) -> Result<()> {
        info!("Scanning existing files...");
        
        for folder in self.config.sync_folders() {
            self.scan_folder(&folder.path).await?;
        }
        
        Ok(())
    }
    
    async fn scan_folder(&self, folder_path: &Path) -> Result<()> {
        let mut entries = tokio::fs::read_dir(folder_path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            
            if path.is_file() {
                if let Err(e) = self.process_existing_file(&path).await {
                    warn!("Failed to process existing file {}: {}", path.display(), e);
                }
            } else if path.is_dir() {
                // Recursively scan subdirectories using Box::pin for async recursion
                if let Err(e) = Box::pin(self.scan_folder(&path)).await {
                    warn!("Failed to scan directory {}: {}", path.display(), e);
                }
            }
        }
        
        Ok(())
    }
    
    async fn process_existing_file(&self, path: &Path) -> Result<()> {
        let relative_path = self.get_relative_path(path)?;
        
        // Create file manifest
        let manifest = FileManifest::from_file(path).await?;
        
        // Store manifest
        {
            let mut manifests = self.file_manifests.write().await;
            manifests.insert(relative_path.clone(), manifest.clone());
        }
        
        // Check if we have all chunks stored locally
        for (chunk_index, &chunk_hash) in manifest.chunk_hashes.iter().enumerate() {
            if !self.chunk_store.has_chunk(&chunk_hash) {
                // Read and store the chunk
                let content = tokio::fs::read(path).await?;
                let chunks = self.split_into_chunks(&content);
                
                if chunk_index < chunks.len() {
                    self.chunk_store.store_chunk(chunk_hash, &chunks[chunk_index]).await?;
                }
            }
        }
        
        info!("Processed existing file: {}", relative_path);
        Ok(())
    }
    
    pub async fn run(&mut self) -> Result<()> {
        info!("Synchronization service running...");
        
        // Initialize the efficient watcher if not already done
        if self.efficient_watcher.is_none() {
            self.init_watcher().await?;
        }
        
        // The efficient watcher handles file events in the background
        // This method now primarily serves as a way to keep the service alive
        // In a real application, you might want to handle shutdown signals here
        
        info!("Efficient file watcher is running in background...");
        info!("Press Ctrl+C to stop the service");
        
        // Wait for shutdown signal (this is a placeholder - in real usage you'd handle signals)
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            
            // Optionally report statistics
            if let Some(ref watcher) = self.efficient_watcher {
                let stats = watcher.get_stats();
                debug!("Watcher stats: events_received={}, cpu_estimate={:.2}%, watched_paths={}", 
                      stats.events_received, stats.cpu_usage_estimate, stats.watched_paths);
            }
        }
    }
    
    #[allow(dead_code)]
    async fn handle_file_event(&self, event: Event) -> Result<()> {
        debug!("File event: {:?}", event);
        
        // For now, process events directly. In future versions, we can add
        // sophisticated debouncing by collecting events over a time window.
        if self.debounce_enabled {
            debug!("Debouncing enabled - processing event with optimizations");
        }
        
        match event.kind {
            notify::EventKind::Create(_) => {
                for path in &event.paths {
                    if path.is_file() {
                        info!("File created: {}", path.display());
                        if let Err(e) = self.handle_file_created(path).await {
                            error!("Error handling file creation {}: {}", path.display(), e);
                        }
                    }
                }
            }
            notify::EventKind::Modify(_) => {
                for path in &event.paths {
                    if path.is_file() {
                        info!("File modified: {}", path.display());
                        if let Err(e) = self.handle_file_modified(path).await {
                            error!("Error handling file modification {}: {}", path.display(), e);
                        }
                    }
                }
            }
            notify::EventKind::Remove(_) => {
                for path in &event.paths {
                    info!("File removed: {}", path.display());
                    if let Err(e) = self.handle_file_removed(path).await {
                        error!("Error handling file removal {}: {}", path.display(), e);
                    }
                }
            }
            _ => {
                debug!("Ignoring file event: {:?}", event.kind);
            }
        }
        
        Ok(())
    }
    
    #[allow(dead_code)]
    async fn handle_file_created(&self, path: &Path) -> Result<()> {
        info!("File created: {}", path.display());
        
        if path.is_file() {
            self.process_file(path).await?;
        }
        
        Ok(())
    }
    
    #[allow(dead_code)]
    async fn handle_file_modified(&self, path: &Path) -> Result<()> {
        info!("File modified: {}", path.display());
        
        if path.is_file() {
            self.process_file(path).await?;
        }
        
        Ok(())
    }
    
    #[allow(dead_code)]
    async fn handle_file_removed(&self, path: &Path) -> Result<()> {
        info!("File removed: {}", path.display());
        
        let relative_path = self.get_relative_path(path)?;
        
        // Remove file manifest
        let manifest = {
            let mut manifests = self.file_manifests.write().await;
            manifests.remove(&relative_path)
        };
        
        // Broadcast deletion to peers if P2P service is available
        if let Some(p2p_service) = &self.p2p_service {
            if let Err(e) = p2p_service.broadcast_file_deletion(&relative_path).await {
                warn!("Failed to broadcast file deletion: {}", e);
            }
        }
        
        // Clean up unreferenced chunks if we had a manifest
        if let Some(manifest) = manifest {
            for chunk_hash in manifest.chunk_hashes {
                if !self.is_chunk_referenced(&chunk_hash).await {
                    if let Err(e) = self.chunk_store.remove_chunk_ref(&chunk_hash).await {
                        warn!("Failed to remove unreferenced chunk: {}", e);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    async fn is_chunk_referenced(&self, chunk_hash: &[u8; 32]) -> bool {
        let manifests = self.file_manifests.read().await;
        manifests.values().any(|m| m.chunk_hashes.contains(chunk_hash))
    }
    
    pub async fn handle_peer_file_deletion(&self, path: &str) -> Result<()> {
        info!("Received file deletion notification: {}", path);
        
        // Remove local manifest if exists
        let manifest = {
            let mut manifests = self.file_manifests.write().await;
            manifests.remove(path)
        };
        
        // Clean up unreferenced chunks
        if let Some(manifest) = manifest {
            for chunk_hash in manifest.chunk_hashes {
                if !self.is_chunk_referenced(&chunk_hash).await {
                    if let Err(e) = self.chunk_store.remove_chunk_ref(&chunk_hash).await {
                        warn!("Failed to remove unreferenced chunk: {}", e);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    async fn process_file(&self, path: &Path) -> Result<()> {
        info!("Processing file: {}", path.display());
        
        let relative_path = self.get_relative_path(path)?;
        
        // Read file content
        let content = tokio::fs::read(path).await?;
        
        // Create file manifest
        let mut manifest = FileManifest::from_file(path).await?;
        
        // Split into chunks and store them
        let chunks = self.split_into_chunks(&content);
        
        for (chunk_index, chunk_data) in chunks.iter().enumerate() {
            let chunk_hash = manifest.chunk_hashes[chunk_index];
            
            // Store chunk locally
            self.chunk_store.store_chunk(chunk_hash, chunk_data).await?;
            
            // Mark chunk as stored in manifest
            manifest.mark_chunk_stored(chunk_index);
        }
        
        info!("File {} split into {} chunks and stored locally", 
              path.display(), chunks.len());
        
        // Update file manifest
        {
            let mut manifests = self.file_manifests.write().await;
            manifests.insert(relative_path.clone(), manifest.clone());
        }
        
        // Broadcast file update to peers if P2P service is available
        if let Some(p2p_service) = &self.p2p_service {
            if let Err(e) = p2p_service.broadcast_file_update(
                &relative_path, 
                manifest.file_hash, 
                manifest.size, 
                manifest.chunk_hashes.clone()
            ).await {
                warn!("Failed to broadcast file update: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// Handle incoming file update from peer
    pub async fn handle_peer_file_update(
        &self,
        path: &str,
        file_hash: [u8; 32],
        file_size: u64,
        chunk_hashes: Vec<[u8; 32]>,
        peer_id: &str,
    ) -> Result<()> {
        use std::io::Write;
        use chrono::Utc;
        info!("Received file update from peer {}: {}", peer_id, path);
        // Check if we already have this file version
        let (needs_update, local_newer) = {
            let manifests = self.file_manifests.read().await;
            match manifests.get(path) {
                Some(existing_manifest) => {
                    let needs_update = existing_manifest.file_hash != file_hash;
                    let local_newer = existing_manifest.modified_at > Utc::now();
                    (needs_update, local_newer)
                },
                _none => (true, false), // New file
            }
        };
        if !needs_update {
            debug!("File {} is already up to date", path);
            return Ok(());
        }
        if local_newer {
            // Conflict detected: local file is newer than incoming update
            println!("\n⚠️  Conflict detected for file: {}", path);
            println!("Local file was modified after the remote version.");
            println!("Choose how to resolve:");
            println!("  [l] Keep local version");
            println!("  [r] Overwrite with remote version");
            println!("  [b] Keep both (rename remote)");
            print!("Enter choice [l/r/b]: ");
            std::io::stdout().flush().ok();
            let mut choice = String::new();
            std::io::stdin().read_line(&mut choice).ok();
            match choice.trim() {
                "l" | "L" => {
                    println!("Keeping local version. Ignoring remote update.");
                    return Ok(());
                },
                "r" | "R" => {
                    println!("Overwriting with remote version.");
                    // continue to update below
                },
                "b" | "B" => {
                    println!("Keeping both versions. Renaming remote file.");
                    let new_path = format!("{}_remote_{}", path, Utc::now().timestamp());
                    self.handle_peer_file_update_rename(&new_path, file_hash, file_size, chunk_hashes, peer_id).await?;
                    return Ok(());
                },
                _ => {
                    println!("Invalid choice. Aborting update.");
                    return Ok(());
                }
            }
        }
        self.handle_peer_file_update_inner(path, file_hash, file_size, chunk_hashes, peer_id).await
    }

    async fn handle_peer_file_update_rename(
        &self,
        new_path: &str,
        file_hash: [u8; 32],
        file_size: u64,
        chunk_hashes: Vec<[u8; 32]>,
        peer_id: &str,
    ) -> Result<()> {
        self.handle_peer_file_update_inner(new_path, file_hash, file_size, chunk_hashes, peer_id).await
    }

    async fn handle_peer_file_update_inner(
        &self,
        path: &str,
        file_hash: [u8; 32],
        file_size: u64,
        chunk_hashes: Vec<[u8; 32]>,
        peer_id: &str,
    ) -> Result<()> {
        // Create new manifest for the updated file
        let mut new_manifest = FileManifest {
            path: PathBuf::from(path),
            size: file_size,
            modified_at: chrono::Utc::now(),
            chunk_hashes: chunk_hashes.clone(),
            file_hash,
            chunks_stored: vec![false; chunk_hashes.len()],
        };
        for (chunk_index, &chunk_hash) in chunk_hashes.iter().enumerate() {
            if self.chunk_store.has_chunk(&chunk_hash) {
                new_manifest.mark_chunk_stored(chunk_index);
            }
        }
        {
            let mut manifests = self.file_manifests.write().await;
            manifests.insert(path.to_string(), new_manifest.clone());
        }
        self.request_missing_chunks(path, &new_manifest, peer_id).await?;
        Ok(())
    }
    
    /// Request missing chunks for a file from peers
    async fn request_missing_chunks(
        &self,
        file_path: &str,
        manifest: &FileManifest,
        source_peer_id: &str,
    ) -> Result<()> {
        let missing_chunk_indices = manifest.missing_chunks();
        let total = missing_chunk_indices.len();
        if total == 0 {
            self.reconstruct_file(file_path, manifest).await?;
            return Ok(());
        }
        let pb = if total > 1 {
            let pb = ProgressBar::new(total as u64);
            pb.set_style(ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} chunks ({eta})")
                .unwrap());
            Some(pb)
        } else {
            None
        };
        info!("Requesting {} missing chunks for file {}", total, file_path);
        if let Some(p2p_service) = &self.p2p_service {
            for (_i, chunk_index) in missing_chunk_indices.iter().enumerate() {
                let chunk_hash = manifest.chunk_hashes[*chunk_index];
                if let Err(e) = p2p_service.request_chunk_from_peer_secure(
                    source_peer_id,
                    chunk_hash,
                    *chunk_index as u32,
                ).await {
                    warn!("Failed to request chunk {} from peer {}: {}", chunk_index, source_peer_id, e);
                } else if let Some(pb) = &pb {
                    pb.inc(1);
                }
            }
        }
        if let Some(pb) = pb {
            pb.finish_with_message("All chunks requested");
        }
        Ok(())
    }
    
    /// Handle incoming chunk from peer
    #[allow(dead_code)]
    pub async fn handle_chunk_received(
        &self,
        chunk_hash: [u8; 32],
        chunk_data: Vec<u8>,
        file_path: &str,
        chunk_index: u32,
    ) -> Result<()> {
        // Store the chunk
        if let Err(e) = self.chunk_store.store_chunk(chunk_hash, &chunk_data).await {
            error!("Failed to store chunk: {}", e);
            return Err(e);
        }
        // Update manifest
        let should_reconstruct = {
            let mut manifests = self.file_manifests.write().await;
            if let Some(manifest) = manifests.get_mut(file_path) {
                manifest.mark_chunk_stored(chunk_index as usize);
                manifest.is_complete()
            } else {
                error!("Manifest not found for file: {}", file_path);
                return Err(anyhow::anyhow!("Manifest not found for file: {}", file_path));
            }
        };
        // If file is complete, reconstruct it
        if should_reconstruct {
            let manifest = {
                let manifests = self.file_manifests.read().await;
                manifests.get(file_path).cloned()
            };
            if let Some(manifest) = manifest {
                if let Err(e) = self.reconstruct_file(file_path, &manifest).await {
                    error!("Failed to reconstruct file: {}", e);
                }
            }
        }
        Ok(())
    }
    
    /// Reconstruct a file from its chunks
    async fn reconstruct_file(&self, file_path: &str, manifest: &FileManifest) -> Result<()> {
        info!("Reconstructing file: {}", file_path);
        
        // Find the actual file path in sync folders
        let actual_path = self.resolve_sync_file_path(file_path)?;
        
        // Retrieve all chunks
        let mut file_data = Vec::with_capacity(manifest.size as usize);
        
        for &chunk_hash in &manifest.chunk_hashes {
            let chunk_data = self.chunk_store.get_chunk(&chunk_hash).await?;
            file_data.extend_from_slice(&chunk_data);
        }
        
        // Verify file hash
        let computed_hash = crate::crypto::hash_file_chunk(&file_data);
        if computed_hash != manifest.file_hash {
            return Err(anyhow::anyhow!("File reconstruction hash mismatch"));
        }
        
        // Ensure parent directory exists
        if let Some(parent) = actual_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        
        // Write file
        tokio::fs::write(&actual_path, &file_data).await?;
        
        info!("Successfully reconstructed file: {}", actual_path.display());
        Ok(())
    }
    
    fn resolve_sync_file_path(&self, relative_path: &str) -> Result<PathBuf> {
        // For now, use the first sync folder
        // In a more sophisticated implementation, we'd track which folder each file belongs to
        if let Some(folder) = self.config.sync_folders().first() {
            let path = folder.path.join(relative_path);
            
            // Validate the resolved path would be within the sync folder without requiring it to exist
            // Check for path traversal attempts by examining the relative path components
            let normalized_relative = std::path::Path::new(relative_path);
            for component in normalized_relative.components() {
                if let std::path::Component::ParentDir = component {
                    return Err(anyhow::anyhow!("Path contains parent directory references: {}", relative_path));
                }
            }
            
            Ok(path)
        } else {
            Err(anyhow::anyhow!("No sync folders configured"))
        }
    }
    
    fn get_relative_path(&self, file_path: &Path) -> Result<String> {
        // First validate the path is within a sync folder and safe
        let canonical_path = self.validate_sync_path(file_path)?;

        // Get the containing sync folder
        let sync_folder = self.config.sync_folders().iter()
            .find(|sf| canonical_path.starts_with(&sf.path))
            .ok_or(anyhow::anyhow!("Path not within any sync folder after validation"))?;

        // Get relative path (already validated)
        let relative = canonical_path.strip_prefix(&sync_folder.path)?;
        Ok(relative.iter().map(|c| c.to_string_lossy()).collect::<Vec<_>>().join("/"))
    }

    /// Strict path validation that ensures:
    /// 1. Path can be canonicalized
    /// 2. Path is within a sync folder
    /// 3. No parent directory references after canonicalization
    fn validate_sync_path(&self, path: &Path) -> Result<PathBuf> {
        // Get absolute path if relative
        let absolute_path = if path.is_relative() {
            std::env::current_dir()?.join(path)
        } else {
            path.to_path_buf()
        };

        // Canonicalize (fail if can't canonicalize)
        let canonical_path = absolute_path.canonicalize()
            .map_err(|e| anyhow::anyhow!("Failed to canonicalize path {}: {}", path.display(), e))?;

        // Check against all sync folders
        for folder in self.config.sync_folders() {
            let folder_path = folder.path.canonicalize()
                .map_err(|e| anyhow::anyhow!("Failed to canonicalize sync folder {}: {}", folder.path.display(), e))?;

            if canonical_path.starts_with(&folder_path) {
                // Check for any parent directory references in relative path
                let relative = canonical_path.strip_prefix(&folder_path)?;
                for component in relative.components() {
                    if let std::path::Component::ParentDir = component {
                        return Err(anyhow::anyhow!("Path contains parent directory references after canonicalization"));
                    }
                }
                return Ok(canonical_path);
            }
        }

        Err(anyhow::anyhow!("Path {} is not within any sync folder", path.display()))
    }

    fn split_into_chunks(&self, data: &[u8]) -> Vec<Vec<u8>> {
        const CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks
        
        data.chunks(CHUNK_SIZE)
            .map(|chunk| chunk.to_vec())
            .collect()
    }

    /// Static version of process_file for use in debouncer
    #[allow(dead_code)] // Add this
    async fn process_file_static(
        path: &Path,
        chunk_store: &Arc<ChunkStore>,
        file_manifests: &Arc<tokio::sync::RwLock<std::collections::HashMap<String, FileManifest>>>,
        p2p_service: &Option<Arc<crate::p2p::P2PService>>,
        config: &crate::config::Config,
    ) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }

        info!("Processing file: {}", path.display());
        
        let relative_path = Self::get_relative_path_static(path, config)?;
        
        // Read file content
        let content = tokio::fs::read(path).await?;
        
        // Create file manifest
        let mut manifest = FileManifest::from_file(path).await?;
        
        // Split into chunks and store them
        let chunks = Self::split_into_chunks_static(&content);
        
        for (chunk_index, chunk_data) in chunks.iter().enumerate() {
            let chunk_hash = manifest.chunk_hashes[chunk_index];
            
            // Store chunk locally
            chunk_store.store_chunk(chunk_hash, chunk_data).await?;
            
            // Mark chunk as stored in manifest
            manifest.mark_chunk_stored(chunk_index);
        }
        
        info!("File {} split into {} chunks and stored locally", 
              path.display(), chunks.len());
        
        // Update file manifest
        {
            let mut manifests = file_manifests.write().await;
            manifests.insert(relative_path.clone(), manifest.clone());
        }
        
        // Broadcast file update to peers if P2P service is available
        if let Some(p2p_service) = p2p_service {
            if let Err(e) = p2p_service.broadcast_file_update(
                &relative_path, 
                manifest.file_hash, 
                manifest.size, 
                manifest.chunk_hashes.clone()
            ).await {
                warn!("Failed to broadcast file update: {}", e);
            }
        }
        
        Ok(())
    }

    /// Static version of handle_file_removed for use in debouncer
    #[allow(dead_code)] // Add this
    async fn handle_file_removed_static(
        path: &Path,
        chunk_store: &Arc<ChunkStore>,
        file_manifests: &Arc<tokio::sync::RwLock<std::collections::HashMap<String, FileManifest>>>,
        p2p_service: &Option<Arc<crate::p2p::P2PService>>,
        config: &crate::config::Config,
    ) -> Result<()> {
        info!("File removed: {}", path.display());
        
        let relative_path = Self::get_relative_path_static(path, config)?;
        
        // Remove file manifest
        let manifest = {
            let mut manifests = file_manifests.write().await;
            manifests.remove(&relative_path)
        };
        
        // Broadcast deletion to peers if P2P service is available
        if let Some(p2p_service) = p2p_service {
            if let Err(e) = p2p_service.broadcast_file_deletion(&relative_path).await {
                warn!("Failed to broadcast file deletion: {}", e);
            }
        }
        
        // Clean up unreferenced chunks if we had a manifest
        if let Some(manifest) = manifest {
            for chunk_hash in manifest.chunk_hashes {
                if !Self::is_chunk_referenced_static(&chunk_hash, file_manifests).await {
                    if let Err(e) = chunk_store.remove_chunk_ref(&chunk_hash).await {
                        warn!("Failed to remove unreferenced chunk: {}", e);
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Static version of get_relative_path for use in debouncer
    #[allow(dead_code)] // Add this
    fn get_relative_path_static(file_path: &Path, config: &crate::config::Config) -> Result<String> {
        let absolute_file_path = if file_path.is_relative() {
            // Resolve relative path against the current working directory
            std::env::current_dir()?.join(file_path)
        } else {
            file_path.to_path_buf()
        };

        // Canonicalize the absolute path
        let canonicalized_file_path = absolute_file_path.canonicalize().map_err(|e| anyhow::anyhow!("Failed to canonicalize path {}: {}", absolute_file_path.display(), e))?;

        for folder in config.sync_folders() {
            let folder_path = folder.path.canonicalize().map_err(|e| anyhow::anyhow!("Failed to canonicalize sync folder {}: {}", folder.path.display(), e))?;
            if let Ok(relative) = canonicalized_file_path.strip_prefix(&folder_path) {
                // Always use forward slashes for consistency
                let rel_str = relative.iter().map(|c| c.to_string_lossy()).collect::<Vec<_>>().join("/");
                return Ok(rel_str);
            }
        }
        anyhow::bail!("File not in any sync folder: {}", file_path.display())
    }

    /// Static version of split_into_chunks for use in debouncer
    #[allow(dead_code)] // Add this
    fn split_into_chunks_static(data: &[u8]) -> Vec<Vec<u8>> {
        const CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks
        
        data.chunks(CHUNK_SIZE)
            .map(|chunk| chunk.to_vec())
            .collect()
    }

    /// Static version of is_chunk_referenced for use in debouncer
    #[allow(dead_code)] // Add this
    async fn is_chunk_referenced_static(
        chunk_hash: &[u8; 32],
        file_manifests: &Arc<tokio::sync::RwLock<std::collections::HashMap<String, FileManifest>>>,
    ) -> bool {
        let manifests = file_manifests.read().await;
        manifests.values().any(|m| m.chunk_hashes.contains(chunk_hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    async fn create_test_config_with_folder(temp_dir: &TempDir) -> crate::config::Config {
        let config_path = temp_dir.path().join("test_config.toml");
        let mut config = crate::config::Config::init().await.unwrap();
        config.config_file_path = config_path;
        
        // Add a sync folder
        let sync_folder = temp_dir.path().join("sync");
        fs::create_dir_all(&sync_folder).await.unwrap();
        // Ensure the sync folder is empty
        let mut dir = tokio::fs::read_dir(&sync_folder).await.unwrap();
        while let Some(entry) = dir.next_entry().await.unwrap() {
            let path = entry.path();
            if path.is_file() {
                tokio::fs::remove_file(&path).await.unwrap();
            } else if path.is_dir() {
                tokio::fs::remove_dir_all(&path).await.unwrap();
            }
        }
        config.add_sync_folder(sync_folder, None).unwrap();
        // Set test_data_dir for test isolation
        let test_data_dir = temp_dir.path().join("data");
        fs::create_dir_all(&test_data_dir).await.unwrap();
        config.test_data_dir = Some(test_data_dir);
        config
    }

    #[tokio::test]
    async fn test_sync_service_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config_with_folder(&temp_dir).await;
        
        let result = SyncService::new(config).await;
        assert!(result.is_ok());
        
        let sync_service = result.unwrap();
        assert_eq!(sync_service.chunk_store.chunk_count(), 0);
    }

    #[test]
    fn test_sync_service_accessors() {
        tokio_test::block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let config = create_test_config_with_folder(&temp_dir).await;
            let sync_service = SyncService::new(config).await.unwrap();
            
            let chunk_store = sync_service.get_chunk_store();
            assert_eq!(chunk_store.chunk_count(), 0);
            
            let request_manager = sync_service.get_request_manager();
            assert_eq!(request_manager.pending_request_count().await, 0);
        });
    }

    #[tokio::test]
    async fn test_split_into_chunks() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config_with_folder(&temp_dir).await;
        let sync_service = SyncService::new(config).await.unwrap();
        
        // Test with small data
        let small_data = b"Hello, World!";
        let chunks = sync_service.split_into_chunks(small_data);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], small_data);
        
        // Test with large data (> 64KB)
        let large_data = vec![42u8; 100_000]; // 100KB
        let chunks = sync_service.split_into_chunks(&large_data);
        assert!(chunks.len() > 1);
        
        // Verify chunks can be recombined
        let recombined: Vec<u8> = chunks.into_iter().flatten().collect();
        assert_eq!(recombined, large_data);
    }

    #[tokio::test]
    async fn test_get_relative_path() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config_with_folder(&temp_dir).await;
        let sync_service = SyncService::new(config).await.unwrap();
        
        // Create a test file in the sync folder
        let sync_folder = temp_dir.path().join("sync");
        let test_file = sync_folder.join("subdir").join("test.txt");
        fs::create_dir_all(test_file.parent().unwrap()).await.unwrap();
        fs::write(&test_file, "test content").await.unwrap();
        
        let relative_path = sync_service.get_relative_path(&test_file).unwrap();
        assert_eq!(relative_path, "subdir/test.txt");
    }

    #[tokio::test]
    async fn test_get_relative_path_not_in_sync_folder() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config_with_folder(&temp_dir).await;
        let sync_service = SyncService::new(config).await.unwrap();
        
        // Test file outside sync folder
        let outside_file = temp_dir.path().join("outside.txt");
        fs::write(&outside_file, "test").await.unwrap();
        
        let result = sync_service.get_relative_path(&outside_file);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resolve_sync_file_path() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config_with_folder(&temp_dir).await;
        let sync_service = SyncService::new(config).await.unwrap();
        
        let relative_path = "subdir/test.txt";
        let resolved = sync_service.resolve_sync_file_path(relative_path).unwrap();
        
        let expected = temp_dir.path().join("sync").join("subdir").join("test.txt");
        assert_eq!(resolved, expected);
    }

    #[tokio::test]
    async fn test_process_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config_with_folder(&temp_dir).await;
        let sync_service = SyncService::new(config).await.unwrap();
        
        // Create a test file in the sync folder
        let sync_folder = temp_dir.path().join("sync");
        let test_file = sync_folder.join("test.txt");
        let test_content = "Hello, this is a test file for sync processing!";
        fs::write(&test_file, test_content).await.unwrap();
        
        // Process the file
        let result = sync_service.process_existing_file(&test_file).await;
        assert!(result.is_ok());
        
        // Check that manifest was created
        let manifests = sync_service.file_manifests.read().await;
        assert!(manifests.contains_key("test.txt"));
        
        let manifest = &manifests["test.txt"];
        assert_eq!(manifest.size, test_content.len() as u64);
        assert!(!manifest.chunk_hashes.is_empty());
    }

    #[tokio::test]
    async fn test_handle_peer_file_update_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config_with_folder(&temp_dir).await;
        let sync_service = SyncService::new(config).await.unwrap();
        
        let file_path = "new_file.txt";
        let file_hash = [42u8; 32];
        let file_size = 1024;
        let chunk_hashes = vec![[1u8; 32], [2u8; 32], [3u8; 32]];
        let peer_id = "test_peer";
        
        let result = sync_service.handle_peer_file_update(
            file_path,
            file_hash,
            file_size,
            chunk_hashes.clone(),
            peer_id,
        ).await;
        
        assert!(result.is_ok());
        
        // Check that manifest was created
        let manifests = sync_service.file_manifests.read().await;
        assert!(manifests.contains_key(file_path));
        
        let manifest = &manifests[file_path];
        assert_eq!(manifest.file_hash, file_hash);
        assert_eq!(manifest.size, file_size);
        assert_eq!(manifest.chunk_hashes, chunk_hashes);
    }

    #[tokio::test]
    async fn test_handle_peer_file_update_existing_file_same_hash() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config_with_folder(&temp_dir).await;
        let sync_service = SyncService::new(config).await.unwrap();
        
        let file_path = "existing_file.txt";
        let file_hash = [42u8; 32];
        let file_size = 1024;
        let chunk_hashes = vec![[1u8; 32], [2u8; 32]];
        
        // Create existing manifest with same hash
        let existing_manifest = FileManifest {
            path: PathBuf::from(file_path),
            size: file_size,
            modified_at: chrono::Utc::now(),
            chunk_hashes: chunk_hashes.clone(),
            file_hash,
            chunks_stored: vec![true, true],
        };
        
        {
            let mut manifests = sync_service.file_manifests.write().await;
            manifests.insert(file_path.to_string(), existing_manifest);
        }
        
        // Update with same hash should not change anything
        let result = sync_service.handle_peer_file_update(
            file_path,
            file_hash,
            file_size,
            chunk_hashes,
            "test_peer",
        ).await;
        
        assert!(result.is_ok());
        
        // Manifest should remain unchanged
        let manifests = sync_service.file_manifests.read().await;
        let manifest = &manifests[file_path];
        assert_eq!(manifest.chunks_stored, vec![true, true]);
    }

    #[tokio::test]
    async fn test_handle_chunk_received() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config_with_folder(&temp_dir).await;
        let sync_service = SyncService::new(config).await.unwrap();
        
        let file_path = "test_file.txt";
        let chunk_hash = [42u8; 32];
        let chunk_data = b"test chunk data".to_vec();
        let chunk_index = 0;
        
        // Create manifest for the file
        let manifest = FileManifest {
            path: PathBuf::from(file_path),
            size: chunk_data.len() as u64,
            modified_at: chrono::Utc::now(),
            chunk_hashes: vec![chunk_hash],
            file_hash: [99u8; 32],
            chunks_stored: vec![false],
        };
        
        {
            let mut manifests = sync_service.file_manifests.write().await;
            manifests.insert(file_path.to_string(), manifest);
        }
        
        let result = sync_service.handle_chunk_received(
            chunk_hash,
            chunk_data.clone(),
            file_path,
            chunk_index,
        ).await;
        
        assert!(result.is_ok());
        
        // Check that chunk was stored
        assert!(sync_service.chunk_store.has_chunk(&chunk_hash));
        
        // Check that manifest was updated
        let manifests = sync_service.file_manifests.read().await;
        let updated_manifest = &manifests[file_path];
        assert!(updated_manifest.chunks_stored[0]);
    }

    #[tokio::test]
    async fn test_reconstruct_file() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config_with_folder(&temp_dir).await;
        let sync_service = SyncService::new(config).await.unwrap();
        
        let file_path = "reconstruct_test.txt";
        let file_content = b"This is test content for file reconstruction";
        let chunks = sync_service.split_into_chunks(file_content);
        
        // Create chunk hashes and store chunks
        let mut chunk_hashes = Vec::new();
        for chunk in &chunks {
            let chunk_hash = crate::crypto::hash_file_chunk(chunk);
            chunk_hashes.push(chunk_hash);
            sync_service.chunk_store.store_chunk(chunk_hash, chunk).await.unwrap();
        }
        
        // Create manifest
        let file_hash = crate::crypto::hash_file_chunk(file_content);
        let manifest = FileManifest {
            path: PathBuf::from(file_path),
            size: file_content.len() as u64,
            modified_at: chrono::Utc::now(),
            chunk_hashes,
            file_hash,
            chunks_stored: vec![true; chunks.len()],
        };
        
        let result = sync_service.reconstruct_file(file_path, &manifest).await;
        assert!(result.is_ok());
        
        // Check that file was created
        let reconstructed_path = sync_service.resolve_sync_file_path(file_path).unwrap();
        assert!(reconstructed_path.exists());
        
        // Check file content
        let reconstructed_content = fs::read(&reconstructed_path).await.unwrap();
        assert_eq!(reconstructed_content, file_content);
    }

    #[tokio::test]
    async fn test_reconstruct_file_hash_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config_with_folder(&temp_dir).await;
        let sync_service = SyncService::new(config).await.unwrap();
        
        let file_path = "bad_hash_test.txt";
        let file_content = b"Test content";
        let chunks = sync_service.split_into_chunks(file_content);
        
        // Store chunks
        let mut chunk_hashes = Vec::new();
        for chunk in &chunks {
            let chunk_hash = crate::crypto::hash_file_chunk(chunk);
            chunk_hashes.push(chunk_hash);
            sync_service.chunk_store.store_chunk(chunk_hash, chunk).await.unwrap();
        }
        
        // Create manifest with wrong file hash
        let wrong_file_hash = [99u8; 32]; // Wrong hash
        let manifest = FileManifest {
            path: PathBuf::from(file_path),
            size: file_content.len() as u64,
            modified_at: chrono::Utc::now(),
            chunk_hashes,
            file_hash: wrong_file_hash,
            chunks_stored: vec![true; chunks.len()],
        };
        
        let result = sync_service.reconstruct_file(file_path, &manifest).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("hash mismatch"));
    }

    #[tokio::test]
    async fn test_sync_service_with_p2p() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config_with_folder(&temp_dir).await;
        let mut sync_service = SyncService::new(config.clone()).await.unwrap();
        
        // Create a mock P2P service
        let identity = crate::crypto::Identity::generate().unwrap();
        let p2p_service = match crate::p2p::P2PService::new(identity, config).await {
            Ok(service) => Arc::new(service),
            Err(_) => {
                // P2P service creation may fail in test environment
                return;
            }
        };
        
        sync_service.set_p2p_service(p2p_service);
        assert!(sync_service.p2p_service.is_some());
    }
}
