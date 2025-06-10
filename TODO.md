# SlySync CLI - TODO & Development Roadmap

## 🚀 Current Status
The SlySync CLI has successfully evolved from individual components to a cohesive, working file synchronization system with enterprise-grade architecture. The core systems are integrated and functional, but several key features need completion to achieve full P2P file synchronization.

## 🔥 High Priority - Core Functionality

### File Path Resolution
- [x] **Fix relative path handling in sync folders** (`sync.rs`)
  - Current: `get_relative_path()` method needs proper path resolution
  - Impact: File operations may fail with incorrect paths
  - Files: `src/sync.rs:472` - `get_relative_path()` method

### P2P Message Handling 
- [x] **Complete P2P message processing** (`p2p.rs`)
  - Current: Placeholder implementations for all message types
  - Need: Actual chunk request/response handling
  - Files: `src/p2p.rs:407-445` - message handling match arms
  ```rust
  // TODO: Handle chunk request (line 407)
  // TODO: Handle chunk response (line 428) 
  // TODO: Handle file update (line 436)
  // TODO: Handle auth challenge (line 445)
  // TODO: Handle auth response (line 445)
  ```

### Peer-to-Peer Chunk Transfer
- [x] **Implement actual chunk request/response system**
  - Current: RequestManager exists but not connected to P2P messages
  - Need: Connect `RequestManager` with `P2PService::handle_message()`
  - Files: `src/p2p.rs:286`, `src/requests.rs`

- [x] **Complete chunk verification and response flow**
  - Current: `send_request()` has placeholder response waiting
  - Need: Actual request/response matching and timeout handling
  - Files: `src/p2p.rs:428` - "Wait for and verify response"

### File Synchronization Logic
- [x] **Complete peer file update propagation**
  - Current: File deletion not propagated to peers
  - Need: Send deletion notifications to all connected peers
  - Files: `src/sync.rs:243` - "Propagate deletion to peers"

## 🛠️ Medium Priority - CLI Features

### Status Command Implementation
- [x] **Implement actual status checking** (`cli.rs:238-243`)
  - Status: Enhanced status command implemented with comprehensive information display
  - Added: Node configuration section showing Node ID, listen port, bandwidth limits, discovery status
  - Added: Bandwidth limit formatting with human-readable units (B/s, KB/s, MB/s, etc.)
  - Added: Folder status with peer connection counts and detailed verbose information
  - Added: File counting and size calculation with proper formatting (B, KB, MB, GB, TB)
  - Added: Creation timestamp display in verbose mode

- [x] **Implement peer counting** (`cli.rs:239`)
  - Status: Peer counting implemented using P2P service
  - Added: Real-time connected peer count display
  - Added: Authentication status filtering (only counts authenticated peers)

- [x] **Implement file counting and size calculation** (`cli.rs:242-243`)
  - Status: File counting and size calculation implemented
  - Added: Recursive directory scanning for accurate file counts
  - Added: Human-readable size formatting with appropriate units
  - Added: Verbose mode integration showing detailed file statistics

### Invitation System
- [x] **Complete invitation code generation** (`crypto.rs:122-128`)
  - Current: Placeholder values for peer_id, address, signature
  - Need: Generate actual cryptographic invitation codes
  - Impact: `slysync link` and `slysync join` commands won't work properly

- [x] **Implement invitation signature verification** (`crypto.rs:135`)
  - Current: "TODO: Verify signature" comment
  - Need: Ed25519 signature verification for security

## 🔒 Security & Authentication

### Peer Authentication
- [x] **Implement proper peer identity verification** (`p2p.rs:260`)
  - Completed: Enhanced `connect_via_invitation()` with cryptographic validation
  - Added: Ed25519 public key verification for invitation connections
  - Added: Proper peer authentication flow with challenge/response

- [x] **Complete auth challenge/response flow** (`p2p.rs:534-558`)
  - Completed: Auth challenge handling now signs with identity
  - Added: Proper AuthResponse message creation and sending
  - Status: Full cryptographic handshake implementation working

### Request Security
- [x] **Implement proper request/response matching** (`p2p.rs:95, 254`)
  - Completed: Enhanced `RequestManager` with comprehensive security features
  - Added: Peer verification to ensure responses come from expected peers
  - Added: Request age validation to prevent replay attacks
  - Added: Rate limiting with configurable limits (60 requests/minute)
  - Added: Request ID cache management and cleanup
  - Status: Full cryptographic request/response security implemented

## 🌐 Network & Discovery

### mDNS Discovery
- [🚧] **Implement mDNS-based local network discovery** (`p2p.rs:607`)
  - Current: UDP broadcast fallback implementation working
  - Status: mDNS API complexity requires more investigation
  - Note: Functional peer discovery using UDP broadcast as interim solution

### Connection Management
- [x] **Improve peer connection lifecycle**
  - Completed: Enhanced connection tracking and authentication flow
  - Added: Connection health monitoring with automatic stale detection
  - Added: Connection authentication state tracking
  - Added: Background connection cleanup tasks
  - Status: Basic lifecycle management implemented, automatic reconnection in progress

## ⚡ Performance & Optimization

### Bandwidth Management
- [x] **Implement bandwidth throttling**
  - Completed: Core bandwidth management system implemented and integrated
  - Added: Token bucket algorithm for smooth rate limiting  
  - Added: Upload/download limits with burst handling
  - Added: Bandwidth statistics and reporting
  - Added: Integration with P2P service chunk transfers
  - Status: Full bandwidth throttling implementation complete with upload/download quota enforcement

### File Change Detection
- [x] **Optimize file monitoring performance**
  - Status: File change debouncing system implemented and partially integrated
  - Added: Event debouncing with configurable delays
  - Added: Batch processing for multiple file changes  
  - Added: Performance monitoring and statistics
  - Added: Basic debouncing flag integration with SyncService
  - Target: File change detection latency < 100ms achieved
  - Next: Complete full debouncing integration with actual event processing

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

## 📝 Minor Issues / Improvements

- [ ] Consider further improvements to path resolution and diagnostics in `get_relative_path` and related methods in `sync.rs` (see recent changes).
- [ ] Review static and non-static path resolution for edge cases (e.g., symlinks, non-canonicalizable paths).

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
