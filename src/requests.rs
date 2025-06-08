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
    peer_id: String,
    request_type: RequestType,
    response_sender: oneshot::Sender<P2PResponse>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
enum RequestType {
    ChunkRequest { hash: [u8; 32], chunk_id: u32 },
    AuthChallenge { challenge: [u8; 32] },
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
    pub fn pending_count(&self) -> usize {
        self.pending_requests.read().len()
    }

    /// Get pending requests for a specific peer
    pub fn pending_for_peer(&self, peer_id: &str) -> Vec<String> {
        self.pending_requests
            .read()
            .values()
            .filter(|req| req.peer_id == peer_id)
            .map(|req| req.request_id.clone())
            .collect()
    }

    /// Cancel all pending requests for a peer (when peer disconnects)
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
}

/// Helper for creating request/response pairs
pub mod messages {
    use super::*;

    pub fn chunk_request(hash: [u8; 32], chunk_id: u32) -> P2PRequestType {
        P2PRequestType::ChunkRequest { hash, chunk_id }
    }

    pub fn chunk_response(hash: [u8; 32], chunk_id: u32, data: Vec<u8>) -> P2PResponseType {
        P2PResponseType::ChunkResponse { hash, chunk_id, data }
    }

    pub fn auth_challenge(challenge: [u8; 32]) -> P2PRequestType {
        P2PRequestType::AuthChallenge { challenge }
    }

    pub fn auth_response(response: Vec<u8>) -> P2PResponseType {
        P2PResponseType::AuthResponse { response }
    }

    pub fn file_update(
        path: String,
        hash: [u8; 32],
        size: u64,
        chunks: Vec<[u8; 32]>,
    ) -> P2PRequestType {
        P2PRequestType::FileUpdate { path, hash, size, chunks }
    }

    pub fn file_update_ack(success: bool, message: String) -> P2PResponseType {
        P2PResponseType::FileUpdateAck { success, message }
    }

    pub fn error_response(message: String) -> P2PResponseType {
        P2PResponseType::Error { message }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_request_response() {
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
                        assert_eq!(request.request_type, P2PRequestType::ChunkRequest { hash: [1; 32], chunk_id: 0 });
                        Ok(())
                    },
                    Duration::from_secs(1),
                ).await
            })
        };

        // Give the request time to be registered
        tokio::time::sleep(Duration::from_millis(10)).await;

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
    }
}
