# SlySync Product Overview

SlySync is a next-generation, peer-to-peer file synchronization utility designed for secure, decentralized, and high-performance file sync across devices and platforms. It is built in Rust for reliability, speed, and cross-platform compatibility.

---

## Key Features

- **Decentralized P2P Architecture:** No central servers required; all devices communicate directly.
- **End-to-End Encryption:** All data and communication are encrypted using modern cryptography (Ed25519, QUIC/TLS 1.3, BLAKE3).
- **Real-Time Synchronization:** Instant file change detection and propagation using efficient file system monitoring.
- **Local Folder Mirroring:** Mirror folders between local drives or devices without P2P networking.
- **Cross-Platform:** Works on Linux, macOS, and Windows from a single Rust codebase.
- **Offline-First:** Designed to work on local networks and handle network interruptions gracefully.
- **Performance:** Optimized for both LAN and WAN, with low idle CPU usage and high throughput.
- **Resilience:** Automatic reconnection and offline change handling.
- **User-Friendly CLI:** Intuitive command-line interface with clear help, status, and control commands.

---

## Use Cases

- **Personal Backups:** Mirror important folders to external drives or backup servers.
- **Team Collaboration:** Sync project folders between team members securely and efficiently.
- **Cross-Device Sync:** Keep files up-to-date across desktops, laptops, and servers.
- **Media Management:** Sync large media libraries between NAS, workstations, and portable drives.
- **Development Workflows:** Mirror source code and assets between environments or CI systems.

---

## Security Highlights

- Peer authentication with public-key cryptography
- All peer communication is encrypted
- File chunks are cryptographically verified
- No sensitive data stored in plaintext

---

## Architecture Principles

1. **Decentralization First**
2. **Security by Design**
3. **Performance**
4. **Cross-Platform**
5. **Resilience**

---

## Technologies Used

- **Rust** (async/await, tokio)
- **clap** (CLI framework)
- **notify** (file system events)
- **quinn** (QUIC networking)
- **rustls** (TLS)
- **ed25519-dalek** (cryptography)
- **blake3** (hashing)
- **serde/toml** (configuration)
- **tracing** (logging)
- **parking_lot** (fast synchronization)

---

## Learn More

- See the [User Manual](./MANUAL.md) for setup and usage.
- See the [Contributor Workflow](./CONTRIBUTOR_WORKFLOW.md) for development guidelines.
- Visit the [GitHub repository](https://github.com/your-org/slysync) for source code and issues.
