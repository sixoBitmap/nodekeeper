//! Portable-mode (and Linux-without-Secret-Service) secrets storage: a
//! single file encrypted with a key derived from a user master password
//! via Argon2id (per-file random salt), using XChaCha20-Poly1305 for
//! authenticated encryption (docs/SPEC.md Foundation E).
//!
//! This module never stores wallet encryption passphrases or mnemonics —
//! per the security rules, those are never persisted anywhere, by anyone,
//! full stop. It's for settings-adjacent secrets only (e.g. a remote-mode
//! SSH key, once that feature exists).

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, Generate, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use std::path::Path;
use thiserror::Error;
use zeroize::Zeroizing;

const MAGIC: &[u8; 4] = b"NKS1";
const SALT_LEN: usize = 16;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const HEADER_LEN: usize = MAGIC.len() + SALT_LEN + NONCE_LEN;

#[derive(Debug, Error)]
pub enum SecretsFileError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("wrong master password, or the file is corrupt")]
    DecryptionFailed,
    #[error("file is not a Nodekeeper encrypted secrets file")]
    InvalidFormat,
}

/// Argon2id, well above OWASP's minimum: this runs once per unlock (not
/// per-request), so a desktop app can afford to be generous. 64 MiB
/// memory, 3 iterations, 4-way parallelism.
fn argon2() -> Argon2<'static> {
    let params = Params::new(65536, 3, 4, Some(KEY_LEN)).expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

fn derive_key(
    master_password: &[u8],
    salt: &[u8],
) -> Result<Zeroizing<[u8; KEY_LEN]>, SecretsFileError> {
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    // Argon2's only failure mode here is a bad param/output-length
    // combination, which `argon2()`'s fixed params never trigger.
    argon2()
        .hash_password_into(master_password, salt, key.as_mut())
        .expect("argon2 params are statically valid for a 32-byte output");
    Ok(key)
}

/// Encrypts `plaintext` with a fresh random salt and nonce and writes it
/// to `path`, overwriting any existing file. File layout: `MAGIC (4) |
/// salt (16) | nonce (24) | ciphertext+tag`.
pub fn write(
    path: &Path,
    master_password: &[u8],
    plaintext: &[u8],
) -> Result<(), SecretsFileError> {
    let salt = <[u8; SALT_LEN]>::generate();
    let key = derive_key(master_password, &salt)?;
    let cipher = XChaCha20Poly1305::new(&Key::from(*key));
    let nonce = XNonce::generate();
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .expect("encryption with a freshly generated nonce cannot fail");

    let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    std::fs::write(path, out)?;
    Ok(())
}

/// Decrypts `path` with `master_password`. A wrong password and a
/// corrupt/tampered file are indistinguishable on purpose (both are
/// `DecryptionFailed`) — the authenticated-encryption tag check is what
/// actually fails, giving no oracle for password guessing beyond "it
/// didn't work."
pub fn read(path: &Path, master_password: &[u8]) -> Result<Zeroizing<Vec<u8>>, SecretsFileError> {
    let data = std::fs::read(path)?;
    if data.len() < HEADER_LEN || &data[..MAGIC.len()] != MAGIC {
        return Err(SecretsFileError::InvalidFormat);
    }
    let salt = &data[MAGIC.len()..MAGIC.len() + SALT_LEN];
    let nonce_bytes = &data[MAGIC.len() + SALT_LEN..HEADER_LEN];
    let ciphertext = &data[HEADER_LEN..];

    let key = derive_key(master_password, salt)?;
    let cipher = XChaCha20Poly1305::new(&Key::from(*key));
    let nonce = XNonce::try_from(nonce_bytes).map_err(|_| SecretsFileError::InvalidFormat)?;
    let plaintext = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| SecretsFileError::DecryptionFailed)?;
    Ok(Zeroizing::new(plaintext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_with_the_correct_password() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.enc");
        write(&path, b"correct horse battery staple", b"top secret bytes").unwrap();

        let plaintext = read(&path, b"correct horse battery staple").unwrap();
        assert_eq!(&*plaintext, b"top secret bytes");
    }

    #[test]
    fn wrong_master_password_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.enc");
        write(&path, b"correct horse battery staple", b"top secret bytes").unwrap();

        let result = read(&path, b"wrong password");
        assert!(matches!(result, Err(SecretsFileError::DecryptionFailed)));
    }

    #[test]
    fn tampered_ciphertext_fails_rather_than_returning_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.enc");
        write(&path, b"pw", b"top secret bytes").unwrap();

        let mut data = std::fs::read(&path).unwrap();
        let last = data.len() - 1;
        data[last] ^= 0xFF;
        std::fs::write(&path, data).unwrap();

        let result = read(&path, b"pw");
        assert!(matches!(result, Err(SecretsFileError::DecryptionFailed)));
    }

    #[test]
    fn each_write_uses_a_fresh_salt_and_nonce() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.enc");
        write(&path, b"pw", b"same plaintext").unwrap();
        let first = std::fs::read(&path).unwrap();
        write(&path, b"pw", b"same plaintext").unwrap();
        let second = std::fs::read(&path).unwrap();

        assert_ne!(
            first, second,
            "identical plaintext must not produce identical ciphertext"
        );
    }

    #[test]
    fn not_a_secrets_file_is_rejected_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-a-secrets-file.txt");
        std::fs::write(&path, b"hello world").unwrap();

        let result = read(&path, b"pw");
        assert!(matches!(result, Err(SecretsFileError::InvalidFormat)));
    }
}
