//! # SyncCore
//! 
//! A next-generation, peer-to-peer file synchronization CLI utility built in Rust.
//! 
//! SyncCore provides secure, decentralized file synchronization without relying on 
//! central servers. It uses modern cryptographic protocols and efficient P2P networking
//! to keep files synchronized across multiple devices.
//! 
//! ## Features
//! 
//! - **Decentralized P2P Architecture**: No central servers required
//! - **End-to-End Encryption**: All data is encrypted using modern cryptography
//! - **Chunk-based Storage**: Efficient deduplication and incremental sync
//! - **Real-time Monitoring**: Instant file change detection and propagation
//! - **Cross-platform**: Works on Linux, macOS, and Windows
//! 
//! ## Core Modules
//! 
//! - [`cli`] - Command-line interface and user interaction
//! - [`config`] - Configuration management and storage
//! - [`crypto`] - Cryptographic operations and identity management
//! - [`p2p`] - Peer-to-peer networking and communication
//! - [`storage`] - Chunk-based file storage system
//! - [`sync`] - File synchronization engine
//! - [`requests`] - Request/response management for P2P operations

pub mod cli;
pub mod config;
pub mod crypto;
pub mod p2p;
pub mod requests;
pub mod storage;
pub mod sync;

pub use config::Config;
pub use crypto::Identity;
