//! Message storage (APP.md, "Stored record" and "Store modes"). The [`Store`] trait is the
//! seam the service uses; [`MemoryStore`] is the default backend. Every method is a single
//! atomic step, so the service decides the order and the store never recreates a row it
//! no longer has.
//!
//! Each stored row gets a generation number that is never reused. Retrieval verifies the
//! PIN against one row's metadata and then acts with that row's generation, so if the row
//! expired, was swept, and a new message took the same id in the meantime, the stale
//! request finds nothing instead of consuming or burning the new message.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

/// A message about to be stored.
pub struct NewMessage {
    pub id: String,
    pub blob: Vec<u8>,
    pub pin_salt: String,
    pub pin_verifier: String,
    pub expires_at: SystemTime,
    /// The protocol generation of the server that accepted the message, never read from
    /// the blob (APP.md invariant 3).
    pub protocol_version: u32,
}

/// What retrieval needs before deciding: everything except the blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageMeta {
    /// Identifies this stored row; never reused for another row.
    pub generation: u64,
    pub pin_salt: String,
    pub pin_verifier: String,
    pub expires_at: SystemTime,
    pub failed_attempts: u32,
}

impl MessageMeta {
    /// Expired when `expires_at <= now` (APP.md).
    #[must_use]
    pub fn is_expired_at(&self, now: SystemTime) -> bool {
        self.expires_at <= now
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InsertError {
    /// The id is taken. Carries the blob back so the caller can retry with another id
    /// without copying it.
    #[error("a message with this id exists")]
    IdTaken(Vec<u8>),
    #[error("the store is full")]
    Full,
    #[error("storage failed")]
    Storage,
}

/// The storage backend failed. Callers answer 503 and never release blob bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("storage failed")]
pub struct StoreError;

/// The result of recording a wrong PIN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attempt {
    /// Counted; the message survives with this many failures.
    Counted(u32),
    /// The limit was reached and the message is gone.
    Burned,
}

pub trait Store: Send + Sync + 'static {
    /// Stores a message if the id is free and the caps allow it, sweeping expired messages
    /// first when a cap would otherwise refuse it.
    fn insert(&self, message: NewMessage, now: SystemTime) -> Result<(), InsertError>;
    /// The message's metadata, if it is stored.
    fn meta(&self, id: &str) -> Result<Option<MessageMeta>, StoreError>;
    /// Counts a wrong PIN against row `generation`, deleting it at `max_attempts`. `None`
    /// if that row is gone (even if another row now has the id); nothing is recreated.
    fn record_failure(
        &self,
        id: &str,
        generation: u64,
        max_attempts: u32,
    ) -> Result<Option<Attempt>, StoreError>;
    /// Removes row `generation` and returns its blob, if that row is still stored.
    fn consume(&self, id: &str, generation: u64) -> Result<Option<Vec<u8>>, StoreError>;
    /// Removes row `generation`, if that row is still stored.
    fn remove(&self, id: &str, generation: u64) -> Result<bool, StoreError>;
    /// Removes every message expired at `now`; returns how many.
    fn sweep(&self, now: SystemTime) -> Result<usize, StoreError>;
}

struct Entry {
    blob: Vec<u8>,
    meta: MessageMeta,
    #[allow(dead_code)]
    protocol_version: u32,
}

#[derive(Default)]
struct Inner {
    messages: HashMap<String, Entry>,
    bytes: usize,
    next_generation: u64,
}

impl Inner {
    fn take(&mut self, id: &str) -> Option<Entry> {
        let entry = self.messages.remove(id)?;
        self.bytes -= entry.blob.len();
        Some(entry)
    }

    /// Removes the row only if it is still generation `generation`.
    fn take_generation(&mut self, id: &str, generation: u64) -> Option<Entry> {
        let current = self.messages.get(id)?.meta.generation;
        if current == generation {
            self.take(id)
        } else {
            None
        }
    }

    fn sweep(&mut self, now: SystemTime) -> usize {
        let expired: Vec<String> = self
            .messages
            .iter()
            .filter(|(_, e)| e.meta.is_expired_at(now))
            .map(|(id, _)| id.clone())
            .collect();
        for id in &expired {
            self.take(id);
        }
        expired.len()
    }

    fn admits(&self, len: usize, max_messages: usize, max_bytes: usize) -> bool {
        self.messages.len() < max_messages && self.bytes + len <= max_bytes
    }
}

/// The in-memory store. One mutex guards the map and the byte total together; every
/// operation holds it only briefly and never across hashing or I/O.
pub struct MemoryStore {
    inner: Mutex<Inner>,
    max_messages: usize,
    max_bytes: usize,
}

impl MemoryStore {
    #[must_use]
    pub fn new(max_messages: usize, max_bytes: usize) -> Self {
        Self {
            inner: Mutex::default(),
            max_messages,
            max_bytes,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A panic while holding the lock can't leave the map half-updated: every mutation
        // is a single insert or remove plus its byte adjustment.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Stored message count and bytes, for tests.
    #[must_use]
    pub fn usage(&self) -> (usize, usize) {
        let inner = self.lock();
        (inner.messages.len(), inner.bytes)
    }
}

impl Store for MemoryStore {
    fn insert(&self, message: NewMessage, now: SystemTime) -> Result<(), InsertError> {
        let mut inner = self.lock();
        // An expired row is logically gone (APP.md invariant 8); it gives up its id at once
        // instead of holding it until the next sweep.
        if inner
            .messages
            .get(&message.id)
            .is_some_and(|e| e.meta.is_expired_at(now))
        {
            inner.take(&message.id);
        }
        if inner.messages.contains_key(&message.id) {
            return Err(InsertError::IdTaken(message.blob));
        }
        let len = message.blob.len();
        if !inner.admits(len, self.max_messages, self.max_bytes) {
            inner.sweep(now);
            if !inner.admits(len, self.max_messages, self.max_bytes) {
                return Err(InsertError::Full);
            }
        }
        inner.bytes += len;
        inner.next_generation += 1;
        let generation = inner.next_generation;
        let entry = Entry {
            blob: message.blob,
            meta: MessageMeta {
                generation,
                pin_salt: message.pin_salt,
                pin_verifier: message.pin_verifier,
                expires_at: message.expires_at,
                failed_attempts: 0,
            },
            protocol_version: message.protocol_version,
        };
        inner.messages.insert(message.id, entry);
        Ok(())
    }

    fn meta(&self, id: &str) -> Result<Option<MessageMeta>, StoreError> {
        Ok(self.lock().messages.get(id).map(|e| e.meta.clone()))
    }

    fn record_failure(
        &self,
        id: &str,
        generation: u64,
        max_attempts: u32,
    ) -> Result<Option<Attempt>, StoreError> {
        let mut inner = self.lock();
        let Some(entry) = inner
            .messages
            .get_mut(id)
            .filter(|e| e.meta.generation == generation)
        else {
            return Ok(None);
        };
        entry.meta.failed_attempts += 1;
        let failures = entry.meta.failed_attempts;
        if failures >= max_attempts {
            inner.take(id);
            return Ok(Some(Attempt::Burned));
        }
        Ok(Some(Attempt::Counted(failures)))
    }

    fn consume(&self, id: &str, generation: u64) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.lock().take_generation(id, generation).map(|e| e.blob))
    }

    fn remove(&self, id: &str, generation: u64) -> Result<bool, StoreError> {
        Ok(self.lock().take_generation(id, generation).is_some())
    }

    fn sweep(&self, now: SystemTime) -> Result<usize, StoreError> {
        Ok(self.lock().sweep(now))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::{Attempt, InsertError, MemoryStore, NewMessage, Store};

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

    fn t(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn srv_caps_count_bytes_and_reclaim_expired() {
        let store = MemoryStore::new(2, 100);
        store.insert(message("a", 60, t(10)), t(0)).unwrap();
        assert_eq!(
            store.insert(message("a", 1, t(10)), t(0)),
            Err(InsertError::IdTaken(vec![7]))
        );
        assert_eq!(
            store.insert(message("b", 41, t(20)), t(0)),
            Err(InsertError::Full),
            "byte cap"
        );
        store.insert(message("b", 40, t(20)), t(0)).unwrap();
        assert_eq!(
            store.insert(message("c", 1, t(20)), t(0)),
            Err(InsertError::Full),
            "count cap"
        );
        // At t=10, "a" has expired: a full store sweeps before refusing.
        store.insert(message("c", 1, t(20)), t(10)).unwrap();
        assert_eq!(store.usage(), (2, 41));
    }

    #[test]
    fn inv_7_failures_count_then_burn_and_never_recreate() {
        let store = MemoryStore::new(10, 1000);
        store.insert(message("a", 5, t(100)), t(0)).unwrap();
        let g = store.meta("a").unwrap().unwrap().generation;
        assert_eq!(
            store.record_failure("a", g, 3),
            Ok(Some(Attempt::Counted(1)))
        );
        assert_eq!(store.meta("a").unwrap().unwrap().failed_attempts, 1);
        assert_eq!(
            store.record_failure("a", g, 3),
            Ok(Some(Attempt::Counted(2)))
        );
        assert_eq!(store.record_failure("a", g, 3), Ok(Some(Attempt::Burned)));
        assert_eq!(store.meta("a"), Ok(None));
        assert_eq!(
            store.record_failure("a", g, 3),
            Ok(None),
            "a gone row is not recreated"
        );
        assert_eq!(store.usage(), (0, 0));
    }

    #[test]
    fn inv_6_consume_is_at_most_once() {
        let store = MemoryStore::new(10, 1000);
        store.insert(message("a", 5, t(100)), t(0)).unwrap();
        let g = store.meta("a").unwrap().unwrap().generation;
        assert_eq!(store.consume("a", g), Ok(Some(vec![7; 5])));
        assert_eq!(store.consume("a", g), Ok(None));
        assert_eq!(store.usage(), (0, 0));
    }

    #[test]
    fn inv_8_sweep_removes_only_expired_at_the_boundary() {
        let store = MemoryStore::new(10, 1000);
        store.insert(message("a", 1, t(10)), t(0)).unwrap();
        store.insert(message("b", 1, t(11)), t(0)).unwrap();
        assert_eq!(store.sweep(t(9)), Ok(0));
        assert_eq!(store.sweep(t(10)), Ok(1), "expires_at <= now is expired");
        assert!(store.meta("a").unwrap().is_none() && store.meta("b").unwrap().is_some());
    }

    #[test]
    fn inv_6_stale_generation_never_touches_a_reused_id() {
        let store = MemoryStore::new(10, 1000);
        store.insert(message("a", 5, t(10)), t(0)).unwrap();
        let old = store.meta("a").unwrap().unwrap().generation;
        assert_eq!(store.sweep(t(10)), Ok(1));
        store.insert(message("a", 3, t(100)), t(10)).unwrap();
        let new = store.meta("a").unwrap().unwrap().generation;
        assert_ne!(old, new);
        assert_eq!(store.record_failure("a", old, 1), Ok(None));
        assert_eq!(store.consume("a", old), Ok(None));
        assert_eq!(store.remove("a", old), Ok(false));
        assert_eq!(
            store.meta("a").unwrap().unwrap().failed_attempts,
            0,
            "the new row is untouched"
        );
        assert_eq!(store.consume("a", new), Ok(Some(vec![7; 3])));
    }
}
