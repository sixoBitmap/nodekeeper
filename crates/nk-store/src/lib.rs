//! SQLite storage and versioned schema migrations for settings, command
//! history, templates, and script metadata.
//!
//! Migrations are tracked via SQLite's `user_version` pragma
//! (`rusqlite_migration`), not a table, so opening an already-migrated
//! database is a no-op.

mod history_bridge;

pub use history_bridge::persist_exec_events;

/// docs/SPEC.md item 7: "Rolling history (e.g. last 5,000 entries per
/// environment) in SQLite." How many `command_history` rows are kept per
/// environment; older ones are pruned (see `history_bridge` and
/// `Store::prune_all_command_history`).
pub const COMMAND_HISTORY_KEEP_PER_ENVIRONMENT: u32 = 5_000;

use rusqlite::{params, Connection};
use rusqlite_migration::{Migrations, M};
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),
}

/// Every migration, in order. Append-only: once released, a migration's
/// SQL must never change — add a new one instead.
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(
            "CREATE TABLE settings (
                key   TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );",
        ),
        M::up(
            // Rolling command history for the Live Command Monitor
            // (docs/SPEC.md item 7: "Rolling history (e.g. last 5,000
            // entries per environment) in SQLite, relative to the data
            // folder so it works in portable mode"). One row per command
            // (not per output chunk); `output` accumulates streamed
            // stdout/stderr as it arrives. Only ever fed from nk-exec's
            // already-redacted broadcast stream, so there is nothing
            // here to separately redact -- a `Sensitivity::Sensitive`
            // command's real output never reaches this table (Phase 2's
            // sensitive-output channel withholds it before it gets this
            // far).
            "CREATE TABLE command_history (
                id                 TEXT PRIMARY KEY NOT NULL,
                environment        TEXT NOT NULL,
                source             TEXT NOT NULL,
                triggering_action  TEXT NOT NULL,
                command_display    TEXT NOT NULL,
                started_at_ms      INTEGER NOT NULL,
                status             TEXT NOT NULL,
                exit_code          INTEGER,
                duration_ms        INTEGER,
                output             TEXT NOT NULL DEFAULT '',
                background         INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX command_history_environment_idx
                ON command_history (environment, started_at_ms);",
        ),
    ])
}

/// What `Store::scrub_private_keys_from_command_history` did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScrubReport {
    /// Rows deleted outright because they reveal a secret.
    pub rows_deleted: usize,
    /// Rows kept, with an extended private key scrubbed out of them.
    pub rows_scrubbed: usize,
}

impl ScrubReport {
    pub fn is_empty(&self) -> bool {
        self.rows_deleted == 0 && self.rows_scrubbed == 0
    }
}

/// One row of `command_history`, as read back for the Live Command
/// Monitor UI.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct CommandHistoryEntry {
    pub id: String,
    pub environment: String,
    pub source: String,
    pub triggering_action: String,
    pub command_display: String,
    #[ts(type = "number")]
    pub started_at_ms: i64,
    pub status: CommandHistoryStatus,
    pub exit_code: Option<i32>,
    #[ts(type = "number | null")]
    pub duration_ms: Option<i64>,
    pub output: String,
    pub background: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum CommandHistoryStatus {
    Running,
    Success,
    Error,
}

impl CommandHistoryStatus {
    fn as_db_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Success => "success",
            Self::Error => "error",
        }
    }

    fn from_db_str(s: &str) -> Self {
        match s {
            "success" => Self::Success,
            "error" => Self::Error,
            _ => Self::Running,
        }
    }
}

pub struct Store {
    conn: Connection,
    /// Set by [`Store::quarantine_command_history`]: the command history is
    /// not handed out for the rest of this run.
    history_quarantined: AtomicBool,
}

impl Store {
    /// Opens (creating if needed) the SQLite database at `path` and runs
    /// any pending migrations.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let mut conn = Connection::open(path)?;
        Self::configure(&conn)?;
        migrations().to_latest(&mut conn)?;
        Ok(Self::from_connection(conn))
    }

    fn from_connection(conn: Connection) -> Self {
        Self {
            conn,
            history_quarantined: AtomicBool::new(false),
        }
    }

    #[cfg(test)]
    pub(crate) fn open_in_memory() -> Result<Self, StoreError> {
        let mut conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        migrations().to_latest(&mut conn)?;
        Ok(Self::from_connection(conn))
    }

    /// `secure_delete`: a deleted or overwritten row's bytes are zeroed in
    /// the file instead of left in a free page. Without it, deleting a
    /// history row (pruning, or erasing one that revealed a secret) removes
    /// it from the table but not from the database file, where it can still
    /// be read. Set on every connection -- it is a per-connection setting.
    fn configure(conn: &Connection) -> Result<(), StoreError> {
        conn.pragma_update(None, "secure_delete", "ON")?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            (key, value),
        )?;
        Ok(())
    }

    /// Inserts a new `command_history` row in the `Running` state. `id`
    /// is the command's `CommandId` (as a string) so later output/finish
    /// events can find the same row.
    #[allow(clippy::too_many_arguments)]
    pub fn record_command_started(
        &self,
        id: &str,
        environment: &str,
        source: &str,
        triggering_action: &str,
        command_display: &str,
        started_at_ms: i64,
        background: bool,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO command_history
                (id, environment, source, triggering_action, command_display,
                 started_at_ms, status, background)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                environment,
                source,
                triggering_action,
                command_display,
                started_at_ms,
                CommandHistoryStatus::Running.as_db_str(),
                background,
            ],
        )?;
        Ok(())
    }

    /// Appends a streamed output chunk to an existing row. A no-op if
    /// the row doesn't exist (e.g. history was pruned mid-stream) rather
    /// than an error — losing a stale tail of output is harmless.
    pub fn append_command_output(&self, id: &str, chunk: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE command_history SET output = output || ?2 WHERE id = ?1",
            params![id, chunk],
        )?;
        Ok(())
    }

    /// Marks a row finished. Same no-op-if-missing behavior as
    /// `append_command_output`.
    pub fn record_command_finished(
        &self,
        id: &str,
        exit_code: Option<i32>,
        duration_ms: i64,
    ) -> Result<(), StoreError> {
        let status = match exit_code {
            Some(0) => CommandHistoryStatus::Success,
            _ => CommandHistoryStatus::Error,
        };
        self.conn.execute(
            "UPDATE command_history
             SET status = ?2, exit_code = ?3, duration_ms = ?4
             WHERE id = ?1",
            params![id, status.as_db_str(), exit_code, duration_ms],
        )?;
        Ok(())
    }

    /// Most recent entries first, optionally filtered to one
    /// environment. `limit` caps how many rows come back (the Live
    /// Command Monitor pages/filters client-side beyond that).
    pub fn list_command_history(
        &self,
        environment: Option<&str>,
        limit: u32,
    ) -> Result<Vec<CommandHistoryEntry>, StoreError> {
        if self.history_quarantined.load(Ordering::Relaxed) {
            return Ok(Vec::new());
        }
        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<CommandHistoryEntry> {
            Ok(CommandHistoryEntry {
                id: row.get("id")?,
                environment: row.get("environment")?,
                source: row.get("source")?,
                triggering_action: row.get("triggering_action")?,
                command_display: row.get("command_display")?,
                started_at_ms: row.get("started_at_ms")?,
                status: CommandHistoryStatus::from_db_str(&row.get::<_, String>("status")?),
                exit_code: row.get("exit_code")?,
                duration_ms: row.get("duration_ms")?,
                output: row.get("output")?,
                background: row.get("background")?,
            })
        };

        if let Some(environment) = environment {
            let mut stmt = self.conn.prepare(
                "SELECT * FROM command_history WHERE environment = ?1
                 ORDER BY started_at_ms DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![environment, limit], map_row)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        } else {
            let mut stmt = self
                .conn
                .prepare("SELECT * FROM command_history ORDER BY started_at_ms DESC LIMIT ?1")?;
            let rows = stmt.query_map(params![limit], map_row)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        }
    }

    /// Deletes the oldest rows for `environment` beyond `keep`, keeping
    /// history bounded per environment (docs/SPEC.md item 7's "last
    /// 5,000 entries per environment"). Returns how many rows were
    /// deleted. Called by `history_bridge` every so many new commands
    /// (not after every one: a busy poll records a lot of rows, and the
    /// history only has to stay *about* that size), and for every
    /// environment at startup via `prune_all_command_history`.
    pub fn prune_command_history(&self, environment: &str, keep: u32) -> Result<usize, StoreError> {
        let deleted = self.conn.execute(
            "DELETE FROM command_history
             WHERE environment = ?1 AND id NOT IN (
                 SELECT id FROM command_history
                 WHERE environment = ?1
                 ORDER BY started_at_ms DESC
                 LIMIT ?2
             )",
            params![environment, keep],
        )?;
        Ok(deleted)
    }

    /// Erases private key material from the stored command history, for
    /// rows recorded **before** the console refused the commands that print
    /// it (`listdescriptors true`, `gethdkeys` with `private`, `ord wallet
    /// dump`) and before the executor scrubbed extended private keys from
    /// what it emits -- the "logs, database, exports" clause of the
    /// secrets rule (CLAUDE.md), which until then held only for newly
    /// recorded rows.
    ///
    /// - A row that `reveals_secrets(command_display, output)` says reveals
    ///   one (a command that prints keys or a recovery phrase, a passphrase
    ///   recorded in the clear) is **deleted**, whole: its output is the
    ///   thing to remove, and its display may itself carry a key (a pasted
    ///   private descriptor).
    /// - Any other row has extended private keys scrubbed from its display
    ///   and output in place (the same scrub the executor applies to new
    ///   output), and is left as it is otherwise.
    ///
    /// Idempotent (a second run finds nothing), and cheap enough to run at
    /// every launch: the table holds at most a few thousand rows per
    /// environment. Runs in one transaction. The predicate is a parameter
    /// so this crate doesn't need to know *which* commands are the
    /// dangerous ones; that lives with the console's safety layer.
    ///
    /// Deleting a row does not remove its bytes from the file by itself: the
    /// connection has `secure_delete` on (they are zeroed, not left in a
    /// free page), and when anything was changed the file is then
    /// `VACUUM`ed, which rewrites it without the free pages and the old
    /// copies of updated rows.
    pub fn scrub_private_keys_from_command_history(
        &self,
        reveals_secrets: impl Fn(&str, &str) -> bool,
    ) -> Result<ScrubReport, StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        let rows: Vec<(String, String, String)> = {
            let mut stmt = tx.prepare("SELECT id, command_display, output FROM command_history")?;
            let mapped = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            mapped.collect::<Result<_, _>>()?
        };

        let mut report = ScrubReport::default();
        for (id, display, output) in rows {
            if reveals_secrets(&display, &output) {
                tx.execute("DELETE FROM command_history WHERE id = ?1", params![id])?;
                report.rows_deleted += 1;
                continue;
            }
            let clean_display = nk_exec::redact::scrub_private_keys(&display);
            let clean_output = nk_exec::redact::scrub_private_keys(&output);
            if clean_display != display.as_str() || clean_output != output.as_str() {
                tx.execute(
                    "UPDATE command_history SET command_display = ?2, output = ?3 WHERE id = ?1",
                    params![id, clean_display.as_ref(), clean_output.as_ref()],
                )?;
                report.rows_scrubbed += 1;
            }
        }
        tx.commit()?;
        if !report.is_empty() {
            self.conn.execute_batch("VACUUM")?;
        }
        Ok(report)
    }

    /// Runs `VACUUM` **once** (per `marker_key`, remembered in the settings
    /// table once it has succeeded, so a failure is retried on the next
    /// launch). `secure_delete` only zeroes what is deleted *from now on*; the
    /// free pages that an earlier build left behind -- rows its pruning had
    /// already deleted, the old copies of rows it appended output to -- still
    /// hold their bytes, and that includes any secret the history ever
    /// contained. `VACUUM` rewrites the file without them, whether or not the
    /// scrub found anything to change. Returns whether it ran.
    pub fn vacuum_once(&self, marker_key: &str) -> Result<bool, StoreError> {
        if self.get_setting(marker_key)?.as_deref() == Some("1") {
            return Ok(false);
        }
        self.conn.execute_batch("VACUUM")?;
        self.set_setting(marker_key, "1")?;
        Ok(true)
    }

    /// The fallback when erasing old history **failed** (a full disk, a locked
    /// or unreadable file): a history that may still hold a secret must not
    /// stay visible in the Live Command Monitor or an export. Hides it for the
    /// rest of this run, and tries to delete it (best effort: whatever made
    /// the scrub fail may fail this too). The next launch tries the scrub
    /// again.
    pub fn quarantine_command_history(&self) {
        self.history_quarantined.store(true, Ordering::Relaxed);
        let _ = self.conn.execute("DELETE FROM command_history", []);
    }

    /// `prune_command_history` for every environment that has any
    /// history. Run once at startup so a history that grew past the limit
    /// (before pruning existed, or while the app was closed mid-batch)
    /// shrinks back without waiting for new commands to trigger it.
    /// Returns the total number of rows deleted.
    pub fn prune_all_command_history(&self, keep: u32) -> Result<usize, StoreError> {
        let environments: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT DISTINCT environment FROM command_history")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        let mut deleted = 0;
        for environment in environments {
            deleted += self.prune_command_history(&environment, keep)?;
        }
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_internally_consistent() {
        migrations().validate().expect("migrations should validate");
    }

    #[test]
    fn settings_round_trip() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.get_setting("theme").unwrap(), None);

        store.set_setting("theme", "dark").unwrap();
        assert_eq!(
            store.get_setting("theme").unwrap(),
            Some("dark".to_string())
        );

        // upsert overwrites, doesn't duplicate
        store.set_setting("theme", "light").unwrap();
        assert_eq!(
            store.get_setting("theme").unwrap(),
            Some("light".to_string())
        );
    }

    #[test]
    fn reopening_an_already_migrated_file_db_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodekeeper.sqlite3");

        {
            let store = Store::open(&path).unwrap();
            store.set_setting("k", "v").unwrap();
        }
        // Reopen: migrations must not error or reset existing data.
        let store = Store::open(&path).unwrap();
        assert_eq!(store.get_setting("k").unwrap(), Some("v".to_string()));
    }

    #[test]
    fn command_history_round_trip_started_output_and_finished() {
        let store = Store::open_in_memory().unwrap();
        store
            .record_command_started(
                "cmd-1",
                "regtest",
                "bitcoincli",
                "Dashboard -> check status",
                "bitcoin-cli -regtest getblockchaininfo",
                1_700_000_000_000,
                false,
            )
            .unwrap();
        store.append_command_output("cmd-1", "line one\n").unwrap();
        store.append_command_output("cmd-1", "line two\n").unwrap();
        store.record_command_finished("cmd-1", Some(0), 42).unwrap();

        let entries = store.list_command_history(None, 10).unwrap();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.id, "cmd-1");
        assert_eq!(entry.environment, "regtest");
        assert_eq!(entry.status, CommandHistoryStatus::Success);
        assert_eq!(entry.exit_code, Some(0));
        assert_eq!(entry.duration_ms, Some(42));
        assert_eq!(entry.output, "line one\nline two\n");
        assert!(!entry.background);
    }

    #[test]
    fn a_nonzero_exit_code_is_recorded_as_error_status() {
        let store = Store::open_in_memory().unwrap();
        store
            .record_command_started("cmd-1", "regtest", "ordcli", "test", "ord test", 0, false)
            .unwrap();
        store.record_command_finished("cmd-1", Some(1), 5).unwrap();

        let entries = store.list_command_history(None, 10).unwrap();
        assert_eq!(entries[0].status, CommandHistoryStatus::Error);
    }

    #[test]
    fn an_unfinished_command_stays_running() {
        let store = Store::open_in_memory().unwrap();
        store
            .record_command_started("cmd-1", "regtest", "ordcli", "test", "ord test", 0, false)
            .unwrap();

        let entries = store.list_command_history(None, 10).unwrap();
        assert_eq!(entries[0].status, CommandHistoryStatus::Running);
        assert_eq!(entries[0].exit_code, None);
        assert_eq!(entries[0].duration_ms, None);
    }

    #[test]
    fn list_command_history_filters_by_environment_and_orders_newest_first() {
        let store = Store::open_in_memory().unwrap();
        for (id, env, ts) in [
            ("a", "regtest", 100),
            ("b", "mainnet", 200),
            ("c", "regtest", 300),
        ] {
            store
                .record_command_started(id, env, "ordcli", "t", "cmd", ts, false)
                .unwrap();
        }

        let regtest_only = store.list_command_history(Some("regtest"), 10).unwrap();
        assert_eq!(
            regtest_only
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            vec!["c", "a"]
        );

        let all = store.list_command_history(None, 10).unwrap();
        assert_eq!(
            all.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["c", "b", "a"]
        );
    }

    #[test]
    fn list_command_history_respects_the_limit() {
        let store = Store::open_in_memory().unwrap();
        for i in 0..5 {
            store
                .record_command_started(&i.to_string(), "regtest", "ordcli", "t", "cmd", i, false)
                .unwrap();
        }
        assert_eq!(store.list_command_history(None, 2).unwrap().len(), 2);
    }

    #[test]
    fn background_flag_round_trips() {
        let store = Store::open_in_memory().unwrap();
        store
            .record_command_started(
                "cmd-1",
                "regtest",
                "rpc",
                "background polling",
                "bitcoin-cli getblockchaininfo",
                0,
                true,
            )
            .unwrap();
        assert!(store.list_command_history(None, 10).unwrap()[0].background);
    }

    #[test]
    fn prune_command_history_keeps_only_the_newest_n_per_environment() {
        let store = Store::open_in_memory().unwrap();
        for i in 0..10 {
            store
                .record_command_started(
                    &format!("regtest-{i}"),
                    "regtest",
                    "ordcli",
                    "t",
                    "cmd",
                    i,
                    false,
                )
                .unwrap();
        }
        // A different environment's rows must never be touched by
        // another environment's prune.
        store
            .record_command_started("mainnet-1", "mainnet", "ordcli", "t", "cmd", 0, false)
            .unwrap();

        let deleted = store.prune_command_history("regtest", 3).unwrap();
        assert_eq!(deleted, 7, "10 rows, keeping 3");

        let regtest = store.list_command_history(Some("regtest"), 100).unwrap();
        assert_eq!(regtest.len(), 3);
        // Kept the newest three (highest started_at_ms): 9, 8, 7.
        assert_eq!(
            regtest.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["regtest-9", "regtest-8", "regtest-7"]
        );

        assert_eq!(
            store
                .list_command_history(Some("mainnet"), 100)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn prune_all_command_history_trims_every_environment_over_the_limit() {
        let store = Store::open_in_memory().unwrap();
        for (environment, count) in [("regtest", 8), ("mainnet", 5), ("signet", 2)] {
            for i in 0..count {
                store
                    .record_command_started(
                        &format!("{environment}-{i}"),
                        environment,
                        "ordcli",
                        "t",
                        "cmd",
                        i,
                        false,
                    )
                    .unwrap();
            }
        }

        // Limit 5: regtest loses 3; mainnet (exactly 5) and signet (2)
        // are left alone.
        assert_eq!(store.prune_all_command_history(5).unwrap(), 3);
        let count = |environment: &str| {
            store
                .list_command_history(Some(environment), 100)
                .unwrap()
                .len()
        };
        assert_eq!(count("regtest"), 5);
        assert_eq!(count("mainnet"), 5);
        assert_eq!(count("signet"), 2);

        // Nothing over the limit any more: a second pass deletes nothing.
        assert_eq!(store.prune_all_command_history(5).unwrap(), 0);
    }

    /// A synthetic key-shaped string (prefix + 107 base58 characters). Never
    /// a real key.
    fn fake_key(prefix: &str) -> String {
        format!("{prefix}{}", "A".repeat(107))
    }

    fn record_row(store: &Store, id: &str, display: &str, output: &str) {
        store
            .record_command_started(id, "regtest", "bitcoincli", "t", display, 0, false)
            .unwrap();
        store.append_command_output(id, output).unwrap();
    }

    fn row(store: &Store, id: &str) -> Option<CommandHistoryEntry> {
        store
            .list_command_history(None, 100)
            .unwrap()
            .into_iter()
            .find(|e| e.id == id)
    }

    /// The old-history half of "private keys never reach the database":
    /// rows recorded before the console refused key-printing commands.
    #[test]
    fn scrubbing_old_history_deletes_key_printing_commands_and_scrubs_stray_keys() {
        let store = Store::open_in_memory().unwrap();
        let descriptor = format!("wpkh({}/84h/1h/0h/0/*)", fake_key("tprv"));

        // 1. A command that printed private keys: the whole row must go.
        record_row(
            &store,
            "dump",
            "bitcoin-cli listdescriptors true",
            &format!("{{\"desc\":\"{descriptor}\"}}"),
        );
        // 2. An innocent command whose output happens to carry a key (a
        //    script, a pasted descriptor): kept, key removed.
        record_row(
            &store,
            "stray",
            "bitcoin-cli getdescriptorinfo",
            &format!("saw {}", fake_key("xprv")),
        );
        // 3. Innocent and clean: untouched.
        record_row(&store, "clean", "bitcoin-cli getblockcount", "812345");

        let report = store
            .scrub_private_keys_from_command_history(|display, _| {
                display.contains("listdescriptors true")
            })
            .unwrap();
        assert_eq!(
            report,
            ScrubReport {
                rows_deleted: 1,
                rows_scrubbed: 1
            }
        );

        assert!(
            row(&store, "dump").is_none(),
            "the key-printing row is gone"
        );
        let stray = row(&store, "stray").expect("kept");
        assert!(!stray.output.contains("prv"), "{}", stray.output);
        assert!(stray
            .output
            .contains(nk_exec::redact::PRIVATE_KEY_PLACEHOLDER));
        assert_eq!(row(&store, "clean").unwrap().output, "812345");

        // Nothing anywhere in the table still looks like a private key.
        for entry in store.list_command_history(None, 100).unwrap() {
            assert!(!entry.output.contains("tprv") && !entry.output.contains("xprv"));
            assert!(!entry.command_display.contains("tprv"));
        }
    }

    #[test]
    fn a_display_carrying_a_key_is_scrubbed_in_place() {
        let store = Store::open_in_memory().unwrap();
        record_row(
            &store,
            "pasted",
            &format!("bitcoin-cli deriveaddresses wpkh({}/0/*)", fake_key("tprv")),
            "[]",
        );
        let report = store
            .scrub_private_keys_from_command_history(|_, _| false)
            .unwrap();
        assert_eq!(report.rows_scrubbed, 1);
        assert!(!row(&store, "pasted")
            .unwrap()
            .command_display
            .contains("prv"));
    }

    #[test]
    fn scrubbing_is_idempotent_and_a_clean_history_is_untouched() {
        let store = Store::open_in_memory().unwrap();
        record_row(&store, "clean", "bitcoin-cli getblockcount", "812345");
        assert!(store
            .scrub_private_keys_from_command_history(|_, _| false)
            .unwrap()
            .is_empty());

        record_row(&store, "dump", "bitcoin-cli listdescriptors true", "x");
        let first = store
            .scrub_private_keys_from_command_history(|d, _| d.contains("listdescriptors true"))
            .unwrap();
        assert_eq!(first.rows_deleted, 1);
        let second = store
            .scrub_private_keys_from_command_history(|d, _| d.contains("listdescriptors true"))
            .unwrap();
        assert!(second.is_empty(), "a second pass finds nothing: {second:?}");
    }

    /// "Erased" has to mean gone from the *file*: a deleted row's bytes used
    /// to survive in a free page, readable with any hex viewer. This reads
    /// the database file's raw bytes.
    #[test]
    fn erased_history_is_gone_from_the_database_file_itself() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodekeeper.sqlite3");
        let marker = "MARKER-recovery-phrase-abandon-ability-able";

        {
            let store = Store::open(&path).unwrap();
            // Enough other rows that the deleted one lands in the middle of
            // the file, not in a page that is simply truncated away.
            for i in 0..50 {
                record_row(
                    &store,
                    &format!("keep-{i}"),
                    "bitcoin-cli getblockcount",
                    "812345",
                );
            }
            record_row(
                &store,
                "secret",
                "bitcoin-cli listdescriptors true",
                &format!("{{\"note\":\"{marker}\"}}"),
            );
            // A row that is kept but scrubbed in place: the old text of an
            // updated row is the other place a stale copy could live.
            record_row(
                &store,
                "stray",
                "bitcoin-cli getdescriptorinfo",
                &format!("{marker} {}", fake_key("xprv")),
            );
            let before = std::fs::read(&path).unwrap();
            assert!(
                before.windows(marker.len()).any(|w| w == marker.as_bytes()),
                "the test needs the marker to be in the file to begin with"
            );

            let report = store
                .scrub_private_keys_from_command_history(|display, _| {
                    display.contains("listdescriptors true")
                })
                .unwrap();
            assert_eq!(report.rows_deleted, 1);
            assert_eq!(report.rows_scrubbed, 1);
        }

        let after = std::fs::read(&path).unwrap();
        // The deleted row's marker is gone. (The scrubbed row keeps its
        // marker text -- only the key is removed from it -- so look for the
        // one thing that must not survive anywhere: the key.)
        assert!(
            !after.windows(4).any(|w| w == b"xprv"),
            "the scrubbed key must not survive in the file"
        );
        let markers = after
            .windows(marker.len())
            .filter(|w| *w == marker.as_bytes())
            .count();
        assert_eq!(
            markers, 1,
            "only the kept row's marker remains, not the deleted row's copy"
        );
    }

    /// What an **earlier build** left behind: it ran without `secure_delete`,
    /// so a row its pruning had deleted still sits, bytes and all, in a free
    /// page. The scrub finds no row to change (there is none any more), so
    /// only `vacuum_once` gets the bytes out of the file.
    #[test]
    fn vacuum_once_removes_what_an_earlier_build_left_in_free_pages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodekeeper.sqlite3");
        let marker = "MARKER-old-build-secret-abandon-ability";
        {
            let store = Store::open(&path).unwrap();
            // Behave like the old build.
            store
                .conn
                .pragma_update(None, "secure_delete", "OFF")
                .unwrap();
            for i in 0..30 {
                record_row(
                    &store,
                    &format!("keep-{i}"),
                    "bitcoin-cli getblockcount",
                    "812345",
                );
            }
            // Larger than a page, so it lives in overflow pages.
            let big = format!("{marker} {}", "x".repeat(6000));
            record_row(&store, "old", "bitcoin-cli listdescriptors true", &big);
            store
                .conn
                .execute("DELETE FROM command_history WHERE id = 'old'", [])
                .unwrap();
        }
        let count = |path: &std::path::Path| {
            let raw = std::fs::read(path).unwrap();
            raw.windows(marker.len())
                .filter(|w| *w == marker.as_bytes())
                .count()
        };
        assert!(
            count(&path) > 0,
            "positive control: the deleted row's bytes are still in the file"
        );

        let store = Store::open(&path).unwrap();
        // The scrub has nothing to do -- and so does not vacuum.
        let report = store
            .scrub_private_keys_from_command_history(|_, _| false)
            .unwrap();
        assert!(report.is_empty());
        assert!(count(&path) > 0, "a no-op scrub does not remove them");

        assert!(store.vacuum_once("history_vacuumed_test").unwrap());
        drop(store);
        assert_eq!(count(&path), 0, "vacuum_once removed the old bytes");

        // Once done, it is remembered (and not repeated).
        let store = Store::open(&path).unwrap();
        assert!(!store.vacuum_once("history_vacuumed_test").unwrap());
        // A different marker key runs again.
        assert!(store.vacuum_once("history_vacuumed_other").unwrap());
    }

    #[test]
    fn a_quarantined_history_is_hidden_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodekeeper.sqlite3");
        {
            let store = Store::open(&path).unwrap();
            record_row(&store, "a", "bitcoin-cli listdescriptors true", "secret");
            assert_eq!(store.list_command_history(None, 10).unwrap().len(), 1);
            store.quarantine_command_history();
            assert!(store.list_command_history(None, 10).unwrap().is_empty());
        }
        let store = Store::open(&path).unwrap();
        assert!(
            store.list_command_history(None, 10).unwrap().is_empty(),
            "the rows were deleted, not just hidden"
        );
    }

    #[test]
    fn command_history_survives_reopening_the_file_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodekeeper.sqlite3");

        {
            let store = Store::open(&path).unwrap();
            store
                .record_command_started("cmd-1", "regtest", "ordcli", "t", "cmd", 0, false)
                .unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.list_command_history(None, 10).unwrap().len(), 1);
    }
}
