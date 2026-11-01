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
/// After a checkpoint resets the WAL, its file is truncated to at most this size. It doesn't
/// cap a WAL that a long-lived reader keeps from resetting.
const JOURNAL_SIZE_LIMIT: i64 = 64 * 1024 * 1024;
/// How long a statement waits for a lock another connection holds before failing (503).
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(2);

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

/// A time as Unix nanoseconds, or `None` outside 1970 to 2262 (what an `i64` holds). Times
/// are never clamped: an unrepresentable time is an error, not a different deadline.
fn nanos(time: SystemTime) -> Option<i64> {
    let since = time.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    i64::try_from(since.as_nanos()).ok()
}

/// The inverse of [`nanos`]; a negative stored value is a corrupt row.
fn from_nanos(value: i64) -> Option<SystemTime> {
    Some(SystemTime::UNIX_EPOCH + Duration::from_nanos(u64::try_from(value).ok()?))
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
        // One database, one lock, whatever name it is reached by.
        let resolved = identity(path).map_err(io)?;
        let mut lock_path = resolved.as_os_str().to_owned();
        lock_path.push(".lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(|e| io(format!("lock file: {e}")))?;
        lock.try_lock()
            .map_err(|_| OpenError::Locked(path.to_path_buf()))?;

        let conn = Connection::open(&resolved).map_err(|e| io(e.to_string()))?;
        // Only this process writes, but another program (a backup tool) may hold a read
        // snapshot or a lock for a moment: wait up to this long, then answer 503.
        conn.busy_timeout(BUSY_TIMEOUT)
            .map_err(|e| io(e.to_string()))?;
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

        prepare(&conn, path, pepper)?;

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

/// The resolved path `SQLite` and the lock use. A symlink (or Windows junction) is resolved,
/// so every name for the database maps to one lock. Hard links can't be told apart by name,
/// so a database file with more than one link is refused, on every platform.
fn identity(path: &Path) -> Result<PathBuf, String> {
    let resolved = match std::fs::symlink_metadata(path) {
        Ok(_) => {
            std::fs::canonicalize(path).map_err(|e| format!("cannot resolve the path: {e}"))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = match path.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => parent,
                _ => Path::new("."),
            };
            let name = path.file_name().ok_or("the path has no file name")?;
            std::fs::canonicalize(parent)
                .map_err(|e| format!("cannot resolve the directory: {e}"))?
                .join(name)
        }
        Err(e) => return Err(e.to_string()),
    };
    if resolved.exists()
        && pinveil_fs::link_count(&resolved)
            .map_err(|e| format!("cannot inspect the database file: {e}"))?
            > 1
    {
        return Err("the database file has other hard links; give it a single name".into());
    }
    Ok(strip_verbatim(resolved))
}

/// `canonicalize` on Windows returns a `\\?\C:\...` path; a plain drive path is used instead,
/// which `SQLite` and messages handle as usual.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\")
            && rest.as_bytes().get(1) == Some(&b':')
        {
            return PathBuf::from(rest);
        }
    }
    path
}

/// The schema objects (tables, indexes, triggers, views), by type and name, with their SQL
/// whitespace-normalized. `SQLite`'s own objects are left out.
fn schema_objects(conn: &Connection) -> rusqlite::Result<Vec<(String, String, String)>> {
    conn.prepare(
        // GLOB, not LIKE: in LIKE `_` is a wildcard, which would also hide `sqliteevil`.
        "SELECT type, name, sql FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*' ORDER BY type, name",
    )?
    .query_map([], |r| {
        let sql: Option<String> = r.get(2)?;
        Ok((
            r.get(0)?,
            r.get(1)?,
            sql.unwrap_or_default()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        ))
    })?
    .collect()
}

/// Initializes an empty database (schema, version, and pepper fingerprint in one
/// transaction), or checks that an existing one is exactly a version-`SCHEMA_VERSION`
/// pinveil store made with this pepper. Nothing is ever filled in for an existing store.
fn prepare(conn: &Connection, path: &Path, pepper: &str) -> Result<(), OpenError> {
    let io = |why: String| OpenError::Io {
        path: path.to_path_buf(),
        why,
    };
    let incompatible = |why: &str| incompatible(path, why);
    let fingerprint = pepper_fingerprint(pepper);
    let objects = schema_objects(conn).map_err(|e| incompatible(&e.to_string()))?;
    if objects.is_empty() {
        conn.execute_batch(&format!(
            "BEGIN IMMEDIATE; {SCHEMA}
             INSERT INTO meta (key, value) VALUES ('schema_version', '{SCHEMA_VERSION}');
             INSERT INTO meta (key, value) VALUES ('pepper_fingerprint', '{fingerprint}');
             COMMIT;"
        ))
        .map_err(|e| io(e.to_string()))?;
        return Ok(());
    }

    let meta_value = |key: &str| -> Option<String> {
        conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .ok()
    };
    let version = meta_value("schema_version");
    if version.as_deref() != Some(SCHEMA_VERSION) {
        return Err(incompatible(&format!(
            "schema version {version:?}, expected {SCHEMA_VERSION}"
        )));
    }
    let expected = Connection::open_in_memory()
        .and_then(|memory| {
            memory.execute_batch(SCHEMA)?;
            schema_objects(&memory)
        })
        .map_err(|e| io(e.to_string()))?;
    if objects != expected {
        return Err(incompatible(
            "its tables, columns, constraints, or indexes differ from a version 1 store",
        ));
    }
    match meta_value("pepper_fingerprint") {
        Some(stored) if stored == fingerprint => Ok(()),
        Some(stored) if stored.len() == 64 && stored.bytes().all(|b| b.is_ascii_hexdigit()) => {
            Err(OpenError::PepperMismatch(path.to_path_buf()))
        }
        // A store missing its fingerprint is never adopted under the current pepper: its
        // messages may have been stored under another one.
        _ => Err(incompatible("its pepper fingerprint is missing or damaged")),
    }
}

fn sweep_in(tx: &rusqlite::Transaction<'_>, now: i64) -> rusqlite::Result<usize> {
    tx.execute("DELETE FROM messages WHERE expires_at <= ?1", [now])
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
        let (Some(now), Some(expires_at)) = (nanos(now), nanos(message.expires_at)) else {
            return Err(InsertError::Storage);
        };
        let mut conn = self.lock();
        let result = (|| -> rusqlite::Result<Result<(), InsertError>> {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            // An expired row gives up its id at once (APP.md invariant 8).
            tx.execute(
                "DELETE FROM messages WHERE id = ?1 AND expires_at <= ?2",
                params![message.id, now],
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
                    expires_at,
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
                        expires_at: from_nanos(r.get(3)?)
                            .ok_or(rusqlite::Error::IntegralValueOutOfRange(3, -1))?,
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
        let now = nanos(now).ok_or(StoreError)?;
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(storage)?;
        let removed = sweep_in(&tx, now).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(removed)
    }

    fn maintain(&self) -> Result<(), StoreError> {
        // Moves committed pages into the database and truncates the WAL. A checkpoint that a
        // reader blocks returns a normal row with `busy` set, not an error: report it, so the
        // caller knows the WAL was not truncated.
        let busy: i64 = self
            .lock()
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
            .map_err(storage)?;
        if busy == 0 { Ok(()) } else { Err(StoreError) }
    }
}

#[cfg(test)]
mod tests;
