//! # Request/Response Management Module
//! 
//! This module provides a robust request/response matching system for P2P communications.
//! It handles asynchronous request-response patterns, timeouts, cleanup, and error handling.
//! 
//! ## Features
//! 
//! - **Request/Response Matching**: Correlates requests with their responses using unique IDs
//! - **Timeout Management**: Automatically times out requests that don't receive responses
//! - **Peer Management**: Tracks and cancels requests when peers disconnect
//! - **Background Cleanup**: Removes stale requests to prevent memory leaks
//! - **Type Safety**: Strongly typed request and response messages
//! 
//! ## Architecture
//! 
//! The system works by:
//! 1. Generating unique request IDs for each outgoing request
//! 2. Storing pending requests with oneshot channels for response delivery
//! 3. Matching incoming responses to pending requests by ID
//! 4. Delivering responses through channels or timing out
//! 5. Cleaning up expired or cancelled requests
//! 
//! ## Usage Example
//! 
//! ```rust,no_run
//! use synccore::requests::{RequestManager, messages};
//! use std::time::Duration;
//! 
//! async fn send_chunk_request() -> anyhow::Result<()> {
//!     let request_manager = RequestManager::new();
//!     
//!     let response = request_manager.send_request(
//!         "peer_123".to_string(),
//!         messages::chunk_request([42u8; 32], 0),
//!         |request| async move {
//!             // Send request over network
//!             println!("Sending request: {:?}", request);
//!             Ok(())
//!         },
//!         Duration::from_secs(10),
//!     ).await?;
//!     
//!     println!("Received response: {:?}", response);
//!     Ok(())
//! }
//! ```


use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};
use tracing::{debug, warn};
use uuid::Uuid;

/// Request/Response matching system for P2P communications
#[derive(Clone)]
pub struct RequestManager {
    pending_requests: Arc<RwLock<HashMap<String, PendingRequest>>>,
}

#[derive(Debug)]
struct PendingRequest {
    request_id: String,
    #[allow(dead_code)]
    peer_id: String,
    #[allow(dead_code)]
    request_type: RequestType,
    response_sender: oneshot::Sender<P2PResponse>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum RequestType {
    ChunkRequest { 
        #[allow(dead_code)]
        hash: [u8; 32], 
        #[allow(dead_code)]
        chunk_id: u32 
    },
    AuthChallenge { 
        #[allow(dead_code)]
        challenge: [u8; 32] 
    },
    FileUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PRequest {
    pub request_id: String,
    pub request_type: P2PRequestType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2PRequestType {
    ChunkRequest { hash: [u8; 32], chunk_id: u32 },
    AuthChallenge { challenge: [u8; 32] },
    FileUpdate { path: String, hash: [u8; 32], size: u64, chunks: Vec<[u8; 32]> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PResponse {
    pub request_id: String,
    pub response_type: P2PResponseType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2PResponseType {
    ChunkResponse { hash: [u8; 32], chunk_id: u32, data: Vec<u8> },
    AuthResponse { response: Vec<u8> },
    FileUpdateAck { success: bool, message: String },
    Error { message: String },
}

impl RequestManager {
    pub fn new() -> Self {
        let manager = Self {
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
        };

        // Start cleanup task
        manager.start_cleanup_task();
        manager
    }

    /// Send a request and wait for response
    #[allow(dead_code)]
    pub async fn send_request<F, Fut>(
        &self,
        peer_id: String,
        request_type: P2PRequestType,
        send_fn: F,
        timeout_duration: Duration,
    ) -> Result<P2PResponse>
    where
        F: FnOnce(P2PRequest) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let request_id = Uuid::new_v4().to_string();
        let (response_tx, response_rx) = oneshot::channel();

        let request = P2PRequest {
            request_id: request_id.clone(),
            request_type: request_type.clone(),
        };

        let pending = PendingRequest {
            request_id: request_id.clone(),
            peer_id: peer_id.clone(),
            request_type: match request_type {
                P2PRequestType::ChunkRequest { hash, chunk_id } => {
                    RequestType::ChunkRequest { hash, chunk_id }
                }
                P2PRequestType::AuthChallenge { challenge } => {
                    RequestType::AuthChallenge { challenge }
                }
                P2PRequestType::FileUpdate { .. } => RequestType::FileUpdate,
            },
            response_sender: response_tx,
            created_at: chrono::Utc::now(),
        };

        // Store pending request
        {
            let mut pending_requests = self.pending_requests.write();
            pending_requests.insert(request_id.clone(), pending);
        }

        // Send the request
        if let Err(e) = send_fn(request).await {
            // Remove from pending if send failed
            self.pending_requests.write().remove(&request_id);
            return Err(e);
        }

        // Wait for response with timeout
        match timeout(timeout_duration, response_rx).await {
            Ok(Ok(response)) => {
                debug!("Received response for request {}", request_id);
                Ok(response)
            }
            Ok(Err(_)) => {
                self.pending_requests.write().remove(&request_id);
                Err(anyhow!("Response channel closed"))
            }
            Err(_) => {
                self.pending_requests.write().remove(&request_id);
                Err(anyhow!("Request timeout"))
            }
        }
    }

    /// Handle incoming response
    pub fn handle_response(&self, response: P2PResponse) -> Result<()> {
        let mut pending_requests = self.pending_requests.write();
        
        if let Some(pending) = pending_requests.remove(&response.request_id) {
            match pending.response_sender.send(response) {
                Ok(_) => {
                    debug!("Delivered response for request {}", pending.request_id);
                    Ok(())
                }
                Err(_) => {
                    warn!("Failed to deliver response - receiver dropped");
                    Err(anyhow!("Response receiver dropped"))
                }
            }
        } else {
            warn!("Received response for unknown request: {}", response.request_id);
            Err(anyhow!("Unknown request ID"))
        }
    }

    /// Get pending request count
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.pending_requests.read().len()
    }

    /// Get pending requests for a specific peer
    #[allow(dead_code)]
    pub fn pending_for_peer(&self, peer_id: &str) -> Vec<String> {
        self.pending_requests
            .read()
            .values()
            .filter(|req| req.peer_id == peer_id)
            .map(|req| req.request_id.clone())
            .collect()
    }

    /// Cancel all pending requests for a peer (when peer disconnects)
    #[allow(dead_code)]
    pub fn cancel_peer_requests(&self, peer_id: &str) {
        let mut pending_requests = self.pending_requests.write();
        let to_remove: Vec<String> = pending_requests
            .values()
            .filter(|req| req.peer_id == peer_id)
            .map(|req| req.request_id.clone())
            .collect();

        for request_id in to_remove {
            if let Some(pending) = pending_requests.remove(&request_id) {
                let error_response = P2PResponse {
                    request_id: pending.request_id.clone(),
                    response_type: P2PResponseType::Error {
                        message: "Peer disconnected".to_string(),
                    },
                };
                let _ = pending.response_sender.send(error_response);
            }
        }
    }

    /// Start background cleanup task
    fn start_cleanup_task(&self) {
        let pending_requests = self.pending_requests.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                let now = chrono::Utc::now();
                let timeout_cutoff = now - chrono::Duration::minutes(5);
                
                let mut requests = pending_requests.write();
                let to_remove: Vec<String> = requests
                    .values()
                    .filter(|req| req.created_at < timeout_cutoff)
                    .map(|req| req.request_id.clone())
                    .collect();
                
                for request_id in to_remove {
                    if let Some(pending) = requests.remove(&request_id) {
                        let timeout_response = P2PResponse {
                            request_id: pending.request_id.clone(),
                            response_type: P2PResponseType::Error {
                                message: "Request timeout".to_string(),
                            },
                        };
                        let _ = pending.response_sender.send(timeout_response);
                    }
                }
            }
        });
    }

    /// Get pending request count (public version)
    #[allow(dead_code)]
    pub async fn pending_request_count(&self) -> usize {
        self.pending_requests.read().len()
    }
}

/// Helper for creating request/response pairs
pub mod messages {
    use super::*;

    #[allow(dead_code)]
    pub fn chunk_request(hash: [u8; 32], chunk_id: u32) -> P2PRequestType {
        P2PRequestType::ChunkRequest { hash, chunk_id }
    }

    #[allow(dead_code)]
    pub fn chunk_response(hash: [u8; 32], chunk_id: u32, data: Vec<u8>) -> P2PResponseType {
        P2PResponseType::ChunkResponse { hash, chunk_id, data }
    }

    #[allow(dead_code)]
    pub fn auth_challenge(challenge: [u8; 32]) -> P2PRequestType {
        P2PRequestType::AuthChallenge { challenge }
    }

    #[allow(dead_code)]
    pub fn auth_response(response: Vec<u8>) -> P2PResponseType {
        P2PResponseType::AuthResponse { response }
    }

    #[allow(dead_code)]
    pub fn file_update(
        path: String,
        hash: [u8; 32],
        size: u64,
        chunks: Vec<[u8; 32]>,
    ) -> P2PRequestType {
        P2PRequestType::FileUpdate { path, hash, size, chunks }
    }

    #[allow(dead_code)]
    pub fn file_update_ack(success: bool, message: String) -> P2PResponseType {
        P2PResponseType::FileUpdateAck { success, message }
    }

    #[allow(dead_code)]
    pub fn error_response(message: String) -> P2PResponseType {
        P2PResponseType::Error { message }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_request_manager_creation() {
        let manager = RequestManager::new();
        assert_eq!(manager.pending_count(), 0);
    }

    #[tokio::test] 
    async fn test_chunk_request_response() {
        let manager = RequestManager::new();
        let peer_id = "test_peer".to_string();
        
        // Simulate sending a chunk request
        let request_task = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager.send_request(
                    peer_id,
                    messages::chunk_request([1; 32], 0),
                    |request| async move {
                        // Simulate sending the request
                        match request.request_type {
                            P2PRequestType::ChunkRequest { hash, chunk_id } => {
                                assert_eq!(hash, [1; 32]);
                                assert_eq!(chunk_id, 0);
                            }
                            _ => panic!("Wrong request type"),
                        }
                        Ok(())
                    },
                    Duration::from_secs(1),
                ).await
            })
        };

        // Give the request time to be registered
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(manager.pending_count(), 1);

        // Find the request ID and send a response
        let request_id = {
            let pending = manager.pending_requests.read();
            pending.keys().next().unwrap().clone()
        };

        let response = P2PResponse {
            request_id,
            response_type: messages::chunk_response([1; 32], 0, vec![1, 2, 3, 4]),
        };

        manager.handle_response(response).unwrap();

        // Wait for the request to complete
        let result = request_task.await.unwrap().unwrap();
        
        match result.response_type {
            P2PResponseType::ChunkResponse { hash, chunk_id, data } => {
                assert_eq!(hash, [1; 32]);
                assert_eq!(chunk_id, 0);
                assert_eq!(data, vec![1, 2, 3, 4]);
            }
            _ => panic!("Unexpected response type"),
        }

        assert_eq!(manager.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_auth_challenge_response() {
        let manager = RequestManager::new();
        let peer_id = "auth_peer".to_string();
        let challenge = [42u8; 32];
        
        let request_task = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager.send_request(
                    peer_id,
                    messages::auth_challenge(challenge),
                    |request| async move {
                        match request.request_type {
                            P2PRequestType::AuthChallenge { challenge: recv_challenge } => {
                                assert_eq!(recv_challenge, [42u8; 32]);
                            }
                            _ => panic!("Wrong request type"),
                        }
                        Ok(())
                    },
                    Duration::from_secs(1),
                ).await
            })
        };

        tokio::time::sleep(Duration::from_millis(10)).await;

        let request_id = {
            let pending = manager.pending_requests.read();
            pending.keys().next().unwrap().clone()
        };

        let auth_response = vec![1, 2, 3, 4, 5];
        let response = P2PResponse {
            request_id,
            response_type: messages::auth_response(auth_response.clone()),
        };

        manager.handle_response(response).unwrap();

        let result = request_task.await.unwrap().unwrap();
        match result.response_type {
            P2PResponseType::AuthResponse { response } => {
                assert_eq!(response, auth_response);
            }
            _ => panic!("Unexpected response type"),
        }
    }

    #[tokio::test]
    async fn test_file_update_request() {
        let manager = RequestManager::new();
        let peer_id = "file_peer".to_string();
        
        let file_path = "test.txt".to_string();
        let file_hash = [99u8; 32];
        let file_size = 1024;
        let chunks = vec![[1u8; 32], [2u8; 32]];
        
        let request_task = {
            let manager = manager.clone();
            let file_path = file_path.clone();
            let chunks = chunks.clone();
            tokio::spawn(async move {
                manager.send_request(
                    peer_id,
                    messages::file_update(file_path.clone(), file_hash, file_size, chunks.clone()),
                    |request| async move {
                        match request.request_type {
                            P2PRequestType::FileUpdate { path, hash, size, chunks: req_chunks } => {
                                assert_eq!(path, file_path);
                                assert_eq!(hash, file_hash);
                                assert_eq!(size, file_size);
                                assert_eq!(req_chunks, chunks);
                            }
                            _ => panic!("Wrong request type"),
                        }
                        Ok(())
                    },
                    Duration::from_secs(1),
                ).await
            })
        };

        tokio::time::sleep(Duration::from_millis(10)).await;

        let request_id = {
            let pending = manager.pending_requests.read();
            pending.keys().next().unwrap().clone()
        };

        let response = P2PResponse {
            request_id,
            response_type: messages::file_update_ack(true, "File received".to_string()),
        };

        manager.handle_response(response).unwrap();

        let result = request_task.await.unwrap().unwrap();
        match result.response_type {
            P2PResponseType::FileUpdateAck { success, message } => {
                assert!(success);
                assert_eq!(message, "File received");
            }
            _ => panic!("Unexpected response type"),
        }
    }

    #[tokio::test]
    async fn test_request_timeout() {
        let manager = RequestManager::new();
        let peer_id = "timeout_peer".to_string();
        
        let result = manager.send_request(
            peer_id,
            messages::chunk_request([1; 32], 0),
            |_request| async move {
                // Don't send any response, let it timeout
                Ok(())
            },
            Duration::from_millis(100), // Short timeout
        ).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timeout"));
        assert_eq!(manager.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_send_failure() {
        let manager = RequestManager::new();
        let peer_id = "fail_peer".to_string();
        
        let result = manager.send_request(
            peer_id,
            messages::chunk_request([1; 32], 0),
            |_request| async move {
                Err(anyhow!("Send failed"))
            },
            Duration::from_secs(1),
        ).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Send failed"));
        assert_eq!(manager.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_unknown_response() {
        let manager = RequestManager::new();
        
        let response = P2PResponse {
            request_id: "unknown_id".to_string(),
            response_type: messages::error_response("Test error".to_string()),
        };

        let result = manager.handle_response(response);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown request ID"));
    }

    #[tokio::test]
    async fn test_pending_for_peer() {
        let manager = RequestManager::new();
        let peer1 = "peer1".to_string();
        let peer2 = "peer2".to_string();
        
        // Start requests for different peers
        let _task1 = {
            let manager = manager.clone();
            tokio::spawn(async move {
                let _ = manager.send_request(
                    peer1,
                    messages::chunk_request([1; 32], 0),
                    |_| async { Ok(()) },
                    Duration::from_secs(10),
                ).await;
            })
        };

        let _task2 = {
            let manager = manager.clone();
            tokio::spawn(async move {
                let _ = manager.send_request(
                    peer2,
                    messages::chunk_request([2; 32], 1),
                    |_| async { Ok(()) },
                    Duration::from_secs(10),
                ).await;
            })
        };

        tokio::time::sleep(Duration::from_millis(10)).await;

        let peer1_requests = manager.pending_for_peer("peer1");
        let peer2_requests = manager.pending_for_peer("peer2");
        let unknown_requests = manager.pending_for_peer("unknown");

        assert_eq!(peer1_requests.len(), 1);
        assert_eq!(peer2_requests.len(), 1);
        assert_eq!(unknown_requests.len(), 0);
        assert_eq!(manager.pending_count(), 2);
    }

    #[tokio::test]
    async fn test_cancel_peer_requests() {
        let manager = RequestManager::new();
        let peer_id = "cancel_peer".to_string();
        
        let request_task = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager.send_request(
                    peer_id,
                    messages::chunk_request([1; 32], 0),
                    |_| async { Ok(()) },
                    Duration::from_secs(10),
                ).await
            })
        };

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(manager.pending_count(), 1);

        // Cancel requests for the peer
        manager.cancel_peer_requests("cancel_peer");

        // Request should complete with error
        let result = request_task.await.unwrap();
        assert!(result.is_ok()); // The response is delivered, but it's an error response
        
        let response = result.unwrap();
        match response.response_type {
            P2PResponseType::Error { message } => {
                assert_eq!(message, "Peer disconnected");
            }
            _ => panic!("Expected error response"),
        }

        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_message_helpers() {
        // Test chunk request
        let chunk_req = messages::chunk_request([42u8; 32], 123);
        match chunk_req {
            P2PRequestType::ChunkRequest { hash, chunk_id } => {
                assert_eq!(hash, [42u8; 32]);
                assert_eq!(chunk_id, 123);
            }
            _ => panic!("Wrong request type"),
        }

        // Test chunk response
        let chunk_resp = messages::chunk_response([42u8; 32], 123, vec![1, 2, 3]);
        match chunk_resp {
            P2PResponseType::ChunkResponse { hash, chunk_id, data } => {
                assert_eq!(hash, [42u8; 32]);
                assert_eq!(chunk_id, 123);
                assert_eq!(data, vec![1, 2, 3]);
            }
            _ => panic!("Wrong response type"),
        }

        // Test auth challenge
        let auth_challenge = messages::auth_challenge([99u8; 32]);
        match auth_challenge {
            P2PRequestType::AuthChallenge { challenge } => {
                assert_eq!(challenge, [99u8; 32]);
            }
            _ => panic!("Wrong request type"),
        }

        // Test auth response
        let auth_resp = messages::auth_response(vec![4, 5, 6]);
        match auth_resp {
            P2PResponseType::AuthResponse { response } => {
                assert_eq!(response, vec![4, 5, 6]);
            }
            _ => panic!("Wrong response type"),
        }

        // Test file update
        let file_update = messages::file_update("test.txt".to_string(), [77u8; 32], 1024, vec![[1u8; 32]]);
        match file_update {
            P2PRequestType::FileUpdate { path, hash, size, chunks } => {
                assert_eq!(path, "test.txt");
                assert_eq!(hash, [77u8; 32]);
                assert_eq!(size, 1024);
                assert_eq!(chunks, vec![[1u8; 32]]);
            }
            _ => panic!("Wrong request type"),
        }

        // Test file update ack
        let file_ack = messages::file_update_ack(true, "Success".to_string());
        match file_ack {
            P2PResponseType::FileUpdateAck { success, message } => {
                assert!(success);
                assert_eq!(message, "Success");
            }
            _ => panic!("Wrong response type"),
        }

        // Test error response
        let error_resp = messages::error_response("Test error".to_string());
        match error_resp {
            P2PResponseType::Error { message } => {
                assert_eq!(message, "Test error");
            }
            _ => panic!("Wrong response type"),
        }
    }

    #[test]
    fn test_request_response_serialization() {
        // Test request serialization
        let request = P2PRequest {
            request_id: "test_123".to_string(),
            request_type: P2PRequestType::ChunkRequest { hash: [42u8; 32], chunk_id: 5 },
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized: P2PRequest = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.request_id, request.request_id);
        match (request.request_type, deserialized.request_type) {
            (P2PRequestType::ChunkRequest { hash: h1, chunk_id: c1 }, 
             P2PRequestType::ChunkRequest { hash: h2, chunk_id: c2 }) => {
                assert_eq!(h1, h2);
                assert_eq!(c1, c2);
            }
            _ => panic!("Serialization failed"),
        }

        // Test response serialization
        let response = P2PResponse {
            request_id: "test_456".to_string(),
            response_type: P2PResponseType::ChunkResponse { 
                hash: [99u8; 32], 
                chunk_id: 7, 
                data: vec![1, 2, 3, 4] 
            },
        };

        let serialized = serde_json::to_string(&response).unwrap();
        let deserialized: P2PResponse = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.request_id, response.request_id);
        match (response.response_type, deserialized.response_type) {
            (P2PResponseType::ChunkResponse { hash: h1, chunk_id: c1, data: d1 }, 
             P2PResponseType::ChunkResponse { hash: h2, chunk_id: c2, data: d2 }) => {
                assert_eq!(h1, h2);
                assert_eq!(c1, c2);
                assert_eq!(d1, d2);
            }
            _ => panic!("Serialization failed"),
        }
    }

    #[tokio::test]
    async fn test_cleanup_task() {
        // This test is harder to verify directly since the cleanup task runs in background
        // We'll just ensure the manager starts correctly and doesn't panic
        let manager = RequestManager::new();
        assert_eq!(manager.pending_count(), 0);
        
        // Let it run for a short time
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(manager.pending_count(), 0);
    }
}
