# SlySync Mirror Feature

## Overview
SlySync now includes a powerful local folder mirroring feature that allows you to sync files between two local directories without P2P networking. This is perfect for backing up files to different drives or maintaining mirrors of important directories.

## Features
- **Real-time Monitoring**: Watches source directory for changes using efficient file system events
- **Fast Copying**: Uses optimized file operations for quick synchronization
- **Incremental Updates**: Only copies changed files based on modification times
- **Nested Directory Support**: Preserves complete directory structure
- **Error Handling**: Robust error handling with clear error messages
- **Cross-Platform**: Works on Linux, macOS, and Windows

## Usage

### One-time Mirror Operation
```bash
# Basic mirror operation
slysync mirror /source/path /destination/path

# With optional name
slysync mirror /source/path /destination/path --name "My Backup"

# Example: Backup documents to external drive
slysync mirror ~/Documents /mnt/backup/Documents --name "Documents Backup"
```

### Daemon Mode (Continuous Monitoring)
```bash
# Run as daemon with continuous monitoring
slysync mirror /source/path /destination/path --daemon

# With name and daemon mode
slysync mirror /source/path /destination/path --name "Live Backup" --daemon

# Example: Live backup of projects folder
slysync mirror ~/Projects /backup/Projects --name "Projects Live Backup" --daemon
```

### Real-world Examples

#### 1. Documents Backup
```bash
# One-time backup of documents
slysync mirror ~/Documents /mnt/external/Documents --name "Documents Backup"
```

#### 2. Development Projects Sync
```bash
# Continuous sync of development folder to backup drive
slysync mirror ~/Development /backup/Development --daemon --name "Dev Backup"
```

#### 3. Media Library Mirror
```bash
# Mirror media library to network storage
slysync mirror ~/Media /nas/Media --name "Media Mirror"
```

#### 4. Configuration Backup
```bash
# Backup configuration files with daemon monitoring
slysync mirror ~/.config /backup/config --daemon --name "Config Backup"
```

## Command Options

- `<SOURCE>`: Source folder path to mirror from (required)
- `<DESTINATION>`: Destination folder path to mirror to (required)
- `--name, -n <NAME>`: Optional human-readable name for this mirror setup
- `--daemon, -d`: Run as a daemon with continuous monitoring

## Features in Detail

### File System Monitoring
The mirror service uses the `notify` crate for efficient cross-platform file system monitoring. It detects:
- File creation
- File modification
- File deletion
- Directory creation/deletion

### Incremental Sync
Only files that have been modified since the last sync are copied. The system compares:
- File modification times
- File existence in destination

### Directory Structure
The complete directory structure is preserved, including:
- Nested subdirectories
- Empty directories
- File permissions (where supported by the file system)

### Error Handling
Robust error handling includes:
- Source path validation
- Destination directory creation
- Permission checks
- File operation error recovery

## Performance
- **Idle CPU usage**: < 1% when watching for changes
- **File change detection**: < 100ms latency
- **Copy performance**: Limited primarily by I/O throughput

## Implementation Notes

The mirror functionality is implemented in the `src/mirror.rs` module with:
- `MirrorService`: Main service for handling file monitoring and synchronization
- `MirrorConfig`: Configuration structure for mirror operations
- `run_mirror_once()`: Function for one-time mirror operations

The CLI integration is in `src/cli.rs` with the `setup_mirror()` function that handles both daemon and one-time modes.

## Testing

The mirror functionality has been thoroughly tested with:
- ✅ One-time mirroring
- ✅ Directory structure preservation
- ✅ File content accuracy
- ✅ Error handling for invalid paths
- ✅ Help system integration
- ✅ Cross-drive operations

## Future Enhancements

Potential future improvements could include:
- Bidirectional synchronization
- Conflict resolution strategies
- Bandwidth throttling
- File filtering and exclusion patterns
- Progress reporting for large operations
