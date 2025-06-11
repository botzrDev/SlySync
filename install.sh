#!/usr/bin/env bash
set -e

# SlySync Install Script
# Builds and installs SlySync to ~/.cargo/bin

# Check for cargo
if ! command -v cargo >/dev/null 2>&1; then
  echo "Error: Rust and Cargo are required but not found. Please install Rust from https://rustup.rs/ and try again."
  exit 1
fi

# Build in release mode
echo "Building SlySync in release mode..."
cargo build --release

# Ensure ~/.cargo/bin exists
if [ ! -d "$HOME/.cargo/bin" ]; then
  mkdir -p "$HOME/.cargo/bin"
fi

# Copy the binary
echo "Installing SlySync binary to $HOME/.cargo/bin..."
cp target/release/slysync "$HOME/.cargo/bin/"

# Make sure it's executable
chmod +x "$HOME/.cargo/bin/slysync"

echo "\nSlySync installed successfully! Make sure $HOME/.cargo/bin is in your PATH."
echo "You can now run: slysync --help"
