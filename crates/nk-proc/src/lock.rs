//! Single-instance lock: a lock file (hostname, PID, timestamp) prevents
//! two Nodekeeper processes from using the same data folder at once,
//! including the same portable drive opened from two machines
//! (docs/SPEC.md Foundation C).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use sysinfo::{Pid, System};
use thiserror::Error;

const LOCK_FILE_NAME: &str = ".nodekeeper.lock";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LockError {
    #[error("Nodekeeper is already running on this data folder (pid {pid}, since {timestamp})")]
    AlreadyRunning { pid: u32, timestamp: u64 },
    #[error(
        "This data folder's lock was taken by a different machine (\"{hostname}\", pid {pid}, \
         since {timestamp}). If that machine really isn't using it anymore (e.g. it crashed), \
         the lock file can be removed manually."
    )]
    HeldByAnotherHost {
        hostname: String,
        pid: u32,
        timestamp: u64,
    },
    #[error("could not determine this machine's hostname")]
    UnknownHostname,
    #[error("lock file is corrupt: {reason}")]
    Corrupt { reason: String },
    #[error("io error: {message}")]
    Io { message: String },
}

impl From<std::io::Error> for LockError {
    fn from(e: std::io::Error) -> Self {
        LockError::Io {
            message: e.to_string(),
        }
    }
}

struct LockInfo {
    hostname: String,
    pid: u32,
    timestamp: u64,
}

impl LockInfo {
    fn parse(contents: &str) -> Result<Self, LockError> {
        let corrupt = |reason: &str| LockError::Corrupt {
            reason: reason.to_string(),
        };
        let mut lines = contents.lines();
        let hostname = lines
            .next()
            .ok_or_else(|| corrupt("missing hostname line"))?
            .to_string();
        let pid: u32 = lines
            .next()
            .ok_or_else(|| corrupt("missing pid line"))?
            .parse()
            .map_err(|_| corrupt("pid is not a valid number"))?;
        let timestamp: u64 = lines
            .next()
            .ok_or_else(|| corrupt("missing timestamp line"))?
            .parse()
            .map_err(|_| corrupt("timestamp is not a valid number"))?;
        Ok(Self {
            hostname,
            pid,
            timestamp,
        })
    }

    fn serialize(&self) -> String {
        format!("{}\n{}\n{}\n", self.hostname, self.pid, self.timestamp)
    }
}

/// Holds the single-instance lock for one data folder for as long as it's
/// alive. Removes the lock file on drop (normal-exit cleanup) — a crash
/// leaves the file in place, which the next `acquire()` detects as stale
/// (same host, PID no longer running) and cleans up itself.
pub struct SingleInstanceLock {
    path: PathBuf,
}

impl SingleInstanceLock {
    pub fn acquire(data_root: &Path) -> Result<Self, LockError> {
        fs::create_dir_all(data_root)?;
        let path = data_root.join(LOCK_FILE_NAME);
        let hostname = current_hostname()?;
        let pid = std::process::id();

        if try_create(&path, &hostname, pid)? {
            return Ok(Self { path });
        }

        // Someone already holds it -- inspect and decide whether it's
        // stale.
        let contents = fs::read_to_string(&path)?;
        let info = LockInfo::parse(&contents)?;

        if info.hostname != hostname {
            // Can't probe a process on a different machine at all; always
            // treat as held (spec: "including the same portable drive
            // opened from two machines").
            return Err(LockError::HeldByAnotherHost {
                hostname: info.hostname,
                pid: info.pid,
                timestamp: info.timestamp,
            });
        }

        if process_is_alive(info.pid) {
            return Err(LockError::AlreadyRunning {
                pid: info.pid,
                timestamp: info.timestamp,
            });
        }

        // Stale: same host, that PID isn't running anymore. Clean up and
        // take the lock.
        fs::remove_file(&path)?;
        if try_create(&path, &hostname, pid)? {
            Ok(Self { path })
        } else {
            // Lost a race with another process cleaning up/acquiring at
            // the same instant -- surface as contention rather than
            // looping.
            Err(LockError::AlreadyRunning {
                pid,
                timestamp: now_unix(),
            })
        }
    }
}

impl Drop for SingleInstanceLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Atomically creates the lock file only if it doesn't already exist
/// (`O_EXCL` semantics via `create_new`), avoiding the race window a
/// naive "check then write" would have. Returns `Ok(true)` if this call
/// won the lock, `Ok(false)` if it already existed.
fn try_create(path: &Path, hostname: &str, pid: u32) -> Result<bool, LockError> {
    let info = LockInfo {
        hostname: hostname.to_string(),
        pid,
        timestamp: now_unix(),
    };
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut f) => {
            f.write_all(info.serialize().as_bytes())?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn current_hostname() -> Result<String, LockError> {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .ok_or(LockError::UnknownHostname)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn process_is_alive(pid: u32) -> bool {
    let mut sys = System::new();
    sys.refresh_all();
    sys.process(Pid::from_u32(pid)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquires_a_fresh_lock_and_releases_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join(LOCK_FILE_NAME);
        assert!(!lock_path.exists());

        let lock = SingleInstanceLock::acquire(dir.path()).unwrap();
        assert!(lock_path.exists());
        drop(lock);
        assert!(!lock_path.exists(), "lock file should be removed on drop");
    }

    #[test]
    fn a_second_instance_on_the_same_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let _first = SingleInstanceLock::acquire(dir.path()).unwrap();

        // Our own process is definitely alive, so this must be refused
        // as AlreadyRunning (not treated as stale).
        let second = SingleInstanceLock::acquire(dir.path());
        assert!(matches!(second, Err(LockError::AlreadyRunning { .. })));
    }

    #[test]
    fn a_lock_from_a_different_host_is_never_treated_as_stale() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join(LOCK_FILE_NAME);
        let info = LockInfo {
            hostname: "some-other-machine".to_string(),
            pid: definitely_dead_pid(),
            timestamp: 0,
        };
        fs::write(&lock_path, info.serialize()).unwrap();

        let result = SingleInstanceLock::acquire(dir.path());
        assert!(matches!(result, Err(LockError::HeldByAnotherHost { .. })));
        // Must not have deleted or touched the other host's lock file.
        assert!(lock_path.exists());
    }

    #[test]
    fn a_stale_lock_from_the_same_host_is_detected_and_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join(LOCK_FILE_NAME);
        let info = LockInfo {
            hostname: current_hostname().unwrap(),
            pid: definitely_dead_pid(),
            timestamp: 0,
        };
        fs::write(&lock_path, info.serialize()).unwrap();

        let lock = SingleInstanceLock::acquire(dir.path());
        assert!(
            lock.is_ok(),
            "a stale same-host lock should be acquired, not refused"
        );
    }

    /// A PID that's real enough to have existed a moment ago but is
    /// guaranteed not to be running anymore -- unlike a hardcoded large
    /// number (which risks flaky false positives from PID reuse/wraparound
    /// on some systems), this spawns and reaps a trivial real process and
    /// returns its now-dead PID.
    fn definitely_dead_pid() -> u32 {
        let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" })
            .args(if cfg!(windows) {
                &["/C", "exit"][..]
            } else {
                &[][..]
            })
            .spawn()
            .expect("failed to spawn a throwaway process");
        let pid = child.id();
        child.wait().expect("failed to wait for throwaway process");
        pid
    }
}
