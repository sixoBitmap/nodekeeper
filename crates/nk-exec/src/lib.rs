//! The central command executor: the only place `std::process::Command` /
//! `tokio::process::Command` may be used outside `nk-proc`.
//!
//! See `ARCHITECTURE.md` ("The central command executor") and
//! `docs/SPEC.md` Foundation B.
#![allow(clippy::disallowed_methods)]

pub mod console;
pub mod executor;
pub mod redact;
pub mod types;

pub use console::no_console_window;
#[cfg(windows)]
pub use console::CREATE_NO_WINDOW;
pub use executor::{ExecError, Executor};
pub use types::{
    CommandId, CommandSource, CommandSpec, ExecEvent, ExecOutcome, OutputStream, RecordSpec,
    Sensitivity, SENSITIVE_OUTPUT_PLACEHOLDER,
};
