/// Encrypted wallet file persistence.
///
/// File format: version (4 bytes, LE u32) || salt (32 bytes) || nonce (12 bytes) || ciphertext
/// Key derivation: Argon2id from password + salt (params determined by version)
/// Encryption: AES-256-GCM
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

const FORMAT_VERSION: u32 = 1;
const VERSION_LEN: usize = 4;
const SALT_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// KDF parameters for a given format version.
struct KdfParams {
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
}

/// Map a format version to its Argon2id parameters.
fn kdf_params_for_version(version: u32) -> Result<KdfParams, WalletFileError> {
    match version {
        1 => Ok(KdfParams {
            m_cost: 65536, // 64 MiB
            t_cost: 3,
            p_cost: 1,
        }),
        _ => Err(WalletFileError::UnsupportedVersion(version)),
    }
}

#[derive(Debug, Error)]
pub enum WalletFileError {
    #[error("wallet file is too short to contain valid encrypted data")]
    FileTooShort,
    #[error("unsupported wallet format version: {0}")]
    UnsupportedVersion(u32),
    #[error("decryption failed — wrong password or corrupt file")]
    DecryptionFailed,
    #[error("key derivation failed — argon2 internal error")]
    KeyDerivationFailed,
    #[error("failed to serialize wallet data: {0}")]
    SerializationError(String),
    #[error("failed to deserialize wallet data: {0}")]
    DeserializationError(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Derive a 256-bit key from password + salt using Argon2id.
fn derive_key(
    password: &[u8],
    salt: &[u8],
    kdf: &KdfParams,
) -> Result<[u8; KEY_LEN], WalletFileError> {
    let params = Params::new(kdf.m_cost, kdf.t_cost, kdf.p_cost, Some(KEY_LEN))
        .map_err(|_| WalletFileError::KeyDerivationFailed)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(password, salt, &mut key)
        .map_err(|_| WalletFileError::KeyDerivationFailed)?;
    Ok(key)
}

/// Encrypt plaintext with AES-256-GCM. Returns version || salt || nonce || ciphertext.
pub fn encrypt(plaintext: &[u8], password: &[u8]) -> Result<Vec<u8>, WalletFileError> {
    let kdf = kdf_params_for_version(FORMAT_VERSION)?;
    let salt: [u8; SALT_LEN] = rand::random();
    let nonce_bytes: [u8; NONCE_LEN] = rand::random();

    let mut key = derive_key(password, &salt, &kdf)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| WalletFileError::DecryptionFailed)?;
    key.zeroize();

    let nonce = Nonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| WalletFileError::DecryptionFailed)?;

    let mut output = Vec::with_capacity(VERSION_LEN + SALT_LEN + NONCE_LEN + ciphertext.len());
    output.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt data produced by `encrypt`. Expects version || salt || nonce || ciphertext.
/// Returns `Zeroizing<Vec<u8>>` so the plaintext is automatically zeroized on drop.
pub fn decrypt(data: &[u8], password: &[u8]) -> Result<Zeroizing<Vec<u8>>, WalletFileError> {
    let min_len = VERSION_LEN + SALT_LEN + NONCE_LEN + 1;
    if data.len() < min_len {
        return Err(WalletFileError::FileTooShort);
    }

    let version = u32::from_le_bytes(
        data[..VERSION_LEN]
            .try_into()
            .map_err(|_| WalletFileError::FileTooShort)?,
    );
    let kdf = kdf_params_for_version(version)?;

    let salt = &data[VERSION_LEN..VERSION_LEN + SALT_LEN];
    let nonce_bytes = &data[VERSION_LEN + SALT_LEN..VERSION_LEN + SALT_LEN + NONCE_LEN];
    let ciphertext = &data[VERSION_LEN + SALT_LEN + NONCE_LEN..];

    let mut key = derive_key(password, salt, &kdf)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| WalletFileError::DecryptionFailed)?;
    key.zeroize();

    let nonce_arr: [u8; NONCE_LEN] = nonce_bytes
        .try_into()
        .map_err(|_| WalletFileError::FileTooShort)?;
    let nonce = Nonce::from(nonce_arr);
    let plaintext = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| WalletFileError::DecryptionFailed)?;

    Ok(Zeroizing::new(plaintext))
}

/// Save encrypted data to a file, creating parent directories if needed.
/// Uses atomic write (write to temp, fsync, rename) to prevent corruption.
/// Sets restrictive permissions: directory 0700, file 0600 on Unix.
pub fn save_to_file(
    path: &std::path::Path,
    plaintext: &[u8],
    password: &[u8],
) -> Result<(), WalletFileError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let encrypted = encrypt(plaintext, password)?;

    // Atomic write: temp file → fsync → rename
    let tmp_path = path.with_extension("wallet.tmp");

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp_path)?;
        file.write_all(&encrypted)?;
        file.sync_all()?;
    }
    // Windows: no restrictive ACLs without extra deps, but we still fsync for durability.
    #[cfg(not(unix))]
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)?;
        file.write_all(&encrypted)?;
        file.sync_all()?;
    }

    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Load and decrypt data from a file.
/// Returns `Zeroizing<Vec<u8>>` so the plaintext is automatically zeroized on drop.
pub fn load_from_file(
    path: &std::path::Path,
    password: &[u8],
) -> Result<Zeroizing<Vec<u8>>, WalletFileError> {
    let data = std::fs::read(path)?;
    decrypt(&data, password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let plaintext = b"secret wallet mnemonic data here";
        let password = b"hunter2";

        let encrypted = encrypt(plaintext, password).unwrap();
        let decrypted = decrypt(&encrypted, password).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn wrong_password_fails() {
        let plaintext = b"secret data";
        let password = b"correct-password";
        let wrong = b"wrong-password";

        let encrypted = encrypt(plaintext, password).unwrap();
        let result = decrypt(&encrypted, wrong);

        assert!(result.is_err());
        match result.unwrap_err() {
            WalletFileError::DecryptionFailed => {}
            other => panic!("expected DecryptionFailed, got: {other}"),
        }
    }

    #[test]
    fn corrupt_data_fails() {
        let plaintext = b"secret data";
        let password = b"password";

        let mut encrypted = encrypt(plaintext, password).unwrap();
        // Flip a byte in the ciphertext
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xff;

        let result = decrypt(&encrypted, password);
        assert!(result.is_err());
    }

    #[test]
    fn too_short_data_fails() {
        let result = decrypt(&[0u8; 10], b"password");
        assert!(result.is_err());
        match result.unwrap_err() {
            WalletFileError::FileTooShort => {}
            other => panic!("expected FileTooShort, got: {other}"),
        }
    }

    #[test]
    fn file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wallet");

        let plaintext = b"mnemonic words go here";
        let password = b"file-password";

        save_to_file(&path, plaintext, password).unwrap();
        let decrypted = load_from_file(&path, password).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn different_encryptions_produce_different_output() {
        let plaintext = b"same data";
        let password = b"same password";

        let enc1 = encrypt(plaintext, password).unwrap();
        let enc2 = encrypt(plaintext, password).unwrap();

        // Different salt + nonce means different ciphertext
        assert_ne!(enc1, enc2);

        // But both decrypt to the same plaintext
        assert_eq!(
            decrypt(&enc1, password).unwrap(),
            decrypt(&enc2, password).unwrap()
        );
    }
}
