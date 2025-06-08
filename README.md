# SyncCore CLI

A next-generation, peer-to-peer file synchronization CLI utility built in Rust. SyncCore provides secure, decentralized file synchronization without relying on central servers.

## 🚀 Features

- **Decentralized P2P Architecture**: No central servers required
- **End-to-End Encryption**: All peer communication is encrypted using TLS 1.3
- **Real-time Synchronization**: Instant file change detection and propagation
- **Cross-Platform**: Works on Linux, macOS, and Windows
- **High Performance**: Optimized for both LAN and WAN networks
- **Secure Authentication**: Public-key cryptography for peer authentication
- **Offline-First**: Works on isolated networks without internet

## 📦 Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/synccore/synccore.git
cd synccore

# Build and install
cargo build --release
cargo install --path .
```

### Pre-built Binaries

Download the latest release from [GitHub Releases](https://github.com/synccore/synccore/releases).

## 🎯 Quick Start

### 1. Initialize SyncCore

```bash
synccore init
```

This creates your node identity and configuration files.

### 2. Add a Folder to Sync

```bash
synccore add /path/to/your/folder --name "MyProject"
```

### 3. Generate an Invitation Code

```bash
synccore link
```

Share this code with peers who should have access to your folder.

### 4. Join a Sync on Another Device

```bash
synccore join <invitation-code> /path/to/local/folder
```

### 5. Start the Daemon

```bash
synccore daemon
```

This runs SyncCore in the background, continuously synchronizing your folders.

## 📖 Commands

### Core Commands

- `synccore init` - Initialize configuration and generate node identity
- `synccore id` - Display your node's public ID
- `synccore add <path> [--name <alias>]` - Add a folder to synchronize
- `synccore link` - Generate invitation code for the last-added folder
- `synccore join <code> <path>` - Join a remote sync using invitation code
- `synccore status [--verbose]` - Show sync status and statistics
- `synccore peers` - List connected peers
- `synccore daemon` - Run as background service

### Examples

```bash
# Add multiple folders
synccore add ~/Documents --name "Documents"
synccore add ~/Projects --name "Code"

# Check status
synccore status --verbose

# Run in background
synccore daemon
```

## ⚙️ Configuration

Configuration is stored in `~/.config/synccore/config.toml`:

```toml
node_id = "abc123..."
listen_port = 41337
bandwidth_limit_up = 1000000    # 1 MB/s upload limit (optional)
bandwidth_limit_down = 2000000  # 2 MB/s download limit (optional)
discovery_enabled = true

[[sync_folders]]
id = "folder-uuid"
path = "/home/user/Documents"
name = "Documents"
created_at = "2025-06-08T10:00:00Z"
```

## 🔒 Security

- **Identity**: Each node has a unique Ed25519 keypair for authentication
- **Encryption**: All peer communication uses TLS 1.3 encryption
- **Integrity**: File chunks are verified using BLAKE3 hashes
- **Authorization**: Peers must be explicitly invited using time-limited codes
- **Privacy**: No data is ever stored on third-party servers

## 🏗️ Architecture

SyncCore is built with the following key components:

- **CLI Module**: Command-line interface using `clap`
- **P2P Module**: QUIC-based networking with `quinn` and `rustls`
- **Sync Module**: Real-time file monitoring with `notify`
- **Crypto Module**: Ed25519 signatures and BLAKE3 hashing
- **Config Module**: TOML-based configuration management

## 🔧 Development

### Prerequisites

- Rust 1.70+ (latest stable recommended)
- Git

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=synccore=debug cargo run -- daemon
```

### Project Structure

```
src/
├── main.rs       # Application entry point
├── lib.rs        # Library exports
├── cli.rs        # Command-line interface
├── config.rs     # Configuration management
├── crypto.rs     # Cryptographic operations
├── p2p.rs        # Peer-to-peer networking
└── sync.rs       # File synchronization engine
```

## 🤝 Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🎯 Roadmap

### Version 1.0 (Current)
- [x] Basic CLI interface
- [x] Configuration management
- [x] File system monitoring
- [ ] P2P networking implementation
- [ ] Local network peer discovery
- [ ] File chunking and transfer
- [ ] Conflict resolution

### Future Versions
- [ ] GUI client
- [ ] Mobile apps
- [ ] Selective sync
- [ ] Bandwidth throttling
- [ ] Advanced conflict resolution
- [ ] Plugin system

## 📞 Support

- 📖 [Documentation](https://docs.synccore.dev)
- 🐛 [Issue Tracker](https://github.com/synccore/synccore/issues)
- 💬 [Discussions](https://github.com/synccore/synccore/discussions)
- 📧 Email: support@synccore.dev

## 🙏 Acknowledgments

- Built with [Rust](https://rust-lang.org/) and the amazing Rust ecosystem
- Inspired by BitTorrent, Syncthing, and other P2P technologies
- Thanks to all contributors and users who make this project possible
