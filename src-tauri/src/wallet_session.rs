//! In-memory "remember this wallet's unlock passphrase for a while"
//! state (docs/SPEC.md item 3: "Optional setting: 'Remember for this
//! session', held in memory only, cleared on lock, app exit, or after
//! a configurable idle timeout (default 15 minutes)"). Never persisted
//! anywhere (no settings table, no keychain, no secrets file) -- it
//! exists only as long as the app process is running, exactly like
//! `NodeManager`'s own in-memory-only tracked state.

use nk_core::Chain;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

const DEFAULT_REMEMBER_TIMEOUT: Duration = Duration::from_secs(15 * 60);

struct Remembered {
    passphrase: Zeroizing<String>,
    expires_at: Instant,
}

/// App-wide tracker of remembered wallet passphrases, keyed by chain --
/// same one-per-chain shape as `NodeManager`'s tracked processes, since
/// each chain has (for now) exactly one wallet.
#[derive(Default)]
pub struct WalletSession {
    remembered: Mutex<HashMap<Chain, Remembered>>,
}

impl WalletSession {
    pub fn new() -> Self {
        Self::default()
    }

    /// Remembers `passphrase` for `chain` for the default idle timeout
    /// (15 minutes), starting now. A later action that reads it via
    /// `get` refreshes nothing -- the clock always runs from when this
    /// was called, not from last use, matching "idle timeout" read as
    /// "since the user last actively unlocked," the simpler and safer
    /// reading (a busy signing session doesn't stay unlocked forever).
    /// Takes an already-`Zeroizing` passphrase, not a plain `String` --
    /// Phase 5 security self-review (DECISIONS.md): the caller should
    /// never need to hold a plain-text copy just to call this.
    pub fn remember(&self, chain: Chain, passphrase: Zeroizing<String>) {
        let mut remembered = self
            .remembered
            .lock()
            .expect("mutex should not be poisoned");
        remembered.insert(
            chain,
            Remembered {
                passphrase,
                expires_at: Instant::now() + DEFAULT_REMEMBER_TIMEOUT,
            },
        );
    }

    /// The remembered passphrase for `chain`, if any and not yet timed
    /// out. An expired entry is removed (zeroizing it via `Zeroizing`'s
    /// own `Drop`) rather than just ignored, so a stale passphrase
    /// never lingers in memory past its stated timeout. Returns a fresh
    /// `Zeroizing` clone rather than a plain `String` -- same self-
    /// review reasoning as `remember`.
    pub fn get(&self, chain: Chain) -> Option<Zeroizing<String>> {
        let mut remembered = self
            .remembered
            .lock()
            .expect("mutex should not be poisoned");
        match remembered.get(&chain) {
            Some(entry) if entry.expires_at > Instant::now() => Some(entry.passphrase.clone()),
            Some(_) => {
                remembered.remove(&chain);
                None
            }
            None => None,
        }
    }

    /// Forgets `chain`'s remembered passphrase immediately (docs/
    /// SPEC.md: "cleared on lock" -- an explicit "lock now" action,
    /// distinct from the automatic unlock/lock cycle around every
    /// individual signing action, which never remembers anything on
    /// its own unless the caller opted in). Called when Core rejects the
    /// remembered passphrase (`with_wallet_unlocked`); the "lock now" UI
    /// control is not built yet.
    pub fn forget(&self, chain: Chain) {
        self.remembered
            .lock()
            .expect("mutex should not be poisoned")
            .remove(&chain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_remembered_returns_none() {
        let session = WalletSession::new();
        assert_eq!(session.get(Chain::Regtest), None);
    }

    #[test]
    fn a_remembered_passphrase_is_returned_until_forgotten() {
        let session = WalletSession::new();
        session.remember(Chain::Regtest, Zeroizing::new("hunter2".to_string()));
        assert_eq!(
            session.get(Chain::Regtest),
            Some(Zeroizing::new("hunter2".to_string()))
        );

        session.forget(Chain::Regtest);
        assert_eq!(session.get(Chain::Regtest), None);
    }

    #[test]
    fn remembering_a_chain_does_not_affect_another() {
        let session = WalletSession::new();
        session.remember(Chain::Regtest, Zeroizing::new("hunter2".to_string()));
        assert_eq!(session.get(Chain::Mainnet), None);
    }

    #[test]
    fn an_expired_entry_is_treated_as_not_remembered() {
        let session = WalletSession::new();
        session.remembered.lock().unwrap().insert(
            Chain::Regtest,
            Remembered {
                passphrase: Zeroizing::new("hunter2".to_string()),
                // Already in the past.
                expires_at: Instant::now() - Duration::from_secs(1),
            },
        );
        assert_eq!(session.get(Chain::Regtest), None);
        // Also actually removed, not just skipped -- confirm the entry
        // is gone rather than merely unreachable via `get`.
        assert!(!session
            .remembered
            .lock()
            .unwrap()
            .contains_key(&Chain::Regtest));
    }
}
