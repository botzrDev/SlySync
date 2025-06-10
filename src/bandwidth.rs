//! # Bandwidth Management Module
//! 
//! This module provides bandwidth throttling and traffic shaping capabilities
//! for SlySync to ensure optimal network performance without overwhelming
//! network connections.
//! 
//! ## Features
//! 
//! - **Token Bucket Algorithm**: Implements smooth rate limiting using token bucket
//! - **Upload/Download Limits**: Separate configurable limits for each direction
//! - **Burst Handling**: Allows short bursts while maintaining average rate
//! - **Dynamic Adjustment**: Can adjust limits during runtime based on network conditions
//! - **Statistics**: Tracks bandwidth usage and efficiency metrics
//! 
//! ## Usage Example
//! 
//! ```rust,no_run
//! use synccore::bandwidth::BandwidthManager;
//! 
//! async fn transfer_with_throttling() -> anyhow::Result<()> {
//!     let mut manager = BandwidthManager::new(1_048_576, 2_097_152); // 1MB up, 2MB down
//!     
//!     // Request permission to send 1KB
//!     manager.request_upload_quota(1024).await;
//!     
//!     // Transfer data here...
//!     
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, info};

/// Token bucket rate limiter for bandwidth throttling
#[derive(Debug)]
pub struct TokenBucket {
    capacity: u64,
    tokens: Arc<RwLock<f64>>,
    refill_rate: f64, // tokens per second
    last_refill: Arc<RwLock<Instant>>,
}

#[allow(dead_code)]
impl TokenBucket {
    pub fn new(rate_bytes_per_sec: u64) -> Self {
        let capacity = rate_bytes_per_sec.max(1024); // Minimum 1KB burst
        Self {
            capacity,
            tokens: Arc::new(RwLock::new(capacity as f64)),
            refill_rate: rate_bytes_per_sec as f64,
            last_refill: Arc::new(RwLock::new(Instant::now())),
        }
    }

    /// Request tokens for data transfer, blocking until available
    pub async fn consume(&self, bytes: u64) -> Result<()> {
        if bytes == 0 {
            return Ok(());
        }

        loop {
            self.refill();
            
            let available = {
                let mut tokens = self.tokens.write();
                if *tokens >= bytes as f64 {
                    *tokens -= bytes as f64;
                    debug!("Consumed {} tokens, {} remaining", bytes, *tokens);
                    return Ok(());
                }
                *tokens
            }; // Guard is dropped here
            
            // Calculate wait time for next refill
            let needed = bytes as f64 - available;
            let wait_time = Duration::from_secs_f64(needed / self.refill_rate);
            
            debug!("Waiting {:?} for {} bytes bandwidth quota", wait_time, bytes);
            sleep(wait_time.min(Duration::from_millis(100))).await;
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&self) {
        let now = Instant::now();
        let mut last_refill = self.last_refill.write();
        let elapsed = now.duration_since(*last_refill);
        
        if elapsed >= Duration::from_millis(10) { // Refill every 10ms minimum
            let new_tokens = elapsed.as_secs_f64() * self.refill_rate;
            let mut tokens = self.tokens.write();
            *tokens = (*tokens + new_tokens).min(self.capacity as f64);
            *last_refill = now;
            
            debug!("Refilled {:.2} tokens, total: {:.2}", new_tokens, *tokens);
        }
    }

    /// Get current token count
    pub fn available_tokens(&self) -> u64 {
        self.refill();
        *self.tokens.read() as u64
    }

    /// Update the rate limit
    pub fn set_rate(&mut self, rate_bytes_per_sec: u64) {
        self.refill_rate = rate_bytes_per_sec as f64;
        self.capacity = rate_bytes_per_sec.max(1024);
        
        // Reset tokens to new capacity if higher
        let mut tokens = self.tokens.write();
        if *tokens > self.capacity as f64 {
            *tokens = self.capacity as f64;
        }
        
        info!("Updated bandwidth rate to {} bytes/sec", rate_bytes_per_sec);
    }
}

/// Bandwidth usage statistics
#[derive(Debug, Clone, Default)]
pub struct BandwidthStats {
    pub bytes_uploaded: u64,
    pub bytes_downloaded: u64,
    pub upload_rate_current: f64, // bytes per second
    pub download_rate_current: f64,
    pub upload_rate_average: f64,
    pub download_rate_average: f64,
    pub session_start: Option<Instant>,
}

/// Main bandwidth management system
#[derive(Debug)]
pub struct BandwidthManager {
    upload_bucket: Option<TokenBucket>,
    download_bucket: Option<TokenBucket>,
    stats: Arc<RwLock<BandwidthStats>>,
    rate_window: Arc<RwLock<Vec<(Instant, u64, bool)>>>, // (time, bytes, is_upload)
}

#[allow(dead_code)]
impl BandwidthManager {
    /// Create new bandwidth manager with upload/download limits in bytes per second
    /// Set to 0 or None to disable throttling for that direction
    pub fn new(upload_limit: Option<u64>, download_limit: Option<u64>) -> Self {
        let mut stats = BandwidthStats::default();
        stats.session_start = Some(Instant::now());

        Self {
            upload_bucket: upload_limit.map(TokenBucket::new),
            download_bucket: download_limit.map(TokenBucket::new),
            stats: Arc::new(RwLock::new(stats)),
            rate_window: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Request quota for uploading data, will block until bandwidth is available
    pub async fn request_upload_quota(&self, bytes: u64) -> Result<()> {
        if let Some(bucket) = &self.upload_bucket {
            bucket.consume(bytes).await?;
        }
        self.record_transfer(bytes, true);
        Ok(())
    }

    /// Request quota for downloading data, will block until bandwidth is available
    pub async fn request_download_quota(&self, bytes: u64) -> Result<()> {
        if let Some(bucket) = &self.download_bucket {
            bucket.consume(bytes).await?;
        }
        self.record_transfer(bytes, false);
        Ok(())
    }

    /// Record a transfer for statistics
    fn record_transfer(&self, bytes: u64, is_upload: bool) {
        let now = Instant::now();
        
        // Update cumulative stats
        {
            let mut stats = self.stats.write();
            if is_upload {
                stats.bytes_uploaded += bytes;
            } else {
                stats.bytes_downloaded += bytes;
            }
        }

        // Update rate window (keep last 5 seconds)
        {
            let mut window = self.rate_window.write();
            window.push((now, bytes, is_upload));
            
            // Remove old entries
            let cutoff = now - Duration::from_secs(5);
            window.retain(|(time, _, _)| *time > cutoff);
        }

        // Calculate current rates
        self.update_current_rates();
    }

    /// Update current transfer rates based on recent activity
    fn update_current_rates(&self) {
        let window = self.rate_window.read();
        if window.is_empty() {
            return;
        }

        let now = Instant::now();
        let window_start = now - Duration::from_secs(5);

        let (upload_bytes, download_bytes): (u64, u64) = window.iter()
            .filter(|(time, _, _)| *time > window_start)
            .fold((0, 0), |(up, down), (_, bytes, is_upload)| {
                if *is_upload {
                    (up + bytes, down)
                } else {
                    (up, down + bytes)
                }
            });

        let window_duration = 5.0; // 5 second window
        let mut stats = self.stats.write();
        stats.upload_rate_current = upload_bytes as f64 / window_duration;
        stats.download_rate_current = download_bytes as f64 / window_duration;

        // Calculate session averages
        if let Some(session_start) = stats.session_start {
            let session_duration = now.duration_since(session_start).as_secs_f64();
            if session_duration > 0.0 {
                stats.upload_rate_average = stats.bytes_uploaded as f64 / session_duration;
                stats.download_rate_average = stats.bytes_downloaded as f64 / session_duration;
            }
        }
    }

    /// Get current bandwidth statistics
    pub fn get_stats(&self) -> BandwidthStats {
        self.update_current_rates();
        self.stats.read().clone()
    }

    /// Update upload rate limit
    pub fn set_upload_limit(&mut self, limit: Option<u64>) {
        if let Some(limit) = limit {
            if let Some(bucket) = &mut self.upload_bucket {
                bucket.set_rate(limit);
            } else {
                self.upload_bucket = Some(TokenBucket::new(limit));
            }
            info!("Set upload limit to {} bytes/sec", limit);
        } else {
            self.upload_bucket = None;
            info!("Disabled upload bandwidth limiting");
        }
    }

    /// Update download rate limit
    pub fn set_download_limit(&mut self, limit: Option<u64>) {
        if let Some(limit) = limit {
            if let Some(bucket) = &mut self.download_bucket {
                bucket.set_rate(limit);
            } else {
                self.download_bucket = Some(TokenBucket::new(limit));
            }
            info!("Set download limit to {} bytes/sec", limit);
        } else {
            self.download_bucket = None;
            info!("Disabled download bandwidth limiting");
        }
    }

    /// Check if upload bandwidth is available without consuming
    pub fn upload_available(&self) -> u64 {
        self.upload_bucket.as_ref()
            .map(|bucket| bucket.available_tokens())
            .unwrap_or(u64::MAX)
    }

    /// Check if download bandwidth is available without consuming
    pub fn download_available(&self) -> u64 {
        self.download_bucket.as_ref()
            .map(|bucket| bucket.available_tokens())
            .unwrap_or(u64::MAX)
    }

    /// Start background statistics reporting task
    pub async fn start_stats_reporter(&self) -> tokio::task::JoinHandle<()> {
        let stats = self.stats.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                let stats_snapshot = stats.read().clone();
                
                info!("Bandwidth Stats - Upload: {:.2} KB/s ({:.2} MB total), Download: {:.2} KB/s ({:.2} MB total)",
                    stats_snapshot.upload_rate_current / 1024.0,
                    stats_snapshot.bytes_uploaded as f64 / 1_048_576.0,
                    stats_snapshot.download_rate_current / 1024.0,
                    stats_snapshot.bytes_downloaded as f64 / 1_048_576.0
                );
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_token_bucket_basic() {
        let bucket = TokenBucket::new(1000); // 1000 bytes/sec
        
        // Should be able to consume initial capacity
        bucket.consume(1000).await.unwrap();
        
        // Should wait for refill
        let start = Instant::now();
        bucket.consume(1000).await.unwrap();
        let elapsed = start.elapsed();
        
        // Should take approximately 1 second
        assert!(elapsed >= Duration::from_millis(900));
        assert!(elapsed <= Duration::from_millis(1100));
    }

    #[test]
    async fn test_bandwidth_manager_stats() {
        let manager = BandwidthManager::new(Some(1000), Some(2000));
        
        manager.request_upload_quota(100).await.unwrap();
        manager.request_download_quota(200).await.unwrap();
        
        let stats = manager.get_stats();
        assert_eq!(stats.bytes_uploaded, 100);
        assert_eq!(stats.bytes_downloaded, 200);
    }

    #[test]
    async fn test_unlimited_bandwidth() {
        let manager = BandwidthManager::new(None, None);
        
        // Should not throttle when limits are disabled
        let start = Instant::now();
        manager.request_upload_quota(1_000_000).await.unwrap();
        let elapsed = start.elapsed();
        
        // Should be nearly instantaneous
        assert!(elapsed < Duration::from_millis(10));
    }

    #[test]
    async fn test_rate_limiting_configuration() {
        let mut manager = BandwidthManager::new(Some(1000), None);
        
        // Test initial configuration - bucket should have at most the configured capacity
        // but might have initial burst capacity
        let upload_available = manager.upload_available();
        assert!(upload_available <= 1024); // Buckets can have burst capacity up to max(rate, 1024)
        assert_eq!(manager.download_available(), u64::MAX);
        
        // Test dynamic updates
        manager.set_upload_limit(Some(2000));
        manager.set_download_limit(Some(500));
        
        // Download limit should be applied
        let download_available = manager.download_available();
        assert!(download_available <= 1024); // Again, burst capacity constraint
    }
}
