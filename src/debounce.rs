//! # File Change Debouncing Module
//! 
//! This module provides sophisticated file change detection optimization to handle
//! rapid file modifications efficiently. It implements debouncing to prevent
//! excessive processing of repeated file events.
//! 
//! ## Features
//! 
//! - **Event Debouncing**: Merges rapid successive file events
//! - **Batch Processing**: Groups multiple file changes for efficient handling  
//! - **Performance Monitoring**: Tracks event processing latency
//! - **Configurable Delays**: Adjustable debounce timing for different scenarios
//! - **Memory Efficient**: Bounded event queues to prevent memory leaks
//! 
//! ## Usage Example
//! 
//! ```rust,no_run
//! use synccore::debounce::FileChangeDebouncer;
//! use std::time::Duration;
//! 
//! async fn setup_optimized_monitoring() -> anyhow::Result<()> {
//!     let debouncer = FileChangeDebouncer::new(
//!         Duration::from_millis(100), // debounce delay
//!         |events| async move {
//!             println!("Processing {} debounced events", events.len());
//!             Ok(())
//!         }
//!     );
//!     
//!     // Handle file events through debouncer
//!     debouncer.handle_event(path, event_type).await?;
//!     
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use notify::{Event, EventKind};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, info, warn};

/// Types of file change events for debouncing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileChangeType {
    Created,
    Modified,
    Removed,
}

/// A debounced file change event
#[derive(Debug, Clone)]
pub struct DebouncedFileEvent {
    pub path: PathBuf,
    pub change_type: FileChangeType,
    pub first_seen: Instant,
    pub last_seen: Instant,
    pub event_count: u32,
}

impl DebouncedFileEvent {
    fn new(path: PathBuf, change_type: FileChangeType) -> Self {
        let now = Instant::now();
        Self {
            path,
            change_type,
            first_seen: now,
            last_seen: now,
            event_count: 1,
        }
    }

    fn update(&mut self) {
        self.last_seen = Instant::now();
        self.event_count += 1;
    }

    /// Check if this event should be processed (debounce period expired)
    fn should_process(&self, debounce_duration: Duration) -> bool {
        self.last_seen.elapsed() >= debounce_duration
    }
}

/// Statistics for file change processing
#[derive(Debug, Clone, Default)]
pub struct DebounceStats {
    pub events_received: u64,
    pub events_processed: u64,
    pub events_debounced: u64, // Events that were merged/skipped
    pub average_processing_latency: Duration,
    pub max_processing_latency: Duration,
    pub current_pending_events: usize,
}

/// Configuration for the file change debouncer
#[derive(Debug, Clone)]
pub struct DebouncerConfig {
    /// How long to wait after the last event before processing
    pub debounce_delay: Duration,
    /// Maximum number of pending events before forcing processing
    pub max_pending_events: usize,
    /// How often to check for expired events
    pub cleanup_interval: Duration,
    /// Maximum time an event can remain pending
    pub max_event_age: Duration,
}

impl Default for DebouncerConfig {
    fn default() -> Self {
        Self {
            debounce_delay: Duration::from_millis(100),
            max_pending_events: 1000,
            cleanup_interval: Duration::from_millis(50),
            max_event_age: Duration::from_secs(5),
        }
    }
}

/// High-performance file change debouncer
pub struct FileChangeDebouncer<F>
where
    F: Fn(Vec<DebouncedFileEvent>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> + Send + Sync + 'static,
{
    config: DebouncerConfig,
    pending_events: Arc<RwLock<HashMap<PathBuf, DebouncedFileEvent>>>,
    stats: Arc<RwLock<DebounceStats>>,
    processor: Arc<F>,
    shutdown_tx: Option<mpsc::UnboundedSender<()>>,
}

impl<F> FileChangeDebouncer<F>
where
    F: Fn(Vec<DebouncedFileEvent>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> + Send + Sync + 'static,
{
    /// Create a new file change debouncer
    pub fn new(processor: F) -> Self {
        Self::with_config(DebouncerConfig::default(), processor)
    }

    /// Create debouncer with custom configuration
    pub fn with_config(config: DebouncerConfig, processor: F) -> Self {
        let debouncer = Self {
            config,
            pending_events: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(DebounceStats::default())),
            processor: Arc::new(processor),
            shutdown_tx: None,
        };

        debouncer
    }

    /// Start the background processing task
    pub fn start(&mut self) -> Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel();
        self.shutdown_tx = Some(shutdown_tx);

        let pending_events = self.pending_events.clone();
        let stats = self.stats.clone();
        let processor = self.processor.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut cleanup_interval = interval(config.cleanup_interval);
            
            loop {
                tokio::select! {
                    _ = cleanup_interval.tick() => {
                        if let Err(e) = Self::process_expired_events(
                            &pending_events,
                            &stats,
                            &processor,
                            &config,
                        ).await {
                            warn!("Error processing expired events: {}", e);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("File change debouncer shutting down");
                        break;
                    }
                }
            }
        });

        info!("File change debouncer started with {:?} delay", self.config.debounce_delay);
        Ok(())
    }

    /// Handle a new file change event
    pub async fn handle_event(&self, path: &Path, change_type: FileChangeType) -> Result<()> {
        let start_time = Instant::now();
        
        {
            let mut stats = self.stats.write();
            stats.events_received += 1;
        }

        let mut pending = self.pending_events.write();
        
        // Check if we need to force processing due to too many pending events
        if pending.len() >= self.config.max_pending_events {
            warn!("Max pending events reached ({}), forcing processing", pending.len());
            drop(pending); // Release lock before processing
            self.force_process_all().await?;
            return Ok(());
        }

        // Update or create the event
        match pending.get_mut(path) {
            Some(existing_event) => {
                // Update existing event, potentially changing the type if it's more recent
                existing_event.update();
                existing_event.change_type = change_type; // Latest event type wins
                
                let mut stats = self.stats.write();
                stats.events_debounced += 1;
            }
            None => {
                // Create new pending event
                let event = DebouncedFileEvent::new(path.to_path_buf(), change_type);
                pending.insert(path.to_path_buf(), event);
            }
        }

        // Update current pending count in stats
        {
            let mut stats = self.stats.write();
            stats.current_pending_events = pending.len();
            
            // Update latency tracking
            let processing_time = start_time.elapsed();
            if processing_time > stats.max_processing_latency {
                stats.max_processing_latency = processing_time;
            }
            
            // Simple moving average for processing latency
            stats.average_processing_latency = Duration::from_nanos(
                (stats.average_processing_latency.as_nanos() as u64 * 9 + processing_time.as_nanos() as u64) / 10
            );
        }

        debug!("Debounced file event: {} {:?} (pending: {})", 
               path.display(), change_type, pending.len());

        Ok(())
    }

    /// Force processing of all pending events immediately
    pub async fn force_process_all(&self) -> Result<()> {
        let events_to_process = {
            let mut pending = self.pending_events.write();
            let events: Vec<DebouncedFileEvent> = pending.drain().map(|(_, event)| event).collect();
            events
        };

        if !events_to_process.is_empty() {
            info!("Force processing {} pending events", events_to_process.len());
            self.process_events(events_to_process).await?;
        }

        Ok(())
    }

    /// Get current debouncing statistics
    pub fn get_stats(&self) -> DebounceStats {
        let stats = self.stats.read().clone();
        let pending_count = self.pending_events.read().len();
        
        DebounceStats {
            current_pending_events: pending_count,
            ..stats
        }
    }

    /// Shutdown the debouncer and process remaining events
    pub async fn shutdown(&mut self) -> Result<()> {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        // Process any remaining events
        self.force_process_all().await?;

        let stats = self.get_stats();
        info!("File change debouncer stopped. Stats: {:?}", stats);

        Ok(())
    }

    /// Internal method to process expired events
    async fn process_expired_events(
        pending_events: &Arc<RwLock<HashMap<PathBuf, DebouncedFileEvent>>>,
        stats: &Arc<RwLock<DebounceStats>>,
        processor: &Arc<F>,
        config: &DebouncerConfig,
    ) -> Result<()> {
        let events_to_process = {
            let mut pending = pending_events.write();
            let _now = Instant::now();
            
            // Find events that should be processed
            let mut expired_events = Vec::new();
            let mut paths_to_remove = Vec::new();
            
            for (path, event) in pending.iter() {
                if event.should_process(config.debounce_delay) || 
                   event.first_seen.elapsed() > config.max_event_age {
                    expired_events.push(event.clone());
                    paths_to_remove.push(path.clone());
                }
            }
            
            // Remove processed events from pending
            for path in paths_to_remove {
                pending.remove(&path);
            }
            
            expired_events
        };

        if !events_to_process.is_empty() {
            debug!("Processing {} expired debounced events", events_to_process.len());
            
            // Process the events
            if let Err(e) = (processor)(events_to_process.clone()).await {
                warn!("Error processing debounced events: {}", e);
                return Err(e);
            }

            // Update stats
            {
                let mut stats_guard = stats.write();
                stats_guard.events_processed += events_to_process.len() as u64;
            }
        }

        Ok(())
    }

    /// Process a batch of events using the configured processor
    async fn process_events(&self, events: Vec<DebouncedFileEvent>) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let start_time = Instant::now();
        
        // Call the processor function
        (self.processor)(events.clone()).await?;

        // Update statistics
        {
            let mut stats = self.stats.write();
            stats.events_processed += events.len() as u64;
            
            let processing_time = start_time.elapsed();
            if processing_time > stats.max_processing_latency {
                stats.max_processing_latency = processing_time;
            }
        }

        debug!("Processed {} debounced events in {:?}", events.len(), start_time.elapsed());
        Ok(())
    }

    /// Convert notify Event to our FileChangeType
    pub fn event_to_change_type(event: &Event) -> Option<FileChangeType> {
        match &event.kind {
            EventKind::Create(_) => Some(FileChangeType::Created),
            EventKind::Modify(_) => Some(FileChangeType::Modified),
            EventKind::Remove(_) => Some(FileChangeType::Removed),
            _ => None, // Ignore other event types
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_basic_debouncing() {
        let processed_count = Arc::new(AtomicUsize::new(0));
        let processed_count_clone = processed_count.clone();

        let processor = move |events: Vec<DebouncedFileEvent>| {
            let count = processed_count_clone.clone();
            Box::pin(async move {
                count.fetch_add(events.len(), Ordering::SeqCst);
                Ok(())
            })
        };

        let mut debouncer = FileChangeDebouncer::with_config(
            DebouncerConfig {
                debounce_delay: Duration::from_millis(50),
                ..Default::default()
            },
            processor,
        );

        debouncer.start().unwrap();

        let test_path = PathBuf::from("/test/file.txt");

        // Send multiple rapid events
        for _ in 0..5 {
            debouncer.handle_event(&test_path, FileChangeType::Modified).await.unwrap();
        }

        // Wait for debounce period
        sleep(Duration::from_millis(100)).await;

        // Should have processed only 1 event (debounced)
        assert_eq!(processed_count.load(Ordering::SeqCst), 1);

        let stats = debouncer.get_stats();
        assert_eq!(stats.events_received, 5);
        assert_eq!(stats.events_processed, 1);
        assert_eq!(stats.events_debounced, 4);

        debouncer.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_different_files_not_debounced() {
        let processed_count = Arc::new(AtomicUsize::new(0));
        let processed_count_clone = processed_count.clone();

        let processor = move |events: Vec<DebouncedFileEvent>| {
            let count = processed_count_clone.clone();
            Box::pin(async move {
                count.fetch_add(events.len(), Ordering::SeqCst);
                Ok(())
            })
        };

        let mut debouncer = FileChangeDebouncer::with_config(
            DebouncerConfig {
                debounce_delay: Duration::from_millis(50),
                ..Default::default()
            },
            processor,
        );

        debouncer.start().unwrap();

        // Send events for different files
        debouncer.handle_event(&PathBuf::from("/test/file1.txt"), FileChangeType::Modified).await.unwrap();
        debouncer.handle_event(&PathBuf::from("/test/file2.txt"), FileChangeType::Modified).await.unwrap();
        debouncer.handle_event(&PathBuf::from("/test/file3.txt"), FileChangeType::Modified).await.unwrap();

        // Wait for debounce period
        sleep(Duration::from_millis(100)).await;

        // Should have processed 3 events (different files)
        assert_eq!(processed_count.load(Ordering::SeqCst), 3);

        debouncer.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_force_processing_on_max_events() {
        let processed_events = Arc::new(RwLock::new(Vec::new()));
        let processed_events_clone = processed_events.clone();

        let processor = move |events: Vec<DebouncedFileEvent>| {
            let events_store = processed_events_clone.clone();
            Box::pin(async move {
                events_store.write().extend(events);
                Ok(())
            })
        };

        let mut debouncer = FileChangeDebouncer::with_config(
            DebouncerConfig {
                debounce_delay: Duration::from_secs(10), // Long delay
                max_pending_events: 3, // Small limit
                ..Default::default()
            },
            processor,
        );

        debouncer.start().unwrap();

        // Send events up to the limit
        debouncer.handle_event(&PathBuf::from("/test/file1.txt"), FileChangeType::Modified).await.unwrap();
        debouncer.handle_event(&PathBuf::from("/test/file2.txt"), FileChangeType::Modified).await.unwrap();
        debouncer.handle_event(&PathBuf::from("/test/file3.txt"), FileChangeType::Modified).await.unwrap();

        // This should trigger force processing
        debouncer.handle_event(&PathBuf::from("/test/file4.txt"), FileChangeType::Modified).await.unwrap();

        // Give time for processing
        sleep(Duration::from_millis(50)).await;

        let processed = processed_events.read();
        assert!(processed.len() >= 3, "Should have force-processed events");

        debouncer.shutdown().await.unwrap();
    }

    #[test]
    fn test_debounced_event_should_process() {
        let event = DebouncedFileEvent::new(PathBuf::from("/test"), FileChangeType::Modified);
        
        // Immediately should not process
        assert!(!event.should_process(Duration::from_millis(100)));
        
        // After some time, should process
        std::thread::sleep(Duration::from_millis(150));
        assert!(event.should_process(Duration::from_millis(100)));
    }

    #[test]
    fn test_event_conversion() {
        // Test create event
        let create_event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/test")],
            attrs: Default::default(),
        };
        assert_eq!(
            FileChangeDebouncer::<fn(Vec<DebouncedFileEvent>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>>::event_to_change_type(&create_event),
            Some(FileChangeType::Created)
        );

        // Test modify event
        let modify_event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(notify::event::DataChange::Content)),
            paths: vec![PathBuf::from("/test")],
            attrs: Default::default(),
        };
        assert_eq!(
            FileChangeDebouncer::<fn(Vec<DebouncedFileEvent>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>>::event_to_change_type(&modify_event),
            Some(FileChangeType::Modified)
        );
    }
}
