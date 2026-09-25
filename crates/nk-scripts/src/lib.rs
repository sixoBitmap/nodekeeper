//! Script runner (docs/SPEC.md item 6): detects which interpreters are
//! actually usable on this machine and runs a script file through the
//! central executor with the standard environment variables every
//! script receives. Scripts are "trusted code with full node control"
//! -- unlike the console (`nk_core::console_safety`'s per-command
//! classification), there's no fine-grained safety layer here; the
//! caller (the Tauri command layer) is responsible for the mandatory
//! import/create warning and the "regtest only" environment check
//! before any of this ever runs (docs/SPEC.md: "The 'regtest only'
//! restriction is enforced by the runner, not the script").

use nk_exec::{CommandSource, CommandSpec, ExecError, ExecOutcome, Executor, Sensitivity};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Interpreter {
    Python,
    Node,
    Bash,
}

impl Interpreter {
    pub const ALL: [Interpreter; 3] = [Interpreter::Python, Interpreter::Node, Interpreter::Bash];

    /// Program name(s) to try, in order. "python3" first: the
    /// unambiguous name on macOS/Linux (a bare "python" there can be
    /// missing or Python 2); Windows installs from python.org commonly
    /// provide only "python", so it's the fallback everywhere.
    fn candidate_names(self) -> &'static [&'static str] {
        match self {
            Interpreter::Python => &["python3", "python"],
            Interpreter::Node => &["node"],
            // Confirmed live on Windows (DECISIONS.md, Phase 7): a bare
            // "bash" can resolve to Windows' own WSL-install-prompt
            // stub (System32\bash.exe) instead of real bash when WSL
            // isn't installed. That stub exits non-zero with a
            // distinct WSL error rather than succeeding, so the same
            // "spawn --version, check exit 0" probe below already
            // reports it correctly as unavailable -- no special-case
            // string matching needed.
            Interpreter::Bash => &["bash"],
        }
    }

    pub fn file_extension(self) -> &'static str {
        match self {
            Interpreter::Python => "py",
            Interpreter::Node => "js",
            Interpreter::Bash => "sh",
        }
    }
}

pub struct DetectedInterpreter {
    pub interpreter: Interpreter,
    /// The actual program name that worked (e.g. "python3" vs
    /// "python") -- what `run_script` must spawn, not necessarily
    /// `interpreter`'s "canonical" name.
    pub program: String,
}

/// Probes for each interpreter by actually trying to run `<name>
/// --version` through the executor (docs/SPEC.md item 6: "Detect
/// whether Python and Node are installed on the machine and explain
/// what's missing; bash is unavailable on stock Windows, so say so").
/// Real detection, not a PATH guess: a candidate that isn't on PATH at
/// all fails to spawn (swallowed here as "not found"); one that's on
/// PATH but not actually usable (the Windows WSL-stub `bash.exe`) spawns
/// fine but exits non-zero, also correctly treated as not found.
pub async fn detect_interpreters(
    executor: &Executor,
    environment_name: &str,
) -> Vec<DetectedInterpreter> {
    let mut found = Vec::new();
    for interpreter in Interpreter::ALL {
        for &name in interpreter.candidate_names() {
            let outcome = executor
                .execute(CommandSpec {
                    program: name.to_string(),
                    args: vec!["--version".to_string()],
                    stdin: None,
                    environment: environment_name.to_string(),
                    source: CommandSource::Script,
                    triggering_action: "detect script interpreters".to_string(),
                    sensitivity: Sensitivity::Normal,
                    redact: vec![],
                    background: true,
                    env_vars: vec![],
                })
                .await;
            if matches!(&outcome, Ok(o) if o.exit_code == Some(0)) {
                found.push(DetectedInterpreter {
                    interpreter,
                    program: name.to_string(),
                });
                break;
            }
        }
    }
    found
}

/// The standard environment variables every script receives (docs/
/// SPEC.md item 6: "they receive the RPC URL, cookie path, ord server
/// URL, and NKP_NETWORK"). Only `NKP_NETWORK`'s exact name is given by
/// the spec; the other three follow the same `NKP_` prefix for
/// consistency.
pub fn script_env_vars(
    network: &str,
    rpc_url: &str,
    cookie_path: &Path,
    ord_server_url: &str,
) -> Vec<(String, String)> {
    vec![
        ("NKP_NETWORK".to_string(), network.to_string()),
        ("NKP_RPC_URL".to_string(), rpc_url.to_string()),
        (
            "NKP_COOKIE_PATH".to_string(),
            cookie_path.display().to_string(),
        ),
        ("NKP_ORD_URL".to_string(), ord_server_url.to_string()),
    ]
}

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("exec error: {0}")]
    Exec(#[from] ExecError),
}

/// Runs `script_path` with `interpreter`, passing `script_args` as its
/// command-line arguments and `env_vars` (build with
/// `script_env_vars`). The caller has already done the "regtest only"
/// check and shown the mandatory warning -- this just runs what it's
/// given, same division of responsibility as `nk_ord::wallet::
/// run_console_subcommand`.
pub async fn run_script(
    executor: &Executor,
    interpreter: &DetectedInterpreter,
    script_path: &Path,
    script_args: Vec<String>,
    env_vars: Vec<(String, String)>,
    environment_name: &str,
) -> Result<ExecOutcome, ScriptError> {
    let mut args = vec![script_path.display().to_string()];
    args.extend(script_args);
    let outcome = executor
        .execute(CommandSpec {
            program: interpreter.program.clone(),
            args,
            stdin: None,
            environment: environment_name.to_string(),
            source: CommandSource::Script,
            triggering_action: format!("run script {}", script_path.display()),
            sensitivity: Sensitivity::Normal,
            redact: vec![],
            background: false,
            env_vars,
        })
        .await?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn file_extensions_are_distinct() {
        let exts: Vec<_> = Interpreter::ALL
            .iter()
            .map(|i| i.file_extension())
            .collect();
        let mut unique = exts.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(exts.len(), unique.len());
    }

    #[test]
    fn script_env_vars_has_the_four_documented_names() {
        let vars = script_env_vars(
            "regtest",
            "http://127.0.0.1:18443",
            Path::new("/data/regtest/.cookie"),
            "http://127.0.0.1:8081",
        );
        let names: Vec<&str> = vars.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "NKP_NETWORK",
                "NKP_RPC_URL",
                "NKP_COOKIE_PATH",
                "NKP_ORD_URL"
            ]
        );
        assert_eq!(vars[0].1, "regtest");
    }

    /// Real detection against whatever's actually installed on the
    /// machine running this test -- not mocked, and deliberately not
    /// hard-asserting any *specific* interpreter is present. This
    /// matters more than it looks: on the Windows machine this was
    /// developed on, `python`/`python3` resolve on PATH but are only
    /// the Microsoft Store app-execution-alias stub (confirmed live --
    /// running it prints "Python was not found; run without arguments
    /// to install from the Microsoft Store" and exits 49, not 0), so a
    /// naive "is python on PATH" check would have reported a false
    /// positive. Node, by contrast, really is installed there. The one
    /// thing worth asserting generally: detection must find *something*
    /// runnable on any real dev/CI machine, even if which interpreter
    /// varies.
    #[tokio::test]
    async fn detects_at_least_one_real_interpreter() {
        let executor = Executor::new();
        let found = detect_interpreters(&executor, "regtest").await;
        assert!(
            !found.is_empty(),
            "expected to find at least one real interpreter on this machine"
        );
    }

    /// Real end-to-end: writes a tiny script (in whichever real
    /// interpreter's language was actually detected) that reads back
    /// its own env vars, runs it for real through the executor, and
    /// confirms the values actually arrived in the child process -- not
    /// just that `CommandSpec.env_vars` was populated correctly.
    #[tokio::test]
    async fn run_script_actually_passes_the_env_vars_to_the_child_process() {
        let executor = Executor::new();
        let found = detect_interpreters(&executor, "regtest").await;
        let Some(detected) = found.into_iter().next() else {
            eprintln!("skipping: no real interpreter found on this machine");
            return;
        };

        let (filename, source) = match detected.interpreter {
            Interpreter::Python => (
                "print_env.py",
                "import os\nprint(os.environ.get('NKP_NETWORK', 'MISSING'))\n".to_string(),
            ),
            Interpreter::Node => (
                "print_env.js",
                "console.log(process.env.NKP_NETWORK || 'MISSING')\n".to_string(),
            ),
            Interpreter::Bash => (
                "print_env.sh",
                "echo \"${NKP_NETWORK:-MISSING}\"\n".to_string(),
            ),
        };

        let dir = tempfile::tempdir().unwrap();
        let script_path = dir.path().join(filename);
        let mut file = std::fs::File::create(&script_path).unwrap();
        file.write_all(source.as_bytes()).unwrap();
        drop(file);

        let env_vars = script_env_vars(
            "regtest",
            "http://127.0.0.1:18443",
            Path::new("/data/regtest/.cookie"),
            "http://127.0.0.1:8081",
        );
        let outcome = run_script(
            &executor,
            &detected,
            &script_path,
            vec![],
            env_vars,
            "regtest",
        )
        .await
        .unwrap();

        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(String::from_utf8_lossy(&outcome.stdout).trim(), "regtest");
    }
}
