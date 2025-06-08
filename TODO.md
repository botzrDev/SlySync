# SlySync CLI - TODO & Development Roadmap

## 🚀 Current Status
The SlySync CLI has successfully evolved from individual components to a cohesive, working file synchronization system with enterprise-grade architecture. The core systems are integrated and functional, but several key features need completion to achieve full P2P file synchronization.

## 🔥 High Priority - Core Functionality

### File Path Resolution
- [ ] **Fix relative path handling in sync folders** (`sync.rs`)
  - Current: `get_relative_path()` method needs proper path resolution
  - Impact: File operations may fail with incorrect paths
  - Files: `src/sync.rs:341` - `get_relative_path()` method

### P2P Message Handling 
- [ ] **Complete P2P message processing** (`p2p.rs`)
  - Current: Placeholder implementations for all message types
  - Need: Actual chunk request/response handling
  - Files: `src/p2p.rs:446-477` - message handling match arms
  ```rust
  // TODO: Handle chunk request (line 446)
  // TODO: Handle chunk response (line 450) 
  // TODO: Handle file update (line 454)
  // TODO: Handle auth challenge (line 458)
  // TODO: Handle auth response (line 462)
  ```

### Peer-to-Peer Chunk Transfer
- [ ] **Implement actual chunk request/response system**
  - Current: RequestManager exists but not connected to P2P messages
  - Need: Connect `RequestManager` with `P2PService::handle_message()`
  - Files: `src/p2p.rs:254`, `src/requests.rs`

- [ ] **Complete chunk verification and response flow**
  - Current: `send_request()` has placeholder response waiting
  - Need: Actual request/response matching and timeout handling
  - Files: `src/p2p.rs:477` - "Wait for and verify response"

### File Synchronization Logic
- [ ] **Complete peer file update propagation**
  - Current: File deletion not propagated to peers
  - Need: Send deletion notifications to all connected peers
  - Files: `src/sync.rs:206` - "Propagate deletion to peers"

## 🛠️ Medium Priority - CLI Features

### Status Command Implementation
- [ ] **Implement actual status checking** (`cli.rs:164`)
  - Current: Shows "Up to date" placeholder
  - Need: Check sync status with peers, pending files, conflicts

- [ ] **Implement peer counting** (`cli.rs:165`)
  - Current: Shows "0 connected" placeholder
  - Need: Query P2PService for active peer connections

- [ ] **Implement file counting and size calculation** (`cli.rs:168-169`)
  - Current: Shows "0 files, 0 bytes" placeholder
  - Need: Scan sync folders and calculate totals

### Invitation System
- [ ] **Complete invitation code generation** (`crypto.rs:109-112`)
  - Current: Placeholder values for peer_id, address, signature
  - Need: Generate actual cryptographic invitation codes
  - Impact: `slysync link` and `slysync join` commands won't work

- [ ] **Implement invitation signature verification** (`crypto.rs:135`)
  - Current: "TODO: Verify signature" comment
  - Need: Ed25519 signature verification for security

## 🔒 Security & Authentication

### Peer Authentication
- [ ] **Implement proper peer identity verification** (`p2p.rs:210`)
  - Current: Placeholder peer authentication
  - Need: Ed25519 public key verification for all peers

- [ ] **Complete auth challenge/response flow** (`p2p.rs:458-462`)
  - Current: Auth messages not processed
  - Need: Cryptographic handshake implementation

### Request Security
- [ ] **Implement proper request/response matching** (`p2p.rs:95, 254`)
  - Current: No request validation or matching
  - Need: Prevent replay attacks and unauthorized requests

## 🌐 Network & Discovery

### mDNS Discovery
- [ ] **Implement mDNS-based local network discovery** (`p2p.rs:346`)
  - Current: Placeholder implementation
  - Need: Automatic peer discovery on local networks
  - Libraries: Consider `mdns` or `zeroconf` crate

### Connection Management
- [ ] **Improve peer connection lifecycle**
  - Current: Basic connection tracking
  - Need: Automatic reconnection, connection health monitoring
  - Need: Graceful handling of network interruptions

## ⚡ Performance & Optimization

### Bandwidth Management
- [ ] **Implement bandwidth throttling**
  - Current: No bandwidth limits
  - Need: Configurable upload/download rate limiting
  - Requirements: Must saturate 1 Gbps LAN when unthrottled

### File Change Detection
- [ ] **Optimize file monitoring performance**
  - Current: Basic notify implementation
  - Target: File change detection latency < 100ms
  - Need: Debouncing for rapid file changes

### Resource Usage
- [ ] **Optimize idle CPU usage**
  - Target: Idle CPU usage < 1%
  - Need: Profile and optimize hot paths
  - Need: Efficient file system watching

## 📋 Advanced Features

### Conflict Resolution
- [ ] **Implement conflict resolution for simultaneous modifications**
  - Current: No conflict detection
  - Need: Timestamp-based or vector clock conflict resolution
  - Need: User interface for manual conflict resolution

### Selective Synchronization
- [ ] **Add file filtering and ignore patterns**
  - Current: Syncs all files in watched folders
  - Need: `.syncignore` file support
  - Need: Configurable file type filters

### File History
- [ ] **Implement file version history**
  - Current: Only current file versions
  - Need: Keep configurable number of file versions
  - Need: Ability to restore previous versions

## 🧪 Testing & Validation

### Integration Tests
- [ ] **Create comprehensive integration tests**
  - Need: Two-peer sync scenario tests
  - Need: Network interruption recovery tests
  - Need: Large file synchronization tests

### Performance Tests
- [ ] **Create performance benchmarks**
  - Need: 1 Gbps network saturation tests
  - Need: Large directory sync performance
  - Need: Memory usage profiling

### Security Auditing
- [ ] **Security audit of cryptographic implementations**
  - Need: Review Ed25519 key generation and usage
  - Need: Audit QUIC/TLS configuration
  - Need: Test against common attack vectors

## 🐛 Known Issues

### Current Bugs
- [ ] **Path resolution in `get_relative_path()`**
  - Symptom: May fail with relative paths in sync folders
  - Files: `src/sync.rs:341`

- [ ] **Incomplete error handling in P2P message processing**
  - Symptom: Errors may cause connection drops
  - Files: `src/p2p.rs:440-470`

## 📝 Code Quality

### Documentation
- [ ] **Add comprehensive API documentation**
  - Current: Basic inline comments
  - Need: Full rustdoc documentation for all public APIs
  - Need: Usage examples and tutorials

### Error Handling
- [ ] **Improve error messages and user feedback**
  - Current: Technical error messages
  - Need: User-friendly error descriptions
  - Need: Actionable error recovery suggestions

### Logging
- [ ] **Enhance structured logging**
  - Current: Basic tracing implementation
  - Need: Structured fields for better log analysis
  - Need: Configurable log levels per module

## 🎯 Next Steps (Recommended Order)

1. **Fix path resolution** - Critical for basic file operations
2. **Complete P2P message handling** - Enables actual synchronization
3. **Implement chunk transfer** - Core synchronization functionality
4. **Fix CLI status commands** - Improves user experience
5. **Complete authentication** - Essential for security
6. **Add mDNS discovery** - Improves peer discovery UX
7. **Implement conflict resolution** - Advanced synchronization feature

## 📊 Progress Tracking

### Completed ✅
- Core CLI architecture and command parsing
- Configuration management with TOML
- Ed25519 cryptographic identity system
- QUIC-based P2P networking foundation
- Chunk-based storage system with deduplication
- Real-time file system monitoring
- Basic sync service with file processing
- Request/response management framework
- Storage system integration
- File manifest tracking

### In Progress 🚧
- P2P message processing (placeholders implemented)
- File synchronization logic (partially complete)
- CLI status reporting (basic structure in place)

### Not Started ❌ 
- mDNS peer discovery
- Conflict resolution
- File version history
- Bandwidth throttling
- Comprehensive testing

---

**Total estimated work remaining:** ~40-50 hours for core functionality, ~80-100 hours for advanced features and polish.

**Critical path:** Path resolution → P2P messages → Chunk transfer → Authentication
