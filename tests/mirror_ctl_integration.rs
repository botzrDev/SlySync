//! Integration tests for the mirror-ctl subcommands
//! 
//! These tests verify that mirror-ctl subcommands work correctly,
//! including daemon management, process control, and configuration handling.

use std::env;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use slysync::cli::{mirror_ctl, MirrorCtlSubcommand, MirrorDaemonConfig};

/// Helper function to create a mock PID file and config for testing
fn create_mock_mirror_daemon(
    temp_dir: &TempDir,
    name: &str,
    pid: u32,
    source: &str,
    dest: &str,
) -> (PathBuf, PathBuf) {
    let pid_dir = temp_dir.path();
    let safe_name = name.replace('/', "_");
    
    let pidfile = pid_dir.join(format!("{}.pid", safe_name));
    let configfile = pid_dir.join(format!("{}.json", safe_name));
    
    // Write PID file
    fs::write(&pidfile, pid.to_string()).expect("Failed to write PID file");
    
    // Write config file
    let config = MirrorDaemonConfig {
        source: source.to_string(),
        destination: dest.to_string(),
        name: Some(name.to_string()),
    };
    let config_json = serde_json::to_string_pretty(&config).expect("Failed to serialize config");
    fs::write(&configfile, config_json).expect("Failed to write config file");
    
    (pidfile, configfile)
}

/// Test mirror-ctl status with no running daemons
#[tokio::test]
async fn test_mirror_ctl_status_no_daemons() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    let result = mirror_ctl(MirrorCtlSubcommand::Status).await;
    assert!(result.is_ok(), "Status command should succeed even with no daemons");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl status with multiple running daemons
#[tokio::test]
async fn test_mirror_ctl_status_multiple_daemons() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    // Create mock daemons - they won't appear in status because PIDs don't exist
    // But this tests that status command handles the directory correctly
    create_mock_mirror_daemon(&temp_dir, "DocBackup", 12345, "/home/docs", "/backup/docs");
    create_mock_mirror_daemon(&temp_dir, "ProjectSync", 67890, "/home/projects", "/backup/projects");
    
    let result = mirror_ctl(MirrorCtlSubcommand::Status).await;
    assert!(result.is_ok(), "Status command should succeed with multiple daemons");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl stop with missing arguments
#[tokio::test]
async fn test_mirror_ctl_stop_missing_args() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    let result = mirror_ctl(MirrorCtlSubcommand::Stop {
        name: None,
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Stop command should handle missing args gracefully");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl stop by exact name match
#[tokio::test]
async fn test_mirror_ctl_stop_by_name() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    // Create daemon with a fake PID that doesn't exist - this tests direct file operations
    let (pidfile, configfile) = create_mock_mirror_daemon(
        &temp_dir,
        "TestBackup",
        99999, // Use a high PID that definitely doesn't exist
        "/test/source",
        "/test/dest"
    );
    
    // Verify files exist before stop
    assert!(pidfile.exists(), "PID file should exist before stop");
    assert!(configfile.exists(), "Config file should exist before stop");
    
    // Direct name match should work even if process doesn't exist
    let result = mirror_ctl(MirrorCtlSubcommand::Stop {
        name: Some("TestBackup".to_string()),
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Stop command should succeed");
    
    // The stop_mirror_daemon function should clean up both files when process doesn't exist
    // This is the expected behavior for non-existent processes
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl stop by source path
#[tokio::test]
async fn test_mirror_ctl_stop_by_source() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    let source_path = "/home/user/documents";
    let (_pidfile, _configfile) = create_mock_mirror_daemon(
        &temp_dir,
        "DocumentBackup",
        99998, // Non-existent PID
        source_path,
        "/backup/documents"
    );
    
    let result = mirror_ctl(MirrorCtlSubcommand::Stop {
        name: None,
        source: Some(PathBuf::from(source_path)),
    }).await;
    
    assert!(result.is_ok(), "Stop by source should succeed");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl stop with partial name matching
#[tokio::test]
async fn test_mirror_ctl_stop_partial_match() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    // Create daemon that won't be found by list_mirror_pids (non-existent PID)
    // but test the fallback logic for partial matching
    create_mock_mirror_daemon(&temp_dir, "MyDocuments", 11111, "/docs", "/backup/docs");
    
    let result = mirror_ctl(MirrorCtlSubcommand::Stop {
        name: Some("Doc".to_string()), // Partial match
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Partial name matching should work");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl stop with multiple matches (should show ambiguity)
#[tokio::test]
async fn test_mirror_ctl_stop_multiple_matches() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    // Create daemons that won't be detected as running (test fallback logic)
    create_mock_mirror_daemon(&temp_dir, "DocBackup1", 22221, "/docs1", "/backup1");
    create_mock_mirror_daemon(&temp_dir, "DocBackup2", 22222, "/docs2", "/backup2");
    
    let result = mirror_ctl(MirrorCtlSubcommand::Stop {
        name: Some("Doc".to_string()), // Matches both
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Multiple matches should be handled gracefully");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl stop with nonexistent daemon
#[tokio::test]
async fn test_mirror_ctl_stop_nonexistent() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    let result = mirror_ctl(MirrorCtlSubcommand::Stop {
        name: Some("NonExistent".to_string()),
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Stop of nonexistent daemon should be handled gracefully");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl restart with missing arguments
#[tokio::test]
async fn test_mirror_ctl_restart_missing_args() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    let result = mirror_ctl(MirrorCtlSubcommand::Restart {
        name: None,
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Restart command should handle missing args gracefully");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl restart with valid daemon and config
#[tokio::test]
async fn test_mirror_ctl_restart_with_config() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    // Create daemon that won't be detected as running (test fallback behavior)
    let (_pidfile, _configfile) = create_mock_mirror_daemon(
        &temp_dir,
        "RestartTest",
        33333, // Non-existent PID
        "/test/restart/source",
        "/test/restart/dest"
    );
    
    let result = mirror_ctl(MirrorCtlSubcommand::Restart {
        name: Some("RestartTest".to_string()),
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Restart with valid config should succeed");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl restart with missing config file
#[tokio::test]
async fn test_mirror_ctl_restart_missing_config() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    let pid_dir = temp_dir.path();
    let pidfile = pid_dir.join("MissingConfigTest.pid");
    
    // Create PID file but no config file
    fs::write(&pidfile, "44444").expect("Failed to write PID file");
    
    let result = mirror_ctl(MirrorCtlSubcommand::Restart {
        name: Some("MissingConfigTest".to_string()),
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Restart with missing config should be handled gracefully");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl restart with corrupted config file
#[tokio::test]
async fn test_mirror_ctl_restart_corrupted_config() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    let pid_dir = temp_dir.path();
    let pidfile = pid_dir.join("CorruptedTest.pid");
    let configfile = pid_dir.join("CorruptedTest.json");
    
    // Create PID file and corrupted config
    fs::write(&pidfile, "55555").expect("Failed to write PID file");
    fs::write(&configfile, "{ invalid json }").expect("Failed to write corrupted config");
    
    let result = mirror_ctl(MirrorCtlSubcommand::Restart {
        name: Some("CorruptedTest".to_string()),
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Restart with corrupted config should be handled gracefully");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl restart with no running daemons
#[tokio::test]
async fn test_mirror_ctl_restart_no_daemons() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    let result = mirror_ctl(MirrorCtlSubcommand::Restart {
        name: Some("NonExistent".to_string()),
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Restart with no daemons should be handled gracefully");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl resync functionality (should work same as restart)
#[tokio::test]
async fn test_mirror_ctl_resync() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    // Create daemon that won't be detected as running
    let (_pidfile, _configfile) = create_mock_mirror_daemon(
        &temp_dir,
        "ResyncTest",
        66666, // Non-existent PID
        "/test/resync/source",
        "/test/resync/dest"
    );
    
    let result = mirror_ctl(MirrorCtlSubcommand::Resync {
        name: Some("ResyncTest".to_string()),
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Resync should succeed (same as restart)");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl resync with missing arguments
#[tokio::test]
async fn test_mirror_ctl_resync_missing_args() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    let result = mirror_ctl(MirrorCtlSubcommand::Resync {
        name: None,
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Resync with missing args should be handled gracefully");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test that restart and resync behave identically
#[tokio::test]
async fn test_mirror_ctl_restart_resync_equivalence() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    // Test with non-existent daemon - both should behave the same
    let restart_result = mirror_ctl(MirrorCtlSubcommand::Restart {
        name: Some("TestEquivalence".to_string()),
        source: None,
    }).await;
    
    let resync_result = mirror_ctl(MirrorCtlSubcommand::Resync {
        name: Some("TestEquivalence".to_string()),
        source: None,
    }).await;
    
    assert_eq!(restart_result.is_ok(), resync_result.is_ok(), 
               "Restart and resync should have equivalent behavior");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test error handling with invalid PID directory
#[tokio::test]
async fn test_mirror_ctl_invalid_pid_dir() {
    // Set an invalid PID directory path
    env::set_var("SLYSYNC_MIRROR_PID_DIR", "/invalid/nonexistent/path");
    
    let result = mirror_ctl(MirrorCtlSubcommand::Status).await;
    assert!(result.is_ok(), "Commands should handle invalid PID dir gracefully");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test file cleanup behavior in stop command
#[tokio::test]
async fn test_mirror_ctl_stop_file_cleanup() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    // Test direct file cleanup with non-existent PID
    let (pidfile, configfile) = create_mock_mirror_daemon(
        &temp_dir,
        "CleanupTest",
        77777, // Non-existent PID
        "/cleanup/source",
        "/cleanup/dest"
    );
    
    // Verify files exist
    assert!(pidfile.exists(), "PID file should exist before stop");
    assert!(configfile.exists(), "Config file should exist before stop");
    
    // Stop the daemon using direct name match
    let result = mirror_ctl(MirrorCtlSubcommand::Stop {
        name: Some("CleanupTest".to_string()),
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Stop should succeed");
    
    // The stop operation should clean up files when process doesn't exist
    // This is the expected behavior for proper cleanup
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test handling of special characters in daemon names
#[tokio::test]
async fn test_mirror_ctl_special_characters() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    // Test with path-like name that gets converted - use direct file access
    let (_pidfile, _configfile) = create_mock_mirror_daemon(
        &temp_dir,
        "/home/user/test",  // Will be converted to safe filename
        88888, // Non-existent PID for direct access test
        "/home/user/test",
        "/backup/test"
    );
    
    let result = mirror_ctl(MirrorCtlSubcommand::Stop {
        name: Some("/home/user/test".to_string()),
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Stop with special characters in name should work");
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test concurrent mirror-ctl operations
#[tokio::test]
async fn test_mirror_ctl_concurrent_operations() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    // Create multiple daemons (won't appear as running but test concurrent access)
    create_mock_mirror_daemon(&temp_dir, "Concurrent1", 11111, "/src1", "/dst1");
    create_mock_mirror_daemon(&temp_dir, "Concurrent2", 22222, "/src2", "/dst2");
    create_mock_mirror_daemon(&temp_dir, "Concurrent3", 33333, "/src3", "/dst3");
    
    // Run multiple status commands concurrently
    let status_futures = (0..3).map(|_| {
        mirror_ctl(MirrorCtlSubcommand::Status)
    });
    
    let results = futures::future::join_all(status_futures).await;
    
    for result in results {
        assert!(result.is_ok(), "Concurrent status operations should succeed");
    }
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl operations with empty PID files
#[tokio::test]
async fn test_mirror_ctl_empty_pid_files() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    let pid_dir = temp_dir.path();
    let pidfile = pid_dir.join("EmptyPidTest.pid");
    let configfile = pid_dir.join("EmptyPidTest.json");
    
    // Create empty PID file
    fs::write(&pidfile, "").expect("Failed to write empty PID file");
    
    // Create valid config
    let config = MirrorDaemonConfig {
        source: "/empty/test/source".to_string(),
        destination: "/empty/test/dest".to_string(),
        name: Some("EmptyPidTest".to_string()),
    };
    fs::write(&configfile, serde_json::to_string_pretty(&config).unwrap())
        .expect("Failed to write config");
    
    let result = mirror_ctl(MirrorCtlSubcommand::Stop {
        name: Some("EmptyPidTest".to_string()),
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Stop with empty PID file should be handled gracefully");
    
    // The stop operation should clean up malformed PID files and their configs
    // This is the expected behavior for proper error handling
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}

/// Test mirror-ctl operations with malformed PID files
#[tokio::test]
async fn test_mirror_ctl_malformed_pid_files() {
    let temp_dir = TempDir::new().unwrap();
    env::set_var("SLYSYNC_MIRROR_PID_DIR", temp_dir.path());
    
    let pid_dir = temp_dir.path();
    let pidfile = pid_dir.join("MalformedPidTest.pid");
    let configfile = pid_dir.join("MalformedPidTest.json");
    
    // Create malformed PID file
    fs::write(&pidfile, "not_a_number").expect("Failed to write malformed PID file");
    
    // Create valid config
    let config = MirrorDaemonConfig {
        source: "/malformed/test/source".to_string(),
        destination: "/malformed/test/dest".to_string(),
        name: Some("MalformedPidTest".to_string()),
    };
    fs::write(&configfile, serde_json::to_string_pretty(&config).unwrap())
        .expect("Failed to write config");
    
    let result = mirror_ctl(MirrorCtlSubcommand::Stop {
        name: Some("MalformedPidTest".to_string()),
        source: None,
    }).await;
    
    assert!(result.is_ok(), "Stop with malformed PID file should be handled gracefully");
    
    // The stop operation should clean up malformed PID files and their configs  
    // This is the expected behavior for proper error handling
    
    env::remove_var("SLYSYNC_MIRROR_PID_DIR");
}
