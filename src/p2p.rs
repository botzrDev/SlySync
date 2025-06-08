use anyhow::{anyhow, Result};
use quinn::{ClientConfig, Connection, Endpoint, ServerConfig};
use rustls::{Certificate, PrivateKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration, Instant};
use tracing::{error, info, warn};
use uuid::Uuid;

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
        challenge: [u8; 32],
    },
    /// Peer authentication response
    AuthResponse {
        response: Vec<u8>, // Signed challenge
    },
}

/// Information about a discovered peer
#[derive(Clone, Debug)]
pub struct PeerInfo {
    pub id: String,
    pub address: SocketAddr,
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
        };
        
        // Start background tasks
        service.start_peer_discovery().await?;
        service.start_connection_handler().await?;
        service.start_message_handler().await?;
        
        Ok(service)
    }
    
    pub async fn discover_peers(&self) -> Result<Vec<PeerInfo>> {
        info!("Starting peer discovery...");
        
        // Start mDNS discovery
        self.start_mdns_discovery().await?;
        
        // Return current peer list
        let peers = self.peers.read().await;
        Ok(peers.values().cloned().collect())
    }
    
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
        })
    }
    
    pub async fn connect_via_invitation(&self, invitation_code: &str) -> Result<PeerConnection> {
        info!("Connecting via invitation code");
        
        // Parse invitation code
        let invitation = crate::crypto::parse_invitation_code(invitation_code)?;
        let peer_addr: SocketAddr = invitation.address.parse()?;
        
        // Connect to peer
        let connection = self.connect_to_peer(peer_addr).await?;
        
        // Verify peer identity matches invitation
        // TODO: Implement proper peer identity verification
        
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
    
    pub async fn request_chunk_from_peer(
        &self,
        peer_id: &str,
        chunk_hash: [u8; 32],
        chunk_index: u32,
    ) -> Result<Vec<u8>> {
        let connections = self.connections.read().await;
        
        if let Some(connection) = connections.get(peer_id) {
            let request = P2PMessage::ChunkRequest {
                hash: chunk_hash,
                chunk_id: chunk_index,
            };
            
            connection.send_message(&request).await?;
            
            // TODO: Implement proper request/response matching using RequestManager
            // For now, return empty vec as placeholder
            warn!("Chunk request sent to peer {}, but response handling not fully implemented", peer_id);
            Ok(Vec::new())
        } else {
            Err(anyhow!("Peer {} not connected", peer_id))
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
        
        tokio::spawn(async move {
            while let Some((peer_id, message)) = message_rx.recv().await {
                Self::handle_message(peer_id, message, &peers, &connections).await;
            }
        });
        
        Ok(())
    }
    
    async fn start_mdns_discovery(&self) -> Result<()> {
        // TODO: Implement mDNS-based local network discovery
        // For now, use UDP broadcast discovery
        
        let _config = self.config.clone();
        let peers = self.peers.clone();
        
        tokio::spawn(async move {
            let socket = match UdpSocket::bind("0.0.0.0:0").await {
                Ok(socket) => socket,
                Err(e) => {
                    error!("Failed to bind UDP socket for discovery: {}", e);
                    return;
                }
            };
            
            if let Err(e) = socket.set_broadcast(true) {
                error!("Failed to enable broadcast: {}", e);
                return;
            }
            
            let mut buf = [0u8; 1024];
            
            loop {
                if let Ok((len, addr)) = socket.recv_from(&mut buf).await {
                    if let Ok(message) = serde_json::from_slice::<P2PMessage>(&buf[..len]) {
                        if let P2PMessage::Announce { peer_id, public_key, listen_port } = message {
                            let peer_addr = SocketAddr::new(addr.ip(), listen_port);
                            
                            let peer_info = PeerInfo {
                                id: peer_id,
                                address: peer_addr,
                                public_key,
                                last_seen: chrono::Utc::now(),
                                authenticated: false,
                            };
                            
                            peers.write().await.insert(peer_info.id.clone(), peer_info);
                        }
                    }
                }
            }
        });
        
        Ok(())
    }
    
    async fn send_announce_broadcast(identity: &crate::crypto::Identity, config: &crate::config::Config) -> Result<()> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.set_broadcast(true)?;
        
        let announce = P2PMessage::Announce {
            peer_id: identity.peer_id(),
            public_key: identity.public_key_bytes().to_vec(),
            listen_port: config.listen_port,
        };
        
        let data = serde_json::to_vec(&announce)?;
        let broadcast_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), 41337);
        
        socket.send_to(&data, broadcast_addr).await?;
        
        Ok(())
    }
    
    async fn cleanup_stale_peers(peers: &Arc<RwLock<HashMap<String, PeerInfo>>>) {
        let cutoff = chrono::Utc::now() - chrono::Duration::minutes(5);
        
        let mut peers_guard = peers.write().await;
        peers_guard.retain(|_, peer| peer.last_seen > cutoff);
    }
    
    async fn handle_connection_streams(connection: Connection, peer_id: String, message_tx: mpsc::UnboundedSender<(String, P2PMessage)>) {
        loop {
            match connection.accept_uni().await {
                Ok(mut stream) => {
                    let peer_id = peer_id.clone();
                    let message_tx = message_tx.clone();
                    
                    tokio::spawn(async move {
                        if let Ok(buf) = stream.read_to_end(1024 * 1024).await {
                            if let Ok(message) = serde_json::from_slice::<P2PMessage>(&buf) {
                                let _ = message_tx.send((peer_id, message));
                            }
                        }
                    });
                }
                Err(_) => break,
            }
        }
    }
    
    async fn handle_message(
        peer_id: String,
        message: P2PMessage,
        _peers: &Arc<RwLock<HashMap<String, PeerInfo>>>,
        _connections: &Arc<RwLock<HashMap<String, PeerConnection>>>,
    ) {
        match message {
            P2PMessage::ChunkRequest { hash, chunk_id } => {
                info!("Received chunk request from {}: {:?}:{}", peer_id, hash, chunk_id);
                // TODO: Handle chunk request
            }
            P2PMessage::ChunkResponse { hash, chunk_id, data } => {
                info!("Received chunk response from {}: {:?}:{} ({} bytes)", peer_id, hash, chunk_id, data.len());
                // TODO: Handle chunk response
            }
            P2PMessage::FileUpdate { path, hash: _, size, chunks } => {
                info!("Received file update from {}: {} ({} bytes, {} chunks)", peer_id, path, size, chunks.len());
                // TODO: Handle file update
            }
            P2PMessage::AuthChallenge { challenge: _ } => {
                info!("Received auth challenge from {}", peer_id);
                // TODO: Handle auth challenge
            }
            P2PMessage::AuthResponse { response: _ } => {
                info!("Received auth response from {}", peer_id);
                // TODO: Handle auth response
            }
            P2PMessage::Announce { .. } => {
                // Handled by discovery system
            }
        }
    }
    
    async fn authenticate_peer(&self, peer: &PeerConnection) -> Result<()> {
        // Generate challenge
        let challenge = rand::random::<[u8; 32]>();
        
        let challenge_msg = P2PMessage::AuthChallenge { challenge };
        peer.send_message(&challenge_msg).await?;
        
        // TODO: Wait for and verify response
        
        Ok(())
    }
}

// Helper functions for QUIC configuration

fn generate_self_signed_cert(_identity: &crate::crypto::Identity) -> Result<(Vec<u8>, Vec<u8>)> {
    use rcgen::{generate_simple_self_signed, CertifiedKey};
    
    // Generate a simple self-signed certificate for localhost
    let subject_alt_names = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    
    let CertifiedKey { cert, key_pair } = generate_simple_self_signed(subject_alt_names)?;
    
    let cert_der = cert.der().to_vec();
    let key_der = key_pair.serialize_der();
    
    Ok((cert_der, key_der))
}

fn configure_server(cert_der: Vec<u8>, key_der: Vec<u8>) -> Result<ServerConfig> {
    let cert = Certificate(cert_der);
    let key = PrivateKey(key_der);
    
    let server_config = ServerConfig::with_single_cert(vec![cert], key)
        .map_err(|e| anyhow!("Failed to configure server: {}", e))?;
    
    Ok(server_config)
}

fn configure_client() -> Result<ClientConfig> {
    let client_config = ClientConfig::new(Arc::new(
        rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification {}))
            .with_no_client_auth(),
    ));
    
    Ok(client_config)
}

/// Custom certificate verifier that skips verification (for P2P usage)
struct SkipServerVerification;

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &Certificate,
        _intermediates: &[Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}
