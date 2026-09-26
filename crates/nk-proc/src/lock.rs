//! Single-instance lock: a lock file (hostname, PID, timestamp) prevents
//! two Nodekeeper processes from using the same data folder at once,
//! including the same portable drive opened from two machines
//! (docs/SPEC.md Foundation C).

use crate::process_check::lock_holder_is_alive;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

const LOCK_FILE_NAME: &str = ".nodekeeper.lock";

/// Where the lock file for `data_root` lives -- for telling a user which
/// file to delete when the lock is held by another computer and cannot be
/// cleaned up automatically.
pub fn lock_file_path(data_root: &Path) -> PathBuf {
    data_root.join(LOCK_FILE_NAME)
}

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
/// (same host, holder no longer running) and cleans up itself.
pub struct SingleInstanceLock {
    path: PathBuf,
}

/// The waits `acquire` uses, as a value so tests can exercise the slow
/// paths (a lock file that is still being written, an orphaned takeover
/// guard) without sleeping in real time.
#[derive(Clone, Copy)]
struct Timing {
    /// How long to pause before re-inspecting when another process is
    /// mid-write or mid-takeover.
    poll: Duration,
    /// How many times `acquire` re-inspects the folder before giving up.
    max_attempts: u32,
    /// How long an unparseable lock file is presumed to be *being written*
    /// by a live copy (a real write takes milliseconds) before it is
    /// treated as damaged.
    partial_grace: Duration,
    /// A lock file that still cannot be parsed, or a takeover guard, that
    /// is older than this cannot belong to a live writer -- it is what an
    /// unclean exit (crash, drive unplugged mid-write) leaves behind.
    orphan_age: Duration,
}

const REAL_TIMING: Timing = Timing {
    poll: Duration::from_millis(50),
    max_attempts: 100,
    partial_grace: Duration::from_millis(500),
    orphan_age: Duration::from_secs(10),
};

impl SingleInstanceLock {
    pub fn acquire(data_root: &Path) -> Result<Self, LockError> {
        Self::acquire_with(data_root, REAL_TIMING)
    }

    /// A bounded retry loop rather than a straight line, because every
    /// step can race another launch: the lock file can appear, vanish, be
    /// half-written, or be replaced between any two of our own steps.
    /// Each pass either wins the lock, refuses with a definite answer, or
    /// changes the folder's state and looks again.
    fn acquire_with(data_root: &Path, timing: Timing) -> Result<Self, LockError> {
        fs::create_dir_all(data_root)?;
        let path = lock_file_path(data_root);
        let hostname = current_hostname()?;
        let pid = std::process::id();
        let mut unparseable_since: Option<Instant> = None;

        for _ in 0..timing.max_attempts {
            if try_create(&path, &hostname, pid)? {
                return Ok(Self { path });
            }

            // Someone's lock is there -- inspect it. It can disappear
            // between `create_new` failing and this read (its owner quit,
            // or another launch is taking it over): that just means look
            // again, not "an error".
            let contents = match fs::read_to_string(&path) {
                Ok(contents) => contents,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };

            let info = match LockInfo::parse(&contents) {
                Ok(info) => {
                    unparseable_since = None;
                    info
                }
                Err(damaged) => {
                    // Empty or garbled: either a live copy is in the middle
                    // of writing it (it is created, *then* filled in), or
                    // it is a leftover of an unclean exit. Give a writer a
                    // moment; after that, only a file too old to belong to
                    // a live writer is replaced -- a young unreadable one
                    // is reported, never deleted.
                    let since = *unparseable_since.get_or_insert_with(Instant::now);
                    if since.elapsed() < timing.partial_grace {
                        std::thread::sleep(timing.poll);
                        continue;
                    }
                    if age_of(&path).is_some_and(|age| age >= timing.orphan_age) {
                        replace_stale(&path, &contents, timing);
                        continue;
                    }
                    return Err(damaged);
                }
            };

            if info.hostname != hostname {
                // Can't probe a process on a different machine at all;
                // always treat as held (spec: "including the same portable
                // drive opened from two machines").
                return Err(LockError::HeldByAnotherHost {
                    hostname: info.hostname,
                    pid: info.pid,
                    timestamp: info.timestamp,
                });
            }

            // Not just "is something running with that pid": after a crash
            // or a reboot the number may belong to an unrelated process,
            // which must not keep the folder locked forever.
            if lock_holder_is_alive(info.pid, info.timestamp) {
                return Err(LockError::AlreadyRunning {
                    pid: info.pid,
                    timestamp: info.timestamp,
                });
            }

            // Stale: same host, and the holder is gone. Clear it and loop
            // back to the atomic `create_new`, which decides any race.
            replace_stale(&path, &contents, timing);
        }

        Err(LockError::Io {
            message: "the lock file kept changing while Nodekeeper was starting -- try again"
                .to_string(),
        })
    }
}

impl Drop for SingleInstanceLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Removes the lock file at `path` -- but only if it still holds exactly
/// `judged`, the contents that were inspected and found stale, and only
/// one contender at a time may do so.
///
/// Inspecting and removing are separate steps, and the inspection (a
/// process-list scan) can take hundreds of milliseconds, so a plain
/// `remove_file` would delete *whatever* is at the path by then: another
/// launch's brand-new, live lock, if it got there first -- and both copies
/// would run. Under an exclusive takeover guard (a second file, created
/// with `create_new`), the contents are re-read and compared before the
/// removal, so a lock that has been replaced in the meantime is left
/// alone. Removing it does not grant the lock; the caller's next
/// `create_new` still decides who gets it.
fn replace_stale(path: &Path, judged: &str, timing: Timing) {
    let guard = guard_path(path);
    match OpenOptions::new().write(true).create_new(true).open(&guard) {
        Ok(file) => {
            drop(file);
            if fs::read_to_string(path).is_ok_and(|now| now == judged) {
                let _ = fs::remove_file(path);
            }
            let _ = fs::remove_file(&guard);
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Another launch is mid-takeover -- or died mid-takeover,
            // which its guard's age gives away.
            if age_of(&guard).is_some_and(|age| age >= timing.orphan_age) {
                let _ = fs::remove_file(&guard);
            } else {
                std::thread::sleep(timing.poll);
            }
        }
        // Anything else (a transient error): pause; the caller's retry
        // loop tries again and eventually gives up with its own error.
        Err(_) => std::thread::sleep(timing.poll),
    }
}

fn guard_path(lock_path: &Path) -> PathBuf {
    let mut name = lock_path.file_name().unwrap_or_default().to_os_string();
    name.push(".takeover");
    lock_path.with_file_name(name)
}

/// How long ago `path` was last written, if that can be determined.
fn age_of(path: &Path) -> Option<Duration> {
    fs::metadata(path).ok()?.modified().ok()?.elapsed().ok()
}

/// Atomically creates the lock file only if it doesn't already exist
/// (`O_EXCL` semantics via `create_new`), avoiding the race window a
/// naive "check then write" would have. Returns `Ok(true)` if this call
/// won the lock, `Ok(false)` if it already existed. If filling the file in
/// fails (disk full, drive unplugged) the empty file just created is
/// removed again, so a failed start never leaves a lock that reads as
/// "damaged" to the next launch.
fn try_create(path: &Path, hostname: &str, pid: u32) -> Result<bool, LockError> {
    let info = LockInfo {
        hostname: hostname.to_string(),
        pid,
        timestamp: now_unix(),
    };
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(info.serialize().as_bytes()) {
                drop(f);
                let _ = fs::remove_file(path);
                return Err(e.into());
            }
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

    /// The crash-then-reboot case: the lock names a pid that a completely
    /// unrelated program now has. That is a stale lock, not a running
    /// Nodekeeper, so the folder must not stay locked (and must not be
    /// wrongly reported as "already running").
    #[test]
    fn a_lock_whose_pid_was_reused_by_an_unrelated_program_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join(LOCK_FILE_NAME);

        // A live process that is definitely not us.
        let mut other = std::process::Command::new(if cfg!(windows) { "ping" } else { "sleep" });
        if cfg!(windows) {
            other.args(["-n", "30", "127.0.0.1"]);
        } else {
            other.arg("30");
        }
        let mut other = other
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to spawn a long-lived unrelated process");

        // The lock was written an hour ago; the unrelated program is
        // running now -- so it started after the lock, as any real reuse of
        // the pid must have.
        let info = LockInfo {
            hostname: current_hostname().unwrap(),
            pid: other.id(),
            timestamp: now_unix() - 3600,
        };
        fs::write(&lock_path, info.serialize()).unwrap();

        let result = SingleInstanceLock::acquire(dir.path());
        let _ = other.kill();
        let _ = other.wait();
        assert!(
            result.is_ok(),
            "an unrelated live process must not keep the folder locked: {:?}",
            result.err()
        );
    }

    #[test]
    fn the_lock_file_path_is_inside_the_data_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            lock_file_path(dir.path()),
            dir.path().join(".nodekeeper.lock")
        );
    }

    fn quick_timing() -> Timing {
        Timing {
            poll: Duration::from_millis(10),
            max_attempts: 100,
            partial_grace: Duration::from_millis(30),
            orphan_age: Duration::from_secs(3600),
        }
    }

    fn set_age(path: &Path, age: Duration) {
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - age).unwrap();
    }

    /// The leftover of an unclean exit -- launched, created the lock file,
    /// then died (or the drive was pulled) before filling it in. Nothing
    /// live can own an empty file that old, so it must not block the folder
    /// forever behind a "damaged" message.
    #[test]
    fn an_empty_lock_left_by_an_unclean_exit_is_replaced_once_it_is_old_enough() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join(LOCK_FILE_NAME);
        fs::write(&lock_path, "").unwrap();
        set_age(&lock_path, Duration::from_secs(600));

        let timing = Timing {
            orphan_age: Duration::from_secs(60),
            ..quick_timing()
        };
        let lock = SingleInstanceLock::acquire_with(dir.path(), timing);
        assert!(lock.is_ok(), "{:?}", lock.err());
        assert!(fs::read_to_string(&lock_path)
            .unwrap()
            .contains(&std::process::id().to_string()));
    }

    /// ...but a *young* unreadable lock is reported, and never deleted: it
    /// might be a live writer's, and the user is told what to do.
    #[test]
    fn a_young_unreadable_lock_is_reported_as_damaged_and_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join(LOCK_FILE_NAME);
        fs::write(&lock_path, "garbage\n").unwrap();

        let result = SingleInstanceLock::acquire_with(dir.path(), quick_timing());
        assert!(
            matches!(result, Err(LockError::Corrupt { .. })),
            "{:?}",
            result.err()
        );
        assert!(lock_path.exists());
    }

    /// The lock file is created and *then* filled in, so a second launch can
    /// catch it empty. That is a writer mid-write, not a damaged file: wait
    /// for it and act on what it says.
    #[test]
    fn a_lock_that_is_still_being_written_is_waited_for_not_called_damaged() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join(LOCK_FILE_NAME);
        fs::write(&lock_path, "").unwrap();

        let writer_path = lock_path.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            let info = LockInfo {
                hostname: "some-other-machine".to_string(),
                pid: 1,
                timestamp: now_unix(),
            };
            fs::write(&writer_path, info.serialize()).unwrap();
        });

        let timing = Timing {
            partial_grace: Duration::from_secs(5),
            ..quick_timing()
        };
        let result = SingleInstanceLock::acquire_with(dir.path(), timing);
        writer.join().unwrap();
        assert!(
            matches!(result, Err(LockError::HeldByAnotherHost { .. })),
            "it should have waited and read the real lock: {:?}",
            result.err()
        );
    }

    /// A lock that vanishes while it is being inspected (its owner quit; or
    /// another launch is replacing it) is a reason to look again -- not an
    /// error worth a "can't use this data folder" message.
    #[test]
    fn a_lock_that_vanishes_mid_inspection_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join(LOCK_FILE_NAME);
        // A stale lock (dead pid, this host) that a "previous run" removes
        // itself right as we start looking.
        let info = LockInfo {
            hostname: current_hostname().unwrap(),
            pid: definitely_dead_pid(),
            timestamp: 0,
        };
        fs::write(&lock_path, info.serialize()).unwrap();
        let remover_path = lock_path.clone();
        let remover = std::thread::spawn(move || {
            for _ in 0..200 {
                let _ = fs::remove_file(&remover_path);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let result = SingleInstanceLock::acquire_with(dir.path(), quick_timing());
        remover.join().unwrap();
        assert!(result.is_ok(), "{:?}", result.err());
    }

    /// The takeover race: several launches find the same stale lock at the
    /// same moment. Inspecting it (a process-list scan) takes hundreds of
    /// milliseconds, so the window between "judged stale" and "removed" is
    /// wide; with a plain remove, a slow launch deletes the *winner's* brand
    /// new lock and creates its own, and two copies run on one folder.
    /// Exactly one may win, every round.
    #[test]
    fn racing_launches_over_a_stale_lock_produce_exactly_one_winner() {
        use std::sync::{Arc, Barrier};
        const LAUNCHES: usize = 8;

        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join(LOCK_FILE_NAME);
        let dead = definitely_dead_pid();
        for round in 0..12 {
            let stale = LockInfo {
                hostname: current_hostname().unwrap(),
                pid: dead,
                timestamp: 0,
            };
            fs::write(&lock_path, stale.serialize()).unwrap();

            let barrier = Arc::new(Barrier::new(LAUNCHES));
            let handles: Vec<_> = (0..LAUNCHES)
                .map(|_| {
                    let barrier = barrier.clone();
                    let root = dir.path().to_path_buf();
                    std::thread::spawn(move || {
                        barrier.wait();
                        SingleInstanceLock::acquire(&root)
                    })
                })
                .collect();
            let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

            let winners = results.iter().filter(|r| r.is_ok()).count();
            assert_eq!(
                winners, 1,
                "round {round}: {LAUNCHES} launches, {winners} won"
            );
            // Every loser was told the folder is taken (never an I/O or
            // "damaged" error from the race itself).
            assert!(
                results
                    .iter()
                    .filter_map(|r| r.as_ref().err())
                    .all(|e| matches!(e, LockError::AlreadyRunning { .. })),
                "round {round}: {:?}",
                results
                    .iter()
                    .filter_map(|r| r.as_ref().err())
                    .collect::<Vec<_>>()
            );
            drop(results); // releases the winner's lock for the next round
            assert!(!lock_path.exists());
        }
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
