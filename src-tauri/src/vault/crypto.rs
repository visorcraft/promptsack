//! App-level lock encryption (AES-256-GCM + scrypt). Independent of MongrelDB at-rest encryption.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;
use scrypt::{scrypt, Params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct LockedBlob {
    v: u8,
    salt: String,
    iv: String,
    tag: String,
    ciphertext: String,
}

fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    let params = Params::new(14, 8, 1, 32).map_err(|e| e.to_string())?; // N=2^14
    let mut key = [0u8; 32];
    scrypt(password.as_bytes(), salt, &params, &mut key).map_err(|e| e.to_string())?;
    Ok(key)
}

pub fn encrypt_content(body: &str, notes: &str, password: &str) -> Result<String, String> {
    if password.is_empty() {
        return Err("Lock password is required".into());
    }
    let mut salt = [0u8; 16];
    let mut iv = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut iv);
    let key = derive_key(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let payload = serde_json::json!({ "body": body, "notes": notes }).to_string();
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&iv), payload.as_bytes())
        .map_err(|e| e.to_string())?;
    // aes-gcm crate appends tag at end
    if encrypted.len() < 16 {
        return Err("encryption failed".into());
    }
    let (ct, tag) = encrypted.split_at(encrypted.len() - 16);
    let blob = LockedBlob {
        v: 1,
        salt: B64.encode(salt),
        iv: B64.encode(iv),
        tag: B64.encode(tag),
        ciphertext: B64.encode(ct),
    };
    Ok(serde_json::to_string(&blob).map_err(|e| e.to_string())?)
}

pub fn decrypt_content(blob_json: &str, password: &str) -> Result<(String, String), String> {
    if password.is_empty() {
        return Err("Unlock password is required".into());
    }
    let blob: LockedBlob = serde_json::from_str(blob_json).map_err(|_| "Invalid locked content")?;
    if blob.v != 1 {
        return Err("Unsupported lock blob version".into());
    }
    let salt = B64.decode(&blob.salt).map_err(|e| e.to_string())?;
    let iv = B64.decode(&blob.iv).map_err(|e| e.to_string())?;
    let tag = B64.decode(&blob.tag).map_err(|e| e.to_string())?;
    let ct = B64.decode(&blob.ciphertext).map_err(|e| e.to_string())?;
    let key = derive_key(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let mut combined = ct;
    combined.extend_from_slice(&tag);
    let plain = cipher
        .decrypt(Nonce::from_slice(&iv), combined.as_ref())
        .map_err(|_| "Wrong password or tampered content")?;
    let v: serde_json::Value = serde_json::from_slice(&plain).map_err(|e| e.to_string())?;
    Ok((
        v.get("body").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        v.get("notes").and_then(|x| x.as_str()).unwrap_or("").to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let blob = encrypt_content("hello body", "hello notes", "pw").unwrap();
        let (b, n) = decrypt_content(&blob, "pw").unwrap();
        assert_eq!(b, "hello body");
        assert_eq!(n, "hello notes");
        assert!(decrypt_content(&blob, "nope").is_err());
    }
}
