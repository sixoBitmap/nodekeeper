//! Environment model, per-chain path resolution, and config generation.
//!
//! See `ARCHITECTURE.md` ("The environment model") and `docs/SPEC.md`
//! Foundation A.

pub mod bitcoin_conf;
pub mod chain;
pub mod environment;
pub mod error_code;
pub mod paths;
pub mod system_check;

pub use chain::Chain;
pub use environment::Environment;
pub use error_code::AppErrorCode;
