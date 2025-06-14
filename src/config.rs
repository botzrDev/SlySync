//! # Configuration Management
//! 
//! This module handles SlySync's configuration system, including:
//! - TOML-based configuration files
//! - Sync folder management
//! - Node settings and preferences
//! - Persistent storage of application state
//! 
//! The configuration is stored in a platform-appropriate directory
//! (e.g., `~/.config/slysync/` on Linux) and includes both node-level
//! settings and per-folder synchronization configurations.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

// Default values for watcher configuration
fn default_watcher_debounce_ms() -> u64 { 200 }
fn default_watcher_polling_ms() -> u64 { 250 }
fn default_watcher_max_events() -> usize { 500 }
fn default_watcher_performance_monitoring() -> bool { true }

/// Main configuration structure for SlySync.
/// 
/// This structure contains all the settings and state information
/// needed to run a SlySync node, including network settings,
/// bandwidth limits, and the list of synchronized folders.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub node_id: String,
    pub listen_port: u16,
    pub bandwidth_limit_up: Option<u64>,
    pub bandwidth_limit_down: Option<u64>,
    pub discovery_enabled: bool,
    pub sync_folders: Vec<SyncFolder>,
    
    // File watcher optimization settings
    #[serde(default = "default_watcher_debounce_ms")]
    pub watcher_debounce_ms: u64,
    #[serde(default = "default_watcher_polling_ms")]
    pub watcher_polling_ms: u64,
    #[serde(default = "default_watcher_max_events")]
    pub watcher_max_events: usize,
    #[serde(default = "default_watcher_performance_monitoring")]
    pub watcher_performance_monitoring: bool,
    
    #[serde(skip)]
    pub config_file_path: PathBuf,
    #[serde(skip)]
    pub test_data_dir: Option<PathBuf>, // Only used in tests for isolation
}

/// Represents a folder that is being synchronized.
/// 
/// Each sync folder has a unique ID, a local filesystem path,
/// an optional human-readable name, and synchronization state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncFolder {
    pub id: String,
    pub path: PathBuf,
    pub name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Information about a remote folder shared by another peer.
/// 
/// This structure contains the metadata needed to join a remote
/// synchronization folder via an invitation code.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteFolderInfo {
    pub folder_id: String,
    pub peer_id: String,
    pub name: Option<String>,
}

impl Config {
    pub async fn init() -> Result<Self> {
        let config_dir = Self::config_dir()?;
        std::fs::create_dir_all(&config_dir)?;
        
        let config_file = config_dir.join("config.toml");
        
        let config = Self {
            node_id: String::new(), // Will be set by identity generation
            listen_port: 41337,
            bandwidth_limit_up: None,
            bandwidth_limit_down: None,
            discovery_enabled: true,
            sync_folders: Vec::new(),
            watcher_debounce_ms: default_watcher_debounce_ms(),
            watcher_polling_ms: default_watcher_polling_ms(),
            watcher_max_events: default_watcher_max_events(),
            watcher_performance_monitoring: default_watcher_performance_monitoring(),
            config_file_path: config_file.clone(),
            test_data_dir: None,
        };
        
        config.save().await?;
        
        Ok(config)
    }
    
    pub async fn load() -> Result<Self> {
        let config_file = Self::config_dir()?.join("config.toml");
        
        if !config_file.exists() {
            anyhow::bail!("SlySync not initialized. Run 'slysync init' first.");
        }
        
        let content = tokio::fs::read_to_string(&config_file).await?;
        let mut config: Config = toml::from_str(&content)?;
        config.config_file_path = config_file;
        
        Ok(config)
    }
    
    pub async fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        tokio::fs::write(&self.config_file_path, content).await?;
        Ok(())
    }
    
    pub fn add_sync_folder(&mut self, path: PathBuf, name: Option<String>) -> Result<String> {
        let folder_id = Uuid::new_v4().to_string();
        
        let sync_folder = SyncFolder {
            id: folder_id.clone(),
            path,
            name,
            created_at: chrono::Utc::now(),
        };
        
        self.sync_folders.push(sync_folder);
        
        Ok(folder_id)
    }
    
    pub fn add_remote_sync_folder(&mut self, path: PathBuf, remote_info: RemoteFolderInfo) -> Result<()> {
        let sync_folder = SyncFolder {
            id: remote_info.folder_id,
            path,
            name: remote_info.name,
            created_at: chrono::Utc::now(),
        };
        
        self.sync_folders.push(sync_folder);
        
        Ok(())
    }
    
    pub fn sync_folders(&self) -> &[SyncFolder] {
        &self.sync_folders
    }
    
    pub fn config_path(&self) -> &Path {
        &self.config_file_path
    }
    
    pub fn identity_path(&self) -> PathBuf {
        Self::config_dir().unwrap().join("identity.key")
    }
    
    /// Create a watcher configuration from the current config settings
    pub fn to_watcher_config(&self) -> crate::watcher::WatcherConfig {
        crate::watcher::WatcherConfig {
            debounce_delay: std::time::Duration::from_millis(self.watcher_debounce_ms),
            polling_interval: std::time::Duration::from_millis(self.watcher_polling_ms),
            max_pending_events: self.watcher_max_events,
            max_event_age: std::time::Duration::from_secs(30), // Fixed reasonable default
            enable_performance_monitoring: self.watcher_performance_monitoring,
        }
    }

    pub fn data_dir(&self) -> Result<PathBuf> {
        if let Some(ref test_dir) = self.test_data_dir {
            return Ok(test_dir.clone());
        }
        let config_dir = Self::config_dir()?;
        let data_dir = config_dir.join("data");
        std::fs::create_dir_all(&data_dir)?;
        Ok(data_dir)
    }

    fn config_dir() -> Result<PathBuf> {
        // Check for environment variable override first
        if let Ok(env_config_dir) = std::env::var("SLYSYNC_CONFIG_DIR") {
            return Ok(PathBuf::from(env_config_dir));
        }
        
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
            .join("slysync");
        
        Ok(config_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_config() -> (Config, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config_file = temp_dir.path().join("config.toml");
        
        let config = Config {
            node_id: "test_node".to_string(),
            listen_port: 41337,
            bandwidth_limit_up: None,
            bandwidth_limit_down: None,
            discovery_enabled: true,
            sync_folders: Vec::new(),
            watcher_debounce_ms: default_watcher_debounce_ms(),
            watcher_polling_ms: default_watcher_polling_ms(),
            watcher_max_events: default_watcher_max_events(),
            watcher_performance_monitoring: default_watcher_performance_monitoring(),
            config_file_path: config_file,
            test_data_dir: None,
        };
        
        (config, temp_dir)
    }

    #[tokio::test]
    async fn test_config_save_and_load() {
        let (config, _temp_dir) = create_test_config().await;
        
        // Save config
        config.save().await.unwrap();
        
        // Load config
        let content = tokio::fs::read_to_string(&config.config_file_path).await.unwrap();
        let loaded_config: Config = toml::from_str(&content).unwrap();
        
        assert_eq!(loaded_config.node_id, "test_node");
        assert_eq!(loaded_config.listen_port, 41337);
        assert_eq!(loaded_config.discovery_enabled, true);
    }

    #[tokio::test]
    async fn test_add_sync_folder() {
        let (mut config, temp_dir) = create_test_config().await;
        let test_path = temp_dir.path().join("test_folder");
        std::fs::create_dir(&test_path).unwrap();
        
        let folder_id = config.add_sync_folder(test_path.clone(), Some("Test Folder".to_string())).unwrap();
        
        assert_eq!(config.sync_folders.len(), 1);
        assert_eq!(config.sync_folders[0].id, folder_id);
        assert_eq!(config.sync_folders[0].path, test_path);
        assert_eq!(config.sync_folders[0].name, Some("Test Folder".to_string()));
    }

    #[tokio::test]
    async fn test_add_remote_sync_folder() {
        let (mut config, temp_dir) = create_test_config().await;
        let test_path = temp_dir.path().join("remote_folder");
        std::fs::create_dir(&test_path).unwrap();
        
        let remote_info = RemoteFolderInfo {
            folder_id: "remote_folder_id".to_string(),
            peer_id: "remote_peer_id".to_string(),
            name: Some("Remote Folder".to_string()),
        };
        
        config.add_remote_sync_folder(test_path.clone(), remote_info).unwrap();
        
        assert_eq!(config.sync_folders.len(), 1);
        assert_eq!(config.sync_folders[0].id, "remote_folder_id");
        assert_eq!(config.sync_folders[0].path, test_path);
        assert_eq!(config.sync_folders[0].name, Some("Remote Folder".to_string()));
    }

    #[test]
    fn test_sync_folders_getter() {
        let config = Config {
            node_id: "test".to_string(),
            listen_port: 41337,
            bandwidth_limit_up: None,
            bandwidth_limit_down: None,
            discovery_enabled: true,
            sync_folders: vec![
                SyncFolder {
                    id: "folder1".to_string(),
                    path: PathBuf::from("/test1"),
                    name: Some("Test 1".to_string()),
                    created_at: chrono::Utc::now(),
                }
            ],
            watcher_debounce_ms: default_watcher_debounce_ms(),
            watcher_polling_ms: default_watcher_polling_ms(),
            watcher_max_events: default_watcher_max_events(),
            watcher_performance_monitoring: default_watcher_performance_monitoring(),
            config_file_path: PathBuf::from("/test/config.toml"),
            test_data_dir: None,
        };
        
        let folders = config.sync_folders();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].id, "folder1");
    }

    #[test]
    fn test_config_paths() {
        let config = Config {
            node_id: "test".to_string(),
            listen_port: 41337,
            bandwidth_limit_up: None,
            bandwidth_limit_down: None,
            discovery_enabled: true,
            sync_folders: Vec::new(),
            watcher_debounce_ms: default_watcher_debounce_ms(),
            watcher_polling_ms: default_watcher_polling_ms(),
            watcher_max_events: default_watcher_max_events(),
            watcher_performance_monitoring: default_watcher_performance_monitoring(),
            config_file_path: PathBuf::from("/test/config.toml"),
            test_data_dir: None,
        };
        
        assert_eq!(config.config_path(), Path::new("/test/config.toml"));
        assert!(config.identity_path().to_string_lossy().ends_with("identity.key"));
        assert!(config.data_dir().unwrap().to_string_lossy().ends_with("data"));
    }
}
