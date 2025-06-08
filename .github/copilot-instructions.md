# SyncCore CLI - Copilot Instructions

<!-- Use this file to provide workspace-specific custom instructions to Copilot. For more details, visit https://code.visualstudio.com/docs/copilot/copilot-customization#_use-a-githubcopilotinstructionsmd-file -->

## Project Overview
SyncCore is a next-generation, peer-to-peer file synchronization CLI utility built in Rust. It provides secure, decentralized file synchronization without relying on central servers.

## Architecture Principles
1. **Decentralization First**: Pure P2P model with no central servers
2. **Security by Design**: End-to-end encryption for all communications
3. **Performance**: Optimized for both LAN and WAN networks
4. **Cross-Platform**: Single Rust codebase for Linux, macOS, and Windows
5. **Resilience**: Offline-first design with automatic reconnection

## Key Technologies
- **Async Runtime**: tokio for all async operations
- **CLI Framework**: clap with derive macros
- **File Monitoring**: notify for cross-platform file system events
- **Networking**: quinn (QUIC) and rustls for secure P2P communication
- **Cryptography**: ed25519-dalek for identity and blake3 for hashing
- **Configuration**: toml and serde for human-readable config files

## Code Style Guidelines
- Use `anyhow::Result` for error handling in main functions
- Use `thiserror` for custom error types
- Prefer `tracing` over `println!` for logging
- Use `parking_lot` instead of `std::sync` for better performance
- Follow async/await patterns throughout
- Structure code into clear modules: cli, p2p, sync, crypto, config

## Security Requirements
- All peer communication MUST be encrypted
- File chunks MUST be cryptographically verified
- Peer authentication MUST use public-key cryptography
- Never store sensitive data in plaintext

## Performance Requirements
- Idle CPU usage < 1%
- Must saturate 1 Gbps LAN connections
- File change detection latency < 100ms
- Graceful handling of network interruptions
