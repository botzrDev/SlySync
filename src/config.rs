use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub node_id: String,
    pub listen_port: u16,
    pub bandwidth_limit_up: Option<u64>,
    pub bandwidth_limit_down: Option<u64>,
    pub discovery_enabled: bool,
    pub sync_folders: Vec<SyncFolder>,
    
    #[serde(skip)]
    config_file_path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncFolder {
    pub id: String,
    pub path: PathBuf,
    pub name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

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
            config_file_path: config_file.clone(),
        };
        
        config.save().await?;
        
        Ok(config)
    }
    
    pub async fn load() -> Result<Self> {
        let config_file = Self::config_dir()?.join("config.toml");
        
        if !config_file.exists() {
            anyhow::bail!("SyncCore not initialized. Run 'synccore init' first.");
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
    
    pub fn data_dir(&self) -> Result<PathBuf> {
        let config_dir = Self::config_dir()?;
        Ok(config_dir.join("data"))
    }

    fn config_dir() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
            .join("synccore");
        
        Ok(config_dir)
    }
}
