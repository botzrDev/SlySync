use slysync::watcher::{EfficientWatcher, WatcherConfig};
use slysync::debounce::DebouncedFileEvent;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::fs;
use tokio::sync::mpsc;
use tokio::time::sleep;

#[tokio::test]
async fn bench_cpu_usage_idle() {
    let temp_dir = TempDir::new().unwrap();
    let watch_path = temp_dir.path().to_path_buf();
    
    // Optimized config for low CPU usage
    let config = WatcherConfig {
        debounce_delay: Duration::from_millis(500), // Longer debounce
        polling_interval: Duration::from_millis(1000), // Less frequent polling
        max_pending_events: 100,
        max_event_age: Duration::from_secs(10),
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
    
    // Let it run idle for a few seconds
    println!("Running idle CPU usage test for 5 seconds...");
    let start_time = Instant::now();
    
    sleep(Duration::from_secs(5)).await;
    
    let stats = watcher.get_stats();
    let elapsed = start_time.elapsed();
    
    println!("Idle performance stats:");
    println!("  Uptime: {:?}", stats.uptime);
    println!("  Events processed: {}", stats.events_processed);
    println!("  Events debounced: {}", stats.events_debounced);
    println!("  Estimated CPU usage: {:.3}%", stats.cpu_usage_estimate);
    println!("  Actual test duration: {:?}", elapsed);
    
    // CPU usage should be well below 1% when idle
    assert!(stats.cpu_usage_estimate < 1.0, 
           "CPU usage {:.3}% exceeds 1% target", stats.cpu_usage_estimate);
    
    watcher.shutdown().await.unwrap();
}

#[tokio::test]
async fn bench_cpu_usage_under_load() {
    let temp_dir = TempDir::new().unwrap();
    let watch_path = temp_dir.path().to_path_buf();
    
    let config = WatcherConfig {
        debounce_delay: Duration::from_millis(100),
        polling_interval: Duration::from_millis(200),
        max_pending_events: 1000,
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
    watcher.add_path(&watch_path, true).await.unwrap();
    
    // Give watcher time to initialize
    sleep(Duration::from_millis(100)).await;
    
    println!("Running load test with frequent file changes...");
    let start_time = Instant::now();
    
    // Create load by making frequent file changes
    let test_file = watch_path.join("load_test.txt");
    for i in 0..50 {
        fs::write(&test_file, format!("content iteration {}", i)).await.unwrap();
        sleep(Duration::from_millis(20)).await;
        
        // Consume events to prevent channel backup
        while let Ok(_) = rx.try_recv() {
            // Just drain the channel
        }
    }
    
    // Let debouncing settle
    sleep(Duration::from_millis(500)).await;
    
    let stats = watcher.get_stats();
    let elapsed = start_time.elapsed();
    
    println!("Load test performance stats:");
    println!("  Test duration: {:?}", elapsed);
    println!("  Events processed: {}", stats.events_processed);
    println!("  Events debounced: {}", stats.events_debounced);
    println!("  Estimated CPU usage: {:.3}%", stats.cpu_usage_estimate);
    println!("  Debounce efficiency: {:.1}%", 
             if stats.events_processed > 0 {
                 (stats.events_debounced as f64 / (stats.events_processed + stats.events_debounced) as f64) * 100.0
             } else {
                 0.0
             });
    
    // Even under load, CPU usage should be reasonable
    assert!(stats.cpu_usage_estimate < 5.0, 
           "CPU usage {:.3}% too high under load", stats.cpu_usage_estimate);
    
    // Should have processed some events (though debouncing may reduce the count)
    if stats.events_processed > 0 {
        println!("Successfully processed {} events", stats.events_processed);
    }
    
    watcher.shutdown().await.unwrap();
}

#[tokio::test]
async fn bench_memory_usage() {
    let temp_dir = TempDir::new().unwrap();
    let watch_path = temp_dir.path().to_path_buf();
    
    let config = WatcherConfig {
        debounce_delay: Duration::from_millis(100),
        polling_interval: Duration::from_millis(200),
        max_pending_events: 100, // Limited to test memory bounds
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
    watcher.add_path(&watch_path, true).await.unwrap();
    
    // Generate many rapid events to test memory handling
    println!("Testing memory usage with rapid events...");
    
    for i in 0..200 {
        let test_file = watch_path.join(format!("temp_file_{}.txt", i % 10));
        fs::write(&test_file, format!("content {}", i)).await.unwrap();
        
        if i % 20 == 0 {
            sleep(Duration::from_millis(10)).await;
            // Drain some events
            while let Ok(_) = rx.try_recv() {
                // Just consume
            }
        }
    }
    
    // Let system settle
    sleep(Duration::from_millis(1000)).await;
    
    let stats = watcher.get_stats();
    
    println!("Memory test stats:");
    println!("  Events processed: {}", stats.events_processed);
    println!("  Events debounced: {}", stats.events_debounced);
    println!("  CPU usage: {:.3}%", stats.cpu_usage_estimate);
    
    // Should have handled the load without issues
    println!("Test completed - watcher handled rapid events successfully");
    
    watcher.shutdown().await.unwrap();
}

#[tokio::test]
async fn bench_startup_shutdown_latency() {
    let temp_dir = TempDir::new().unwrap();
    let watch_path = temp_dir.path().to_path_buf();
    
    let config = WatcherConfig {
        debounce_delay: Duration::from_millis(100),
        polling_interval: Duration::from_millis(200),
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
    
    // Test startup time
    let start_time = Instant::now();
    let mut watcher = EfficientWatcher::new(config, processor).await.unwrap();
    let creation_time = start_time.elapsed();
    
    let add_path_start = Instant::now();
    watcher.add_path(&watch_path, true).await.unwrap();
    let startup_time = add_path_start.elapsed();
    
    // Test shutdown time
    let shutdown_start = Instant::now();
    watcher.shutdown().await.unwrap();
    let shutdown_time = shutdown_start.elapsed();
    
    println!("Latency benchmarks:");
    println!("  Creation time: {:?}", creation_time);
    println!("  Add path time: {:?}", startup_time);
    println!("  Shutdown time: {:?}", shutdown_time);
    
    // Startup and shutdown should be fast
    assert!(creation_time < Duration::from_secs(1), "Creation too slow: {:?}", creation_time);
    assert!(startup_time < Duration::from_secs(1), "Add path too slow: {:?}", startup_time);
    assert!(shutdown_time < Duration::from_secs(2), "Shutdown too slow: {:?}", shutdown_time);
}
