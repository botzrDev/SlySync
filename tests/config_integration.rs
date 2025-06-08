//! Integration tests for configuration management
//! 
//! These tests verify that configuration files are created, loaded,
//! and modified correctly across different scenarios.

use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn test_config_creation_and_loading() {
    let temp_dir = TempDir::new().unwrap();
    
    // Set config dir for this test
    std::env::set_var("SLYSYNC_CONFIG_DIR", temp_dir.path());

    // Create new config
    let config = slysync::config::Config::init().await.unwrap();
    
    // Load existing config
    let loaded_config = slysync::config::Config::load().await.unwrap();
    assert_eq!(config.listen_port, loaded_config.listen_port);

    // Clean up
    std::env::remove_var("SLYSYNC_CONFIG_DIR");
}

#[tokio::test]
async fn test_config_folder_management() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = slysync::config::Config::init().await.unwrap();
    
    // Create test directories
    let folder1 = temp_dir.path().join("folder1");
    let folder2 = temp_dir.path().join("folder2");
    fs::create_dir_all(&folder1).await.unwrap();
    fs::create_dir_all(&folder2).await.unwrap();
    
    // Add folders
    assert_eq!(config.sync_folders().len(), 0);
    
    config.add_sync_folder(folder1.clone(), None).unwrap();
    assert_eq!(config.sync_folders().len(), 1);
    assert_eq!(config.sync_folders()[0].path, folder1);
    
    config.add_sync_folder(folder2.clone(), None).unwrap();
    assert_eq!(config.sync_folders().len(), 2);
    
    // Save and reload
    config.save().await.unwrap();
    // Use load() if load_from is not available
    let reloaded = slysync::config::Config::load().await.unwrap();
    assert_eq!(reloaded.sync_folders().len(), 2);
}

#[tokio::test]
async fn test_config_data_directory() {
    let config = slysync::config::Config::init().await.unwrap();
    let data_dir = config.data_dir().unwrap();
    assert!(data_dir.exists());
    assert!(data_dir.is_dir());
}

#[tokio::test]
async fn test_config_identity_path() {
    let config = slysync::config::Config::init().await.unwrap();
    let identity_path = config.identity_path();
    assert!(identity_path.to_string_lossy().contains("identity"));
}

#[tokio::test]
async fn test_config_default_values() {
    let config = slysync::config::Config::init().await.unwrap();
    // Check default values
    assert!(config.listen_port > 0);
    assert!(config.sync_folders().is_empty());
}

#[tokio::test]
async fn test_config_serialization() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = slysync::config::Config::init().await.unwrap();
    
    // Add some data
    let test_folder = temp_dir.path().join("test_sync");
    fs::create_dir_all(&test_folder).await.unwrap();
    config.add_sync_folder(test_folder.clone(), None).unwrap();
    
    // Save and check file content
    config.save().await.unwrap();
    let content = fs::read_to_string(&temp_dir.path().join("slysync_config.toml")).await.unwrap();
    
    // Should be valid TOML
    assert!(content.contains("listen_port"));
    assert!(content.contains("sync_folders"));
    
    // Should be parseable back
    let _parsed: toml::Value = toml::from_str(&content).unwrap();
}

#[tokio::test]
async fn test_config_concurrent_access() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create initial config
    let _config1 = slysync::config::Config::init().await.unwrap();
    // Load same config from another instance
    let _config2 = slysync::config::Config::load().await.unwrap();
    
    // Both should work without conflicts
}
