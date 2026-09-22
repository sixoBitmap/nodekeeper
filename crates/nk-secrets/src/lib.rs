//! Secrets storage: OS keychain (installed mode) or an Argon2id +
//! XChaCha20-Poly1305 encrypted file (portable mode / no Secret Service),
//! plus zeroization helpers. Wallet passphrases and mnemonics are never
//! stored here — see `docs/SPEC.md` Foundation E.
