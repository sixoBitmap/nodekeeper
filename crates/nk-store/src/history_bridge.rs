//! Feeds `nk-exec`'s live `ExecEvent` broadcast stream into
//! `command_history` (docs/SPEC.md item 7: the Live Command Monitor's
//! persisted rolling history). Runs as a long-lived background task,
//! started once at app startup alongside the shared `Executor`.
//!
//! Events arriving here have already been through `nk-exec`'s
//! redaction and sensitive-output withholding (Phase 2) — there is
//! nothing left to redact at this layer; it only needs to shape events
//! into rows.

use crate::{Store, COMMAND_HISTORY_KEEP_PER_ENVIRONMENT};
use nk_exec::ExecEvent;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast::error::RecvError;

/// How many rows to keep per environment, and how many new commands in an
/// environment to record between prunes of it. Pruning after *every*
/// command would be a `DELETE` per command -- and background polling
/// records a lot of them -- for a limit that only has to hold *about*
/// (docs/SPEC.md item 7: "e.g. last 5,000"): the table can overshoot by
/// at most `prune_every` rows per environment between prunes.
#[derive(Debug, Clone, Copy)]
struct Retention {
    keep: u32,
    prune_every: u32,
}

const DEFAULT_RETENTION: Retention = Retention {
    keep: COMMAND_HISTORY_KEEP_PER_ENVIRONMENT,
    prune_every: 100,
};

/// Consumes `events` until the sending `Executor` (and every one of its
/// clones/subscribers) is dropped. A store write failure is logged and
/// skipped rather than propagated — losing one history row must never
/// interrupt command execution, which has already happened by the time
/// its event reaches here.
pub async fn persist_exec_events(
    store: Arc<Mutex<Store>>,
    events: tokio::sync::broadcast::Receiver<ExecEvent>,
) {
    persist_exec_events_with(store, events, DEFAULT_RETENTION).await
}

async fn persist_exec_events_with(
    store: Arc<Mutex<Store>>,
    mut events: tokio::sync::broadcast::Receiver<ExecEvent>,
    retention: Retention,
) {
    // New rows recorded per environment since it was last pruned.
    let mut since_prune: HashMap<String, u32> = HashMap::new();
    loop {
        let event = match events.recv().await {
            Ok(event) => event,
            // A slow consumer missed some events -- carry on with
            // whatever arrives next rather than stalling forever.
            Err(RecvError::Lagged(_)) => continue,
            Err(RecvError::Closed) => return,
        };

        // Only a command *starting* adds a row (its output and finish
        // update it), so that is what counts towards the next prune.
        let started_in = match &event {
            ExecEvent::Started { environment, .. } => Some(environment.clone()),
            _ => None,
        };

        let result = {
            let store = store.lock().expect("store mutex should not be poisoned");
            persist_one(&store, event)
        };
        if let Err(e) = result {
            tracing::warn!("failed to persist command history: {e}");
            continue;
        }

        if let Some(environment) = started_in {
            let recorded = since_prune.entry(environment.clone()).or_insert(0);
            *recorded += 1;
            if *recorded >= retention.prune_every {
                *recorded = 0;
                let pruned = {
                    let store = store.lock().expect("store mutex should not be poisoned");
                    store.prune_command_history(&environment, retention.keep)
                };
                // Like a failed write above: losing a prune must never
                // interrupt anything; the next one will catch up.
                if let Err(e) = pruned {
                    tracing::warn!("failed to prune command history: {e}");
                }
            }
        }
    }
}

fn persist_one(store: &Store, event: ExecEvent) -> Result<(), crate::StoreError> {
    match event {
        ExecEvent::Started {
            id,
            environment,
            source,
            triggering_action,
            command_display,
            background,
        } => store.record_command_started(
            &id.0.to_string(),
            &environment,
            source.as_str(),
            &triggering_action,
            &command_display,
            now_ms(),
            background,
        ),
        ExecEvent::Output { id, chunk, .. } => {
            store.append_command_output(&id.0.to_string(), &chunk)
        }
        ExecEvent::Finished {
            id,
            exit_code,
            duration_ms,
        } => {
            // `duration_ms` is a `u64` on the wire (Tauri IPC's
            // u64-as-number convention, see ARCHITECTURE.md); SQLite has
            // no unsigned type, so it's stored as `i64` here -- fine up
            // to ~292 million years of milliseconds.
            store.record_command_finished(&id.0.to_string(), exit_code, duration_ms as i64)
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nk_exec::{CommandSource, CommandSpec, Executor, Sensitivity};
    use std::time::Duration;
    use tokio::time::timeout;

    fn shell_spec(one_liner: &str) -> CommandSpec {
        #[cfg(windows)]
        let (program, shell_args) = ("cmd", vec!["/C".to_string()]);
        #[cfg(not(windows))]
        let (program, shell_args) = ("sh", vec!["-c".to_string()]);
        let mut args = shell_args;
        args.push(one_liner.to_string());
        CommandSpec {
            program: program.to_string(),
            args,
            stdin: None,
            environment: "regtest".to_string(),
            source: CommandSource::BitcoinCli,
            triggering_action: "test".to_string(),
            sensitivity: Sensitivity::Normal,
            redact: vec![],
            background: false,
            env_vars: vec![],
        }
    }

    /// Polls `list_command_history` until `condition` holds or the
    /// timeout elapses -- the bridge task runs concurrently, so there's
    /// no synchronous point at which "it must have processed the event
    /// by now" is guaranteed.
    async fn wait_until(
        store: &Arc<Mutex<Store>>,
        condition: impl Fn(&[crate::CommandHistoryEntry]) -> bool,
    ) -> Vec<crate::CommandHistoryEntry> {
        timeout(Duration::from_secs(5), async {
            loop {
                let entries = store
                    .lock()
                    .unwrap()
                    .list_command_history(None, 10)
                    .unwrap();
                if condition(&entries) {
                    return entries;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("condition should become true before the timeout")
    }

    #[tokio::test]
    async fn a_full_command_lifecycle_is_persisted() {
        let executor = Executor::new();
        let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
        let bridge = tokio::spawn(persist_exec_events(store.clone(), executor.subscribe()));

        executor
            .execute(shell_spec("echo hello from the bridge"))
            .await
            .unwrap();

        let entries = wait_until(&store, |entries| {
            entries
                .first()
                .is_some_and(|e| e.status != crate::CommandHistoryStatus::Running)
        })
        .await;

        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.environment, "regtest");
        assert_eq!(entry.source, "bitcoincli");
        assert_eq!(entry.status, crate::CommandHistoryStatus::Success);
        assert_eq!(entry.exit_code, Some(0));
        assert!(entry.output.contains("hello from the bridge"));

        bridge.abort();
    }

    #[tokio::test]
    async fn a_sensitive_commands_real_output_never_reaches_history() {
        let executor = Executor::new();
        let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
        let bridge = tokio::spawn(persist_exec_events(store.clone(), executor.subscribe()));

        let mut spec = shell_spec("echo mnemonic abandon ability able about");
        spec.sensitivity = Sensitivity::Sensitive;
        executor.execute(spec).await.unwrap();

        let entries = wait_until(&store, |entries| {
            entries
                .first()
                .is_some_and(|e| e.status != crate::CommandHistoryStatus::Running)
        })
        .await;

        assert!(!entries[0].output.contains("abandon"));
        assert!(entries[0]
            .output
            .contains(nk_exec::SENSITIVE_OUTPUT_PLACEHOLDER));

        bridge.abort();
    }

    #[tokio::test]
    async fn the_background_flag_is_persisted_from_the_real_event() {
        let executor = Executor::new();
        let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
        let bridge = tokio::spawn(persist_exec_events(store.clone(), executor.subscribe()));

        let mut spec = shell_spec("echo polling");
        spec.background = true;
        executor.execute(spec).await.unwrap();

        let entries = wait_until(&store, |entries| {
            entries
                .first()
                .is_some_and(|e| e.status != crate::CommandHistoryStatus::Running)
        })
        .await;

        assert!(entries[0].background);
        bridge.abort();
    }

    /// The shipped limits are the spec's: last 5,000 per environment
    /// (docs/SPEC.md item 7), pruned in batches of 100.
    #[test]
    fn the_default_retention_is_the_specs() {
        assert_eq!(DEFAULT_RETENTION.keep, 5_000);
        assert_eq!(DEFAULT_RETENTION.prune_every, 100);
    }

    /// Runs `count` one-liners in `environment`, one at a time (so their
    /// timestamps differ), and waits until the bridge has fully processed
    /// the last -- through its `Finished` event, which the bridge handles
    /// *after* the `Started` event that may trigger a prune, so everything
    /// that should have happened by now has.
    async fn run_and_settle(
        executor: &Executor,
        store: &Arc<Mutex<Store>>,
        environment: &str,
        label: &str,
        range: std::ops::Range<u32>,
    ) {
        for n in range.clone() {
            let mut spec = shell_spec(&format!("echo {label} {n}"));
            spec.environment = environment.to_string();
            executor.execute(spec).await.unwrap();
        }
        let last = format!("{label} {}", range.end - 1);
        timeout(Duration::from_secs(10), async {
            loop {
                let rows = store
                    .lock()
                    .unwrap()
                    .list_command_history(Some(environment), 100)
                    .unwrap();
                if rows.first().is_some_and(|newest| {
                    newest.command_display.ends_with(&last)
                        && newest.status != crate::CommandHistoryStatus::Running
                }) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the bridge should process every event");
    }

    fn kept(store: &Arc<Mutex<Store>>, environment: &str) -> Vec<String> {
        store
            .lock()
            .unwrap()
            .list_command_history(Some(environment), 100)
            .unwrap()
            .iter()
            .map(|row| row.command_display.rsplit(' ').next().unwrap().to_string())
            .collect()
    }

    /// docs/SPEC.md item 7's bounded rolling history, end to end through
    /// the real event stream, with a small limit (keep 2, prune every 5):
    ///
    /// - **throttled**: after four commands nothing has been pruned yet --
    ///   pruning after *every* command (the per-command `DELETE` this
    ///   batching exists to avoid) would already have cut it to two;
    /// - **bounded**: after the fifth, and again after the tenth, it is cut
    ///   back to the newest rows, newest first;
    /// - **per environment**: another environment that has not reached its
    ///   own interval is never touched.
    #[tokio::test]
    async fn history_is_pruned_in_batches_to_the_newest_rows_per_environment() {
        let executor = Executor::new();
        let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
        let retention = Retention {
            keep: 2,
            prune_every: 5,
        };
        let bridge = tokio::spawn(persist_exec_events_with(
            store.clone(),
            executor.subscribe(),
            retention,
        ));

        run_and_settle(&executor, &store, "mainnet", "mainnet", 0..2).await;
        run_and_settle(&executor, &store, "regtest", "regtest", 0..4).await;
        assert_eq!(
            kept(&store, "regtest"),
            ["3", "2", "1", "0"],
            "below the interval: not pruned yet"
        );

        run_and_settle(&executor, &store, "regtest", "regtest", 4..5).await;
        assert_eq!(kept(&store, "regtest"), ["4", "3"], "pruned at the fifth");

        run_and_settle(&executor, &store, "regtest", "regtest", 5..9).await;
        assert_eq!(
            kept(&store, "regtest"),
            ["8", "7", "6", "5", "4", "3"],
            "growing again between prunes"
        );

        run_and_settle(&executor, &store, "regtest", "regtest", 9..10).await;
        assert_eq!(kept(&store, "regtest"), ["9", "8"], "pruned at the tenth");

        assert_eq!(
            kept(&store, "mainnet").len(),
            2,
            "another environment is never touched"
        );
        bridge.abort();
    }

    #[tokio::test]
    async fn the_bridge_task_exits_once_the_executor_is_dropped() {
        let executor = Executor::new();
        let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
        let bridge = tokio::spawn(persist_exec_events(store, executor.subscribe()));

        drop(executor);

        timeout(Duration::from_secs(5), bridge)
            .await
            .expect("bridge task should exit promptly")
            .expect("bridge task should not panic");
    }
}
