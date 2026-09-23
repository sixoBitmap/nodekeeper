//! Feeds `nk-exec`'s live `ExecEvent` broadcast stream into
//! `command_history` (docs/SPEC.md item 7: the Live Command Monitor's
//! persisted rolling history). Runs as a long-lived background task,
//! started once at app startup alongside the shared `Executor`.
//!
//! Events arriving here have already been through `nk-exec`'s
//! redaction and sensitive-output withholding (Phase 2) — there is
//! nothing left to redact at this layer; it only needs to shape events
//! into rows.

use crate::Store;
use nk_exec::ExecEvent;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast::error::RecvError;

/// Consumes `events` until the sending `Executor` (and every one of its
/// clones/subscribers) is dropped. A store write failure is logged and
/// skipped rather than propagated — losing one history row must never
/// interrupt command execution, which has already happened by the time
/// its event reaches here.
pub async fn persist_exec_events(
    store: Arc<Mutex<Store>>,
    mut events: tokio::sync::broadcast::Receiver<ExecEvent>,
) {
    loop {
        let event = match events.recv().await {
            Ok(event) => event,
            // A slow consumer missed some events -- carry on with
            // whatever arrives next rather than stalling forever.
            Err(RecvError::Lagged(_)) => continue,
            Err(RecvError::Closed) => return,
        };

        let result = {
            let store = store.lock().expect("store mutex should not be poisoned");
            persist_one(&store, event)
        };
        if let Err(e) = result {
            tracing::warn!("failed to persist command history: {e}");
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
        } => store.record_command_started(
            &id.0.to_string(),
            &environment,
            source.as_str(),
            &triggering_action,
            &command_display,
            now_ms(),
            // Background-polling tagging is wired in a later Phase 3
            // task, once something actually produces background-tagged
            // commands (see PROGRESS.md).
            false,
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
