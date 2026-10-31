//! The `SQLite` store (APP.md, "Store modes"). Same contract as [`MemoryStore`]: every method
//! is one short transaction, nothing hashes inside one, and a row that is gone is never
//! recreated.
//!
//! Durability: WAL with `synchronous=FULL`, both read back and verified at startup, and a
//! `journal_size_limit` plus a periodic `wal_checkpoint(TRUNCATE)` keep the WAL bounded. A
//! second server on the same database is refused through an exclusive lock on a sidecar
//! `<path>.lock` file, held for the life of the store.
//!
//! [`MemoryStore`]: crate::store::MemoryStore

use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use crate::store::{Attempt, InsertError, MessageMeta, NewMessage, Store, StoreError};

/// The schema this build reads and writes. Any other version is refused.
pub const SCHEMA_VERSION: &str = "1";
/// The WAL is truncated back to this size after checkpoints.
const JOURNAL_SIZE_LIMIT: i64 = 64 * 1024 * 1024;

const SCHEMA: &str = "
CREATE TABLE meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;
CREATE TABLE messages (
    generation INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    blob BLOB NOT NULL,
    size INTEGER NOT NULL,
    pin_salt TEXT NOT NULL,
    pin_verifier TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    failed_attempts INTEGER NOT NULL DEFAULT 0,
    protocol_version INTEGER NOT NULL
) STRICT;
CREATE INDEX messages_expires_at ON messages (expires_at);
";

/// The columns `messages` must have, in order, for this build to use the database.
const MESSAGE_COLUMNS: &[&str] = &[
    "generation",
    "id",
    "blob",
    "size",
    "pin_salt",
    "pin_verifier",
    "expires_at",
    "failed_attempts",
    "protocol_version",
];

/// Why the store can't be opened. The server refuses to start.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("another pinveil-server is already using {0}")]
    Locked(PathBuf),
    #[error("{path} is not a pinveil store this version can use: {why}")]
    Incompatible { path: PathBuf, why: String },
    #[error(
        "{0} was created with a different pin.pepper. A changed pepper would make every correct PIN fail and burn the stored messages. Restore the old pepper, or delete the store files to start empty"
    )]
    PepperMismatch(PathBuf),
    #[error("cannot open the store at {path}: {why}")]
    Io { path: PathBuf, why: String },
}

pub struct SqliteStore {
    conn: Mutex<Connection>,
    max_messages: usize,
    max_bytes: usize,
    /// Held for the life of the store; dropping it releases the lock.
    _lock: File,
    #[cfg(test)]
    fault: Mutex<Option<&'static str>>,
}

/// A fingerprint of the pepper that reveals nothing useful about it: SHA-256 over a fixed
/// label and the pepper.
fn pepper_fingerprint(pepper: &str) -> String {
    let digest = Sha256::new()
        .chain_update(b"pinveil store pepper fingerprint v1\0")
        .chain_update(pepper.as_bytes())
        .finalize();
    digest.iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    })
}

fn nanos(time: SystemTime) -> i64 {
    let since = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    i64::try_from(since.as_nanos()).unwrap_or(i64::MAX)
}

fn from_nanos(value: i64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_nanos(u64::try_from(value).unwrap_or(0))
}

fn storage(_: rusqlite::Error) -> StoreError {
    StoreError
}

impl SqliteStore {
    /// Opens (creating if needed) the store at `path`, locks it, applies and verifies the
    /// pragmas, checks the schema, and checks or records the pepper fingerprint.
    pub fn open(
        path: &Path,
        pepper: &str,
        max_messages: usize,
        max_bytes: usize,
    ) -> Result<Self, OpenError> {
        let io = |why: String| OpenError::Io {
            path: path.to_path_buf(),
            why,
        };
        let mut lock_path = path.as_os_str().to_owned();
        lock_path.push(".lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(|e| io(format!("lock file: {e}")))?;
        lock.try_lock()
            .map_err(|_| OpenError::Locked(path.to_path_buf()))?;

        let conn = Connection::open(path).map_err(|e| io(e.to_string()))?;
        let pragma = |sql: &str| -> Result<String, OpenError> {
            conn.query_row(sql, [], |row| row.get::<_, rusqlite::types::Value>(0))
                .map(|value| match value {
                    rusqlite::types::Value::Text(text) => text,
                    rusqlite::types::Value::Integer(n) => n.to_string(),
                    other => format!("{other:?}"),
                })
                .map_err(|e| io(e.to_string()))
        };
        if !pragma("PRAGMA journal_mode = WAL")?.eq_ignore_ascii_case("wal") {
            return Err(io("the WAL journal mode could not be enabled".into()));
        }
        conn.execute_batch(&format!(
            "PRAGMA synchronous = FULL; PRAGMA journal_size_limit = {JOURNAL_SIZE_LIMIT};"
        ))
        .map_err(|e| io(e.to_string()))?;
        if pragma("PRAGMA synchronous")? != "2" {
            return Err(io("synchronous = FULL could not be set".into()));
        }

        check_schema(&conn, path)?;
        check_pepper(&conn, path, pepper)?;

        Ok(Self {
            conn: Mutex::new(conn),
            max_messages,
            max_bytes,
            _lock: lock,
            #[cfg(test)]
            fault: Mutex::new(None),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        // rusqlite rolls back an unfinished transaction on drop, so a panic can't leave
        // a half-applied change behind.
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Stored message count and bytes, for tests.
    pub fn usage(&self) -> Result<(usize, usize), StoreError> {
        self.lock()
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM messages",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .map(|(n, b)| {
                (
                    usize::try_from(n).unwrap_or(0),
                    usize::try_from(b).unwrap_or(0),
                )
            })
            .map_err(storage)
    }

    /// Fails the next operation that reaches `point`, once. Test builds only: release
    /// binaries have no fault interface at all.
    #[cfg(test)]
    pub fn fail_at(&self, point: &'static str) {
        *self.fault.lock().unwrap() = Some(point);
    }

    #[cfg(test)]
    fn fault(&self, point: &'static str) -> Result<(), StoreError> {
        let mut fault = self.fault.lock().unwrap();
        if *fault == Some(point) {
            *fault = None;
            return Err(StoreError);
        }
        Ok(())
    }

    #[cfg(not(test))]
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    #[inline]
    fn fault(&self, _: &'static str) -> Result<(), StoreError> {
        Ok(())
    }
}

fn incompatible(path: &Path, why: &str) -> OpenError {
    OpenError::Incompatible {
        path: path.to_path_buf(),
        why: why.to_owned(),
    }
}

/// Creates the schema in an empty database, or checks that an existing one is exactly a
/// version-`SCHEMA_VERSION` pinveil store.
fn check_schema(conn: &Connection, path: &Path) -> Result<(), OpenError> {
    let io = |why: String| OpenError::Io {
        path: path.to_path_buf(),
        why,
    };
    let incompatible = |why: &str| incompatible(path, why);
    let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
            .map_err(|e| incompatible(&e.to_string()))?;
    if tables.is_empty() {
        conn.execute_batch(&format!(
            "BEGIN; {SCHEMA}
                 INSERT INTO meta (key, value) VALUES ('schema_version', '{SCHEMA_VERSION}');
                 COMMIT;"
        ))
        .map_err(|e| io(e.to_string()))?;
    } else if tables != ["messages", "meta"] {
        return Err(incompatible("it has tables other than a pinveil store's"));
    }
    let version: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| incompatible(&e.to_string()))?;
    if version.as_deref() != Some(SCHEMA_VERSION) {
        return Err(incompatible(&format!(
            "schema version {version:?}, expected {SCHEMA_VERSION}"
        )));
    }
    let columns: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('messages') ORDER BY cid")
        .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
        .map_err(|e| incompatible(&e.to_string()))?;
    if columns != MESSAGE_COLUMNS {
        return Err(incompatible("the messages table has unexpected columns"));
    }
    Ok(())
}

/// Records the pepper fingerprint in a new store, or refuses a store made with another
/// pepper.
fn check_pepper(conn: &Connection, path: &Path, pepper: &str) -> Result<(), OpenError> {
    let io = |why: String| OpenError::Io {
        path: path.to_path_buf(),
        why,
    };
    let fingerprint = pepper_fingerprint(pepper);
    let stored: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'pepper_fingerprint'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| io(e.to_string()))?;
    match stored {
        Some(stored) if stored != fingerprint => {
            return Err(OpenError::PepperMismatch(path.to_path_buf()));
        }
        Some(_) => {}
        None => {
            conn.execute(
                "INSERT INTO meta (key, value) VALUES ('pepper_fingerprint', ?1)",
                [&fingerprint],
            )
            .map_err(|e| io(e.to_string()))?;
        }
    }

    Ok(())
}

fn sweep_in(tx: &rusqlite::Transaction<'_>, now: SystemTime) -> rusqlite::Result<usize> {
    tx.execute("DELETE FROM messages WHERE expires_at <= ?1", [nanos(now)])
}

fn admits(
    tx: &rusqlite::Transaction<'_>,
    len: usize,
    max_messages: usize,
    max_bytes: usize,
) -> rusqlite::Result<bool> {
    let (count, bytes): (i64, i64) = tx.query_row(
        "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM messages",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let count = usize::try_from(count).unwrap_or(usize::MAX);
    let bytes = usize::try_from(bytes).unwrap_or(usize::MAX);
    Ok(count < max_messages && bytes.saturating_add(len) <= max_bytes)
}

impl Store for SqliteStore {
    fn insert(&self, message: NewMessage, now: SystemTime) -> Result<(), InsertError> {
        let mut conn = self.lock();
        let result = (|| -> rusqlite::Result<Result<(), InsertError>> {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            // An expired row gives up its id at once (APP.md invariant 8).
            tx.execute(
                "DELETE FROM messages WHERE id = ?1 AND expires_at <= ?2",
                params![message.id, nanos(now)],
            )?;
            let taken: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM messages WHERE id = ?1)",
                [&message.id],
                |r| r.get(0),
            )?;
            if taken {
                return Ok(Err(InsertError::IdTaken(message.blob)));
            }
            let len = message.blob.len();
            if !admits(&tx, len, self.max_messages, self.max_bytes)? {
                sweep_in(&tx, now)?;
                if !admits(&tx, len, self.max_messages, self.max_bytes)? {
                    // Keep the sweep even though the insert is refused.
                    tx.commit()?;
                    return Ok(Err(InsertError::Full));
                }
            }
            tx.execute(
                "INSERT INTO messages (id, blob, size, pin_salt, pin_verifier, expires_at, protocol_version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    message.id,
                    message.blob,
                    i64::try_from(len).unwrap_or(i64::MAX),
                    message.pin_salt,
                    message.pin_verifier,
                    nanos(message.expires_at),
                    message.protocol_version,
                ],
            )?;
            if self.fault("insert.before_commit").is_err() {
                return Err(rusqlite::Error::InvalidQuery);
            }
            tx.commit()?;
            Ok(Ok(()))
        })();
        match result {
            Ok(Ok(())) => self
                .fault("insert.after_commit")
                .map_err(|_| InsertError::Storage),
            Ok(Err(refused)) => Err(refused),
            Err(_) => Err(InsertError::Storage),
        }
    }

    fn meta(&self, id: &str) -> Result<Option<MessageMeta>, StoreError> {
        self.fault("meta")?;
        self.lock()
            .query_row(
                "SELECT generation, pin_salt, pin_verifier, expires_at, failed_attempts
                 FROM messages WHERE id = ?1",
                [id],
                |r| {
                    Ok(MessageMeta {
                        generation: u64::try_from(r.get::<_, i64>(0)?).unwrap_or(0),
                        pin_salt: r.get(1)?,
                        pin_verifier: r.get(2)?,
                        expires_at: from_nanos(r.get(3)?),
                        failed_attempts: u32::try_from(r.get::<_, i64>(4)?).unwrap_or(u32::MAX),
                    })
                },
            )
            .optional()
            .map_err(storage)
    }

    fn record_failure(
        &self,
        id: &str,
        generation: u64,
        max_attempts: u32,
    ) -> Result<Option<Attempt>, StoreError> {
        let generation = i64::try_from(generation).map_err(|_| StoreError)?;
        let mut conn = self.lock();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let failures: Option<i64> = tx
            .query_row(
                "UPDATE messages SET failed_attempts = failed_attempts + 1
                 WHERE id = ?1 AND generation = ?2 RETURNING failed_attempts",
                params![id, generation],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        let Some(failures) = failures else {
            return Ok(None);
        };
        let failures = u32::try_from(failures).unwrap_or(u32::MAX);
        let outcome = if failures >= max_attempts {
            tx.execute(
                "DELETE FROM messages WHERE id = ?1 AND generation = ?2",
                params![id, generation],
            )
            .map_err(storage)?;
            Attempt::Burned
        } else {
            Attempt::Counted(failures)
        };
        self.fault("attempt.before_commit")?;
        tx.commit().map_err(storage)?;
        self.fault("attempt.after_commit")?;
        Ok(Some(outcome))
    }

    fn consume(&self, id: &str, generation: u64) -> Result<Option<Vec<u8>>, StoreError> {
        let generation = i64::try_from(generation).map_err(|_| StoreError)?;
        let mut conn = self.lock();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let blob: Option<Vec<u8>> = tx
            .query_row(
                "DELETE FROM messages WHERE id = ?1 AND generation = ?2 RETURNING blob",
                params![id, generation],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        self.fault("consume.before_commit")?;
        tx.commit().map_err(storage)?;
        // The row is gone from here on; a failure now must not release the bytes.
        self.fault("consume.after_commit")?;
        Ok(blob)
    }

    fn remove(&self, id: &str, generation: u64) -> Result<bool, StoreError> {
        let generation = i64::try_from(generation).map_err(|_| StoreError)?;
        let removed = self
            .lock()
            .execute(
                "DELETE FROM messages WHERE id = ?1 AND generation = ?2",
                params![id, generation],
            )
            .map_err(storage)?;
        Ok(removed > 0)
    }

    fn sweep(&self, now: SystemTime) -> Result<usize, StoreError> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(storage)?;
        let removed = sweep_in(&tx, now).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(removed)
    }

    fn maintain(&self) -> Result<(), StoreError> {
        // Moves committed pages into the database and truncates the WAL, so it can't grow
        // without bound between automatic checkpoints.
        self.lock()
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .map_err(storage)
    }
}

#[cfg(test)]
mod tests;
