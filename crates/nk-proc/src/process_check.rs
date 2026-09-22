//! Shared "is this PID still a running process" check, used by both the
//! single-instance lock (`lock.rs`) and bitcoind detection (`bitcoind.rs`)
//! for staleness/already-running checks.

use sysinfo::{Pid, System};

pub(crate) fn process_is_alive(pid: u32) -> bool {
    let mut sys = System::new();
    sys.refresh_all();
    sys.process(Pid::from_u32(pid)).is_some()
}
