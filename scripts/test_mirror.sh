#!/bin/bash

# Test script for SlySync mirror functionality
set -e

echo "🧪 Testing SlySync Mirror Functionality"
echo "======================================"

# Clean up any existing test directories
rm -rf /tmp/slysync_test_source /tmp/slysync_test_dest

# Create test source directory with various file types
echo "📁 Creating test files..."
mkdir -p /tmp/slysync_test_source/subdir1/subdir2
echo "Hello from root" > /tmp/slysync_test_source/root.txt
echo "Hello from subdir1" > /tmp/slysync_test_source/subdir1/file1.txt
echo "Hello from subdir2" > /tmp/slysync_test_source/subdir1/subdir2/file2.txt
echo '{"name": "test", "value": 42}' > /tmp/slysync_test_source/data.json

# Test 1: One-time mirror
echo ""
echo "🔄 Test 1: One-time mirror operation"
echo "-----------------------------------"
./target/debug/slysync mirror /tmp/slysync_test_source /tmp/slysync_test_dest --name "Cross-Drive Test"

# Verify files were copied
echo "📋 Verifying copied files..."
if [ -f "/tmp/slysync_test_dest/root.txt" ] && 
   [ -f "/tmp/slysync_test_dest/subdir1/file1.txt" ] && 
   [ -f "/tmp/slysync_test_dest/subdir1/subdir2/file2.txt" ] && 
   [ -f "/tmp/slysync_test_dest/data.json" ]; then
    echo "✅ All files copied successfully!"
else
    echo "❌ File copy verification failed!"
    exit 1
fi

# Verify file contents
if [ "$(cat /tmp/slysync_test_dest/root.txt)" = "Hello from root" ] && 
   [ "$(cat /tmp/slysync_test_dest/subdir1/file1.txt)" = "Hello from subdir1" ] && 
   [ "$(cat /tmp/slysync_test_dest/data.json)" = '{"name": "test", "value": 42}' ]; then
    echo "✅ File contents match!"
else
    echo "❌ File content verification failed!"
    exit 1
fi

# Test 2: Error handling - non-existent source
echo ""
echo "🚫 Test 2: Error handling - non-existent source"
echo "-----------------------------------------------"
if ./target/debug/slysync mirror /tmp/nonexistent /tmp/slysync_test_dest2 2>/dev/null; then
    echo "❌ Should have failed with non-existent source!"
    exit 1
else
    echo "✅ Correctly handled non-existent source!"
fi

# Test 3: Help command
echo ""
echo "❓ Test 3: Help command"
echo "---------------------"
if ./target/debug/slysync mirror --help | grep -q "Mirror a local folder"; then
    echo "✅ Help command works!"
else
    echo "❌ Help command failed!"
    exit 1
fi

# Clean up
echo ""
echo "🧹 Cleaning up test files..."
rm -rf /tmp/slysync_test_source /tmp/slysync_test_dest

echo ""
echo "🎉 All tests passed! SlySync mirror functionality is working correctly."
echo "   ✅ One-time mirroring works"
echo "   ✅ Directory structure is preserved"
echo "   ✅ File contents are accurate"  
echo "   ✅ Error handling works"
echo "   ✅ Help system works"
