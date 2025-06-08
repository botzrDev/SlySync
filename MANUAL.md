# SlySync User Manual

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

SlySync is a next-generation, peer-to-peer file synchronization utility that allows you to sync files between devices without relying on central servers. It uses modern cryptographic protocols and efficient networking to provide secure, fast, and reliable file synchronization.

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
git clone https://github.com/your-org/slysync.git
cd slysync

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

### 1. Initialize SlySync

First, initialize SlySync on your device:

```bash
slysync init
```

This creates your node's cryptographic identity and configuration files.

### 2. Add a Folder to Sync

Add a folder that you want to synchronize:

```bash
slysync add /path/to/your/folder --name "My Documents"
```

### 3. Generate an Invitation Code

To share this folder with another device, generate an invitation code:

```bash
slysync link
```

This outputs a secure invitation code that expires in 24 hours.

### 4. Join from Another Device

On the second device, initialize SlySync and join using the invitation code:

```bash
# On the second device
slysync init
slysync join <invitation-code> /path/to/local/folder
```

### 5. Start the Daemon

Start the synchronization daemon on both devices:

```bash
slysync daemon
```

Your files are now being synchronized automatically!

## Command Reference

### `slysync init`

Initialize SlySync configuration and generate node identity.

**Usage:**
```bash
slysync init
```

**Example:**
```bash
$ slysync init
✅ SlySync initialized successfully!
Node ID: ed25519_Ax7B2Cd3Ef4Gh5Ij6Kl7Mn8Op9Qr0St1Uv2Wx3Yz4
```

### `slysync id`

Display the current node's public ID.

**Usage:**
```bash
slysync id
```

**Example:**
```bash
$ slysync id
ed25519_Ax7B2Cd3Ef4Gh5Ij6Kl7Mn8Op9Qr0St1Uv2Wx3Yz4
```

### `slysync add`

Add a new local folder to be synchronized.

**Usage:**
```bash
slysync add <path> [--name <name>]
```

**Arguments:**
- `<path>` - Path to the folder to synchronize
- `--name, -n <name>` - Optional human-readable alias for the folder

**Example:**
```bash
$ slysync add /home/user/Documents --name "Work Documents"
✅ Added folder: /home/user/Documents
   Alias: Work Documents
   Folder ID: 550e8400-e29b-41d4-a716-446655440001
```

### `slysync link`

Generate a secure invitation code for the last-added folder.

**Usage:**
```bash
slysync link
```

**Example:**
```bash
$ slysync link
📨 Invitation code for folder 'Work Documents' (/home/user/Documents)
SC_INV_1_ed25519_Ax7B2Cd3Ef4Gh5Ij6Kl7Mn8Op9Qr0St1Uv2Wx3Yz4_192.168.1.100:41337_exp1654567890_sig_aB3cD4eF...

💡 Share this code with peers who should have access to this folder.
   Code expires in 24 hours for security.
```

### `slysync join`

Join a remote sync folder using an invitation code.

**Usage:**
```bash
slysync join <code> <path>
```

**Arguments:**
- `<code>` - The invitation code received from another peer
- `<path>` - Local path where the synchronized folder will be saved

**Example:**
```bash
$ slysync join SC_INV_1_ed25519_... /home/user/SharedDocs
✅ Joined sync folder at: /home/user/SharedDocs
   Starting synchronization...
```

### `slysync status`

Display status of all sync jobs.

**Usage:**
```bash
slysync status [--verbose]
```

**Arguments:**
- `--verbose, -v` - Show detailed information for each file

**Example:**
```bash
$ slysync status --verbose
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

### `slysync peers`

List all connected peers.

**Usage:**
```bash
slysync peers
```

**Example:**
```bash
$ slysync peers
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

### `slysync daemon`

Run the SlySync engine as a background daemon.

**Usage:**
```bash
slysync daemon
```

**Example:**
```bash
$ slysync daemon
🚀 SlySync daemon starting...
Node ID: ed25519_Ax7B2Cd3Ef4Gh5Ij6Kl7Mn8Op9Qr0St1Uv2Wx3Yz4
Listening on port: 41337
💚 SlySync daemon is running. Press Ctrl+C to stop.
📂 Monitoring 2 sync folder(s)
```

## Configuration

SlySync stores its configuration in platform-appropriate directories:

- **Linux**: `~/.config/slysync/`
- **macOS**: `~/Library/Application Support/slysync/`
- **Windows**: `%APPDATA%\slysync\`

### Configuration Files

- `config.toml` - Main configuration file
- `identity.key` - Node's cryptographic identity (keep secure!)
- `data/` - Chunk storage directory

### Configuration Options

Edit `config.toml` to customize SlySync:

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

- `SLYSYNC_LOG` - Set log level (`trace`, `debug`, `info`, `warn`, `error`)
- `SLYSYNC_CONFIG_DIR` - Override default configuration directory

**Example:**
```bash
SLYSYNC_LOG=debug slysync daemon
```

## Security

### Cryptographic Security

SlySync uses industry-standard cryptographic algorithms:

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
1. **Monitor peer connections** - Use `slysync peers` to check connected nodes
2. **Regular backups** - SlySync is not a backup solution
5. **Network security** - Use trusted networks when possible

### Threat Model

SlySync protects against:

- ✅ Network eavesdropping (encryption)
- ✅ Man-in-the-middle attacks (authentication)
- ✅ Data tampering (integrity verification)
- ✅ Unauthorized access (invitation codes)

SlySync does NOT protect against:

- ❌ Compromised devices with access to identity.key
- ❌ Physical access to synchronized files
- ❌ Attacks on the underlying operating system

## Troubleshooting

### Common Issues

#### "SlySync not initialized"

**Problem:** You see the error "SlySync not initialized. Run 'slysync init' first."

**Solution:** Run `slysync init` to initialize your node.

#### No peers found

**Problem:** `slysync peers` shows no connected peers.

**Solutions:**
1. Ensure both devices are on the same network
2. Check firewall settings (port 41337 must be accessible)
3. Verify the daemon is running on both devices
4. Try restarting the daemon

#### Files not syncing

**Problem:** Files are not synchronizing between devices.

**Solutions:**
1. Check that the daemon is running: `slysync daemon`
2. Verify peer connections: `slysync peers`
3. Check file permissions
4. Look for error messages in daemon output

#### High CPU usage

**Problem:** SlySync is using too much CPU.

**Solutions:**
1. Check for large files being synchronized
2. Reduce the number of files in sync folders
3. Add file filters to exclude unnecessary files

### Log Analysis

Enable debug logging to troubleshoot issues:

```bash
SLYSYNC_LOG=debug slysync daemon
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
   - SlySync version
   - Configuration file (remove sensitive data)
   - Full error message and logs

## Advanced Usage

### Running as a System Service

#### Linux (systemd)

Create `/etc/systemd/system/slysync.service`:

```ini
[Unit]
Description=SlySync P2P File Synchronization
After=network.target

[Service]
Type=simple
User=your-username
ExecStart=/usr/local/bin/slysync daemon
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl enable slysync
sudo systemctl start slysync
```

#### macOS (launchd)

Create `~/Library/LaunchAgents/com.slysync.daemon.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.slysync.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/slysync</string>
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
launchctl load ~/Library/LaunchAgents/com.slysync.daemon.plist
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
- Enable QoS on your router for SlySync traffic (port 41337)
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

Monitor SlySync with system tools:

```bash
# Check process status
ps aux | grep slysync

# Monitor network usage
netstat -an | grep 41337

# Check disk usage
du -h ~/.config/slysync/data/
```

#### Backup Integration

SlySync works well with backup tools:

```bash
# Include sync folders in backups
rsync -av /home/user/Documents/ /backup/documents/

# Exclude SlySync data directory
rsync -av --exclude='.config/slysync/' /home/user/ /backup/home/
```

### API Integration (Future)

SlySync will support REST API for integration:

```bash
# Future: Check status via API
curl http://localhost:8080/api/v1/status

# Future: Add folder via API
curl -X POST http://localhost:8080/api/v1/folders \
  -d '{"path": "/path/to/folder", "name": "API Folder"}'
```

## Support and Contributing

### Community

- **GitHub**: https://github.com/your-org/slysync
- **Issues**: https://github.com/your-org/slysync/issues
- **Discussions**: https://github.com/your-org/slysync/discussions

### Contributing

We welcome contributions! See CONTRIBUTING.md for guidelines.

### License

SlySync is licensed under the MIT License. See LICENSE for details.

---

*SlySync v1.0.0 - Last updated: June 8, 2025*
