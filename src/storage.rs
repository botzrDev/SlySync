use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, error, info, warn};

/// Chunk storage and retrieval system
#[derive(Clone)]
pub struct ChunkStore {
    storage_dir: PathBuf,
    chunk_index: Arc<RwLock<HashMap<String, ChunkMetadata>>>, // Use hex string keys for JSON compatibility
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub hash: [u8; 32],
    pub size: u64,
    pub ref_count: u32,
    pub file_path: PathBuf,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_accessed: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileManifest {
    pub path: PathBuf,
    pub size: u64,
    pub modified_at: chrono::DateTime<chrono::Utc>,
    pub chunk_hashes: Vec<[u8; 32]>,
    pub file_hash: [u8; 32],
    pub chunks_stored: Vec<bool>, // Track which chunks we have locally
}

impl ChunkStore {
    pub async fn new(storage_dir: PathBuf) -> Result<Self> {
        // Ensure storage directory exists
        fs::create_dir_all(&storage_dir).await?;
        
        let chunk_index_path = storage_dir.join("chunk_index.json");
        let chunk_index = if chunk_index_path.exists() {
            let data = fs::read_to_string(&chunk_index_path).await?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HashMap::new()
        };

        let store = Self {
            storage_dir,
            chunk_index: Arc::new(RwLock::new(chunk_index)),
        };

        // Verify existing chunks on startup
        store.verify_stored_chunks().await?;
        
        Ok(store)
    }

    /// Store a chunk with the given hash
    pub async fn store_chunk(&self, hash: [u8; 32], data: &[u8]) -> Result<()> {
        // Verify hash matches data
        let computed_hash = crate::crypto::hash_file_chunk(data);
        if computed_hash != hash {
            return Err(anyhow!("Chunk hash mismatch"));
        }

        let chunk_path = self.get_chunk_path(&hash);
        let hash_key = hex::encode(hash);
        
        // Create chunk directory if it doesn't exist
        if let Some(parent) = chunk_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write chunk data
        fs::write(&chunk_path, data).await?;

        // Update index
        let metadata = ChunkMetadata {
            hash,
            size: data.len() as u64,
            ref_count: 1,
            file_path: chunk_path.clone(),
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
        };

        {
            let mut index = self.chunk_index.write();
            index.insert(hash_key, metadata);
        }

        // Save index
        self.save_index().await?;
        
        debug!("Stored chunk {} ({} bytes)", hex::encode(hash), data.len());
        Ok(())
    }

    /// Retrieve a chunk by hash
    pub async fn get_chunk(&self, hash: &[u8; 32]) -> Result<Vec<u8>> {
        let hash_key = hex::encode(hash);
        let chunk_path = {
            let mut index = self.chunk_index.write();
            match index.get_mut(&hash_key) {
                Some(metadata) => {
                    metadata.last_accessed = chrono::Utc::now();
                    metadata.file_path.clone()
                }
                None => return Err(anyhow!("Chunk not found: {}", hex::encode(hash))),
            }
        };

        let data = fs::read(&chunk_path).await
            .map_err(|e| anyhow!("Failed to read chunk {}: {}", hex::encode(hash), e))?;

        // Verify chunk integrity
        let computed_hash = crate::crypto::hash_file_chunk(&data);
        if computed_hash != *hash {
            error!("Chunk corruption detected: {}", hex::encode(hash));
            return Err(anyhow!("Chunk corruption detected"));
        }

        debug!("Retrieved chunk {} ({} bytes)", hex::encode(hash), data.len());
        Ok(data)
    }

    /// Check if we have a chunk
    pub fn has_chunk(&self, hash: &[u8; 32]) -> bool {
        let hash_key = hex::encode(hash);
        self.chunk_index.read().contains_key(&hash_key)
    }

    /// Get chunk metadata
    pub fn get_chunk_metadata(&self, hash: &[u8; 32]) -> Option<ChunkMetadata> {
        let hash_key = hex::encode(hash);
        self.chunk_index.read().get(&hash_key).cloned()
    }

    /// Remove a chunk (decrements ref count, removes if zero)
    pub async fn remove_chunk_ref(&self, hash: &[u8; 32]) -> Result<()> {
        let hash_key = hex::encode(hash);
        let should_delete = {
            let mut index = self.chunk_index.write();
            match index.get_mut(&hash_key) {
                Some(metadata) => {
                    metadata.ref_count = metadata.ref_count.saturating_sub(1);
                    metadata.ref_count == 0
                }
                None => false,
            }
        };

        if should_delete {
            self.delete_chunk(hash).await?;
        }

        self.save_index().await?;
        Ok(())
    }

    /// Increment chunk reference count
    pub async fn add_chunk_ref(&self, hash: &[u8; 32]) -> Result<()> {
        let hash_key = hex::encode(hash);
        {
            let mut index = self.chunk_index.write();
            if let Some(metadata) = index.get_mut(&hash_key) {
                metadata.ref_count += 1;
            }
        }
        self.save_index().await?;
        Ok(())
    }

    /// Delete a chunk from storage
    async fn delete_chunk(&self, hash: &[u8; 32]) -> Result<()> {
        let chunk_path = self.get_chunk_path(hash);
        let hash_key = hex::encode(hash);
        
        if chunk_path.exists() {
            fs::remove_file(&chunk_path).await?;
            debug!("Deleted chunk {}", hex::encode(hash));
        }

        {
            let mut index = self.chunk_index.write();
            index.remove(&hash_key);
        }

        Ok(())
    }

    /// Get all stored chunks
    pub fn list_chunks(&self) -> Vec<[u8; 32]> {
        self.chunk_index.read()
            .keys()
            .filter_map(|hex_key| {
                hex::decode(hex_key).ok().and_then(|bytes| {
                    if bytes.len() == 32 {
                        let mut hash = [0u8; 32];
                        hash.copy_from_slice(&bytes);
                        Some(hash)
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    /// Get storage statistics
    pub fn get_stats(&self) -> StorageStats {
        let index = self.chunk_index.read();
        let total_chunks = index.len();
        let total_size: u64 = index.values().map(|m| m.size).sum();
        let total_refs: u32 = index.values().map(|m| m.ref_count).sum();

        StorageStats {
            total_chunks,
            total_size,
            total_refs,
        }
    }

    /// Clean up orphaned chunks
    pub async fn cleanup(&self) -> Result<()> {
        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::days(7); // Remove unused chunks after 7 days

        let to_remove: Vec<String> = {
            let index = self.chunk_index.read();
            index
                .iter()
                .filter(|(_, metadata)| {
                    metadata.ref_count == 0 && metadata.last_accessed < cutoff
                })
                .map(|(hash_key, _)| hash_key.clone())
                .collect()
        };

        for hash_key in &to_remove {
            if let Ok(hash_bytes) = hex::decode(hash_key) {
                if hash_bytes.len() == 32 {
                    let mut hash = [0u8; 32];
                    hash.copy_from_slice(&hash_bytes);
                    self.delete_chunk(&hash).await?;
                }
            }
        }

        if !to_remove.is_empty() {
            info!("Cleaned up {} orphaned chunks", to_remove.len());
            self.save_index().await?;
        }

        Ok(())
    }

    /// Verify all stored chunks exist and have correct hashes
    async fn verify_stored_chunks(&self) -> Result<()> {
        let chunks_to_verify: Vec<([u8; 32], PathBuf)> = {
            let index = self.chunk_index.read();
            index
                .iter()
                .filter_map(|(hash_key, metadata)| {
                    hex::decode(hash_key).ok().and_then(|bytes| {
                        if bytes.len() == 32 {
                            let mut hash = [0u8; 32];
                            hash.copy_from_slice(&bytes);
                            Some((hash, metadata.file_path.clone()))
                        } else {
                            None
                        }
                    })
                })
                .collect()
        };

        let mut corrupted_chunks = Vec::new();

        for (hash, path) in chunks_to_verify {
            match fs::read(&path).await {
                Ok(data) => {
                    let computed_hash = crate::crypto::hash_file_chunk(&data);
                    if computed_hash != hash {
                        warn!("Chunk corruption detected: {}", hex::encode(hash));
                        corrupted_chunks.push(hex::encode(hash));
                    }
                }
                Err(_) => {
                    warn!("Missing chunk file: {}", hex::encode(hash));
                    corrupted_chunks.push(hex::encode(hash));
                }
            }
        }

        // Remove corrupted chunks from index
        if !corrupted_chunks.is_empty() {
            {
                let mut index = self.chunk_index.write();
                for hash_key in &corrupted_chunks {
                    index.remove(hash_key);
                }
            }
            self.save_index().await?;
            warn!("Removed {} corrupted chunks from index", corrupted_chunks.len());
        }

        Ok(())
    }

    fn get_chunk_path(&self, hash: &[u8; 32]) -> PathBuf {
        let hash_str = hex::encode(hash);
        let prefix = &hash_str[0..2];
        self.storage_dir
            .join("chunks")
            .join(prefix)
            .join(&hash_str)
    }

    async fn save_index(&self) -> Result<()> {
        let index_data = {
            let index = self.chunk_index.read();
            serde_json::to_string_pretty(&*index)?
        };

        let index_path = self.storage_dir.join("chunk_index.json");
        fs::write(index_path, index_data).await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total_chunks: usize,
    pub total_size: u64,
    pub total_refs: u32,
}

impl FileManifest {
    pub async fn from_file(path: &Path) -> Result<Self> {
        let metadata = fs::metadata(path).await?;
        let content = fs::read(path).await?;
        
        let chunks: Vec<Vec<u8>> = content
            .chunks(64 * 1024) // 64KB chunks
            .map(|chunk| chunk.to_vec())
            .collect();
        
        let chunk_hashes: Vec<[u8; 32]> = chunks
            .iter()
            .map(|chunk| crate::crypto::hash_file_chunk(chunk))
            .collect();
        
        let file_hash = crate::crypto::hash_file_chunk(&content);
        
        Ok(Self {
            path: path.to_path_buf(),
            size: metadata.len(),
            modified_at: chrono::DateTime::from(metadata.modified()?),
            chunk_hashes,
            file_hash,
            chunks_stored: vec![false; chunks.len()], // Initially no chunks stored
        })
    }

    pub fn mark_chunk_stored(&mut self, chunk_index: usize) {
        if chunk_index < self.chunks_stored.len() {
            self.chunks_stored[chunk_index] = true;
        }
    }

    pub fn is_chunk_stored(&self, chunk_index: usize) -> bool {
        self.chunks_stored.get(chunk_index).copied().unwrap_or(false)
    }

    pub fn is_complete(&self) -> bool {
        self.chunks_stored.iter().all(|&stored| stored)
    }

    pub fn missing_chunks(&self) -> Vec<usize> {
        self.chunks_stored
            .iter()
            .enumerate()
            .filter(|(_, &stored)| !stored)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn completion_percentage(&self) -> f64 {
        if self.chunks_stored.is_empty() {
            return 100.0;
        }
        
        let stored_count = self.chunks_stored.iter().filter(|&&stored| stored).count();
        (stored_count as f64 / self.chunks_stored.len() as f64) * 100.0
    }
}
