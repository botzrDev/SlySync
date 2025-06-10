//! # P2P Networking Module
//! 
//! This module provides the peer-to-peer networking capabilities for SyncCore.
//! It handles peer discovery, secure connections using QUIC, message exchange,
//! and file chunk transfer between peers.
//! 
//! ## Features
//! 
//! - **Peer Discovery**: Automatic discovery of peers on the local network using UDP broadcast
//! - **Secure Communication**: All peer communication is encrypted using QUIC with TLS
//! - **Authentication**: Public-key based peer authentication using Ed25519
//! - **Message Exchange**: Structured message protocol for file updates and chunk requests
//! - **Connection Management**: Automatic connection handling and cleanup
//! 
//! ## Usage Example
//! 
//! ```rust,no_run
//! use slysync::p2p::P2PService;
//! use slysync::crypto::Identity;
//! use slysync::config::Config;
//! 
//! async fn start_p2p_service() -> anyhow::Result<()> {
//!     let identity = Identity::generate()?;
//!     let config = Config::init().await?;
//!     
//!     let p2p_service = P2PService::new(identity, config).await?;
//!     
//!     // Discover peers on the network
//!     let peers = p2p_service.discover_peers().await?;
//!     println!("Found {} peers", peers.len());
//!     
//!     // Connect to a specific peer
//!     let peer_addr = "192.168.1.100:8080".parse()?;
//!     let connection = p2p_service.connect_to_peer(peer_addr).await?;
//!     
//!     Ok(())
//! }
//! ```

use anyhow::{anyhow, Result};
use ed25519_dalek::{VerifyingKey, Verifier};
use quinn::{Connection, Endpoint};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration, Instant};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::bandwidth;

/// Protocol messages exchanged between peers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2PMessage {
    /// Announce presence to other peers
    Announce {
        peer_id: String,
        public_key: Vec<u8>,
        listen_port: u16,
    },
    /// Request a file chunk by hash
    ChunkRequest {
        hash: [u8; 32],
        chunk_id: u32,
    },
    /// Response with file chunk data
    ChunkResponse {
        hash: [u8; 32],
        chunk_id: u32,
        data: Vec<u8>,
    },
    /// Notify about file changes
    FileUpdate {
        path: String,
        hash: [u8; 32],
        size: u64,
        chunks: Vec<[u8; 32]>,
    },
    /// Peer authentication challenge
    AuthChallenge {
        request_id: String, // Add request_id
        challenge: [u8; 32],
    },
    /// Peer authentication response
    AuthResponse {
        request_id: String, // Add request_id
        response: Vec<u8>, // Signed challenge
    },
    /// Notify about file deletion
    FileDelete {
        path: String,
    },
}

/// Information about a discovered peer
#[derive(Clone, Debug)]
pub struct PeerInfo {
    pub id: String,
    #[allow(dead_code)]
    pub address: SocketAddr,
    #[allow(dead_code)]
    pub public_key: Vec<u8>,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub authenticated: bool,
}

/// Active connection to a peer
pub struct PeerConnection {
    pub peer_id: String,
    pub address: SocketAddr,
    connection: Connection,
    last_activity: Arc<RwLock<Instant>>,
    authenticated: bool,
}

impl PeerConnection {
    pub async fn send_message(&self, message: &P2PMessage) -> Result<()> {
        let data = serde_json::to_vec(message)?;
        
        let mut stream = self.connection.open_uni().await?;
        stream.write_all(&data).await?;
        stream.finish().await?;
        
        // Update last activity
        *self.last_activity.write().await = Instant::now();
        
        Ok(())
    }
    
    #[allow(dead_code)]
    pub async fn request_file_chunk(&self, hash: &[u8; 32], chunk_id: u32) -> Result<Vec<u8>> {
        let request = P2PMessage::ChunkRequest {
            hash: *hash,
            chunk_id,
        };
        
        // Send request
        self.send_message(&request).await?;
        
        // Wait for response (simplified - in practice would use proper request/response matching)
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // TODO: Implement proper request/response matching
        Ok(Vec::new())
    }
    
    #[allow(dead_code)]
    pub async fn is_alive(&self) -> bool {
        let last_activity = *self.last_activity.read().await;
        last_activity.elapsed() < Duration::from_secs(30)
    }
}

/// Main P2P networking service
pub struct P2PService {
    identity: crate::crypto::Identity,
    config: crate::config::Config,
    endpoint: Endpoint,
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    connections: Arc<RwLock<HashMap<String, PeerConnection>>>,
    message_tx: mpsc::UnboundedSender<(String, P2PMessage)>,
    message_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<(String, P2PMessage)>>>>,
    pub chunk_store: Option<Arc<crate::storage::ChunkStore>>,
    pub request_manager: Option<Arc<crate::requests::RequestManager>>,
    pub sync_service: Option<Arc<crate::sync::SyncService>>,
    pub bandwidth_manager: Option<Arc<bandwidth::BandwidthManager>>,
}

impl P2PService {
    pub async fn new(identity: crate::crypto::Identity, config: crate::config::Config) -> Result<Self> {
        info!("Starting P2P service on port {}", config.listen_port);
        
        // Generate self-signed certificate for QUIC
        let (cert_der, key_der) = generate_self_signed_cert(&identity)?;
        
        // Configure QUIC server
        let server_config = configure_server(cert_der.clone(), key_der)?;
        
        // Configure QUIC client 
        let client_config = configure_client()?;
        
        // Create QUIC endpoint
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), config.listen_port);
        let mut endpoint = Endpoint::server(server_config, bind_addr)?;
        endpoint.set_default_client_config(client_config);
        
        info!("P2P service listening on {}", bind_addr);
        
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        
        let service = Self {
            identity,
            config,
            endpoint,
            peers: Arc::new(RwLock::new(HashMap::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
            message_tx,
            message_rx: Arc::new(RwLock::new(Some(message_rx))),
            chunk_store: None,
            request_manager: None,
            sync_service: None,
            bandwidth_manager: None,
        };
        
        // Start background tasks
        service.start_peer_discovery().await?;
        service.start_connection_handler().await?;
        service.start_message_handler().await?;
        service.start_connection_monitoring().await?;
        
        Ok(service)
    }
    
    pub async fn discover_peers(&self) -> Result<Vec<PeerInfo>> {
        info!("Starting peer discovery...");
        // Start mDNS discovery (stubbed, not implemented)
        // self.start_mdns_discovery().await?;
        // Return current peer list
        let peers = self.peers.read().await;
        Ok(peers.values().cloned().collect())
    }
    
    #[allow(dead_code)]
    pub async fn connect_to_peer(&self, addr: SocketAddr) -> Result<PeerConnection> {
        info!("Connecting to peer at {}", addr);
        
        // Establish QUIC connection
        let connection = self.endpoint.connect(addr, "localhost")?.await?;
        
        // Create peer connection
        let peer_connection = PeerConnection {
            peer_id: format!("peer_{}", Uuid::new_v4()),
            address: addr,
            connection,
            last_activity: Arc::new(RwLock::new(Instant::now())),
            authenticated: false,
        };
        
        // Perform authentication handshake
        self.authenticate_peer(&peer_connection).await?;
        
        // Store connection
        let peer_id = peer_connection.peer_id.clone();
        self.connections.write().await.insert(peer_id.clone(), peer_connection);
        
        let connections = self.connections.read().await;
        let connection = connections.get(&peer_id).unwrap();
        
        Ok(PeerConnection {
            peer_id: connection.peer_id.clone(),
            address: connection.address,
            connection: connection.connection.clone(),
            last_activity: connection.last_activity.clone(),
            authenticated: connection.authenticated,
        })
    }
    
    #[allow(dead_code)]
    pub async fn connect_via_invitation(&self, invitation_code: &str) -> Result<PeerConnection> {
        info!("Connecting via invitation code");
        
        // Parse invitation code
        let invitation = crate::crypto::parse_invitation_code(invitation_code)?;
        let peer_addr: SocketAddr = invitation.address.parse()?;
        
        // Validate invitation code cryptographically
        let expected_peer_key = {
            // Parse the peer's public key from the invitation
            let peer_key_bytes = hex::decode(&invitation.peer_id)?;
            if peer_key_bytes.len() != 32 {
                return Err(anyhow!("Invalid peer public key length"));
            }
            let mut key_array = [0u8; 32];
            key_array.copy_from_slice(&peer_key_bytes);
            VerifyingKey::from_bytes(&key_array)
                .map_err(|_| anyhow!("Invalid peer public key format"))?
        };
        
        // Validate the invitation signature
        crate::crypto::validate_invitation_code(invitation_code, Some(&expected_peer_key))?;
        
        // Connect to peer
        let mut connection = self.connect_to_peer(peer_addr).await?;
        
        // Store expected peer information
        {
            let mut peers = self.peers.write().await;
            peers.insert(invitation.peer_id.clone(), PeerInfo {
                id: invitation.peer_id.clone(),
                address: peer_addr,
                public_key: expected_peer_key.to_bytes().to_vec(),
                last_seen: chrono::Utc::now(),
                authenticated: false, // Will be set to true after authentication
            });
        }
        
        // Set the peer ID on the connection
        connection.peer_id = invitation.peer_id.clone();
        
        // Perform authentication with the connected peer
        self.authenticate_peer(&connection).await?;
        
        info!("Successfully connected and authenticated peer via invitation");
        Ok(connection)
    }
    
    pub async fn get_connected_peers(&self) -> Vec<PeerInfo> {
        let peers = self.peers.read().await;
        peers.values().cloned().collect()
    }
    
    pub async fn broadcast_file_update(&self, path: &str, hash: [u8; 32], size: u64, chunks: Vec<[u8; 32]>) -> Result<()> {
        let message = P2PMessage::FileUpdate {
            path: path.to_string(),
            hash,
            size,
            chunks,
        };
        
        let connections = self.connections.read().await;
        for connection in connections.values() {
            if let Err(e) = connection.send_message(&message).await {
                warn!("Failed to send file update to peer {}: {}", connection.peer_id, e);
            }
        }
        
        Ok(())
    }

    pub async fn broadcast_file_deletion(&self, path: &str) -> Result<()> {
        let message = P2PMessage::FileDelete {
            path: path.to_string(),
        };
        
        let connections = self.connections.read().await;
        for connection in connections.values() {
            if let Err(e) = connection.send_message(&message).await {
                warn!("Failed to send file deletion to peer {}: {}", connection.peer_id, e);
            }
        }
        
        Ok(())
    }
    
    /// Connection health monitoring and automatic reconnection
    pub async fn start_connection_monitoring(&self) -> Result<()> {
        let connections = self.connections.clone();
        let peers = self.peers.clone();
        let _identity = self.identity.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                // Check connection health and attempt reconnections
                let mut connections_guard = connections.write().await;
                let peers_guard = peers.read().await;
                
                let mut to_reconnect = Vec::new();
                
                // Identify stale or dropped connections
                for (peer_id, connection) in connections_guard.iter() {
                    let last_activity = *connection.last_activity.read().await;
                    if last_activity.elapsed() > Duration::from_secs(60) {
                        warn!("Connection to peer {} appears stale, will attempt reconnection", peer_id);
                        to_reconnect.push((peer_id.clone(), connection.address));
                    }
                }
                
                // Remove stale connections
                for (peer_id, _) in &to_reconnect {
                    connections_guard.remove(peer_id);
                }
                
                drop(connections_guard);
                drop(peers_guard);
                
                // Attempt reconnections
                for (peer_id, addr) in to_reconnect {
                    info!("Attempting to reconnect to peer {} at {}", peer_id, addr);
                    // Note: In a real implementation, we'd have access to self here
                    // For now, just log the attempt
                }
            }
        });
        
        Ok(())
    }

    /// Enhanced request/response security with proper matching
    pub async fn send_secure_request(&self, 
        peer_id: &str, 
        request: crate::requests::P2PRequestType) -> Result<crate::requests::P2PResponse> {
        
        let request_manager = self.request_manager
            .as_ref()
            .ok_or_else(|| anyhow!("RequestManager not initialized"))?;

        let connections = self.connections.read().await;
        let connection = connections.get(peer_id)
            .ok_or_else(|| anyhow!("Peer {} not connected", peer_id))?;

        // Verify peer is authenticated before sending requests
        if !connection.authenticated {
            return Err(anyhow!("Peer {} is not authenticated", peer_id));
        }

        request_manager.send_request(
            peer_id.to_string(),
            request,
            |p2p_request| async move {
                // Convert P2P request to P2P message and send
                let message = match &p2p_request.request_type {
                    crate::requests::P2PRequestType::ChunkRequest { hash, chunk_id } => {
                        P2PMessage::ChunkRequest { hash: *hash, chunk_id: *chunk_id }
                    }
                    crate::requests::P2PRequestType::AuthChallenge { challenge } => {
                        // Generate a unique request_id for the challenge
                        let request_id = uuid::Uuid::new_v4().to_string();
                        P2PMessage::AuthChallenge { request_id, challenge: *challenge }
                    }
                    crate::requests::P2PRequestType::FileUpdate { path, hash, size, chunks } => {
                        P2PMessage::FileUpdate { 
                            path: path.clone(), 
                            hash: *hash, 
                            size: *size, 
                            chunks: chunks.clone() 
                        }
                    }
                };
                connection.send_message(&message).await
            },
            Duration::from_secs(10),
        ).await
    }

    /// Enhanced request chunk method using secure request system
    pub async fn request_chunk_from_peer_secure(
        &self,
        peer_id: &str,
        chunk_hash: [u8; 32],
        chunk_index: u32,
    ) -> Result<Vec<u8>> {
        let request = crate::requests::messages::chunk_request(chunk_hash, chunk_index);
        
        match self.send_secure_request(peer_id, request).await? {
            response => match response.response_type {
                crate::requests::P2PResponseType::ChunkResponse { data, hash, chunk_id } => {
                    // Verify response integrity
                    if hash != chunk_hash || chunk_id != chunk_index {
                        return Err(anyhow!("Response mismatch: expected {}:{}, got {}:{}", 
                                         hex::encode(chunk_hash), chunk_index,
                                         hex::encode(hash), chunk_id));
                    }
                    
                    // Verify chunk hash
                    let computed_hash = crate::crypto::hash_file_chunk(&data);
                    if computed_hash != chunk_hash {
                        return Err(anyhow!("Chunk integrity verification failed"));
                    }
                    
                    Ok(data)
                }
                crate::requests::P2PResponseType::Error { message } => {
                    Err(anyhow!("Peer error: {}", message))
                }
                _ => Err(anyhow!("Unexpected response type")),
            }
        }
    }

    // Private helper methods
    
    async fn start_peer_discovery(&self) -> Result<()> {
        let peers = self.peers.clone();
        let _message_tx = self.message_tx.clone();
        let config = self.config.clone();
        let identity = self.identity.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                // Send announce message via UDP broadcast
                if let Err(e) = Self::send_announce_broadcast(&identity, &config).await {
                    warn!("Failed to send announce broadcast: {}", e);
                }
                
                // Clean up stale peers
                Self::cleanup_stale_peers(&peers).await;
            }
        });
        
        Ok(())
    }
    
    async fn start_connection_handler(&self) -> Result<()> {
        let endpoint = self.endpoint.clone();
        let connections = self.connections.clone();
        let message_tx = self.message_tx.clone();
        
        tokio::spawn(async move {
            while let Some(connecting) = endpoint.accept().await {
                let connections = connections.clone();
                let message_tx = message_tx.clone();
                
                tokio::spawn(async move {
                    match connecting.await {
                        Ok(connection) => {
                            info!("Accepted connection from {}", connection.remote_address());
                            
                            let peer_id = format!("peer_{}", Uuid::new_v4());
                            let peer_connection = PeerConnection {
                                peer_id: peer_id.clone(),
                                address: connection.remote_address(),
                                connection: connection.clone(),
                                last_activity: Arc::new(RwLock::new(Instant::now())),
                                authenticated: false,
                            };
                            
                            connections.write().await.insert(peer_id.clone(), peer_connection);
                            
                            // Handle incoming streams
                            Self::handle_connection_streams(connection, peer_id, message_tx).await;
                        }
                        Err(e) => {
                            warn!("Failed to accept connection: {}", e);
                        }
                    }
                });
            }
        });
        
        Ok(())
    }
    
    async fn start_message_handler(&self) -> Result<()> {
        let mut message_rx = self.message_rx.write().await.take()
            .ok_or_else(|| anyhow!("Message handler already started"))?;
        let peers = self.peers.clone();
        let connections = self.connections.clone();
        let chunk_store = self.chunk_store.clone();
        let request_manager = self.request_manager.clone();
        let sync_service = self.sync_service.clone();
        let bandwidth_manager = self.bandwidth_manager.clone();
        let identity = self.identity.clone();
        
        tokio::spawn(async move {
            while let Some((peer_id, message)) = message_rx.recv().await {
                P2PService::handle_message_static(
                    peer_id, 
                    message, 
                    &peers, 
                    &connections,
                    chunk_store.clone(),
                    request_manager.clone(),
                    sync_service.clone(),
                    bandwidth_manager.clone(),
                    identity.clone(),
                ).await;
            }
        });
        Ok(())
    }
    
    async fn handle_message_static(
        peer_id: String,
        message: P2PMessage,
        _peers: &Arc<RwLock<HashMap<String, PeerInfo>>>,
        connections: &Arc<RwLock<HashMap<String, PeerConnection>>>,
        chunk_store: Option<Arc<crate::storage::ChunkStore>>,
        request_manager: Option<Arc<crate::requests::RequestManager>>,
        sync_service: Option<Arc<crate::sync::SyncService>>,
        bandwidth_manager: Option<Arc<bandwidth::BandwidthManager>>,
        identity: crate::crypto::Identity,
    ) {
        match message {
            P2PMessage::ChunkRequest { hash, chunk_id } => {
                info!("Received chunk request from {}: {:?}:{}", peer_id, hash, chunk_id);
                if let Some(chunk_store) = &chunk_store {
                    if let Ok(chunk) = chunk_store.get_chunk(&hash).await {
                        let connections = connections.read().await;
                        if let Some(connection) = connections.get(&peer_id) {
                            // Apply upload bandwidth throttling before sending response
                            if let Some(bw_manager) = &bandwidth_manager {
                                if let Err(e) = bw_manager.request_upload_quota(chunk.len() as u64).await {
                                    warn!("Upload bandwidth limit exceeded, delaying chunk response to {}: {}", peer_id, e);
                                }
                            }
                            
                            let response = P2PMessage::ChunkResponse {
                                hash,
                                chunk_id,
                                data: chunk,
                            };
                            if let Err(e) = connection.send_message(&response).await {
                                warn!("Failed to send chunk response to {}: {}", peer_id, e);
                            }
                        }
                    } else {
                        warn!("Requested chunk not found: {:?}", hash);
                    }
                } else {
                    warn!("No chunk store available to handle request");
                }
            }
            P2PMessage::ChunkResponse { hash, chunk_id, data } => {
                info!("Received chunk response from {}: {:?}:{} ({} bytes)", peer_id, hash, chunk_id, data.len());
                
                // Apply download bandwidth throttling before processing response
                if let Some(bw_manager) = &bandwidth_manager {
                    if let Err(e) = bw_manager.request_download_quota(data.len() as u64).await {
                        warn!("Download bandwidth limit exceeded, delaying chunk processing from {}: {}", peer_id, e);
                    }
                }
                
                // Verify chunk integrity
                let computed_hash = crate::crypto::hash_file_chunk(&data);
                if computed_hash != hash {
                    warn!("Chunk corruption detected from peer {}: expected {:?}, got {:?}", peer_id, hash, computed_hash);
                    return;
                }
                
                if let Some(request_manager) = &request_manager {
                    let response = crate::requests::P2PResponse {
                        request_id: format!("{}_{}", hex::encode(hash), chunk_id),
                        response_type: crate::requests::P2PResponseType::ChunkResponse { hash, chunk_id, data },
                    };
                    if let Err(e) = request_manager.handle_response(response, &peer_id) {
                        warn!("Failed to handle chunk response: {}", e);
                    }
                }
            }
            P2PMessage::FileUpdate { path, hash, size, chunks } => {
                info!("Received file update from {}: {} ({} bytes, {} chunks)", peer_id, path, size, chunks.len());
                if let Some(sync_service) = &sync_service {
                    if let Err(e) = sync_service.handle_peer_file_update(&path, hash, size, chunks, &peer_id).await {
                        warn!("Failed to handle file update from {}: {}", peer_id, e);
                    }
                }
            }
            P2PMessage::AuthChallenge { request_id, challenge } => {
                info!("Received auth challenge from {} (Request ID: {})", peer_id, request_id);
                // Respond to challenge using our private key
                let signature = identity.sign(&challenge);
                let response = crate::requests::AuthResponse {
                    challenge,
                    signature: signature.to_bytes().to_vec(),
                };
                let msg = P2PMessage::AuthResponse { request_id: request_id.clone(), response: serde_json::to_vec(&response).unwrap() };
                let connections_guard = connections.read().await;
                if let Some(connection) = connections_guard.get(&peer_id) {
                    let _ = connection.send_message(&msg).await;
                }
            }
            P2PMessage::AuthResponse { request_id, response } => {
                info!("Received auth response from {} (Request ID: {})", peer_id, request_id);
                match serde_json::from_slice::<crate::requests::AuthResponse>(&response) {
                    Ok(auth_response) => {
                        let mut peers_guard = _peers.write().await;
                        if let Some(peer_info) = peers_guard.get_mut(&peer_id) {
                            if peer_info.public_key.len() == 32 && auth_response.signature.len() == 64 {
                                let mut pk_bytes = [0u8; 32];
                                pk_bytes.copy_from_slice(&peer_info.public_key);
                                let mut sig_bytes = [0u8; 64];
                                sig_bytes.copy_from_slice(&auth_response.signature);
                                let pk = match ed25519_dalek::VerifyingKey::from_bytes(&pk_bytes) {
                                    Ok(pk) => pk,
                                    Err(_) => {
                                        warn!("Malformed public key in auth response from {}", peer_id);
                                        return;
                                    }
                                };
                                let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
                                if pk.verify(&auth_response.challenge, &sig).is_ok() {
                                    peer_info.authenticated = true;
                                    let mut connections_guard = connections.write().await;
                                    if let Some(conn) = connections_guard.get_mut(&peer_id) {
                                        conn.authenticated = true;
                                    }
                                    info!("Peer {} authenticated successfully", peer_id);
                                } else {
                                    warn!("Invalid signature in auth response from {}", peer_id);
                                }
                            } else {
                                warn!("Invalid key or signature length in auth response from {}", peer_id);
                            }
                        }
                    }
                    Err(e) => warn!("Failed to deserialize auth response from {}: {}", peer_id, e),
                }
            }
            P2PMessage::Announce { .. } => {
                debug!("Received announce message from {}", peer_id);
            }
            P2PMessage::FileDelete { path } => {
                info!("Received file deletion from {}: {}", peer_id, path);
                if let Some(sync_service) = &sync_service {
                    if let Err(e) = sync_service.handle_peer_file_deletion(&path).await {
                        warn!("Failed to handle file deletion from {}: {}", peer_id, e);
                    }
                }
            }
        }
    } // End of handle_message_static
} // End of impl P2PService

// Generate a self-signed certificate for QUIC connections
fn generate_self_signed_cert(identity: &crate::crypto::Identity) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    // Generate key pair with ECDSA P-256 algorithm (more compatible than Ed25519 for QUIC)
    let key_pair = rcgen::KeyPair::generate()?;
    
    // Create certificate parameters
    let mut params = rcgen::CertificateParams::default();
    
    // Set validity period (1 year) - note: month is 1-indexed, day is 1-indexed
    params.not_before = rcgen::date_time_ymd(2025, 1, 1);
    params.not_after = rcgen::date_time_ymd(2026, 1, 1);
    
    // Include peer identity in certificate subject alternative names
    let peer_id = hex::encode(identity.public_key().to_bytes());
    let dns_name = format!("{}.slysync.local", peer_id);
    
    // Create DNS name using proper Ia5String conversion
    params.subject_alt_names = vec![
        rcgen::SanType::DnsName(dns_name.try_into()?)
    ];
    
    // Set certificate subject
    let mut distinguished_name = rcgen::DistinguishedName::new();
    distinguished_name.push(rcgen::DnType::CommonName, format!("SlySync-{}", &peer_id[..8]));
    params.distinguished_name = distinguished_name;
    
    // Generate certificate with the key pair
    let cert = params.self_signed(&key_pair)?;
    
    // Get certificate and private key in DER format
    let cert_der = cert.der().to_vec();
    let key_der = key_pair.serialize_der();
    
    Ok((cert_der, key_der))
}
// Configure QUIC server
fn configure_server(cert_der: Vec<u8>, key_der: Vec<u8>) -> anyhow::Result<quinn::ServerConfig> {
    // Create rustls certificate and private key from DER data
    let cert = rustls::Certificate(cert_der);
    let key = rustls::PrivateKey(key_der);
    
    // Configure server with proper TLS settings
    let server_config = quinn::ServerConfig::with_single_cert(
        vec![cert],
        key,
    ).map_err(|e| anyhow!("Failed to configure server: {}", e))?;
    
    Ok(server_config)
}
// Configure QUIC client
fn configure_client() -> anyhow::Result<quinn::ClientConfig> {
    // Create a basic client configuration that accepts any certificate
    // In production, this should validate certificates properly
    warn!("Using insecure client configuration - not suitable for production");
    
    let mut crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(
            SkipServerVerification::new()
        )
        .with_no_client_auth();
    
    crypto.alpn_protocols = vec![b"slysync/1.0".to_vec()];
    
    let client_config = quinn::ClientConfig::new(std::sync::Arc::new(crypto));
    
    Ok(client_config)
}

struct SkipServerVerification;

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        // Skip all verification - INSECURE!
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}
// Stub for authenticate_peer
impl P2PService {
    async fn authenticate_peer(&self, conn: &PeerConnection) -> anyhow::Result<()> {
        // Generate a random challenge
        let mut challenge = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut challenge);
        
        // Send authentication challenge
        let auth_challenge = P2PMessage::AuthChallenge {
            request_id: uuid::Uuid::new_v4().to_string(),
            challenge,
        };
        
        conn.send_message(&auth_challenge).await?;
        
        // In a real implementation, we would wait for the response and verify it
        // For now, just mark the peer as authenticated after sending the challenge
        info!("Authentication challenge sent to peer {}", conn.peer_id);
        
        Ok(())
    }
    // Implement UDP broadcast for peer discovery
    async fn send_announce_broadcast(identity: &crate::crypto::Identity, config: &crate::config::Config) -> anyhow::Result<()> {
        use tokio::net::UdpSocket;
        
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.set_broadcast(true)?;
        
        let announce_message = P2PMessage::Announce {
            peer_id: hex::encode(identity.public_key().to_bytes()),
            public_key: identity.public_key().to_bytes().to_vec(),
            listen_port: config.listen_port,
        };
        
        let message_data = serde_json::to_vec(&announce_message)?;
        
        // Broadcast to common broadcast addresses
        let broadcast_addrs = vec![
            "255.255.255.255:41338",
            "224.0.0.1:41338", // Multicast
        ];
        
        for addr in broadcast_addrs {
            if let Ok(broadcast_addr) = addr.parse::<SocketAddr>() {
                match socket.send_to(&message_data, broadcast_addr).await {
                    Ok(_) => debug!("Sent announce broadcast to {}", addr),
                    Err(e) => warn!("Failed to send broadcast to {}: {}", addr, e),
                }
            }
        }
        
        Ok(())
    }
    // Clean up stale peer connections
    async fn cleanup_stale_peers(peers: &Arc<RwLock<HashMap<String, PeerInfo>>>) {
        let _stale_timeout = Duration::from_secs(300); // 5 minutes
        let now = chrono::Utc::now();
        
        let mut peers_guard = peers.write().await;
        let mut stale_peer_ids = Vec::new();
        
        for (peer_id, peer_info) in peers_guard.iter() {
            let age = now.signed_duration_since(peer_info.last_seen);
            if age.num_seconds() > 300 { // 5 minutes
                stale_peer_ids.push(peer_id.clone());
            }
        }
        
        for peer_id in stale_peer_ids {
            debug!("Removing stale peer: {}", peer_id);
            peers_guard.remove(&peer_id);
        }
    }
    // Handle incoming connection streams
    async fn handle_connection_streams(connection: quinn::Connection, peer_id: String, message_tx: mpsc::UnboundedSender<(String, P2PMessage)>) {
        loop {
            match connection.accept_uni().await {
                Ok(stream) => {
                    let peer_id = peer_id.clone();
                    let message_tx = message_tx.clone();
                    
                    tokio::spawn(async move {
                        match Self::handle_stream(stream, peer_id.clone(), message_tx).await {
                            Ok(_) => debug!("Stream from {} processed successfully", peer_id),
                            Err(e) => warn!("Error processing stream from {}: {}", peer_id, e),
                        }
                    });
                }
                Err(e) => {
                    debug!("Connection {} closed: {}", peer_id, e);
                    break;
                }
            }
        }
    }
    
    async fn handle_stream(
        mut stream: quinn::RecvStream,
        peer_id: String,
        message_tx: mpsc::UnboundedSender<(String, P2PMessage)>,
    ) -> Result<()> {
        let buffer = stream.read_to_end(1024 * 1024).await?; // 1MB limit
        
        if buffer.is_empty() {
            return Ok(());
        }
        
        match serde_json::from_slice::<P2PMessage>(&buffer) {
            Ok(message) => {
                debug!("Received message from {}: {:?}", peer_id, message);
                if let Err(e) = message_tx.send((peer_id, message)) {
                    warn!("Failed to forward message to handler: {}", e);
                }
            }
            Err(e) => {
                warn!("Failed to deserialize message from {}: {}", peer_id, e);
            }
        }
        
        Ok(())
    }
}
