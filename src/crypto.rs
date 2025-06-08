use anyhow::Result;
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    
    pub fn public_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }
    
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }
    
    pub fn private_key_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }
    
    pub fn peer_id(&self) -> String {
        self.public_key_hex()
    }
    
    pub fn sign(&self, data: &[u8]) -> Signature {
        self.signing_key.sign(data)
    }
    
    pub fn verify(&self, data: &[u8], signature: &Signature) -> bool {
        self.signing_key.verifying_key().verify(data, signature).is_ok()
    }
}

pub fn generate_invitation_code(folder_id: &str) -> Result<String> {
    let invitation = InvitationCode {
        folder_id: folder_id.to_string(),
        peer_id: "temp_peer_id".to_string(), // TODO: Get actual peer ID
        address: "0.0.0.0:0".to_string(), // TODO: Get actual address
        expires_at: chrono::Utc::now() + chrono::Duration::hours(24),
        signature: "temp_signature".to_string(), // TODO: Generate actual signature
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

pub fn validate_invitation_code(code: &str) -> Result<crate::config::RemoteFolderInfo> {
    let invitation = parse_invitation_code(code)?;
    
    // Check expiration
    if chrono::Utc::now() > invitation.expires_at {
        anyhow::bail!("Invitation code has expired");
    }
    
    // TODO: Verify signature
    
    Ok(crate::config::RemoteFolderInfo {
        folder_id: invitation.folder_id,
        peer_id: invitation.peer_id,
        name: None,
    })
}

pub fn hash_file_chunk(data: &[u8]) -> [u8; 32] {
    blake3::hash(data).into()
}

pub fn hash_to_hex(hash: &[u8; 32]) -> String {
    hex::encode(hash)
}
