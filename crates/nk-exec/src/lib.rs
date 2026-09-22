//! The central command executor: the only place `std::process::Command` /
//! `tokio::process::Command` may be used outside `nk-proc`.
//!
//! See `ARCHITECTURE.md` ("The central command executor") and
//! `docs/SPEC.md` Foundation B.
#![allow(clippy::disallowed_methods)]
