# SyncCore User Manual

## Table of Contents

1. [Introduction](#introduction)
2. [Installation](#installation)
3. [Quick Start Guide](#quick-start-guide)
4. [Command Reference](#command-reference)
5. [Configuration](#configuration)
6. [Security](#security)
7. [Troubleshooting](#troubleshooting)
8. [Advanced Usage](#advanced-usage)

## Introduction

SyncCore is a next-generation, peer-to-peer file synchronization utility that allows you to sync files between devices without relying on central servers. It uses modern cryptographic protocols and efficient networking to provide secure, fast, and reliable file synchronization.

### Key Features

- **Decentralized P2P Architecture**: No central servers required
- **End-to-End Encryption**: All data is encrypted using modern cryptography
- **Chunk-based Storage**: Efficient deduplication and incremental sync
- **Real-time Monitoring**: Instant file change detection and propagation
- **Cross-platform**: Works on Linux, macOS, and Windows
- **Offline-first**: Works without internet connection on local networks

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/your-org/synccore.git
cd synccore

# Build with Cargo
cargo build --release

# Install to your PATH
cargo install --path .
```

### System Requirements

- **Operating System**: Linux, macOS, or Windows
- **RAM**: Minimum 256MB, recommended 1GB
- **Disk Space**: 100MB for application, plus space for synchronized files
- **Network**: Any network interface (Ethernet, Wi-Fi, etc.)

## Quick Start Guide

### 1. Initialize SyncCore

First, initialize SyncCore on your device:

```bash
synccore init
```

This creates your node's cryptographic identity and configuration files.

### 2. Add a Folder to Sync

Add a folder that you want to synchronize:

```bash
synccore add /path/to/your/folder --name "My Documents"
```

### 3. Generate an Invitation Code

To share this folder with another device, generate an invitation code:

```bash
synccore link
```

This outputs a secure invitation code that expires in 24 hours.

### 4. Join from Another Device

On the second device, initialize SyncCore and join using the invitation code:

```bash
# On the second device
synccore init
synccore join <invitation-code> /path/to/local/folder
```

### 5. Start the Daemon

Start the synchronization daemon on both devices:

```bash
synccore daemon
```

Your files are now being synchronized automatically!

## Command Reference

### `synccore init`

Initialize SyncCore configuration and generate node identity.

**Usage:**
```bash
synccore init
```

**Example:**
```bash
$ synccore init
✅ SyncCore initialized successfully!
Node ID: ed25519_Ax7B2Cd3Ef4Gh5Ij6Kl7Mn8Op9Qr0St1Uv2Wx3Yz4
```

### `synccore id`

Display the current node's public ID.

**Usage:**
```bash
synccore id
```

**Example:**
```bash
$ synccore id
ed25519_Ax7B2Cd3Ef4Gh5Ij6Kl7Mn8Op9Qr0St1Uv2Wx3Yz4
```

### `synccore add`

Add a new local folder to be synchronized.

**Usage:**
```bash
synccore add <path> [--name <name>]
```

**Arguments:**
- `<path>` - Path to the folder to synchronize
- `--name, -n <name>` - Optional human-readable alias for the folder

**Example:**
```bash
$ synccore add /home/user/Documents --name "Work Documents"
✅ Added folder: /home/user/Documents
   Alias: Work Documents
   Folder ID: 550e8400-e29b-41d4-a716-446655440001
```

### `synccore link`

Generate a secure invitation code for the last-added folder.

**Usage:**
```bash
synccore link
```

**Example:**
```bash
$ synccore link
📨 Invitation code for folder 'Work Documents' (/home/user/Documents)
SC_INV_1_ed25519_Ax7B2Cd3Ef4Gh5Ij6Kl7Mn8Op9Qr0St1Uv2Wx3Yz4_192.168.1.100:41337_exp1654567890_sig_aB3cD4eF...

💡 Share this code with peers who should have access to this folder.
   Code expires in 24 hours for security.
```

### `synccore join`

Join a remote sync folder using an invitation code.

**Usage:**
```bash
synccore join <code> <path>
```

**Arguments:**
- `<code>` - The invitation code received from another peer
- `<path>` - Local path where the synchronized folder will be saved

**Example:**
```bash
$ synccore join SC_INV_1_ed25519_... /home/user/SharedDocs
✅ Joined sync folder at: /home/user/SharedDocs
   Starting synchronization...
```

### `synccore status`

Display status of all sync jobs.

**Usage:**
```bash
synccore status [--verbose]
```

**Arguments:**
- `--verbose, -v` - Show detailed information for each file

**Example:**
```bash
$ synccore status --verbose
📂 Sync Status

  Work Documents (/home/user/Documents)
    Status: Up to date
    Peers: 2 connected
    Files: 1,234
    Size: 567.8 MB

  Photos (/home/user/Pictures)
    Status: Syncing
    Peers: 1 connected
    Files: 5,678
    Size: 12.3 GB
```

### `synccore peers`

List all connected peers.

**Usage:**
```bash
synccore peers
```

**Example:**
```bash
$ synccore peers
🌐 Discovering Peers...

Found 2 peer(s):

1. 📡 ed25519_Bx8C3Dd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv3Wx4Yz5
   Address: 192.168.1.101:41337
   Status: 🔐 Authenticated
   Last seen: 2 minutes ago

2. 📡 ed25519_Cx9D4Ed5Ff6Gh7Ij8Kl9Mn0Op1Qr2St3Uv4Wx5Yz6
   Address: 192.168.1.102:41337
   Status: 🔓 Pending
   Last seen: just now
```

### `synccore daemon`

Run the SyncCore engine as a background daemon.

**Usage:**
```bash
synccore daemon
```

**Example:**
```bash
$ synccore daemon
🚀 SyncCore daemon starting...
Node ID: ed25519_Ax7B2Cd3Ef4Gh5Ij6Kl7Mn8Op9Qr0St1Uv2Wx3Yz4
Listening on port: 41337
💚 SyncCore daemon is running. Press Ctrl+C to stop.
📂 Monitoring 2 sync folder(s)
```

## Configuration

SyncCore stores its configuration in platform-appropriate directories:

- **Linux**: `~/.config/synccore/`
- **macOS**: `~/Library/Application Support/synccore/`
- **Windows**: `%APPDATA%\synccore\`

### Configuration Files

- `config.toml` - Main configuration file
- `identity.key` - Node's cryptographic identity (keep secure!)
- `data/` - Chunk storage directory

### Configuration Options

Edit `config.toml` to customize SyncCore:

```toml
node_id = "your-node-id"
listen_port = 41337
discovery_enabled = true
bandwidth_limit_up = 1048576    # 1 MB/s upload limit (optional)
bandwidth_limit_down = 2097152  # 2 MB/s download limit (optional)

[[sync_folders]]
id = "550e8400-e29b-41d4-a716-446655440001"
path = "/home/user/Documents"
name = "Work Documents"
created_at = "2025-06-08T10:30:00Z"
```

### Environment Variables

- `SYNCCORE_LOG` - Set log level (`trace`, `debug`, `info`, `warn`, `error`)
- `SYNCCORE_CONFIG_DIR` - Override default configuration directory

**Example:**
```bash
SYNCCORE_LOG=debug synccore daemon
```

## Security

### Cryptographic Security

SyncCore uses industry-standard cryptographic algorithms:

- **Ed25519** for digital signatures and node identity
- **QUIC with TLS 1.3** for encrypted peer-to-peer communication
- **BLAKE3** for content hashing and verification
- **OS entropy** for secure random number generation

### Network Security

- All peer communication is encrypted end-to-end
- Node identities are verified using public-key cryptography
- Invitation codes expire after 24 hours
- No plaintext data is transmitted over the network

### Best Practices

1. **Keep your identity.key file secure** - This is your node's private key
2. **Use strong invitation codes** - Don't share invitation codes publicly
3. **Monitor peer connections** - Use `synccore peers` to check connected nodes
4. **Regular backups** - SyncCore is not a backup solution
5. **Network security** - Use trusted networks when possible

### Threat Model

SyncCore protects against:

- ✅ Network eavesdropping (encryption)
- ✅ Man-in-the-middle attacks (authentication)
- ✅ Data tampering (integrity verification)
- ✅ Unauthorized access (invitation codes)

SyncCore does NOT protect against:

- ❌ Compromised devices with access to identity.key
- ❌ Physical access to synchronized files
- ❌ Attacks on the underlying operating system

## Troubleshooting

### Common Issues

#### "SyncCore not initialized"

**Problem:** You see the error "SyncCore not initialized. Run 'synccore init' first."

**Solution:** Run `synccore init` to initialize your node.

#### No peers found

**Problem:** `synccore peers` shows no connected peers.

**Solutions:**
1. Ensure both devices are on the same network
2. Check firewall settings (port 41337 must be accessible)
3. Verify the daemon is running on both devices
4. Try restarting the daemon

#### Files not syncing

**Problem:** Files are not synchronizing between devices.

**Solutions:**
1. Check that the daemon is running: `synccore daemon`
2. Verify peer connections: `synccore peers`
3. Check file permissions
4. Look for error messages in daemon output

#### High CPU usage

**Problem:** SyncCore is using too much CPU.

**Solutions:**
1. Check for large files being synchronized
2. Reduce the number of files in sync folders
3. Add file filters to exclude unnecessary files

### Log Analysis

Enable debug logging to troubleshoot issues:

```bash
SYNCCORE_LOG=debug synccore daemon
```

Common log patterns:

- `File event: Create` - New file detected
- `Chunk stored` - File chunk saved to storage
- `Peer connected` - New peer connection established
- `Auth successful` - Peer authentication completed

### Getting Help

1. Check the logs with debug logging enabled
2. Search existing issues on GitHub
3. Create a new issue with:
   - Your operating system
   - SyncCore version
   - Configuration file (remove sensitive data)
   - Full error message and logs

## Advanced Usage

### Running as a System Service

#### Linux (systemd)

Create `/etc/systemd/system/synccore.service`:

```ini
[Unit]
Description=SyncCore P2P File Synchronization
After=network.target

[Service]
Type=simple
User=your-username
ExecStart=/usr/local/bin/synccore daemon
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl enable synccore
sudo systemctl start synccore
```

#### macOS (launchd)

Create `~/Library/LaunchAgents/com.synccore.daemon.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.synccore.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/synccore</string>
        <string>daemon</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
```

Load the service:
```bash
launchctl load ~/Library/LaunchAgents/com.synccore.daemon.plist
```

### Performance Tuning

#### Bandwidth Limiting

Limit bandwidth usage in `config.toml`:

```toml
bandwidth_limit_up = 1048576    # 1 MB/s
bandwidth_limit_down = 2097152  # 2 MB/s
```

#### Large File Handling

For large files (>1GB):
1. Consider excluding them from sync
2. Ensure sufficient disk space for chunks
3. Monitor memory usage during sync

#### Network Optimization

- Use wired connections for initial sync of large datasets
- Enable QoS on your router for SyncCore traffic (port 41337)
- Consider separate network for sync traffic in enterprise environments

### Integration with Other Tools

#### File Exclusion

Create `.syncignore` files (future feature) to exclude:
```
*.tmp
*.log
.DS_Store
node_modules/
.git/
```

#### Monitoring

Monitor SyncCore with system tools:

```bash
# Check process status
ps aux | grep synccore

# Monitor network usage
netstat -an | grep 41337

# Check disk usage
du -h ~/.config/synccore/data/
```

#### Backup Integration

SyncCore works well with backup tools:

```bash
# Include sync folders in backups
rsync -av /home/user/Documents/ /backup/documents/

# Exclude SyncCore data directory
rsync -av --exclude='.config/synccore/' /home/user/ /backup/home/
```

### API Integration (Future)

SyncCore will support REST API for integration:

```bash
# Future: Check status via API
curl http://localhost:8080/api/v1/status

# Future: Add folder via API
curl -X POST http://localhost:8080/api/v1/folders \
  -d '{"path": "/path/to/folder", "name": "API Folder"}'
```

## Support and Contributing

### Community

- **GitHub**: https://github.com/your-org/synccore
- **Issues**: https://github.com/your-org/synccore/issues
- **Discussions**: https://github.com/your-org/synccore/discussions

### Contributing

We welcome contributions! See CONTRIBUTING.md for guidelines.

### License

SyncCore is licensed under the MIT License. See LICENSE for details.

---

*SyncCore v1.0.0 - Last updated: June 8, 2025*
