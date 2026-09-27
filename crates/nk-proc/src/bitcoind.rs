//! Spawns and gracefully stops bitcoind per environment (docs/SPEC.md
//! Foundation C). Long-lived daemon lifecycle is nk-proc's own direct
//! responsibility (hence the `disallowed_methods` exemption on this
//! crate) — distinct from nk-exec, which handles short-lived one-shot
//! commands and RPC calls *against* an already-running node.

use crate::process_check::process_is_alive;
use nk_core::{AppErrorCode, Environment};
use std::net::TcpListener;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BitcoindError {
    #[error("bitcoind is already running on this data directory (pid {pid})")]
    AlreadyRunning { pid: u32 },
    /// Detected *before* spawning (docs/SPEC.md item 8's "port in use"
    /// friendly error): a pre-flight bind check on the RPC/P2P port,
    /// not a parse of bitcoind's own startup failure message, which
    /// would mean spawning it first and racing its stderr against a
    /// string pattern. Checking first is deterministic and testable
    /// without a real bitcoind binary at all.
    #[error("port {port} is already in use")]
    PortInUse { port: u16 },
    #[error("failed to spawn bitcoind: {0}")]
    Spawn(std::io::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("bitcoind did not exit within the timeout after being asked to stop")]
    StopTimeout,
    #[error("bitcoind did not become ready (cookie file + responsive RPC) within the timeout")]
    StartupTimeout,
    #[error("rpc error: {0}")]
    Rpc(#[from] nk_rpc::RpcError),
}

impl BitcoindError {
    /// The shared plain-language error code this failure maps to, if
    /// any (docs/SPEC.md item 8) — `None` for failures with no good
    /// user-facing code, which stay backend-only technical detail.
    pub fn code(&self) -> Option<AppErrorCode> {
        match self {
            Self::PortInUse { .. } => Some(AppErrorCode::PortInUse),
            Self::AlreadyRunning { .. }
            | Self::Spawn(_)
            | Self::Io(_)
            | Self::StopTimeout
            | Self::StartupTimeout
            | Self::Rpc(_) => None,
        }
    }
}

/// How many times the RPC-ready wait in [`BitcoindProcess::wait_ready`] will
/// push its deadline back out because bitcoind answered with warm-up error
/// `-28` (proof it's alive and progressing, not proof it will ever finish) --
/// a hard ceiling so a node that somehow relays `-28` forever still times
/// out eventually rather than waiting without end.
const MAX_WARMUP_EXTENSIONS: u32 = 10;

pub struct BitcoindProcess {
    child: tokio::process::Child,
    pub pid: u32,
    /// When this process was spawned, for the dashboard's uptime display
    /// (docs/SPEC.md item 2: "uptime"). `Instant`, not `SystemTime`: this
    /// value is only ever compared against `Instant::now()` in the same
    /// run, never persisted or shown as a wall-clock time itself.
    pub started_at: std::time::Instant,
}

impl BitcoindProcess {
    /// Starts bitcoind for `environment`. Refuses if one is already
    /// running on the same data directory (docs/SPEC.md Foundation C:
    /// "Never start a second bitcoind on the same data directory").
    ///
    /// Passes `-datadir` and the chain flag only — everything else
    /// (`txindex`, `prune`, RPC binding, `dbcache`, ...) comes from the
    /// generated `bitcoin.conf` already written into that data
    /// directory (`nk_core::bitcoin_conf`), so it isn't duplicated here.
    pub async fn start(
        binary_path: &Path,
        environment: &Environment,
    ) -> Result<Self, BitcoindError> {
        if let Some(pid) = detect_running_bitcoind(environment) {
            return Err(BitcoindError::AlreadyRunning { pid });
        }
        check_port_available(environment.rpc_port)?;
        check_port_available(environment.p2p_port)?;

        let datadir = environment.bitcoin_datadir_arg();
        std::fs::create_dir_all(&datadir)?;

        let mut args = vec![format!("-datadir={}", datadir.display())];
        if let Some(flag) = environment.chain.bitcoin_cli_flag() {
            args.push(flag.to_string());
        }

        let mut command = tokio::process::Command::new(binary_path);
        // stdin is null explicitly, not left to default to "inherit": an
        // inherited stdin duplicates whatever this process's standard
        // handle currently is, which can be a stale value in the release
        // exe after an ord stop (see `console.rs`).
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Windows: no visible console window when Nodekeeper itself has
        // none (the shipped GUI exe); unchanged when it has one. bitcoind
        // is stopped over RPC, so it never needs to share a console.
        crate::console::hide_window_when_console_less(&mut command);
        let child = command.spawn().map_err(BitcoindError::Spawn)?;
        let pid = child.id().expect("a just-spawned child process has a pid");
        // Windows builds of bitcoind have no `-daemon` flag (confirmed in
        // the Phase 0 spike, DECISIONS.md) — nk-proc always runs it as a
        // tracked foreground child on every OS, which is exactly this.
        Ok(Self {
            child,
            pid,
            started_at: std::time::Instant::now(),
        })
    }

    /// `start()` plus waiting for it to actually become usable: bitcoind
    /// writes its cookie file and starts answering RPC shortly *after*
    /// the process exists, not the instant it's spawned, so a caller
    /// that needs a working `RpcClient` right away must poll rather than
    /// assume readiness. Bundles the whole sequence (spawn -> wait for
    /// cookie -> build the RPC client -> wait for it to respond) into
    /// one call so real app code and test fixtures don't each duplicate
    /// it — shared by `nk-testkit`'s `RegtestFixture` and the real app's
    /// dashboard start control.
    ///
    /// `ready_timeout` applies separately to *each* phase (cookie
    /// appearing, then RPC responding), not to their sum — under load
    /// (e.g. several regtest fixtures starting concurrently in CI) the
    /// cookie can legitimately take close to the full budget to appear,
    /// which would otherwise leave the RPC-ready wait starved of time
    /// it needs. A single shared deadline here previously halved the
    /// real-world budget this had before the two phases were
    /// consolidated into one function, and broke CI (see DECISIONS.md).
    ///
    /// The RPC-ready phase is **warm-up aware**: bitcoind answers RPC
    /// warm-up requests with a real response, JSON-RPC error `-28`
    /// ("Loading block index...", "Verifying blocks...", and similar --
    /// Core's own `RPC_IN_WARMUP` code), well before it's ready to serve
    /// real calls. That is proof of life, not a failure to connect at
    /// all -- so seeing it once **extends** the deadline by one more
    /// `ready_timeout` (capped at `MAX_WARMUP_EXTENSIONS` extensions, so
    /// a node stuck relaying `-28` forever still eventually times out).
    /// A plain connection failure (the port isn't listening yet) keeps
    /// the original deadline: found by the mainnet readiness audit
    /// (PROGRESS.md, Phase 10M/10R) -- the flat timeout this replaced
    /// could not tell "still loading a huge block index" from "never
    /// coming up" and was sized for an empty regtest chain either way.
    ///
    /// **Never returns an orphan.** If readiness fails for any reason,
    /// the process this call just spawned is killed (best-effort;
    /// `stop()`'s own graceful RPC path needs a *working* RPC client,
    /// which is exactly what didn't happen) and `bitcoind.pid` is
    /// removed, before the error is returned -- an untracked bitcoind
    /// left running is worse than a failed start: the next `start()`
    /// then refuses with `AlreadyRunning` and nothing in the app can
    /// stop it (found by the same audit).
    #[allow(clippy::too_many_arguments)]
    pub async fn start_and_wait_ready(
        binary_path: &Path,
        environment: &Environment,
        rpc_url: String,
        executor: nk_exec::Executor,
        environment_label: String,
        ready_timeout: Duration,
    ) -> Result<(Self, nk_rpc::RpcClient), BitcoindError> {
        let process = Self::start(binary_path, environment).await?;
        Self::wait_ready_or_clean_up(
            process,
            environment,
            rpc_url,
            executor,
            environment_label,
            ready_timeout,
        )
        .await
    }

    /// `wait_ready`, plus the orphan-cleanup contract documented on
    /// `start_and_wait_ready`. Takes an already-spawned `process` rather
    /// than spawning one itself, so a test can substitute a process it
    /// controls (a long-lived dummy, never meant to become ready) and
    /// assert directly that a failed wait kills it -- spawning a real
    /// bitcoind that is *never* going to become ready isn't something a
    /// test can arrange on demand.
    async fn wait_ready_or_clean_up(
        mut process: Self,
        environment: &Environment,
        rpc_url: String,
        executor: nk_exec::Executor,
        environment_label: String,
        ready_timeout: Duration,
    ) -> Result<(Self, nk_rpc::RpcClient), BitcoindError> {
        match Self::wait_ready(
            environment,
            rpc_url,
            executor,
            environment_label,
            ready_timeout,
        )
        .await
        {
            Ok(rpc) => Ok((process, rpc)),
            Err(e) => {
                process.kill_sync();
                let _ = tokio::time::timeout(Duration::from_secs(5), process.child.wait()).await;
                let _ = std::fs::remove_file(bitcoind_pid_path(environment));
                Err(e)
            }
        }
    }

    /// The waiting half of `start_and_wait_ready`, split out so the
    /// caller can clean up the process on any failure in one place
    /// (below) instead of at every early return.
    async fn wait_ready(
        environment: &Environment,
        rpc_url: String,
        executor: nk_exec::Executor,
        environment_label: String,
        ready_timeout: Duration,
    ) -> Result<nk_rpc::RpcClient, BitcoindError> {
        let cookie_deadline = tokio::time::Instant::now() + ready_timeout;
        let cookie_path = environment.bitcoin_cookie_path();
        while !cookie_path.exists() {
            if tokio::time::Instant::now() >= cookie_deadline {
                return Err(BitcoindError::StartupTimeout);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let rpc = nk_rpc::RpcClient::from_cookie_file(
            rpc_url,
            &cookie_path,
            executor,
            environment_label,
            environment.chain,
        )?;

        let mut rpc_deadline = tokio::time::Instant::now() + ready_timeout;
        let mut warmup_extensions_left = MAX_WARMUP_EXTENSIONS;
        loop {
            // background: true -- this readiness poll can repeat many
            // times (every 200ms) during a slow startup and isn't
            // itself a meaningful user-facing check (docs/SPEC.md item 7).
            //
            // Wrapped in its own timeout: the RPC client has no request
            // timeout of its own (same fact `stop()` already accounts
            // for), so a bitcoind that accepts the connection but whose
            // RPC threads are stuck -- not just slow -- would otherwise
            // hang this whole wait past `rpc_deadline`, which is only
            // ever checked *between* calls. Bounded by `ready_timeout`
            // itself, so one stuck call can cost at most one deadline's
            // worth of time, never the wait's ability to time out at all.
            match tokio::time::timeout(ready_timeout, rpc.get_blockchain_info(true)).await {
                Ok(Ok(_)) => return Ok(rpc),
                Ok(Err(nk_rpc::RpcError::Rpc { code: -28, .. })) if warmup_extensions_left > 0 => {
                    rpc_deadline = tokio::time::Instant::now() + ready_timeout;
                    warmup_extensions_left -= 1;
                }
                Ok(Err(_)) | Err(_) => {}
            }
            if tokio::time::Instant::now() >= rpc_deadline {
                return Err(BitcoindError::StartupTimeout);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Graceful stop (docs/SPEC.md Foundation C): the `stop` RPC, then
    /// wait for the process to actually exit, up to `timeout` (spec
    /// default: 120s, configurable — the caller decides the duration).
    /// The RPC call is allowed to fail (e.g. the node already went away
    /// on its own) — we still wait for the process to exit either way.
    ///
    /// The RPC call is itself bounded by `timeout`: the RPC client has no
    /// request timeout of its own, so a bitcoind that accepts the
    /// connection but never answers (its RPC threads stuck behind a
    /// stalled disk, say -- the portable-drive case) would otherwise hang
    /// this call forever, outside the wait's timeout below. The worst case
    /// is therefore two `timeout`s, not unbounded.
    pub async fn stop(
        mut self,
        rpc: &nk_rpc::RpcClient,
        timeout: Duration,
    ) -> Result<(), BitcoindError> {
        let _ = tokio::time::timeout(timeout, rpc.stop()).await;
        match tokio::time::timeout(timeout, self.child.wait()).await {
            Ok(Ok(_status)) => Ok(()),
            Ok(Err(e)) => Err(BitcoindError::Io(e)),
            Err(_elapsed) => Err(BitcoindError::StopTimeout),
        }
    }

    /// Immediately force-kills the process with no graceful RPC stop.
    /// Only for cleanup paths where graceful shutdown isn't possible
    /// (e.g. a test fixture tearing down after a panic) — prefer
    /// `stop()` for the real, spec-mandated graceful-stop path.
    pub fn kill_sync(&mut self) {
        let _ = self.child.start_kill();
    }
}

fn bitcoind_pid_path(environment: &Environment) -> std::path::PathBuf {
    environment.bitcoin_chain_dir().join("bitcoind.pid")
}

/// Detects an already-running bitcoind on this environment's data
/// directory via its own `bitcoind.pid` file (confirmed live in the
/// Phase 0 spike: plain numeric PID, written to `<chain-dir>/
/// bitcoind.pid`) plus a liveness check — the same pattern the
/// single-instance lock (`lock.rs`) uses for stale-lock detection.
pub fn detect_running_bitcoind(environment: &Environment) -> Option<u32> {
    let pid: u32 = std::fs::read_to_string(bitcoind_pid_path(environment))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    process_is_alive(pid).then_some(pid)
}

/// docs/SPEC.md item 12: "unclean-shutdown recovery with clear
/// guidance if an ord index needs rebuilding." A `bitcoind.pid` file
/// present but pointing to a process that's no longer alive means
/// bitcoind never got the chance to remove it on its way out --
/// confirmed live (`bitcoind_removes_its_own_pid_file_on_a_clean_stop`,
/// this file's tests) that a clean shutdown always deletes this file,
/// so its stale presence specifically signals the *previous* run ended
/// uncleanly (crash, kill, power loss, a portable drive unplugged
/// mid-run) -- not just "never started here before," which leaves no
/// file at all.
pub fn bitcoind_had_unclean_shutdown(environment: &Environment) -> bool {
    let Ok(contents) = std::fs::read_to_string(bitcoind_pid_path(environment)) else {
        return false;
    };
    let Ok(pid) = contents.trim().parse::<u32>() else {
        return false;
    };
    !process_is_alive(pid)
}

/// A pre-flight-only check: binding and immediately dropping a listener
/// tells us the port was free *at that instant*, not that it will still
/// be free by the time bitcoind itself tries to bind it (there's an
/// unavoidable TOCTOU gap either way) — good enough for the spec's
/// "produces the friendly error" bar, not a hard guarantee.
fn check_port_available(port: u16) -> Result<(), BitcoindError> {
    TcpListener::bind(("127.0.0.1", port))
        .map(|_listener| ())
        .map_err(|_| BitcoindError::PortInUse { port })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nk_core::Chain;

    #[test]
    fn no_pid_file_means_nothing_detected() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::new_default(Chain::Regtest, dir.path());
        assert_eq!(detect_running_bitcoind(&env), None);
    }

    #[test]
    fn a_pid_file_for_a_dead_process_is_not_detected_as_running() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::new_default(Chain::Regtest, dir.path());
        let chain_dir = env.bitcoin_chain_dir();
        std::fs::create_dir_all(&chain_dir).unwrap();
        std::fs::write(
            chain_dir.join("bitcoind.pid"),
            definitely_dead_pid().to_string(),
        )
        .unwrap();
        assert_eq!(detect_running_bitcoind(&env), None);
    }

    #[test]
    fn no_pid_file_is_not_an_unclean_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::new_default(Chain::Regtest, dir.path());
        // Never having run here before is a normal, expected state --
        // distinct from a stale file left by a real previous run.
        assert!(!bitcoind_had_unclean_shutdown(&env));
    }

    #[test]
    fn a_pid_file_for_a_live_process_is_not_an_unclean_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::new_default(Chain::Regtest, dir.path());
        let chain_dir = env.bitcoin_chain_dir();
        std::fs::create_dir_all(&chain_dir).unwrap();
        // This test process's own pid is guaranteed alive for the
        // duration of the test.
        std::fs::write(
            chain_dir.join("bitcoind.pid"),
            std::process::id().to_string(),
        )
        .unwrap();
        assert!(!bitcoind_had_unclean_shutdown(&env));
    }

    #[test]
    fn a_pid_file_for_a_dead_process_is_an_unclean_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::new_default(Chain::Regtest, dir.path());
        let chain_dir = env.bitcoin_chain_dir();
        std::fs::create_dir_all(&chain_dir).unwrap();
        std::fs::write(
            chain_dir.join("bitcoind.pid"),
            definitely_dead_pid().to_string(),
        )
        .unwrap();
        assert!(bitcoind_had_unclean_shutdown(&env));
    }

    #[test]
    fn port_in_use_maps_to_the_shared_error_code() {
        let err = BitcoindError::PortInUse { port: 8332 };
        assert_eq!(err.code(), Some(AppErrorCode::PortInUse));
    }

    #[test]
    fn other_errors_have_no_shared_code() {
        assert_eq!(BitcoindError::AlreadyRunning { pid: 1 }.code(), None);
        assert_eq!(BitcoindError::StopTimeout.code(), None);
    }

    /// The Phase 3 [CI] acceptance criterion ("a busy port produces the
    /// friendly error"): occupy the RPC port first, then attempt to
    /// start bitcoind -- the pre-flight check must catch this *before*
    /// ever trying to spawn a process, so a nonexistent binary path
    /// still produces `PortInUse`, not a spawn failure.
    #[tokio::test]
    async fn starting_with_a_busy_rpc_port_fails_with_the_friendly_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut env = Environment::new_default(Chain::Regtest, dir.path());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        env.rpc_port = listener.local_addr().unwrap().port();

        let result = BitcoindProcess::start(Path::new("this-binary-does-not-exist"), &env).await;

        assert!(matches!(
            result,
            Err(BitcoindError::PortInUse { port }) if port == env.rpc_port
        ));
        drop(listener);
    }

    fn random_free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    /// docs/SPEC.md item 12's "unclean-shutdown recovery with clear
    /// guidance if an ord index needs rebuilding" -- before building any
    /// detection logic on top of a stale `bitcoind.pid`, confirm live
    /// (not assumed) that bitcoind actually *removes* its own pid file
    /// on a clean exit. If it didn't, a leftover file would mean
    /// nothing, and "stale pid file present" couldn't be used as an
    /// unclean-shutdown signal at all.
    #[tokio::test]
    #[serial_test::serial(real_bitcoind)]
    async fn bitcoind_removes_its_own_pid_file_on_a_clean_stop() {
        let Some(binary_path) = nk_core::live_tests::live_binary("NK_TEST_BITCOIND") else {
            eprintln!("skipping: NK_TEST_BITCOIND not set");
            return;
        };
        let binary_path = std::path::PathBuf::from(binary_path);

        let dir = tempfile::tempdir().unwrap();
        let mut environment = Environment::new_default(Chain::Regtest, dir.path());
        environment.rpc_port = random_free_port();
        environment.p2p_port = random_free_port();

        // Matches `NodeManager::start`'s real recipe exactly -- without
        // a written `bitcoin.conf` (`server=1`, the RPC port, ...),
        // bitcoind starts with defaults that never open the RPC port
        // this test expects, which is exactly what a first pass of
        // this test got wrong (timed out waiting for readiness that
        // was never coming).
        let datadir = environment.bitcoin_datadir_arg();
        std::fs::create_dir_all(&datadir).unwrap();
        let conf = nk_core::bitcoin_conf::generate_bitcoin_conf(
            Chain::Regtest,
            environment.rpc_port,
            environment.p2p_port,
            4 * 1024 * 1024 * 1024,
            0,
        );
        std::fs::write(datadir.join("bitcoin.conf"), conf).unwrap();

        let rpc_url = format!("http://127.0.0.1:{}", environment.rpc_port);
        let (process, rpc) = BitcoindProcess::start_and_wait_ready(
            &binary_path,
            &environment,
            rpc_url,
            nk_exec::Executor::new(),
            "test".to_string(),
            // 60s, matching `NodeManager::start`'s own real-world
            // margin (DECISIONS.md) -- not just this test's original
            // 30s, which this dev machine's own load already showed
            // could be tight.
            Duration::from_secs(60),
        )
        .await
        .expect("bitcoind should start");

        let pid_path = environment.bitcoin_chain_dir().join("bitcoind.pid");
        assert!(
            pid_path.is_file(),
            "bitcoind should have written its own pid file on startup"
        );

        process
            .stop(&rpc, Duration::from_secs(30))
            .await
            .expect("bitcoind should stop cleanly");

        assert!(
            !pid_path.exists(),
            "bitcoind should remove its own pid file on a clean shutdown -- if this \
             assertion fails, a leftover bitcoind.pid can no longer be trusted as an \
             unclean-shutdown signal and any detection built on that assumption is wrong"
        );
    }

    /// The stop RPC must not be able to hang `stop` forever: the RPC
    /// client has no request timeout, and a bitcoind whose RPC threads
    /// are stuck (a stalled external drive) accepts connections but never
    /// answers. Here a listener does exactly that, and the "process" is a
    /// long-lived throwaway that never exits on its own, so `stop` has to
    /// give up on the RPC *and* on the wait -- after about two timeouts --
    /// and report `StopTimeout`, instead of blocking the caller (and,
    /// through it, `NodeManager::stop_everything`'s single-flight gate).
    #[tokio::test]
    async fn a_stop_rpc_that_never_answers_cannot_hang_the_stop_forever() {
        // Accepts connections and keeps them open without ever replying.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let held = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let held_by_thread = held.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                held_by_thread.lock().unwrap().push(stream);
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let cookie = dir.path().join(".cookie");
        std::fs::write(&cookie, "__cookie__:hunter2").unwrap();
        let rpc = nk_rpc::RpcClient::from_cookie_file(
            format!("http://127.0.0.1:{port}"),
            &cookie,
            nk_exec::Executor::new(),
            "regtest".to_string(),
            Chain::Regtest,
        )
        .unwrap();

        // `kill_on_drop`: the stop gives up on this process and drops it.
        let mut command =
            tokio::process::Command::new(if cfg!(windows) { "ping" } else { "sleep" });
        if cfg!(windows) {
            command.args(["-n", "30", "127.0.0.1"]);
        } else {
            command.arg("30");
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let child = command
            .spawn()
            .expect("failed to spawn a long-lived process");
        let pid = child.id().expect("a just-spawned child has a pid");
        let process = BitcoindProcess {
            child,
            pid,
            started_at: std::time::Instant::now(),
        };

        let started = std::time::Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            process.stop(&rpc, Duration::from_millis(300)),
        )
        .await
        .expect("stop must give up by itself, not hang on the RPC");

        assert!(
            matches!(result, Err(BitcoindError::StopTimeout)),
            "{result:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );
    }

    /// A long-lived dummy process standing in for a `bitcoind` that is
    /// never going to become ready -- deliberately *not* `kill_on_drop`,
    /// so the only thing that can end it is `wait_ready_or_clean_up`'s
    /// own cleanup, never Rust dropping the `Child` handle. Mirrors the
    /// dummy process the stop-timeout test above already uses.
    fn spawn_long_lived_dummy() -> tokio::process::Child {
        let mut command =
            tokio::process::Command::new(if cfg!(windows) { "ping" } else { "sleep" });
        if cfg!(windows) {
            command.args(["-n", "30", "127.0.0.1"]);
        } else {
            command.arg("30");
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
            .spawn()
            .expect("failed to spawn a long-lived process")
    }

    /// The orphan bug the mainnet readiness audit found (PROGRESS.md,
    /// Phase 10M/10R): a bitcoind that never becomes ready used to be
    /// dropped, still running, the moment the wait gave up -- the next
    /// `start()` would then refuse with `AlreadyRunning` against a
    /// process nothing could stop. A dummy process standing in for that
    /// stuck bitcoind (real cookie file, so the cookie phase passes at
    /// once; an RPC port nothing listens on, so every poll fails and the
    /// wait times out) proves the fix: the dummy is dead once
    /// `wait_ready_or_clean_up` returns its error, and `bitcoind.pid` is
    /// gone too.
    #[tokio::test]
    async fn a_process_that_never_becomes_ready_is_killed_not_orphaned() {
        let child = spawn_long_lived_dummy();
        let pid = child.id().expect("a just-spawned child has a pid");
        let process = BitcoindProcess {
            child,
            pid,
            started_at: std::time::Instant::now(),
        };

        let dir = tempfile::tempdir().unwrap();
        let environment = Environment::new_default(Chain::Regtest, dir.path());
        std::fs::create_dir_all(environment.bitcoin_chain_dir()).unwrap();
        std::fs::write(environment.bitcoin_cookie_path(), "__cookie__:hunter2").unwrap();
        std::fs::write(bitcoind_pid_path(&environment), pid.to_string()).unwrap();

        assert!(
            process_is_alive(pid),
            "the dummy should be alive to start with"
        );

        let unused_port = random_free_port();
        let result = BitcoindProcess::wait_ready_or_clean_up(
            process,
            &environment,
            format!("http://127.0.0.1:{unused_port}"),
            nk_exec::Executor::new(),
            "test".to_string(),
            Duration::from_millis(300),
        )
        .await;

        assert!(
            matches!(result, Err(BitcoindError::StartupTimeout)),
            "{:?}",
            result.err()
        );
        assert!(
            !process_is_alive(pid),
            "a process that never became ready must be killed, not left running"
        );
        assert!(
            !bitcoind_pid_path(&environment).exists(),
            "the pid file must not survive pointing at a killed process"
        );
    }

    /// A minimal JSON-RPC server (the same shape `nk-rpc`'s own stub-node
    /// tests use): answers a fixed sequence of responses in order, one per
    /// connection, then `rpc_error(-32601, ...)` forever -- enough to drive
    /// the warm-up-extension logic in `wait_ready` without a real bitcoind
    /// (which, on regtest, never actually stays in warm-up long enough to
    /// test this against). Each response closes the connection, since
    /// `reqwest` may otherwise try to reuse it for the next call.
    fn spawn_scripted_rpc_server(responses: Vec<String>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut responses = responses.into_iter();
            for mut stream in listener.incoming().flatten() {
                use std::io::{Read, Write};
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let body = responses
                    .next()
                    .unwrap_or_else(|| rpc_error(-32601, "no more scripted responses"));
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                     Connection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        port
    }

    fn rpc_ok(result: &str) -> String {
        format!(r#"{{"result":{result},"error":null,"id":"nodekeeper"}}"#)
    }

    fn rpc_error(code: i64, message: &str) -> String {
        format!(
            r#"{{"result":null,"error":{{"code":{code},"message":"{message}"}},"id":"nodekeeper"}}"#
        )
    }

    /// Sets up a fresh environment with a cookie file already in place
    /// (so `wait_ready`'s cookie-wait phase passes at once) and returns it.
    fn environment_ready_for_rpc_wait(dir: &std::path::Path) -> Environment {
        let environment = Environment::new_default(Chain::Regtest, dir);
        std::fs::create_dir_all(environment.bitcoin_chain_dir()).unwrap();
        std::fs::write(environment.bitcoin_cookie_path(), "__cookie__:hunter2").unwrap();
        environment
    }

    /// `-28` ("Loading block index...", Core's own `RPC_IN_WARMUP` code)
    /// is proof of life, so it must extend the deadline rather than just
    /// being "another failed poll" -- a node that answers warm-up errors
    /// the whole time it's loading a huge block index must not be timed
    /// out from under itself just because that takes longer than one
    /// `ready_timeout`. Drives the real `wait_ready`, not a copy of its
    /// logic.
    #[tokio::test]
    async fn a_warmup_response_extends_the_deadline_until_the_node_is_really_ready() {
        let dir = tempfile::tempdir().unwrap();
        let environment = environment_ready_for_rpc_wait(dir.path());
        // Two warm-up responses, then real readiness -- comfortably past
        // where a *first* ready_timeout without the extension would have
        // given up (each -28 pushes the deadline out by another full
        // ready_timeout).
        let port = spawn_scripted_rpc_server(vec![
            rpc_error(-28, "Loading block index..."),
            rpc_error(-28, "Verifying blocks..."),
            rpc_ok(r#"{"blocks":0}"#),
        ]);

        let result = tokio::time::timeout(
            Duration::from_secs(10),
            BitcoindProcess::wait_ready(
                &environment,
                format!("http://127.0.0.1:{port}"),
                nk_exec::Executor::new(),
                "test".to_string(),
                Duration::from_millis(150),
            ),
        )
        .await
        .expect("the test's own outer timeout should never be the one that fires");
        assert!(
            result.is_ok(),
            "warm-up responses should not end the wait early: {:?}",
            result.err()
        );
    }

    /// The RPC client has no request timeout of its own (the same fact
    /// `stop()`'s own tests already exercise). A connection that is
    /// accepted but never answered must not be able to hang the whole
    /// ready wait past its deadline -- only the per-call `tokio::time::
    /// timeout` around `rpc.get_blockchain_info` stands between "one
    /// stuck call" and "wait forever", since the deadline itself is only
    /// ever checked *between* calls.
    #[tokio::test]
    async fn a_connection_that_never_answers_cannot_hang_the_ready_wait_forever() {
        let dir = tempfile::tempdir().unwrap();
        let environment = environment_ready_for_rpc_wait(dir.path());
        // Accepts connections and holds them open without ever replying --
        // same shape as the stop-timeout test's "never answers" listener.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let held = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let held_by_thread = held.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                held_by_thread.lock().unwrap().push(stream);
            }
        });

        let started = std::time::Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            BitcoindProcess::wait_ready(
                &environment,
                format!("http://127.0.0.1:{port}"),
                nk_exec::Executor::new(),
                "test".to_string(),
                Duration::from_millis(200),
            ),
        )
        .await
        .expect("a stuck connection must not hang the wait past the test's own outer timeout");
        assert!(
            matches!(result, Err(BitcoindError::StartupTimeout)),
            "{:?}",
            result.err()
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );
        drop(held);
    }

    /// The extension has a ceiling: a node that relays `-28` forever
    /// (or a genuinely broken one that never leaves warm-up) must still
    /// time out eventually, not wait without end.
    #[tokio::test]
    async fn a_node_stuck_in_warmup_forever_still_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let environment = environment_ready_for_rpc_wait(dir.path());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                use std::io::{Read, Write};
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let body = rpc_error(-28, "Loading block index...");
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                     Connection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        let result = tokio::time::timeout(
            // MAX_WARMUP_EXTENSIONS + 1 rounds of a 30ms ready_timeout,
            // generously bounded -- if this fires instead, the ceiling
            // is missing, not just slow.
            Duration::from_secs(10),
            BitcoindProcess::wait_ready(
                &environment,
                format!("http://127.0.0.1:{port}"),
                nk_exec::Executor::new(),
                "test".to_string(),
                Duration::from_millis(30),
            ),
        )
        .await
        .expect("a node stuck in warm-up forever must still time out on its own");
        assert!(
            matches!(result, Err(BitcoindError::StartupTimeout)),
            "{:?}",
            result.err()
        );
    }

    /// A PID that's real enough to have existed a moment ago but is
    /// guaranteed not to be running anymore -- see the identical helper
    /// (and its rationale) in `lock.rs`'s tests.
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
