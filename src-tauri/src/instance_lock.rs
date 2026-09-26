//! Wires `nk_proc::SingleInstanceLock` into startup (docs/SPEC.md
//! Foundation C: "a lock file in the data folder (hostname, PID,
//! timestamp) prevents two copies of Nodekeeper from using the same data
//! folder, including the same portable drive opened from two machines.
//! Detect and clean stale locks safely.").
//!
//! The lock itself (and its stale-lock detection) lives in `nk-proc`; this
//! module is the app side: take it before anything opens the settings
//! database, explain a refusal in plain words, and make sure it is
//! released on the way out.
//!
//! **Scope**: the lock covers the app's data folder (`data_root()`: the
//! settings database, and -- in portable mode -- everything on the drive,
//! since `config/` and `data/` sit side by side). An environment data
//! folder the user points somewhere else entirely (an external SSD shared
//! between computers) is *not* locked; see DECISIONS.md, "Single-instance
//! lock and command-history pruning".

use nk_proc::{lock_file_path, LockError, SingleInstanceLock};
use std::path::Path;
use std::sync::Mutex;

/// Holds the lock for the whole life of the app, as Tauri managed state.
///
/// Released **explicitly** (`release`, from the `RunEvent::Exit` handler)
/// rather than by relying on `Drop`: Tauri ends the process with
/// `std::process::exit`, which does not run destructors of managed state,
/// so a lock only released by `Drop` would be left behind after every
/// normal quit. (A crash leaves it behind too, which is what the stale-lock
/// detection in `nk-proc` is for.)
pub struct InstanceLock(Mutex<Option<SingleInstanceLock>>);

impl InstanceLock {
    pub fn new(lock: SingleInstanceLock) -> Self {
        Self(Mutex::new(Some(lock)))
    }

    /// Removes the lock file. Safe to call more than once.
    pub fn release(&self) {
        // A poisoned mutex only means another thread panicked while
        // holding it; taking the lock out is still the right thing.
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        drop(slot.take());
    }
}

/// A refusal to start, worded for the person who has to act on it.
#[derive(Debug, PartialEq, Eq)]
pub struct Refusal {
    pub title: String,
    pub message: String,
}

/// Takes the lock on `data_root`, or explains why it could not.
pub fn acquire_or_refuse(data_root: &Path) -> Result<SingleInstanceLock, Refusal> {
    SingleInstanceLock::acquire(data_root).map_err(|e| explain(&e, data_root, now_unix()))
}

/// Shows `refusal` in a native message box and ends the process. Used
/// before the Tauri app exists (the lock must be taken before the settings
/// database is opened), so this can't use the app's own dialog plugin;
/// `rfd` is what that plugin is built on, so it adds nothing new.
pub fn show_refusal_and_exit(refusal: Refusal) -> ! {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(&refusal.title)
        .set_description(&refusal.message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    std::process::exit(1)
}

/// Turns a lock failure into words a non-technical user can act on.
/// `now` (unix seconds) is a parameter so the wording is testable.
fn explain(error: &LockError, data_root: &Path, now: u64) -> Refusal {
    let lock_file = lock_file_path(data_root).display().to_string();
    match error {
        LockError::AlreadyRunning { pid, timestamp } => Refusal {
            title: "Nodekeeper is already running".to_string(),
            message: format!(
                "Another copy of Nodekeeper is already using this data folder (it started {}).\n\n\
                 Switch to that window -- or look for its icon in the system tray -- instead of \
                 opening a second copy. If you can't find it, end it in Task Manager (process id \
                 {pid}) and try again.",
                age(*timestamp, now)
            ),
        },
        LockError::HeldByAnotherHost {
            hostname,
            pid: _,
            timestamp,
        } => Refusal {
            title: "This data folder is in use on another computer".to_string(),
            message: format!(
                "Nodekeeper on the computer \"{hostname}\" opened this data folder {} and hasn't \
                 closed it.\n\n\
                 If it is still running there, close it first: two computers using one drive at \
                 the same time can damage it.\n\n\
                 If that computer is off, or crashed, or the drive was unplugged without closing \
                 Nodekeeper, delete this file and start Nodekeeper again:\n{lock_file}",
                age(*timestamp, now)
            ),
        },
        LockError::Corrupt { reason } => Refusal {
            title: "Nodekeeper can't read this folder's lock file".to_string(),
            message: format!(
                "The lock file in this data folder is damaged ({reason}).\n\n\
                 If no other copy of Nodekeeper is running, delete it and start again:\n{lock_file}"
            ),
        },
        LockError::UnknownHostname | LockError::Io { .. } => Refusal {
            title: "Nodekeeper can't use this data folder".to_string(),
            message: format!(
                "It couldn't create its lock file here:\n{lock_file}\n\n{error}\n\n\
                 Check that the folder exists, that you have permission to write to it, and that \
                 the drive isn't read-only."
            ),
        },
    }
}

/// "just now", "5 minutes ago", "2 hours ago", "3 days ago".
fn age(since: u64, now: u64) -> String {
    let secs = now.saturating_sub(since);
    let (n, unit) = match secs {
        0..=59 => return "just now".to_string(),
        60..=3599 => (secs / 60, "minute"),
        3600..=86_399 => (secs / 3600, "hour"),
        _ => (secs / 86_400, "day"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const NOW: u64 = 1_800_000_000;

    fn folder() -> PathBuf {
        PathBuf::from("data-folder")
    }

    #[test]
    fn ages_read_naturally() {
        assert_eq!(age(NOW, NOW), "just now");
        assert_eq!(age(NOW - 59, NOW), "just now");
        assert_eq!(age(NOW - 60, NOW), "1 minute ago");
        assert_eq!(age(NOW - 5 * 60, NOW), "5 minutes ago");
        assert_eq!(age(NOW - 3600, NOW), "1 hour ago");
        assert_eq!(age(NOW - 2 * 3600, NOW), "2 hours ago");
        assert_eq!(age(NOW - 86_400, NOW), "1 day ago");
        assert_eq!(age(NOW - 3 * 86_400, NOW), "3 days ago");
        // A clock that went backwards must not underflow.
        assert_eq!(age(NOW + 100, NOW), "just now");
    }

    #[test]
    fn a_second_copy_on_this_computer_is_told_where_to_look() {
        let refusal = explain(
            &LockError::AlreadyRunning {
                pid: 4242,
                timestamp: NOW - 120,
            },
            &folder(),
            NOW,
        );
        assert_eq!(refusal.title, "Nodekeeper is already running");
        assert!(
            refusal.message.contains("2 minutes ago"),
            "{}",
            refusal.message
        );
        assert!(
            refusal.message.contains("system tray"),
            "{}",
            refusal.message
        );
        assert!(refusal.message.contains("4242"), "{}", refusal.message);
    }

    /// The portable-drive-from-two-computers case: it must never suggest
    /// carrying on, must name the other computer, and must say which file
    /// to delete if (and only if) that computer is really done with it.
    #[test]
    fn a_lock_from_another_computer_names_it_and_the_file_to_delete() {
        let refusal = explain(
            &LockError::HeldByAnotherHost {
                hostname: "WORK-LAPTOP".to_string(),
                pid: 7,
                timestamp: NOW - 2 * 3600,
            },
            &folder(),
            NOW,
        );
        assert_eq!(
            refusal.title,
            "This data folder is in use on another computer"
        );
        assert!(
            refusal.message.contains("WORK-LAPTOP"),
            "{}",
            refusal.message
        );
        assert!(
            refusal.message.contains("2 hours ago"),
            "{}",
            refusal.message
        );
        assert!(
            refusal.message.contains("close it first"),
            "{}",
            refusal.message
        );
        assert!(
            refusal
                .message
                .contains(&lock_file_path(&folder()).display().to_string()),
            "{}",
            refusal.message
        );
    }

    #[test]
    fn a_damaged_lock_file_and_an_unwritable_folder_each_say_what_to_do() {
        let corrupt = explain(
            &LockError::Corrupt {
                reason: "pid is not a valid number".to_string(),
            },
            &folder(),
            NOW,
        );
        assert!(corrupt.message.contains("pid is not a valid number"));
        assert!(corrupt.message.contains(".nodekeeper.lock"));

        let unwritable = explain(
            &LockError::Io {
                message: "access denied".to_string(),
            },
            &folder(),
            NOW,
        );
        assert_eq!(unwritable.title, "Nodekeeper can't use this data folder");
        assert!(unwritable.message.contains("access denied"));
        assert!(unwritable.message.contains("read-only"));
    }

    #[test]
    fn the_folder_lock_is_exclusive_and_released_explicitly() {
        let dir = tempfile::tempdir().unwrap();
        let lock_file = lock_file_path(dir.path());

        let held = InstanceLock::new(acquire_or_refuse(dir.path()).unwrap());
        assert!(lock_file.exists());

        // A second attempt (a second copy of the app) is refused, and the
        // refusal explains itself.
        let Err(refused) = acquire_or_refuse(dir.path()) else {
            panic!("a second copy on the same folder must be refused");
        };
        assert_eq!(refused.title, "Nodekeeper is already running");
        assert!(
            lock_file.exists(),
            "a refused copy must not disturb the lock"
        );

        // The explicit release the Exit handler performs frees the folder
        // (and is harmless to repeat).
        held.release();
        held.release();
        assert!(!lock_file.exists());
        assert!(acquire_or_refuse(dir.path()).is_ok());
    }
}
