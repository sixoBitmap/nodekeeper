//! SQLite storage and versioned schema migrations for settings, command
//! history, templates, and script metadata.
//!
//! Migrations are tracked via SQLite's `user_version` pragma
//! (`rusqlite_migration`), not a table, so opening an already-migrated
//! database is a no-op.

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use std::path::Path;
use thiserror::Error;

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
    Migrations::new(vec![M::up(
        "CREATE TABLE settings (
            key   TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        );",
    )])
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
    fn open_in_memory() -> Result<Self, StoreError> {
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
}
