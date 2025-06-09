//! # Cryptographic Operations
//! 
//! This module provides cryptographic functionality for SyncCore, including:
//! - Ed25519 digital signatures for node identity and authentication
//! - Invitation code generation and validation
//! - Peer authentication and verification
//! 
//! All cryptographic operations use industry-standard algorithms:
//! - Ed25519 for digital signatures (RFC 8032)
//! - OS-provided random number generation
//! - Base64 encoding for invitation codes

use anyhow::Result;
use base64::Engine;
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::net::SocketAddr;

/// Cryptographic identity for a SyncCore node.
/// 
/// Each node has a unique Ed25519 key pair that serves as its identity.
/// The public key acts as the node ID, while the private key is used
/// for signing messages and proving authenticity.
#[derive(Clone)]
pub struct Identity {
    signing_key: SigningKey,
}

#[derive(Serialize, Deserialize)]
struct IdentityFile {
    secret_key: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InvitationCode {
    pub folder_id: String,
    pub peer_id: String,
    pub address: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub signature: String,
}

impl Identity {
    pub fn generate() -> Result<Self> {
        let mut csprng = OsRng {};
        let signing_key = SigningKey::generate(&mut csprng);
        
        Ok(Self { signing_key })
    }
    
    pub fn load_or_generate(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::load(path)
        } else {
            let identity = Self::generate()?;
            identity.save(path)?;
            Ok(identity)
        }
    }
    
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let identity_file: IdentityFile = toml::from_str(&content)?;
        
        let signing_key = SigningKey::from_bytes(&identity_file.secret_key);
        
        Ok(Self { signing_key })
    }
    
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let identity_file = IdentityFile {
            secret_key: self.signing_key.to_bytes(),
        };
        
        let content = toml::to_string_pretty(&identity_file)?;
        std::fs::write(path, content)?;
        
        // Set restrictive permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(path, perms)?;
        }
        
        Ok(())
    }
    
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().as_bytes())
    }
    
    #[allow(dead_code)]
    pub fn public_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }
    
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }
    
    #[allow(dead_code)]
    pub fn private_key_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }
    
    pub fn peer_id(&self) -> String {
        self.public_key_hex()
    }
    
    pub fn sign(&self, data: &[u8]) -> Signature {
        self.signing_key.sign(data)
    }
    
    #[allow(dead_code)]
    pub fn verify(&self, data: &[u8], signature: &Signature) -> bool {
        self.signing_key.verifying_key().verify(data, signature).is_ok()
    }
}

pub fn generate_invitation_code(folder_id: &str, identity: &Identity, listen_addr: SocketAddr) -> Result<String> {
    // Create challenge data to sign
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(24);
    let challenge_data = format!("{}:{}:{}", folder_id, identity.peer_id(), expires_at.timestamp());
    
    // Sign the challenge
    let signature = identity.sign(challenge_data.as_bytes());
    
    let invitation = InvitationCode {
        folder_id: folder_id.to_string(),
        peer_id: identity.peer_id(),
        address: listen_addr.to_string(),
        expires_at,
        signature: base64::prelude::BASE64_STANDARD.encode(signature.to_bytes()),
    };
    
    use base64::prelude::*;
    let encoded = BASE64_STANDARD.encode(serde_json::to_vec(&invitation)?);
    Ok(encoded)
}

pub fn parse_invitation_code(code: &str) -> Result<InvitationCode> {
    use base64::prelude::*;
    let decoded = BASE64_STANDARD.decode(code)?;
    let invitation: InvitationCode = serde_json::from_slice(&decoded)?;
    Ok(invitation)
}

pub fn validate_invitation_code(code: &str, expected_peer_key: Option<&VerifyingKey>) -> Result<crate::config::RemoteFolderInfo> {
    let invitation = parse_invitation_code(code)?;
    
    // Check expiration
    if chrono::Utc::now() > invitation.expires_at {
        anyhow::bail!("Invitation code has expired");
    }
    
    // Verify signature if we have the peer's public key
    if let Some(peer_key) = expected_peer_key {
        let challenge_data = format!("{}:{}:{}", invitation.folder_id, invitation.peer_id, invitation.expires_at.timestamp());
        let signature_bytes = base64::prelude::BASE64_STANDARD.decode(&invitation.signature)
            .map_err(|_| anyhow::anyhow!("Invalid signature encoding"))?;
        
        if signature_bytes.len() != 64 {
            anyhow::bail!("Invalid signature length");
        }
        
        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(&signature_bytes);
        let signature = Signature::from_bytes(&sig_array);
        
        if peer_key.verify(challenge_data.as_bytes(), &signature).is_err() {
            anyhow::bail!("Invalid invitation signature");
        }
    }
    
    Ok(crate::config::RemoteFolderInfo {
        folder_id: invitation.folder_id,
        peer_id: invitation.peer_id,
        name: None,
    })
}

pub fn hash_file_chunk(data: &[u8]) -> [u8; 32] {
    blake3::hash(data).into()
}

#[allow(dead_code)]
pub fn hash_to_hex(hash: &[u8; 32]) -> String {
    hex::encode(hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_identity_generation() {
        let identity = Identity::generate().unwrap();
        let public_key_hex = identity.public_key_hex();
        
        // Ed25519 public keys are 32 bytes = 64 hex chars
        assert_eq!(public_key_hex.len(), 64);
    }

    #[test]
    fn test_identity_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let key_path = temp_dir.path().join("test_identity.key");
        
        // Generate and save identity
        let original_identity = Identity::generate().unwrap();
        original_identity.save(&key_path).unwrap();
        
        // Load identity
        let loaded_identity = Identity::load(&key_path).unwrap();
        
        // Should have same public key
        assert_eq!(
            original_identity.public_key_hex(),
            loaded_identity.public_key_hex()
        );
    }

    #[test]
    fn test_identity_load_or_generate_existing() {
        let temp_dir = TempDir::new().unwrap();
        let key_path = temp_dir.path().join("existing_identity.key");
        
        // Create an identity file
        let original_identity = Identity::generate().unwrap();
        original_identity.save(&key_path).unwrap();
        
        // Load or generate should load the existing one
        let loaded_identity = Identity::load_or_generate(&key_path).unwrap();
        
        assert_eq!(
            original_identity.public_key_hex(),
            loaded_identity.public_key_hex()
        );
    }

    #[test]
    fn test_identity_load_or_generate_new() {
        let temp_dir = TempDir::new().unwrap();
        let key_path = temp_dir.path().join("new_identity.key");
        
        // File doesn't exist, should generate new
        let identity = Identity::load_or_generate(&key_path).unwrap();
        
        // Should have created the file
        assert!(key_path.exists());
        
        // Should be able to load it again
        let loaded_identity = Identity::load(&key_path).unwrap();
        assert_eq!(identity.public_key_hex(), loaded_identity.public_key_hex());
    }

    #[test]
    fn test_signing_and_verification() {
        let identity = Identity::generate().unwrap();
        let message = b"test message for signing";
        
        let signature = identity.sign(message);
        let is_valid = identity.verify(message, &signature);
        
        assert!(is_valid);
        
        // Test with different message
        let wrong_message = b"different message";
        let is_valid_wrong = identity.verify(wrong_message, &signature);
        assert!(!is_valid_wrong);
    }

    #[test]
    fn test_invitation_code_generation() {
        let folder_id = "test-folder-123";
        let identity = Identity::generate().unwrap();
        let listen_addr = "127.0.0.1:8080".parse().unwrap();
        let invitation = generate_invitation_code(folder_id, &identity, listen_addr).unwrap();
        
        // Should be base64 encoded
        assert!(!invitation.is_empty());
        
        // Should be decodable
        let parsed = parse_invitation_code(&invitation).unwrap();
        assert_eq!(parsed.folder_id, folder_id);
    }

    #[test]
    fn test_invitation_code_validation() {
        let folder_id = "test-folder-123";
        let identity = Identity::generate().unwrap();
        let listen_addr = "127.0.0.1:8080".parse().unwrap();
        let invitation = generate_invitation_code(folder_id, &identity, listen_addr).unwrap();
        
        let folder_info = validate_invitation_code(&invitation, Some(&identity.public_key())).unwrap();
        
        assert_eq!(folder_info.folder_id, folder_id);
        assert_eq!(folder_info.peer_id, identity.peer_id());
        assert_eq!(folder_info.name, None);
    }

    #[test]
    fn test_invitation_code_expired() {
        // Create an expired invitation
        let invitation = InvitationCode {
            folder_id: "test-folder".to_string(),
            peer_id: "test-peer".to_string(),
            address: "127.0.0.1:41337".to_string(),
            expires_at: chrono::Utc::now() - chrono::Duration::hours(1), // Expired
            signature: "test-signature".to_string(),
        };
        
        use base64::prelude::*;
        let encoded = BASE64_STANDARD.encode(serde_json::to_vec(&invitation).unwrap());
        
        // Should fail validation due to expiration
        assert!(validate_invitation_code(&encoded, None).is_err());
    }

    #[test]
    fn test_hash_file_chunk() {
        let data = b"test data for hashing";
        let hash = hash_file_chunk(data);
        
        // BLAKE3 produces 32-byte hashes
        assert_eq!(hash.len(), 32);
        
        // Same data should produce same hash
        let hash2 = hash_file_chunk(data);
        assert_eq!(hash, hash2);
        
        // Different data should produce different hash
        let different_data = b"different test data";
        let different_hash = hash_file_chunk(different_data);
        assert_ne!(hash, different_hash);
    }

    #[test]
    fn test_hash_to_hex() {
        let hash = [0u8; 32]; // All zeros
        let hex = hash_to_hex(&hash);
        assert_eq!(hex, "0".repeat(64));
        
        let hash = [255u8; 32]; // All ones
        let hex = hash_to_hex(&hash);
        assert_eq!(hex, "f".repeat(64));
    }

    #[test]
    fn test_identity_peer_id() {
        let identity = Identity::generate().unwrap();
        let peer_id = identity.peer_id();
        let public_key_hex = identity.public_key_hex();
        
        assert_eq!(peer_id, public_key_hex);
    }

    #[test]
    fn test_identity_key_bytes() {
        let identity = Identity::generate().unwrap();
        
        let public_bytes = identity.public_key_bytes();
        let private_bytes = identity.private_key_bytes();
        
        assert_eq!(public_bytes.len(), 32);
        assert_eq!(private_bytes.len(), 32);
        
        // Public and private keys should be different
        assert_ne!(public_bytes, private_bytes);
    }
}
