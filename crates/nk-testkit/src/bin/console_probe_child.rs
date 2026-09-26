//! Helper for `console_less_probe`: a console-subsystem program that
//! reports whether *it* has a console window, so the probe can prove that
//! a child spawned through the executor from a console-less parent got no
//! visible console (PROGRESS.md Phase 10 step 0).
//!
//! A console child of a GUI-subsystem parent gets a brand-new console
//! *with* a window unless it was spawned with `CREATE_NO_WINDOW`, in which
//! case it has a console but no window and `GetConsoleWindow` is NULL.
//! Prints exactly one line, `console_window=none` or
//! `console_window=present`. Not useful on its own; Windows-only in
//! practice (elsewhere it always says `none`).

fn main() {
    #[cfg(windows)]
    {
        extern "system" {
            fn GetConsoleWindow() -> isize;
        }
        // Safety: no arguments, no pointers; returns NULL (0) when the
        // process has no console window.
        let window = unsafe { GetConsoleWindow() };
        println!(
            "console_window={}",
            if window == 0 { "none" } else { "present" }
        );
    }
    #[cfg(not(windows))]
    println!("console_window=none");
}
