//! Keeping child processes from popping up console windows on Windows.
//!
//! The shipped Nodekeeper exe is a Windows GUI-subsystem program: it has
//! no console of its own. When such a process spawns a console-subsystem
//! program (`bitcoind`, `ord`, `bitcoin-cli`, a script interpreter, ...)
//! without a flag saying otherwise, Windows allocates a brand-new,
//! visible console for the child. Every command Nodekeeper runs would
//! flash a terminal window, and long-lived children (bitcoind, ord) would
//! keep one open on screen. Verified live -- DECISIONS.md, "Windows
//! release builds: console-less process handling (Phase 10 step 0)".
//!
//! `CREATE_NO_WINDOW` gives the child a console with no window instead.
//! The executor applies it to every child unconditionally: all executor
//! stdio is piped, so no child needs to share Nodekeeper's console. The
//! one thing that changes when Nodekeeper *does* have a console (dev,
//! debug, `cargo test`) is that a Ctrl+C in the developer's terminal no
//! longer also reaches an executor child that happens to be mid-run --
//! acceptable for short-lived commands, and Ctrl+C kills Nodekeeper
//! itself anyway. The long-lived children (bitcoind, ord) are handled
//! separately in `nk-proc`, which keeps them on the shared console when
//! there is one, precisely so that a developer's Ctrl+C still stops them.
//!
//! Only `nk-exec` and `nk-proc` may spawn processes (CLAUDE.md), so this
//! lives here, next to the executor, and `nk-proc` reuses it.

/// The Win32 `CREATE_NO_WINDOW` process-creation flag.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Makes `command` run without a visible console window on Windows.
/// A no-op on other platforms. Replaces any process-creation flags set
/// earlier on `command` (Win32 takes one flags word), so a caller that
/// needs other flags too must build the combined value itself.
pub fn no_console_window(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    #[cfg(not(windows))]
    let _ = command;
}
