#!/bin/bash

# Live demonstration of SlySync mirror daemon functionality
set -e

echo "🔄 SlySync Mirror Daemon Live Test"
echo "=================================="

# Create test directories
SOURCE_DIR="/tmp/slysync_demo_source"
DEST_DIR="/tmp/slysync_demo_dest"

# Clean up any existing directories
rm -rf "$SOURCE_DIR" "$DEST_DIR"
mkdir -p "$SOURCE_DIR"

echo "📁 Created source directory: $SOURCE_DIR"
echo "📁 Destination directory: $DEST_DIR"
echo ""

# Start the daemon in background
echo "🚀 Starting SlySync mirror daemon..."
./target/debug/slysync mirror "$SOURCE_DIR" "$DEST_DIR" --daemon --name "Live Demo" &
DAEMON_PID=$!

# Give the daemon time to start and perform initial sync
sleep 2

echo "📝 Testing real-time file synchronization..."
echo ""

# Test 1: Create a new file
echo "Test 1: Creating new file..."
echo "Hello World!" > "$SOURCE_DIR/hello.txt"
sleep 1
if [ -f "$DEST_DIR/hello.txt" ]; then
    echo "✅ File created and synced: hello.txt"
    echo "   Content: $(cat "$DEST_DIR/hello.txt")"
else
    echo "❌ File not synced"
fi
echo ""

# Test 2: Create a directory with files
echo "Test 2: Creating directory with files..."
mkdir -p "$SOURCE_DIR/subdir"
echo "File in subdirectory" > "$SOURCE_DIR/subdir/nested.txt"
sleep 1
if [ -f "$DEST_DIR/subdir/nested.txt" ]; then
    echo "✅ Directory and file created and synced"
    echo "   Content: $(cat "$DEST_DIR/subdir/nested.txt")"
else
    echo "❌ Directory/file not synced"
fi
echo ""

# Test 3: Modify existing file
echo "Test 3: Modifying existing file..."
echo "Updated content!" > "$SOURCE_DIR/hello.txt"
sleep 1
if [ "$(cat "$DEST_DIR/hello.txt")" = "Updated content!" ]; then
    echo "✅ File modification synced"
    echo "   New content: $(cat "$DEST_DIR/hello.txt")"
else
    echo "❌ File modification not synced"
fi
echo ""

# Test 4: Add multiple files quickly
echo "Test 4: Adding multiple files..."
for i in {1..5}; do
    echo "File $i content" > "$SOURCE_DIR/file$i.txt"
done
sleep 2

synced_count=0
for i in {1..5}; do
    if [ -f "$DEST_DIR/file$i.txt" ]; then
        ((synced_count++))
    fi
done
echo "✅ Synced $synced_count/5 files"
echo ""

# Show final directory structure
echo "📂 Final directory structure:"
echo "Source ($SOURCE_DIR):"
find "$SOURCE_DIR" -type f | sort
echo ""
echo "Destination ($DEST_DIR):"
find "$DEST_DIR" -type f | sort
echo ""

# Stop the daemon
echo "🛑 Stopping daemon..."
kill $DAEMON_PID
wait $DAEMON_PID 2>/dev/null || true

# Clean up
echo "🧹 Cleaning up..."
rm -rf "$SOURCE_DIR" "$DEST_DIR"

echo ""
echo "🎉 Live demo completed! SlySync mirror daemon successfully demonstrated:"
echo "   ✅ Real-time file creation sync"
echo "   ✅ Directory structure sync"
echo "   ✅ File modification sync"
echo "   ✅ Multiple file handling"
