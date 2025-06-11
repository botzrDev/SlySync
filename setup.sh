#!/usr/bin/env bash
set -e

# SlySync Setup Script
# This script automates the installation process after cloning the repository.

# Check for cargo
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust and Cargo are required but not found."
  read -p "Would you like to install Rust and Cargo via rustup? [Y/n]: " yn
  case $yn in
    [Nn]*) echo "Aborting setup. Please install Rust from https://rustup.rs/ and re-run this script."; exit 1;;
    *)
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
      export PATH="$HOME/.cargo/bin:$PATH"
      ;;
  esac
fi

# Ensure ~/.cargo/bin is in PATH
if ! echo "$PATH" | grep -q "$HOME/.cargo/bin"; then
  echo "Adding $HOME/.cargo/bin to PATH for this session."
  export PATH="$HOME/.cargo/bin:$PATH"
  echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> "$HOME/.profile"
fi

# Run the install script
echo "Running install.sh to build and install SlySync..."
chmod +x ./install.sh
./install.sh

echo "\nSlySync setup complete! You can now run: slysync --help"
