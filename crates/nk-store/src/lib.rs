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
}

impl Store {
    /// Opens (creating if needed) the SQLite database at `path` and runs
    /// any pending migrations.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let mut conn = Connection::open(path)?;
        migrations().to_latest(&mut conn)?;
        Ok(Self { conn })
    }

    #[cfg(test)]
    pub(crate) fn open_in_memory() -> Result<Self, StoreError> {
        let mut conn = Connection::open_in_memory()?;
        migrations().to_latest(&mut conn)?;
        Ok(Self { conn })
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
