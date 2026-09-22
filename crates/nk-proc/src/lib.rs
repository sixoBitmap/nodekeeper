//! Process manager: tracks child processes per environment via PID files,
//! single-instance locking, and graceful stop.
//!
//! See `ARCHITECTURE.md` ("The process manager") and `docs/SPEC.md`
//! Foundation C. May use `std::process::Command` / `tokio::process::Command`
//! directly (along with `nk-exec`).
#![allow(clippy::disallowed_methods)]

pub mod lock;

pub use lock::{LockError, SingleInstanceLock};
