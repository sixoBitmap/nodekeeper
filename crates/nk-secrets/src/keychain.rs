//! Installed-mode secrets storage: the OS keychain, via the `keyring`
//! crate (macOS Keychain, Windows Credential Manager, Linux Secret
//! Service). docs/SPEC.md Foundation E: on Linux without a Secret Service
//! provider, callers should fall back to `encrypted_file` instead and
//! explain why to the user — this module surfaces that as a distinct
//! error rather than papering over it.

use thiserror::Error;

const SERVICE: &str = "com.nodekeeper.desktop";

#[derive(Debug, Error)]
pub enum KeychainError {
    #[error("no secret found for \"{0}\"")]
    NotFound(String),
    #[error("no OS keychain / Secret Service provider is available on this system")]
    NoBackend,
    #[error("keychain error: {0}")]
    Other(String),
}

impl From<keyring::Error> for KeychainError {
    fn from(e: keyring::Error) -> Self {
        match e {
            keyring::Error::NoEntry => KeychainError::NotFound(String::new()),
            keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => {
                KeychainError::NoBackend
            }
            other => KeychainError::Other(other.to_string()),
        }
    }
}

pub fn set(key: &str, value: &str) -> Result<(), KeychainError> {
    let entry = keyring::Entry::new(SERVICE, key)?;
    entry.set_password(value)?;
    Ok(())
}

pub fn get(key: &str) -> Result<String, KeychainError> {
    let entry = keyring::Entry::new(SERVICE, key)?;
    entry.get_password().map_err(|e| match e {
        keyring::Error::NoEntry => KeychainError::NotFound(key.to_string()),
        other => other.into(),
    })
}

pub fn delete(key: &str) -> Result<(), KeychainError> {
    let entry = keyring::Entry::new(SERVICE, key)?;
    entry.delete_credential()?;
    Ok(())
}

// No automated tests here: OS keychain access needs a real, unlocked
// login session (macOS Keychain / Windows Credential Manager / a Linux
// Secret Service daemon), which typical CI runners don't reliably
// provide — the encrypted-file fallback (`encrypted_file`, used on
// exactly the systems where this isn't available) has the real
// round-trip and wrong-password test coverage. Exercise this module
// manually on each OS per the Phase 1 [MANUAL] checklist.
