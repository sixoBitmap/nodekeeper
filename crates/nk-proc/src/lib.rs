//! Process manager: tracks child processes per environment via PID files,
//! single-instance locking, and graceful stop.
//!
//! See `ARCHITECTURE.md` ("The process manager") and `docs/SPEC.md`
//! Foundation C. May use `std::process::Command` / `tokio::process::Command`
//! directly (along with `nk-exec`).
#![allow(clippy::disallowed_methods)]

pub mod bitcoind;
pub mod lock;
pub mod ord;
mod process_check;

pub use bitcoind::{detect_running_bitcoind, BitcoindError, BitcoindProcess};
pub use lock::{LockError, SingleInstanceLock};
pub use ord::{detect_running_ord, OrdProcess, OrdProcessError};
