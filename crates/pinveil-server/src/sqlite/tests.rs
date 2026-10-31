use std::sync::Arc;
use std::time::{Duration, SystemTime};

use rusqlite::Connection;

use super::{OpenError, SqliteStore};
use crate::store::{Attempt, InsertError, NewMessage, Store};

fn t(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + secs)
}

fn message(id: &str, len: usize, expires_at: SystemTime) -> NewMessage {
    NewMessage {
        id: id.into(),
        blob: vec![7; len],
        pin_salt: "salt".into(),
        pin_verifier: "verifier".into(),
        expires_at,
        protocol_version: 2,
    }
}

fn open(dir: &tempfile::TempDir) -> SqliteStore {
    SqliteStore::open(&dir.path().join("store.db"), "pepper", 10, 10_000).unwrap()
}

#[test]
fn sql_schema_version_and_protocol_stamp_are_written() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    store.insert(message("a", 3, t(100)), t(0)).unwrap();
    drop(store);
    let conn = Connection::open(dir.path().join("store.db")).unwrap();
    let version: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(version, "1");
    let (protocol, size): (i64, i64) = conn
        .query_row("SELECT protocol_version, size FROM messages", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((protocol, size), (2, 3));
}

#[test]
fn inv_5_sqlite_row_has_no_pin() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let mut m = message("a", 3, t(100));
    m.pin_verifier = "$argon2id$v=19$m=19456,t=3,p=1$c2FsdA$hash".into();
    store.insert(m, t(0)).unwrap();
    drop(store);
    // The raw database file holds the verifier, never anything PIN-shaped from the request.
    let conn = Connection::open(dir.path().join("store.db")).unwrap();
    let columns: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('messages')")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!columns.iter().any(|c| c == "pin"), "{columns:?}");
}

#[test]
fn sql_pragmas_wal_and_full_sync_are_verified() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let conn = store.lock();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    let sync: i64 = conn
        .query_row("PRAGMA synchronous", [], |r| r.get(0))
        .unwrap();
    let limit: i64 = conn
        .query_row("PRAGMA journal_size_limit", [], |r| r.get(0))
        .unwrap();
    assert_eq!((mode.as_str(), sync, limit), ("wal", 2, 64 * 1024 * 1024));
    drop(conn);
    store.maintain().unwrap();
}

#[test]
fn sql_lock_refuses_a_second_store_on_the_same_file() {
    let dir = tempfile::tempdir().unwrap();
    let first = open(&dir);
    let second = SqliteStore::open(&dir.path().join("store.db"), "pepper", 10, 10_000);
    assert!(matches!(second, Err(OpenError::Locked(_))));
    drop(first);
    open(&dir);
}

#[test]
fn sql_pepper_mismatch_refuses_to_open() {
    let dir = tempfile::tempdir().unwrap();
    drop(open(&dir));
    let other = SqliteStore::open(&dir.path().join("store.db"), "another", 10, 10_000);
    assert!(matches!(other, Err(OpenError::PepperMismatch(_))));
    // The fingerprint never contains the pepper itself.
    let conn = Connection::open(dir.path().join("store.db")).unwrap();
    let stored: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'pepper_fingerprint'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored.len(), 64);
    assert!(!stored.contains("pepper"));
}

#[test]
fn sql_schema_incompatible_databases_are_refused() {
    for setup in [
        "CREATE TABLE other (x);",
        "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE messages (generation INTEGER PRIMARY KEY, id TEXT);
         INSERT INTO meta VALUES ('schema_version', '1');",
        "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE messages (generation INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE,
           blob BLOB NOT NULL, size INTEGER NOT NULL, pin_salt TEXT NOT NULL, pin_verifier TEXT NOT NULL,
           expires_at INTEGER NOT NULL, failed_attempts INTEGER NOT NULL DEFAULT 0,
           protocol_version INTEGER NOT NULL);
         INSERT INTO meta VALUES ('schema_version', '99');",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.db");
        Connection::open(&path).unwrap().execute_batch(setup).unwrap();
        let opened = SqliteStore::open(&path, "pepper", 10, 10_000);
        assert!(matches!(opened, Err(OpenError::Incompatible { .. })), "{setup}");
    }
    // Not a database at all.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    std::fs::write(&path, b"not a database, just some text that is long enough").unwrap();
    assert!(SqliteStore::open(&path, "pepper", 10, 10_000).is_err());
}

#[test]
fn sql_store_contract_generations_attempts_burn_consume() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    store.insert(message("a", 3, t(100)), t(0)).unwrap();
    assert!(matches!(
        store.insert(message("a", 3, t(100)), t(0)),
        Err(InsertError::IdTaken(blob)) if blob.len() == 3
    ));
    let meta = store.meta("a").unwrap().unwrap();
    assert_eq!(meta.failed_attempts, 0);
    assert_eq!(meta.expires_at, t(100));
    let g = meta.generation;
    assert_eq!(
        store.record_failure("a", g, 3).unwrap(),
        Some(Attempt::Counted(1))
    );
    assert_eq!(
        store.record_failure("a", g + 1, 3).unwrap(),
        None,
        "wrong generation"
    );
    assert_eq!(
        store.record_failure("a", g, 3).unwrap(),
        Some(Attempt::Counted(2))
    );
    assert_eq!(
        store.record_failure("a", g, 3).unwrap(),
        Some(Attempt::Burned)
    );
    assert_eq!(store.meta("a").unwrap(), None);
    assert_eq!(
        store.record_failure("a", g, 3).unwrap(),
        None,
        "never recreated"
    );

    store.insert(message("b", 4, t(100)), t(0)).unwrap();
    let g = store.meta("b").unwrap().unwrap().generation;
    assert_eq!(store.consume("b", g + 1).unwrap(), None);
    assert_eq!(store.consume("b", g).unwrap(), Some(vec![7; 4]));
    assert_eq!(store.consume("b", g).unwrap(), None, "at most once");
    assert_eq!(store.usage().unwrap(), (0, 0));
}

#[test]
fn sql_caps_count_bytes_sweep_before_refusing_and_reclaim_expired_ids() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteStore::open(&dir.path().join("store.db"), "p", 2, 100).unwrap();
    store.insert(message("a", 60, t(10)), t(0)).unwrap();
    assert!(matches!(
        store.insert(message("b", 50, t(100)), t(0)),
        Err(InsertError::Full)
    ));
    store.insert(message("b", 40, t(100)), t(0)).unwrap();
    assert!(matches!(
        store.insert(message("c", 1, t(100)), t(0)),
        Err(InsertError::Full)
    ));
    // Once "a" expired, a sweep before refusing frees room.
    store.insert(message("c", 1, t(100)), t(10)).unwrap();
    assert_eq!(store.usage().unwrap(), (2, 41));
    // An expired row gives up its id at once.
    store.insert(message("x", 1, t(20)), t(10)).unwrap_err();
    store.sweep(t(200)).unwrap();
    store.insert(message("x", 1, t(300)), t(200)).unwrap();
    store.insert(message("y", 1, t(250)), t(200)).unwrap();
    store.insert(message("y", 1, t(400)), t(250)).unwrap();
}

#[test]
fn sql_faults_before_commit_roll_back_and_after_commit_never_release_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);

    store.fail_at("insert.before_commit");
    assert!(matches!(
        store.insert(message("a", 3, t(100)), t(0)),
        Err(InsertError::Storage)
    ));
    assert_eq!(store.meta("a").unwrap(), None, "rolled back");
    store.fail_at("insert.after_commit");
    assert!(matches!(
        store.insert(message("a", 3, t(100)), t(0)),
        Err(InsertError::Storage)
    ));
    assert!(
        store.meta("a").unwrap().is_some(),
        "committed before the failure"
    );

    let g = store.meta("a").unwrap().unwrap().generation;
    store.fail_at("attempt.before_commit");
    assert!(store.record_failure("a", g, 3).is_err());
    assert_eq!(
        store.meta("a").unwrap().unwrap().failed_attempts,
        0,
        "not counted"
    );
    store.fail_at("attempt.after_commit");
    assert!(store.record_failure("a", g, 3).is_err());
    assert_eq!(
        store.meta("a").unwrap().unwrap().failed_attempts,
        1,
        "counted"
    );

    store.fail_at("consume.before_commit");
    assert!(store.consume("a", g).is_err());
    assert!(store.meta("a").unwrap().is_some(), "still there");
    store.fail_at("consume.after_commit");
    assert!(store.consume("a", g).is_err(), "no bytes come back");
    assert_eq!(
        store.meta("a").unwrap(),
        None,
        "consumed: at most once, even unsent"
    );

    // A burn that fails before its commit leaves the attempt uncounted.
    store.insert(message("b", 3, t(100)), t(0)).unwrap();
    let g = store.meta("b").unwrap().unwrap().generation;
    store.record_failure("b", g, 2).unwrap();
    store.fail_at("attempt.before_commit");
    assert!(store.record_failure("b", g, 2).is_err());
    assert_eq!(store.meta("b").unwrap().unwrap().failed_attempts, 1);
    assert_eq!(
        store.record_failure("b", g, 2).unwrap(),
        Some(Attempt::Burned)
    );
}

#[test]
fn sql_races_sweeper_and_attempts_never_recreate_a_row() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(open(&dir));
    for round in 0..50u64 {
        let id = format!("m{round}");
        store.insert(message(&id, 3, t(10)), t(0)).unwrap();
        let g = store.meta(&id).unwrap().unwrap().generation;
        let sweeper = {
            let store = Arc::clone(&store);
            std::thread::spawn(move || store.sweep(t(10)).unwrap())
        };
        let attempts: Vec<_> = (0..3)
            .map(|_| {
                let store = Arc::clone(&store);
                let id = id.clone();
                std::thread::spawn(move || store.record_failure(&id, g, 5).unwrap())
            })
            .collect();
        sweeper.join().unwrap();
        for attempt in attempts {
            attempt.join().unwrap();
        }
        // Whatever the interleaving, once swept the row stays gone.
        store.sweep(t(10)).unwrap();
        assert_eq!(store.meta(&id).unwrap(), None);
        assert_eq!(store.record_failure(&id, g, 5).unwrap(), None);
        assert_eq!(store.consume(&id, g).unwrap(), None);
    }
    assert_eq!(store.usage().unwrap(), (0, 0));
}

#[test]
fn inv_10_sqlite_rows_survive_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    store.insert(message("kept", 3, t(100)), t(0)).unwrap();
    let g = store.meta("kept").unwrap().unwrap().generation;
    store.record_failure("kept", g, 3).unwrap();
    drop(store);
    let store = open(&dir);
    let meta = store.meta("kept").unwrap().unwrap();
    assert_eq!((meta.generation, meta.failed_attempts), (g, 1));
    // Generations are never reused, even after the newest row is deleted and reopened.
    store.consume("kept", g).unwrap();
    drop(store);
    let store = open(&dir);
    store.insert(message("next", 3, t(100)), t(0)).unwrap();
    assert!(store.meta("next").unwrap().unwrap().generation > g);
}

#[test]
fn sql_schema_same_names_wrong_constraints_are_refused() {
    // Exactly the real schema with AUTOINCREMENT removed: generations could be reused.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let weakened = super::SCHEMA.replace(" AUTOINCREMENT", "");
    assert_ne!(weakened, super::SCHEMA);
    Connection::open(&path)
        .unwrap()
        .execute_batch(&format!(
            "{weakened} INSERT INTO meta VALUES ('schema_version', '1');
             INSERT INTO meta VALUES ('pepper_fingerprint', '{}');",
            super::pepper_fingerprint("pepper")
        ))
        .unwrap();
    assert!(matches!(
        SqliteStore::open(&path, "pepper", 10, 10_000),
        Err(OpenError::Incompatible { .. })
    ));
    // An extra trigger is refused too.
    let dir = tempfile::tempdir().unwrap();
    drop(open(&dir));
    Connection::open(dir.path().join("store.db"))
        .unwrap()
        .execute_batch("CREATE TRIGGER t AFTER DELETE ON messages BEGIN SELECT 1; END;")
        .unwrap();
    assert!(matches!(
        SqliteStore::open(&dir.path().join("store.db"), "pepper", 10, 10_000),
        Err(OpenError::Incompatible { .. })
    ));
}

#[test]
fn sql_pepper_missing_fingerprint_in_a_populated_store_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let store = SqliteStore::open(&path, "old-pepper", 10, 10_000).unwrap();
    store.insert(message("a", 3, t(100)), t(0)).unwrap();
    drop(store);
    Connection::open(&path)
        .unwrap()
        .execute_batch("DELETE FROM meta WHERE key = 'pepper_fingerprint'")
        .unwrap();
    let opened = SqliteStore::open(&path, "new-pepper", 10, 10_000);
    assert!(matches!(opened, Err(OpenError::Incompatible { .. })));
    // Nothing was adopted or rewritten.
    let conn = Connection::open(&path).unwrap();
    let fingerprints: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM meta WHERE key = 'pepper_fingerprint'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fingerprints, 0);
}

#[cfg(unix)]
#[test]
fn sql_lock_aliases_of_one_database_share_one_lock() {
    let dir = tempfile::tempdir().unwrap();
    let first = open(&dir);
    let alias = dir.path().join("alias.db");
    std::os::unix::fs::symlink(dir.path().join("store.db"), &alias).unwrap();
    assert!(matches!(
        SqliteStore::open(&alias, "pepper", 10, 10_000),
        Err(OpenError::Locked(_))
    ));
    drop(first);
    // A hard link can't be told apart by name, so it is refused outright.
    let linked = dir.path().join("linked.db");
    std::fs::hard_link(dir.path().join("store.db"), &linked).unwrap();
    assert!(matches!(
        SqliteStore::open(&linked, "pepper", 10, 10_000),
        Err(OpenError::Io { .. })
    ));
}

#[test]
fn sql_busy_another_writer_turns_into_a_storage_error_after_the_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let other = Connection::open(dir.path().join("store.db")).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = std::time::Instant::now();
    assert!(matches!(
        store.insert(message("a", 3, t(100)), t(0)),
        Err(InsertError::Storage)
    ));
    assert!(
        started.elapsed() >= super::BUSY_TIMEOUT,
        "it waited before failing"
    );
    other.execute_batch("ROLLBACK").unwrap();
    store.insert(message("a", 3, t(100)), t(0)).unwrap();
}

#[test]
fn sql_pragmas_a_blocked_checkpoint_is_reported_then_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    store.insert(message("a", 3, t(100)), t(0)).unwrap();
    let reader = Connection::open(dir.path().join("store.db")).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let _: i64 = reader
        .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    store.insert(message("b", 3, t(100)), t(0)).unwrap();
    assert!(
        store.maintain().is_err(),
        "the reader keeps the WAL from truncating"
    );
    reader.execute_batch("COMMIT").unwrap();
    store.maintain().unwrap();
}

#[test]
fn sql_expiry_times_are_checked_not_clamped() {
    use super::{from_nanos, nanos};
    assert_eq!(nanos(SystemTime::UNIX_EPOCH), Some(0));
    let latest = SystemTime::UNIX_EPOCH + Duration::from_nanos(u64::try_from(i64::MAX).unwrap());
    assert_eq!(nanos(latest), Some(i64::MAX));
    assert_eq!(from_nanos(i64::MAX), Some(latest));
    assert_eq!(nanos(latest + Duration::from_nanos(1)), None);
    assert_eq!(nanos(SystemTime::UNIX_EPOCH - Duration::from_secs(1)), None);
    assert_eq!(from_nanos(-1), None);
    let now = t(5);
    assert_eq!(from_nanos(nanos(now).unwrap()), Some(now));
    // An unrepresentable expiry is a clean refusal, not a stored, changed deadline.
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    assert!(matches!(
        store.insert(message("far", 3, latest + Duration::from_secs(1)), t(0)),
        Err(InsertError::Storage)
    ));
    assert_eq!(store.usage().unwrap(), (0, 0));
}
