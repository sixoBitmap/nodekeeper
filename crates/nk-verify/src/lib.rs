//! Binary downloads, SHA-256 checksums, and PGP signature verification
//! against pinned builder keys / pinned hashes. Fails closed.
//!
//! See `docs/SPEC.md` Foundation "Bitcoin Core verification" / "ord
//! verification" and `DECISIONS.md` for pinned values and their sources.

pub mod armor;
pub mod bitcoin_core;
pub mod download;
pub mod ord;
pub mod pinned_keys;
pub mod verify;
