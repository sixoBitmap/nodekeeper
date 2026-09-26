//! Environment model, per-chain path resolution, and config generation.
//!
//! See `ARCHITECTURE.md` ("The environment model") and `docs/SPEC.md`
//! Foundation A.

pub mod bitcoin_conf;
pub mod chain;
pub mod console_parse;
pub mod console_safety;
pub mod disk;
pub mod environment;
pub mod error_code;
pub mod live_tests;
pub mod log_tail;
pub mod ord_conf;
pub mod paths;
pub mod system_check;

pub use chain::Chain;
pub use environment::{Environment, IndexOptions};
pub use error_code::AppErrorCode;
