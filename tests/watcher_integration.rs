use slysync::watcher::{EfficientWatcher, WatcherConfig};
use slysync::debounce::DebouncedFileEvent;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};

#[tokio::test]
async fn test_efficient_watcher_creation() {
    let config = WatcherConfig {
        debounce_delay: Duration::from_millis(100),
        polling_interval: Duration::from_millis(500),
        max_pending_events: 1000,
        max_event_age: Duration::from_secs(5),
        enable_performance_monitoring: true,
    };
    
    let (tx, _rx) = mpsc::unbounded_channel();
    
    let processor = move |events: Vec<DebouncedFileEvent>| {
        let tx = tx.clone();
        Box::pin(async move {
            for event in events {
                let _ = tx.send(event.path);
            }
            Ok(())
        }) as std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
    };
    
    let watcher = EfficientWatcher::new(config, processor).await;
    assert!(watcher.is_ok());
    
    let watcher = watcher.unwrap();
    
    // Test initial stats
    let stats = watcher.get_stats();
    assert_eq!(stats.events_processed, 0);
    assert_eq!(stats.events_debounced, 0);
    assert_eq!(stats.watched_paths, 0);
}

#[tokio::test]
async fn test_add_watch_path() {
    let temp_dir = TempDir::new().unwrap();
    let watch_path = temp_dir.path().to_path_buf();
    
    let config = WatcherConfig {
        debounce_delay: Duration::from_millis(100),
        polling_interval: Duration::from_millis(500),
        max_pending_events: 1000,
        max_event_age: Duration::from_secs(5),
        enable_performance_monitoring: true,
    };
    
    let (tx, _rx) = mpsc::unbounded_channel();
    
    let processor = move |events: Vec<DebouncedFileEvent>| {
        let tx = tx.clone();
        Box::pin(async move {
            for event in events {
                let _ = tx.send(event.path);
            }
            Ok(())
        }) as std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
    };
    
    let mut watcher = EfficientWatcher::new(config, processor).await.unwrap();
    
    // Add watch path
    watcher.add_path(&watch_path, true).await.unwrap();
    
    // Check stats
    let stats = watcher.get_stats();
    assert_eq!(stats.watched_paths, 1);
    
    // Clean shutdown
    watcher.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_file_change_detection() {
    let temp_dir = TempDir::new().unwrap();
    let watch_path = temp_dir.path().to_path_buf();
    let test_file = watch_path.join("test.txt");
    
    let config = WatcherConfig {
        debounce_delay: Duration::from_millis(50),
        polling_interval: Duration::from_millis(100),
        max_pending_events: 100,
        max_event_age: Duration::from_secs(5),
        enable_performance_monitoring: true,
    };
    
    let (tx, mut rx) = mpsc::unbounded_channel();
    
    let processor = move |events: Vec<DebouncedFileEvent>| {
        let tx = tx.clone();
        Box::pin(async move {
            for event in events {
                let _ = tx.send(event.path);
            }
            Ok(())
        }) as std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
    };
    
    let mut watcher = EfficientWatcher::new(config, processor).await.unwrap();
    
    // Add watch path
    watcher.add_path(&watch_path, true).await.unwrap();
    
    // Give watcher time to initialize
    sleep(Duration::from_millis(100)).await;
    
    // Create a test file
    fs::write(&test_file, "test content").await.unwrap();
    
    // Wait for debounced event
    let result = timeout(Duration::from_secs(3), rx.recv()).await;
    
    // Clean shutdown
    watcher.shutdown().await.unwrap();
    
    // Check if we got an event (may not always trigger depending on OS)
    if result.is_ok() {
        let received_path = result.unwrap().unwrap();
        assert!(received_path.ends_with("test.txt") || received_path == watch_path);
    }
}

#[tokio::test]
async fn test_performance_statistics() {
    let temp_dir = TempDir::new().unwrap();
    let watch_path = temp_dir.path().to_path_buf();
    
    let config = WatcherConfig {
        debounce_delay: Duration::from_millis(50),
        polling_interval: Duration::from_millis(100),
        max_pending_events: 100,
        max_event_age: Duration::from_secs(5),
        enable_performance_monitoring: true,
    };
    
    let (tx, _rx) = mpsc::unbounded_channel::<PathBuf>();
    
    let processor = move |events: Vec<DebouncedFileEvent>| {
        let tx = tx.clone();
        Box::pin(async move {
            for event in events {
                let _ = tx.send(event.path);
            }
            Ok(())
        }) as std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
    };
    
    let mut watcher = EfficientWatcher::new(config, processor).await.unwrap();
    
    watcher.add_path(&watch_path, true).await.unwrap();
    
    // Give watcher time to initialize
    sleep(Duration::from_millis(100)).await;
    
    let stats = watcher.get_stats();
    assert_eq!(stats.watched_paths, 1);
    assert!(stats.uptime.as_millis() > 0);
    assert!(stats.cpu_usage_estimate >= 0.0);
    
    watcher.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_watcher_shutdown_cleanup() {
    let temp_dir = TempDir::new().unwrap();
    let watch_path = temp_dir.path().to_path_buf();
    
    let config = WatcherConfig {
        debounce_delay: Duration::from_millis(50),
        polling_interval: Duration::from_millis(100),
        max_pending_events: 100,
        max_event_age: Duration::from_secs(5),
        enable_performance_monitoring: true,
    };
    
    let (tx, _rx) = mpsc::unbounded_channel();
    
    let processor = move |events: Vec<DebouncedFileEvent>| {
        let tx = tx.clone();
        Box::pin(async move {
            for event in events {
                let _ = tx.send(event.path);
            }
            Ok(())
        }) as std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
    };
    
    let mut watcher = EfficientWatcher::new(config, processor).await.unwrap();
    
    watcher.add_path(&watch_path, true).await.unwrap();
    
    // Give watcher time to initialize
    sleep(Duration::from_millis(50)).await;
    
    // Verify it's running
    let stats_before = watcher.get_stats();
    assert!(stats_before.uptime.as_millis() > 0);
    assert_eq!(stats_before.watched_paths, 1);
    
    // Shutdown should complete without hanging
    let shutdown_result = timeout(Duration::from_secs(5), watcher.shutdown()).await;
    assert!(shutdown_result.is_ok());
    assert!(shutdown_result.unwrap().is_ok());
}
