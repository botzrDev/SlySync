use anyhow::Result;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn, error, debug};
use crate::storage::{ChunkStore, FileManifest};
use crate::requests::RequestManager;

pub struct SyncService {
    config: crate::config::Config,
    _watcher: RecommendedWatcher,
    file_event_rx: mpsc::Receiver<notify::Result<Event>>,
    p2p_service: Option<Arc<crate::p2p::P2PService>>,
    chunk_store: Arc<ChunkStore>,
    request_manager: Arc<RequestManager>,
    file_manifests: Arc<tokio::sync::RwLock<std::collections::HashMap<String, FileManifest>>>,
}

impl SyncService {
    pub async fn new(config: crate::config::Config) -> Result<Self> {
        info!("Starting synchronization service...");
        
        let (file_event_tx, file_event_rx) = mpsc::channel(1000);
        
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Err(e) = file_event_tx.blocking_send(res) {
                error!("Failed to send file event: {}", e);
            }
        })?;
        
        // Watch all configured sync folders
        for folder in config.sync_folders() {
            info!("Watching folder: {}", folder.path.display());
            watcher.watch(&folder.path, RecursiveMode::Recursive)?;
        }
        
        // Initialize storage
        let storage_dir = config.data_dir()?.join("storage");
        let chunk_store = Arc::new(ChunkStore::new(storage_dir).await?);
        
        // Initialize request manager
        let request_manager = Arc::new(RequestManager::new());
        
        // Initialize file manifest tracking
        let file_manifests = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));
        
        // Load existing manifests
        let service = Self {
            config,
            _watcher: watcher,
            file_event_rx,
            p2p_service: None,
            chunk_store,
            request_manager,
            file_manifests,
        };
        
        // Scan existing files and build manifests
        service.scan_existing_files().await?;
        
        Ok(service)
    }
    
    pub fn set_p2p_service(&mut self, p2p_service: Arc<crate::p2p::P2PService>) {
        self.p2p_service = Some(p2p_service);
    }
    
    pub fn get_chunk_store(&self) -> Arc<ChunkStore> {
        self.chunk_store.clone()
    }
    
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
        
        while let Some(event_result) = self.file_event_rx.recv().await {
            match event_result {
                Ok(event) => {
                    if let Err(e) = self.handle_file_event(event).await {
                        error!("Error handling file event: {}", e);
                    }
                }
                Err(e) => {
                    warn!("File system watch error: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    async fn handle_file_event(&self, event: Event) -> Result<()> {
        debug!("File event: {:?}", event);
        
        match event.kind {
            notify::EventKind::Create(_) => {
                for path in &event.paths {
                    self.handle_file_created(path).await?;
                }
            }
            notify::EventKind::Modify(_) => {
                for path in &event.paths {
                    self.handle_file_modified(path).await?;
                }
            }
            notify::EventKind::Remove(_) => {
                for path in &event.paths {
                    self.handle_file_removed(path).await?;
                }
            }
            _ => {
                // Ignore other event types for now
            }
        }
        
        Ok(())
    }
    
    async fn handle_file_created(&self, path: &Path) -> Result<()> {
        info!("File created: {}", path.display());
        
        if path.is_file() {
            self.process_file(path).await?;
        }
        
        Ok(())
    }
    
    async fn handle_file_modified(&self, path: &Path) -> Result<()> {
        info!("File modified: {}", path.display());
        
        if path.is_file() {
            self.process_file(path).await?;
        }
        
        Ok(())
    }
    
    async fn handle_file_removed(&self, path: &Path) -> Result<()> {
        info!("File removed: {}", path.display());
        
        // TODO: Propagate deletion to peers
        
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
        info!("Received file update from peer {}: {}", peer_id, path);
        
        // Check if we already have this file version
        let needs_update = {
            let manifests = self.file_manifests.read().await;
            match manifests.get(path) {
                Some(existing_manifest) => existing_manifest.file_hash != file_hash,
                None => true, // New file
            }
        };
        
        if !needs_update {
            debug!("File {} is already up to date", path);
            return Ok(());
        }
        
        // Create new manifest for the updated file
        let mut new_manifest = FileManifest {
            path: PathBuf::from(path),
            size: file_size,
            modified_at: chrono::Utc::now(),
            chunk_hashes: chunk_hashes.clone(),
            file_hash,
            chunks_stored: vec![false; chunk_hashes.len()],
        };
        
        // Check which chunks we already have
        for (chunk_index, &chunk_hash) in chunk_hashes.iter().enumerate() {
            if self.chunk_store.has_chunk(&chunk_hash) {
                new_manifest.mark_chunk_stored(chunk_index);
            }
        }
        
        // Store the manifest
        {
            let mut manifests = self.file_manifests.write().await;
            manifests.insert(path.to_string(), new_manifest.clone());
        }
        
        // Request missing chunks from peers
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
        
        if missing_chunk_indices.is_empty() {
            // All chunks available, reconstruct file
            self.reconstruct_file(file_path, manifest).await?;
            return Ok(());
        }
        
        info!("Requesting {} missing chunks for file {}", 
              missing_chunk_indices.len(), file_path);
        
        // Use P2P service to request chunks
        if let Some(p2p_service) = &self.p2p_service {
            for chunk_index in missing_chunk_indices {
                let chunk_hash = manifest.chunk_hashes[chunk_index];
                
                if let Err(e) = p2p_service.request_chunk_from_peer(
                    source_peer_id,
                    chunk_hash,
                    chunk_index as u32,
                ).await {
                    warn!("Failed to request chunk {} from peer {}: {}", 
                          chunk_index, source_peer_id, e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Handle incoming chunk from peer
    pub async fn handle_chunk_received(
        &self,
        chunk_hash: [u8; 32],
        chunk_data: Vec<u8>,
        file_path: &str,
        chunk_index: u32,
    ) -> Result<()> {
        // Store the chunk
        self.chunk_store.store_chunk(chunk_hash, &chunk_data).await?;
        
        // Update manifest
        let should_reconstruct = {
            let mut manifests = self.file_manifests.write().await;
            if let Some(manifest) = manifests.get_mut(file_path) {
                manifest.mark_chunk_stored(chunk_index as usize);
                manifest.is_complete()
            } else {
                false
            }
        };
        
        // If file is complete, reconstruct it
        if should_reconstruct {
            let manifest = {
                let manifests = self.file_manifests.read().await;
                manifests.get(file_path).cloned()
            };
            
            if let Some(manifest) = manifest {
                self.reconstruct_file(file_path, &manifest).await?;
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
            Ok(folder.path.join(relative_path))
        } else {
            Err(anyhow::anyhow!("No sync folders configured"))
        }
    }
    
    fn get_relative_path(&self, file_path: &Path) -> Result<String> {
        // Find which sync folder this file belongs to
        for folder in self.config.sync_folders() {
            if let Ok(relative) = file_path.strip_prefix(&folder.path) {
                return Ok(relative.to_string_lossy().to_string());
            }
        }
        
        anyhow::bail!("File not in any sync folder: {}", file_path.display())
    }
    
    fn split_into_chunks(&self, data: &[u8]) -> Vec<Vec<u8>> {
        const CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks
        
        data.chunks(CHUNK_SIZE)
            .map(|chunk| chunk.to_vec())
            .collect()
    }
}
