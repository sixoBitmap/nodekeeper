//! How `ord`'s graceful stop is delivered on Windows, and why it depends
//! on whether Nodekeeper itself has a console.
//!
//! `ord` has no stop RPC (DECISIONS.md, Phase 4), so it is stopped with a
//! console signal: `CTRL_BREAK_EVENT` sent to ord's own process group.
//! `GenerateConsoleCtrlEvent` only reaches processes that share the
//! *caller's* console. That held in every build tested through Phase 9 --
//! dev, debug and `cargo test` runs all have a terminal attached -- but
//! the shipped exe is a GUI-subsystem program with no console at all, and
//! there the call fails with "The handle is invalid" and ord is left
//! running (verified live: DECISIONS.md, "Windows release builds:
//! console-less process handling (Phase 10 step 0)").
//!
//! So the mode is decided once, when ord is spawned:
//!
//! - [`ConsoleMode::Shared`] -- Nodekeeper has a console. Exactly the
//!   behavior verified since Phase 0: ord shares it, and the signal is
//!   sent directly.
//! - [`ConsoleMode::Hidden`] -- Nodekeeper has no console. ord is spawned
//!   with `CREATE_NO_WINDOW` (its own console, no window, so nothing
//!   flashes on screen), and to stop it Nodekeeper briefly attaches to
//!   *that* console, sends the signal to ord's process group, and
//!   detaches again. ord runs its own shutdown handler and exits 0.
//!
//! Attach/detach is process-wide state, so every use is serialized by
//! `CONSOLE_LOCK`, and the attach step never detaches a console the
//! process already owns: if the mode assumption is ever wrong,
//! `AttachConsole` fails with an error rather than tearing down a real
//! terminal.
//!
//! One more piece of process-wide state needs care: in a console-less
//! process the standard handles start as NULL, `AttachConsole` fills
//! them in with the target console's handles, and `FreeConsole` closes
//! those handles **but leaves the stale numbers in place**. Rust's
//! `Stdio::inherit` (the default for a spawn's stdin) duplicates whatever
//! `GetStdHandle` returns unless it is NULL or invalid, so the very next
//! `ord`/`bitcoind` spawn in the same session would fail with "The handle
//! is invalid" (found by the review of this change and reproduced with an
//! independent probe). So the stop saves the three standard handles
//! before attaching and puts them back afterwards, and the ord/bitcoind
//! spawns set `stdin` to null explicitly so they never depend on the
//! parent's standard handles at all.

#[cfg(windows)]
use std::sync::{Mutex, MutexGuard};

/// Serializes everything that reads or changes this process's console
/// attachment (the presence check and the attach-signal-detach dance).
#[cfg(windows)]
static CONSOLE_LOCK: Mutex<()> = Mutex::new(());

#[cfg(windows)]
fn lock() -> MutexGuard<'static, ()> {
    // A poisoned lock only means another thread panicked while holding
    // it; the state it guards (console attachment) is restored by the
    // OS, not by us, so carrying on is safe.
    CONSOLE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// How a child's console relates to Nodekeeper's own. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConsoleMode {
    Shared,
    Hidden,
}

impl ConsoleMode {
    /// The mode for a process that does / doesn't have a console. Pure,
    /// so the decision itself is directly testable.
    pub(crate) fn for_console_presence(has_console: bool) -> Self {
        if has_console {
            Self::Shared
        } else {
            Self::Hidden
        }
    }

    /// The mode for this process, right now.
    pub(crate) fn current() -> Self {
        Self::for_console_presence(process_has_console())
    }
}

/// Whether this process is attached to a console. Always `true` off
/// Windows, where signals need no console at all.
#[cfg(windows)]
pub fn process_has_console() -> bool {
    let _guard = lock();
    let mut pid = 0u32;
    // Safety: `pid` is a valid one-element buffer and the count passed
    // matches. The call returns the number of processes attached to the
    // console -- more than the buffer holds is fine, it just reports the
    // required size -- and 0 when there is no console (or on failure).
    unsafe { GetConsoleProcessList(&mut pid, 1) > 0 }
}

#[cfg(not(windows))]
pub fn process_has_console() -> bool {
    true
}

#[cfg(windows)]
extern "system" {
    fn GetConsoleProcessList(process_list: *mut u32, process_count: u32) -> u32;
    fn GenerateConsoleCtrlEvent(ctrl_event: u32, process_group_id: u32) -> i32;
    fn AttachConsole(process_id: u32) -> i32;
    fn FreeConsole() -> i32;
    // A HANDLE is pointer-sized; `isize` carries it (and the special
    // NULL / INVALID_HANDLE_VALUE values) without needing a windows crate.
    fn GetStdHandle(std_handle: u32) -> isize;
    fn SetStdHandle(std_handle: u32, handle: isize) -> i32;
}

#[cfg(windows)]
const CTRL_BREAK_EVENT: u32 = 1;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
/// `STD_INPUT_HANDLE`, `STD_OUTPUT_HANDLE`, `STD_ERROR_HANDLE`
/// (`(DWORD)-10`, `-11`, `-12`).
#[cfg(windows)]
const STD_HANDLES: [u32; 3] = [0xFFFF_FFF6, 0xFFFF_FFF5, 0xFFFF_FFF4];

/// Process-creation flags for spawning ord in `mode`. Always its own
/// process group, so a later `CTRL_BREAK_EVENT` targets only ord (and
/// nothing it spawns), never Nodekeeper's own group.
#[cfg(windows)]
pub(crate) fn ord_creation_flags(mode: ConsoleMode) -> u32 {
    match mode {
        ConsoleMode::Shared => CREATE_NEW_PROCESS_GROUP,
        ConsoleMode::Hidden => CREATE_NEW_PROCESS_GROUP | nk_exec::CREATE_NO_WINDOW,
    }
}

/// Sends `CTRL_BREAK_EVENT` to the process group `pid` (ord's), by the
/// route `mode` requires.
#[cfg(windows)]
pub(crate) fn send_ctrl_break(pid: u32, mode: ConsoleMode) -> std::io::Result<()> {
    match mode {
        ConsoleMode::Shared => {
            // Safety: plain integer arguments, no pointers; the only
            // failure mode is returning 0, checked here.
            let ok = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) };
            if ok != 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        }
        ConsoleMode::Hidden => send_ctrl_break_via_hidden_console(pid),
    }
}

/// The `Hidden` route: attach to ord's console, signal, detach -- and
/// leave this process's standard handles exactly as they were (see the
/// module docs for why that matters).
#[cfg(windows)]
fn send_ctrl_break_via_hidden_console(pid: u32) -> std::io::Result<()> {
    let _guard = lock();
    // Safety (every call below): plain integer arguments, no pointers;
    // each one's only failure mode is returning 0 / an invalid handle,
    // handled at the call.
    unsafe {
        // Saved before attaching: NULL stays NULL, and whatever was there
        // (even an invalid value) is put back verbatim.
        let saved = STD_HANDLES.map(|which| (which, GetStdHandle(which)));

        // Deliberately no `FreeConsole` first. If this process somehow
        // has a console after all, `AttachConsole` fails (access denied)
        // and we report that, instead of detaching a console we don't
        // own the lifetime of. (Nothing was attached, so nothing to
        // restore on this path.)
        if AttachConsole(pid) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let ok = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid);
        // Capture the signal's error before the calls below can
        // overwrite it.
        let signal_error = std::io::Error::last_os_error();
        FreeConsole();
        for (which, handle) in saved {
            SetStdHandle(which, handle);
        }
        if ok != 0 {
            Ok(())
        } else {
            Err(signal_error)
        }
    }
}

/// Gives a child its own console *without a window* -- but only when
/// Nodekeeper has no console (the release exe). With a console (dev,
/// debug, tests) the child stays attached to it exactly as before, so a
/// Ctrl+C in the developer's terminal still reaches it. Used for
/// bitcoind, which is stopped over RPC and needs no console of its own;
/// ord goes through [`ord_creation_flags`] instead because its stop
/// signal depends on the console it was given.
pub(crate) fn hide_window_when_console_less(command: &mut tokio::process::Command) {
    if ConsoleMode::current() == ConsoleMode::Hidden {
        nk_exec::no_console_window(command);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_with_a_console_shares_it_with_ord() {
        assert_eq!(ConsoleMode::for_console_presence(true), ConsoleMode::Shared);
    }

    #[test]
    fn a_process_without_a_console_gives_ord_a_hidden_one() {
        assert_eq!(
            ConsoleMode::for_console_presence(false),
            ConsoleMode::Hidden
        );
    }

    #[cfg(windows)]
    #[test]
    fn shared_mode_keeps_only_the_process_group_flag() {
        // The same creation flags as before this change (the spawns now
        // also set stdin to null, but that is not a creation flag).
        assert_eq!(
            ord_creation_flags(ConsoleMode::Shared),
            CREATE_NEW_PROCESS_GROUP
        );
    }

    #[cfg(windows)]
    #[test]
    fn hidden_mode_adds_no_window_on_top_of_the_process_group() {
        let flags = ord_creation_flags(ConsoleMode::Hidden);
        assert_ne!(flags & CREATE_NEW_PROCESS_GROUP, 0);
        assert_ne!(flags & nk_exec::CREATE_NO_WINDOW, 0);
    }

    /// The failure path must never leave this process attached to (or
    /// detached from) a console it didn't have (or had) before: run it
    /// against a pid that can't exist and compare console state.
    #[cfg(windows)]
    #[test]
    fn a_failed_hidden_console_signal_leaves_the_console_state_alone() {
        let before = process_has_console();
        // Pids are multiples of 4 on Windows, so an odd one is never live.
        let result = send_ctrl_break_via_hidden_console(0x7fff_fff1);
        assert!(result.is_err());
        assert_eq!(process_has_console(), before);
    }
}
