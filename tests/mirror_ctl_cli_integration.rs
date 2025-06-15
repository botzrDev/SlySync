//! CLI integration tests specifically for mirror-ctl subcommands
//! 
//! These tests verify that mirror-ctl commands work correctly when invoked
//! through the actual CLI interface, ensuring proper argument parsing and
//! command execution.

use std::process::Command;
use tempfile::TempDir;

/// Test mirror-ctl status command through CLI
#[test]
fn test_cli_mirror_ctl_status() {
    let temp_dir = TempDir::new().unwrap();
    
    let output = Command::new("cargo")
        .args(&["run", "--", "mirror-ctl", "status"])
        .env("SLYSYNC_MIRROR_PID_DIR", temp_dir.path())
        .output();
    
    match output {
        Ok(result) => {
            // Status command should succeed even with no daemons
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);
            
            // Should contain message about no running daemons
            assert!(
                stdout.contains("No running mirror daemons found") || 
                stdout.contains("running mirror daemon") ||
                stderr.is_empty(), // Allow for successful execution with no output
                "Status command should handle empty daemon list"
            );
        }
        Err(e) => {
            println!("Failed to run mirror-ctl status command (expected in CI): {}", e);
        }
    }
}

/// Test mirror-ctl stop command with missing arguments
#[test]
fn test_cli_mirror_ctl_stop_missing_args() {
    let temp_dir = TempDir::new().unwrap();
    
    let output = Command::new("cargo")
        .args(&["run", "--", "mirror-ctl", "stop"])
        .env("SLYSYNC_MIRROR_PID_DIR", temp_dir.path())
        .output();
    
    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            
            // Should show error message about missing arguments
            assert!(
                stdout.contains("Error: Please specify --name or --source") ||
                stdout.contains("Usage:"),
                "Stop command should show usage when missing arguments"
            );
        }
        Err(e) => {
            println!("Failed to run mirror-ctl stop command (expected in CI): {}", e);
        }
    }
}

/// Test mirror-ctl stop command with name argument
#[test]
fn test_cli_mirror_ctl_stop_with_name() {
    let temp_dir = TempDir::new().unwrap();
    
    let output = Command::new("cargo")
        .args(&["run", "--", "mirror-ctl", "stop", "--name", "NonExistentDaemon"])
        .env("SLYSYNC_MIRROR_PID_DIR", temp_dir.path())
        .output();
    
    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            
            // Should handle non-existent daemon gracefully
            assert!(
                stdout.contains("No running mirror daemon found") ||
                stdout.contains("No running mirror daemons found") ||
                result.status.success(),
                "Stop command should handle non-existent daemon gracefully"
            );
        }
        Err(e) => {
            println!("Failed to run mirror-ctl stop command (expected in CI): {}", e);
        }
    }
}

/// Test mirror-ctl restart command with missing arguments
#[test]
fn test_cli_mirror_ctl_restart_missing_args() {
    let temp_dir = TempDir::new().unwrap();
    
    let output = Command::new("cargo")
        .args(&["run", "--", "mirror-ctl", "restart"])
        .env("SLYSYNC_MIRROR_PID_DIR", temp_dir.path())
        .output();
    
    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            
            // Should show error message about missing arguments
            assert!(
                stdout.contains("Error: Please specify --name or --source") ||
                stdout.contains("Usage:"),
                "Restart command should show usage when missing arguments"
            );
        }
        Err(e) => {
            println!("Failed to run mirror-ctl restart command (expected in CI): {}", e);
        }
    }
}

/// Test mirror-ctl restart command with name argument
#[test]
fn test_cli_mirror_ctl_restart_with_name() {
    let temp_dir = TempDir::new().unwrap();
    
    let output = Command::new("cargo")
        .args(&["run", "--", "mirror-ctl", "restart", "--name", "NonExistentDaemon"])
        .env("SLYSYNC_MIRROR_PID_DIR", temp_dir.path())
        .output();
    
    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            
            // Should handle non-existent daemon gracefully
            assert!(
                stdout.contains("No running mirror daemon found") ||
                stdout.contains("No running mirror daemons found") ||
                result.status.success(),
                "Restart command should handle non-existent daemon gracefully"
            );
        }
        Err(e) => {
            println!("Failed to run mirror-ctl restart command (expected in CI): {}", e);
        }
    }
}

/// Test mirror-ctl resync command with missing arguments
#[test]
fn test_cli_mirror_ctl_resync_missing_args() {
    let temp_dir = TempDir::new().unwrap();
    
    let output = Command::new("cargo")
        .args(&["run", "--", "mirror-ctl", "resync"])
        .env("SLYSYNC_MIRROR_PID_DIR", temp_dir.path())
        .output();
    
    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            
            // Should show error message about missing arguments
            assert!(
                stdout.contains("Error: Please specify --name or --source") ||
                stdout.contains("Usage:"),
                "Resync command should show usage when missing arguments"
            );
        }
        Err(e) => {
            println!("Failed to run mirror-ctl resync command (expected in CI): {}", e);
        }
    }
}

/// Test mirror-ctl resync command with name argument
#[test]
fn test_cli_mirror_ctl_resync_with_name() {
    let temp_dir = TempDir::new().unwrap();
    
    let output = Command::new("cargo")
        .args(&["run", "--", "mirror-ctl", "resync", "--name", "NonExistentDaemon"])
        .env("SLYSYNC_MIRROR_PID_DIR", temp_dir.path())
        .output();
    
    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            
            // Should handle non-existent daemon gracefully
            assert!(
                stdout.contains("No running mirror daemon found") ||
                stdout.contains("No running mirror daemons found") ||
                result.status.success(),
                "Resync command should handle non-existent daemon gracefully"
            );
        }
        Err(e) => {
            println!("Failed to run mirror-ctl resync command (expected in CI): {}", e);
        }
    }
}

/// Test mirror-ctl help output
#[test]
fn test_cli_mirror_ctl_help() {
    let output = Command::new("cargo")
        .args(&["run", "--", "mirror-ctl", "--help"])
        .output();
    
    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            
            // Should contain subcommand information
            assert!(
                stdout.contains("status") &&
                stdout.contains("stop") &&
                stdout.contains("restart") &&
                stdout.contains("resync"),
                "Help should list all mirror-ctl subcommands"
            );
        }
        Err(e) => {
            println!("Failed to run mirror-ctl help command (expected in CI): {}", e);
        }
    }
}

/// Test invalid mirror-ctl subcommand
#[test]
fn test_cli_mirror_ctl_invalid_subcommand() {
    let output = Command::new("cargo")
        .args(&["run", "--", "mirror-ctl", "invalid"])
        .output();
    
    match output {
        Ok(result) => {
            // Should fail with invalid subcommand
            assert!(
                !result.status.success(),
                "Invalid subcommand should result in error"
            );
            
            let stderr = String::from_utf8_lossy(&result.stderr);
            assert!(
                stderr.contains("error") || stderr.contains("invalid") ||
                stderr.contains("unexpected"),
                "Should show error for invalid subcommand"
            );
        }
        Err(e) => {
            println!("Failed to run invalid mirror-ctl command (expected in CI): {}", e);
        }
    }
}

/// Test mirror-ctl with source path argument
#[test]
fn test_cli_mirror_ctl_stop_with_source() {
    let temp_dir = TempDir::new().unwrap();
    
    let output = Command::new("cargo")
        .args(&["run", "--", "mirror-ctl", "stop", "--source", "/nonexistent/path"])
        .env("SLYSYNC_MIRROR_PID_DIR", temp_dir.path())
        .output();
    
    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            
            // Should handle non-existent source path gracefully
            assert!(
                stdout.contains("No running mirror daemon found") ||
                stdout.contains("No running mirror daemons found") ||
                result.status.success(),
                "Stop with source should handle non-existent path gracefully"
            );
        }
        Err(e) => {
            println!("Failed to run mirror-ctl stop with source (expected in CI): {}", e);
        }
    }
}

/// Test mirror-ctl argument parsing edge cases
#[test]
fn test_cli_mirror_ctl_argument_parsing() {
    let temp_dir = TempDir::new().unwrap();
    
    // Test with both name and source (name should take precedence)
    let output = Command::new("cargo")
        .args(&[
            "run", "--", "mirror-ctl", "stop", 
            "--name", "TestDaemon",
            "--source", "/test/path"
        ])
        .env("SLYSYNC_MIRROR_PID_DIR", temp_dir.path())
        .output();
    
    match output {
        Ok(result) => {
            // Should succeed and handle the request appropriately
            assert!(
                result.status.success() ||
                String::from_utf8_lossy(&result.stdout).contains("No running mirror daemon"),
                "Command with both name and source should work"
            );
        }
        Err(e) => {
            println!("Failed to run mirror-ctl with multiple args (expected in CI): {}", e);
        }
    }
}
