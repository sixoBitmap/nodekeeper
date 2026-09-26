//! Shared "is this PID still a running process" checks, used by both the
//! single-instance lock (`lock.rs`) and bitcoind/ord detection
//! (`bitcoind.rs`, `ord.rs`) for staleness/already-running checks.

use sysinfo::{Pid, System};

/// Whether *any* process currently has `pid`. Deliberately nothing more:
/// the bitcoind/ord detection built on this cannot check the process name
/// without risking a false "nothing running" for a node started some other
/// way (Bitcoin-Qt writes the same `bitcoind.pid`), which is the
/// dangerous direction for a "safe to unplug" check -- see DECISIONS.md,
/// "Windows release builds: console-less process handling", second
/// review. The single-instance lock, whose holder is always Nodekeeper
/// itself, can and does check more: [`lock_holder_is_alive`].
pub(crate) fn process_is_alive(pid: u32) -> bool {
    let mut sys = System::new();
    sys.refresh_all();
    sys.process(Pid::from_u32(pid)).is_some()
}

/// How far after the lock's timestamp a process may have started and
/// still be the one that wrote it (clock granularity, and the lock being
/// stamped a moment after the process is created).
const START_SLACK_SECS: u64 = 5;

/// Whether the process `pid` named in a single-instance lock written at
/// `lock_timestamp` (unix seconds) is *still that Nodekeeper*, not just
/// "some process with that pid". A lock outlives its holder whenever the
/// holder can't clean up (crash, Task Manager, power loss, an updater
/// exiting), and pids are recycled aggressively -- especially across a
/// reboot -- so a bare liveness check would leave the data folder locked
/// by, say, an unrelated `chrome.exe` that happened to inherit the number.
/// A process can only have *reused* the pid if the holder was already gone
/// -- so if it started after the lock was written, it is not the holder.
/// That start time is the decisive test; the executable name is only a
/// fallback for when the OS won't report a start time.
pub(crate) fn lock_holder_is_alive(pid: u32, lock_timestamp: u64) -> bool {
    let mut sys = System::new();
    sys.refresh_all();
    let Some(process) = sys.process(Pid::from_u32(pid)) else {
        return false;
    };
    let our_name = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.file_name().map(|n| n.to_string_lossy().into_owned()));
    holder_matches(
        &process.name().to_string_lossy(),
        process.start_time(),
        our_name.as_deref(),
        lock_timestamp,
    )
}

/// The decision behind [`lock_holder_is_alive`], on plain values so it is
/// directly testable.
///
/// - Start time known (`process_start_secs != 0`): decisive. Started no
///   later than the lock was written (plus slack) means it *can* be the
///   holder; started after means it cannot. The name is deliberately **not**
///   consulted: on Linux the OS reports the name the program was launched
///   under (a symlink's name, truncated to 15 characters) while
///   `current_exe` reports the resolved target, so comparing them would
///   judge a live copy started through a symlink stale and let two copies
///   share the folder.
/// - Start time unknown (0: the OS wouldn't say -- e.g. an elevated process
///   seen from a normal one): fall back to the name, if ours is known.
///
/// Every unknown errs toward "still held": wrongly treating a live holder
/// as stale lets two copies share a data folder, while the reverse only
/// asks the user to look.
fn holder_matches(
    process_name: &str,
    process_start_secs: u64,
    our_name: Option<&str>,
    lock_timestamp: u64,
) -> bool {
    if process_start_secs != 0 {
        return process_start_secs <= lock_timestamp + START_SLACK_SECS;
    }
    our_name.is_none_or(|ours| same_program_name(process_name, ours))
}

/// Case-insensitive name comparison that tolerates Linux truncating a
/// process name to 15 characters.
fn same_program_name(a: &str, b: &str) -> bool {
    const LINUX_COMM_MAX: usize = 15;
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    short.len() == LINUX_COMM_MAX && long.starts_with(short.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK_AT: u64 = 1_800_000_000;

    #[test]
    fn our_own_pid_is_alive_and_a_dead_one_is_not() {
        assert!(process_is_alive(std::process::id()));
        assert!(!process_is_alive(u32::MAX - 1));
    }

    #[test]
    fn the_same_program_started_before_the_lock_is_the_holder() {
        assert!(holder_matches(
            "nodekeeper.exe",
            LOCK_AT - 30,
            Some("nodekeeper.exe"),
            LOCK_AT
        ));
    }

    /// A reused pid belongs to a process that started *after* the holder
    /// died, hence after the lock was written -- whatever it is called.
    #[test]
    fn a_different_program_that_reused_the_pid_is_not_the_holder() {
        assert!(!holder_matches(
            "chrome.exe",
            LOCK_AT + 3600,
            Some("nodekeeper.exe"),
            LOCK_AT
        ));
    }

    /// With a known start time the name is not consulted: a copy launched
    /// through a symlink (Linux reports the symlink's name, we know our
    /// resolved one) is still recognised as the live holder.
    #[test]
    fn a_known_start_time_is_decisive_over_a_differing_name() {
        assert!(holder_matches(
            "nodekeeper",
            LOCK_AT - 30,
            Some("nodekeeper-bin"),
            LOCK_AT
        ));
    }

    /// The reboot case: the same program name can even recur (the user
    /// relaunched Nodekeeper), but a process that *started after* the lock
    /// was written cannot be the one that wrote it.
    #[test]
    fn a_process_that_started_after_the_lock_was_written_is_not_the_holder() {
        assert!(!holder_matches(
            "nodekeeper.exe",
            LOCK_AT + 3600,
            Some("nodekeeper.exe"),
            LOCK_AT
        ));
    }

    #[test]
    fn a_few_seconds_of_clock_slack_is_tolerated() {
        assert!(holder_matches(
            "nodekeeper.exe",
            LOCK_AT + START_SLACK_SECS,
            Some("nodekeeper.exe"),
            LOCK_AT
        ));
        assert!(!holder_matches(
            "nodekeeper.exe",
            LOCK_AT + START_SLACK_SECS + 1,
            Some("nodekeeper.exe"),
            LOCK_AT
        ));
    }

    #[test]
    fn with_no_start_time_the_name_decides() {
        assert!(holder_matches("nodekeeper", 0, Some("nodekeeper"), LOCK_AT));
        assert!(!holder_matches(
            "chrome.exe",
            0,
            Some("nodekeeper"),
            LOCK_AT
        ));
        // ...and with neither known, the holder is assumed to be live.
        assert!(holder_matches("anything", 0, None, LOCK_AT));
    }

    #[test]
    fn an_unknown_own_name_leaves_the_start_time_to_decide() {
        assert!(holder_matches("anything", LOCK_AT - 1, None, LOCK_AT));
        assert!(!holder_matches("anything", LOCK_AT + 3600, None, LOCK_AT));
    }

    #[test]
    fn names_compare_case_insensitively_and_tolerate_linux_truncation() {
        assert!(same_program_name("Nodekeeper.EXE", "nodekeeper.exe"));
        // Linux reports at most 15 characters of the name.
        assert!(same_program_name("nodekeeper-por", "nodekeeper-por"));
        assert!(same_program_name("nodekeeper-port", "nodekeeper-portable"));
        assert!(!same_program_name("nodekeeper", "nodekeeper-portable"));
        assert!(!same_program_name("chrome", "nodekeeper"));
    }
}
