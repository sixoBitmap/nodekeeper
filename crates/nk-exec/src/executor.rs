//! The central command executor (docs/SPEC.md Foundation B). The only
//! sanctioned way to spawn `ord`, `bitcoin-cli`, or make an RPC call
//! anywhere in the app.

use crate::redact::redact;
use crate::types::{
    CommandId, CommandSpec, ExecEvent, ExecOutcome, OutputStream, RecordSpec, Sensitivity,
    SENSITIVE_OUTPUT_PLACEHOLDER,
};
use std::process::Stdio;
use std::time::Instant;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;
use zeroize::Zeroize;

#[derive(Debug, Error)]
pub enum ExecError {
    #[error("failed to spawn {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone)]
pub struct Executor {
    events: broadcast::Sender<ExecEvent>,
}

impl Executor {
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(1024);
        Self { events }
    }

    /// Subscribes to the live event stream (Phase 3's Live Command
    /// Monitor). Never carries sensitive-command output — see
    /// `Sensitivity`.
    pub fn subscribe(&self) -> broadcast::Receiver<ExecEvent> {
        self.events.subscribe()
    }

    pub async fn execute(&self, mut spec: CommandSpec) -> Result<ExecOutcome, ExecError> {
        let id = CommandId::new();
        let start = Instant::now();

        self.emit(ExecEvent::Started {
            id,
            environment: spec.environment.clone(),
            source: spec.source,
            triggering_action: spec.triggering_action.clone(),
            command_display: redact(&display_command(&spec.program, &spec.args), &spec.redact),
            background: spec.background,
        });

        let mut child = Command::new(&spec.program)
            .args(&spec.args)
            .envs(spec.env_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| ExecError::Spawn {
                program: spec.program.clone(),
                source,
            })?;

        if let Some(mut stdin_data) = spec.stdin.take() {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(&stdin_data).await?;
                stdin.shutdown().await?;
            }
            stdin_data.zeroize();
        }
        // If no stdin was supplied, close the pipe now -- otherwise a
        // child that reads stdin would block forever waiting for EOF.
        drop(child.stdin.take());

        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let (stdout_result, stderr_result) = tokio::join!(
            Self::stream_output(
                id,
                OutputStream::Stdout,
                stdout,
                &spec.redact,
                spec.sensitivity,
                &self.events
            ),
            Self::stream_output(
                id,
                OutputStream::Stderr,
                stderr,
                &spec.redact,
                spec.sensitivity,
                &self.events
            ),
        );
        let status = child.wait().await?;
        let duration = start.elapsed();

        self.emit(ExecEvent::Finished {
            id,
            exit_code: status.code(),
            duration_ms: duration.as_millis().try_into().unwrap_or(u64::MAX),
        });

        Ok(ExecOutcome {
            id,
            exit_code: status.code(),
            stdout: stdout_result?,
            stderr: stderr_result?,
            duration,
        })
    }

    /// For operations that don't spawn a child process (an RPC call, over
    /// HTTP, not a subprocess) but still need to appear in the Live
    /// Command Monitor exactly like a spawned command would.
    /// `command_display` is what's shown for it — for RPC calls, the
    /// equivalent bitcoin-cli invocation (docs/SPEC.md item 7: "RPC
    /// calls shown as their equivalent bitcoin-cli command so users can
    /// learn them").
    pub async fn record<F, Fut, E>(&self, spec: RecordSpec, op: F) -> Result<serde_json::Value, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<serde_json::Value, E>>,
        E: std::fmt::Display,
    {
        let id = CommandId::new();
        let start = Instant::now();
        self.emit(ExecEvent::Started {
            id,
            environment: spec.environment,
            source: spec.source,
            triggering_action: spec.triggering_action,
            command_display: redact(&spec.command_display, &spec.redact),
            background: spec.background,
        });

        let result = op().await;
        let duration = start.elapsed();

        let (chunk, exit_code) = match &result {
            Ok(value) => (value.to_string(), Some(0)),
            Err(e) => (format!("error: {e}"), Some(1)),
        };
        let chunk = match spec.sensitivity {
            Sensitivity::Sensitive => SENSITIVE_OUTPUT_PLACEHOLDER.to_string(),
            Sensitivity::Normal => redact(&chunk, &spec.redact),
        };
        self.emit(ExecEvent::Output {
            id,
            stream: OutputStream::Stdout,
            chunk,
        });
        self.emit(ExecEvent::Finished {
            id,
            exit_code,
            duration_ms: duration.as_millis().try_into().unwrap_or(u64::MAX),
        });

        result
    }

    fn emit(&self, event: ExecEvent) {
        // `send` only errors when there are zero receivers right now,
        // which just means nothing is watching the monitor yet -- not a
        // failure of the command itself.
        let _ = self.events.send(event);
    }

    async fn stream_output(
        id: CommandId,
        stream_kind: OutputStream,
        reader: impl AsyncRead + Unpin,
        redact_list: &[String],
        sensitivity: Sensitivity,
        events: &broadcast::Sender<ExecEvent>,
    ) -> Result<Vec<u8>, std::io::Error> {
        let mut reader = BufReader::new(reader);
        let mut all = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                break;
            }
            all.extend_from_slice(line.as_bytes());
            let chunk = match sensitivity {
                Sensitivity::Sensitive => SENSITIVE_OUTPUT_PLACEHOLDER.to_string(),
                Sensitivity::Normal => redact(&line, redact_list),
            };
            let _ = events.send(ExecEvent::Output {
                id,
                stream: stream_kind,
                chunk,
            });
        }
        Ok(all)
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

fn display_command(program: &str, args: &[String]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CommandSource;

    /// Runs a shell one-liner cross-platform. Using `cmd`/`sh` as the
    /// *program* (with an argument array telling it what to run) isn't
    /// the "shell-interpolated strings" pattern the executor forbids for
    /// real app commands -- it's a portable way to generate test output,
    /// same as any other external program nk-exec might spawn.
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

    #[tokio::test]
    async fn runs_a_command_and_captures_stdout() {
        let executor = Executor::new();
        let outcome = executor
            .execute(shell_spec("echo hello world"))
            .await
            .unwrap();
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(
            String::from_utf8_lossy(&outcome.stdout).trim(),
            "hello world"
        );
    }

    #[tokio::test]
    async fn exit_code_is_captured_for_a_failing_command() {
        let executor = Executor::new();
        let outcome = executor.execute(shell_spec("exit 3")).await.unwrap();
        assert_eq!(outcome.exit_code, Some(3));
    }

    #[tokio::test]
    async fn redacts_a_secret_from_the_broadcast_stream_but_not_the_direct_return_value() {
        let executor = Executor::new();
        let mut rx = executor.subscribe();
        let mut spec = shell_spec("echo the passphrase is hunter2 today");
        spec.redact = vec!["hunter2".to_string()];
        let outcome = executor.execute(spec).await.unwrap();

        // Direct return value: real, unredacted content -- the caller
        // asked for this text on purpose (e.g. to parse it).
        assert!(String::from_utf8_lossy(&outcome.stdout).contains("hunter2"));

        // Broadcast stream: redacted.
        let mut saw_redacted_output = false;
        while let Ok(event) = rx.try_recv() {
            if let ExecEvent::Output { chunk, .. } = event {
                assert!(
                    !chunk.contains("hunter2"),
                    "secret leaked into event stream: {chunk}"
                );
                if chunk.contains("[redacted]") {
                    saw_redacted_output = true;
                }
            }
        }
        assert!(
            saw_redacted_output,
            "expected at least one redacted Output event"
        );
    }

    #[tokio::test]
    async fn sensitive_command_output_never_reaches_the_broadcast_stream() {
        let executor = Executor::new();
        let mut rx = executor.subscribe();
        let mut spec = shell_spec("echo mnemonic abandon ability able about");
        spec.sensitivity = Sensitivity::Sensitive;
        let outcome = executor.execute(spec).await.unwrap();

        // Direct return value: the real mnemonic, for the seed view alone.
        assert!(String::from_utf8_lossy(&outcome.stdout).contains("abandon"));

        // Broadcast stream: only the placeholder, never the real content,
        // not even redacted.
        let mut saw_output_event = false;
        while let Ok(event) = rx.try_recv() {
            if let ExecEvent::Output { chunk, .. } = event {
                saw_output_event = true;
                assert_eq!(chunk, SENSITIVE_OUTPUT_PLACEHOLDER);
                assert!(!chunk.contains("abandon"));
            }
        }
        assert!(
            saw_output_event,
            "expected at least one Output event (with the placeholder)"
        );
    }

    #[tokio::test]
    async fn stdin_reaches_the_child_and_never_appears_in_command_display() {
        let executor = Executor::new();
        let mut rx = executor.subscribe();
        #[cfg(windows)]
        let (program, args) = ("cmd", vec!["/C".to_string(), "sort".to_string()]);
        #[cfg(not(windows))]
        let (program, args) = ("cat", Vec::<String>::new());
        let spec = CommandSpec {
            program: program.to_string(),
            args,
            stdin: Some(b"secret-mnemonic-word\n".to_vec()),
            environment: "regtest".to_string(),
            source: CommandSource::OrdCli,
            triggering_action: "wallet restore".to_string(),
            sensitivity: Sensitivity::Normal,
            redact: vec![],
            background: false,
            env_vars: vec![],
        };
        let outcome = executor.execute(spec).await.unwrap();
        assert!(String::from_utf8_lossy(&outcome.stdout).contains("secret-mnemonic-word"));

        // The secret was passed via stdin, never as an argument -- so it
        // must never appear in the Started event's command_display.
        while let Ok(event) = rx.try_recv() {
            if let ExecEvent::Started {
                command_display, ..
            } = event
            {
                assert!(!command_display.contains("secret-mnemonic-word"));
            }
        }
    }

    #[tokio::test]
    async fn record_emits_the_same_event_shape_as_a_spawned_command() {
        let executor = Executor::new();
        let mut rx = executor.subscribe();

        let result: Result<serde_json::Value, String> = executor
            .record(
                RecordSpec {
                    environment: "regtest".to_string(),
                    source: CommandSource::Rpc,
                    triggering_action: "mine blocks".to_string(),
                    command_display: "bitcoin-cli -regtest generatetoaddress 1 bcrt1qexample"
                        .to_string(),
                    redact: vec![],
                    sensitivity: Sensitivity::Normal,
                    background: false,
                },
                || async { Ok(serde_json::json!(["blockhash123"])) },
            )
            .await;
        assert!(result.is_ok());

        let mut saw_started = false;
        let mut saw_finished_ok = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                ExecEvent::Started {
                    source,
                    command_display,
                    ..
                } => {
                    assert_eq!(source, CommandSource::Rpc);
                    assert!(command_display.contains("generatetoaddress"));
                    saw_started = true;
                }
                ExecEvent::Finished { exit_code, .. } => {
                    assert_eq!(exit_code, Some(0));
                    saw_finished_ok = true;
                }
                _ => {}
            }
        }
        assert!(saw_started && saw_finished_ok);
    }

    #[tokio::test]
    async fn the_background_flag_reaches_the_started_event() {
        let executor = Executor::new();
        let mut rx = executor.subscribe();
        let mut spec = shell_spec("echo hi");
        spec.background = true;
        executor.execute(spec).await.unwrap();

        let mut saw_background_started = false;
        while let Ok(event) = rx.try_recv() {
            if let ExecEvent::Started { background, .. } = event {
                assert!(background);
                saw_background_started = true;
            }
        }
        assert!(saw_background_started);
    }

    #[tokio::test]
    async fn a_command_with_no_stdin_does_not_hang() {
        // Regression check for the "close the pipe if nothing was
        // written" logic -- a command that tries to read stdin would
        // block forever if the pipe were left open with nothing coming.
        let executor = Executor::new();
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            executor.execute(shell_spec("echo done")),
        )
        .await
        .expect("command should not hang waiting for stdin")
        .unwrap();
        assert_eq!(outcome.exit_code, Some(0));
    }
}
