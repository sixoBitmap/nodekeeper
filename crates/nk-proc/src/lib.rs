//! Process manager: tracks child processes per environment via PID files,
//! single-instance locking, and graceful stop.
//!
//! See `ARCHITECTURE.md` ("The process manager") and `docs/SPEC.md`
//! Foundation C. May use `std::process::Command` / `tokio::process::Command`
//! directly (along with `nk-exec`).
#![allow(clippy::disallowed_methods)]

pub mod bitcoind;
mod console;
pub mod lock;
pub mod ord;
mod process_check;

pub use bitcoind::{
    bitcoind_had_unclean_shutdown, detect_running_bitcoind, BitcoindError, BitcoindProcess,
};
pub use console::process_has_console;
pub use lock::{LockError, SingleInstanceLock};
pub use ord::{
    detect_running_ord, ord_had_unclean_shutdown, wait_until_caught_up, OrdProcess, OrdProcessError,
};
