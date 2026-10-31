//! The message service: create and retrieve, with every APP.md lifecycle rule.
//!
//! - Retrievals of one id are serialized through a fixed set of striped locks; different ids
//!   run in parallel. The lock count is fixed, so no per-id state can grow without bound.
//! - Argon2 runs on [`HashPool`]: a fixed number of permits plus a bounded wait queue.
//! - Retrieval runs in its own task, so a client that disconnects can't skip the commit of
//!   its own failed attempt.
//! - Each consume, failed attempt, and burn is committed before the response.

use std::hash::{BuildHasher, RandomState};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use pinveil_core::entropy::{Entropy, EntropyError, SystemEntropy};
use pinveil_core::generate::{is_valid_pin, message_id};
use pinveil_core::protocol::PROTOCOL_VERSION;
use pinveil_core::wordlist::Wordlist;
use tokio::sync::{Mutex, MutexGuard, Semaphore};

use crate::config::Config;
use crate::error::ApiError;
use crate::pin::PinVerifier;
use crate::store::{InsertError, NewMessage, Store, StoreError};

/// Message ids tried before giving up on collisions (APP.md, "Message ids").
pub const MAX_ID_ATTEMPTS: usize = 10;
const LOCK_STRIPES: usize = 1024;
const PIN_SALT_BYTES: usize = 16;

/// The current time. Tests use a controllable clock.
pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> SystemTime;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Where message ids come from. Tests use colliding or fixed sources.
pub trait IdSource: Send + Sync + 'static {
    fn next_id(&self) -> Result<String, EntropyError>;
}

/// Diceware ids from the validated wordlist and the system CSPRNG.
pub struct WordIds {
    wordlist: Wordlist<'static>,
    words: usize,
    digits: u32,
}

impl WordIds {
    #[must_use]
    pub fn new(wordlist: Wordlist<'static>, words: usize, digits: u32) -> Self {
        Self {
            wordlist,
            words,
            digits,
        }
    }
}

impl IdSource for WordIds {
    fn next_id(&self) -> Result<String, EntropyError> {
        message_id(&self.wordlist, &mut SystemEntropy, self.words, self.digits)
    }
}

/// Runs blocking hash work with bounded concurrency and a bounded queue (APP.md,
/// "Concurrency and resources").
pub struct HashPool {
    permits: Arc<Semaphore>,
    waiting: AtomicUsize,
    concurrency: usize,
    max_waiting: usize,
}

/// Decrements the waiting count even if the waiting future is dropped.
struct Waiting<'a>(&'a AtomicUsize);

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl HashPool {
    #[must_use]
    pub fn new(concurrency: usize, max_waiting: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(concurrency)),
            waiting: AtomicUsize::new(0),
            concurrency,
            max_waiting,
        }
    }

    /// Runs `work` on the blocking pool once a permit is free. Refuses with 503 when the
    /// wait queue is full instead of letting it grow.
    pub async fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, ApiError> {
        let permit = {
            let ahead = self.waiting.fetch_add(1, Ordering::SeqCst);
            let _waiting = Waiting(&self.waiting);
            if ahead >= self.max_waiting {
                return Err(ApiError::ServiceUnavailable(
                    "The server is busy, try again shortly",
                ));
            }
            Arc::clone(&self.permits)
                .acquire_owned()
                .await
                .map_err(|_| ApiError::Internal)?
        };
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            work()
        })
        .await
        .map_err(|_| ApiError::Internal)
    }
}

struct Stripes {
    locks: Vec<Mutex<()>>,
    hasher: RandomState,
}

impl Stripes {
    fn new() -> Self {
        Self {
            locks: (0..LOCK_STRIPES).map(|_| Mutex::new(())).collect(),
            hasher: RandomState::new(),
        }
    }

    fn index(&self, id: &str) -> usize {
        usize::try_from(self.hasher.hash_one(id)).unwrap_or(0) % self.locks.len()
    }

    async fn lock(&self, id: &str) -> MutexGuard<'_, ()> {
        self.locks[self.index(id)].lock().await
    }
}

/// The limits the service enforces.
#[derive(Debug, Clone)]
pub struct Policy {
    pub min_ttl: Duration,
    pub max_ttl: Duration,
    pub max_payload_bytes: usize,
    pub max_failed_attempts: u32,
}

impl Policy {
    #[must_use]
    pub fn from_config(config: &Config) -> Self {
        Self {
            min_ttl: config.messages.min_ttl,
            max_ttl: config.messages.max_ttl,
            max_payload_bytes: config.messages.max_payload_bytes,
            max_failed_attempts: config.pin.max_failed_attempts,
        }
    }
}

/// A created message. `expires_at` is already formatted as RFC 3339 UTC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    pub id: String,
    pub expires_at: String,
}

/// RFC 3339 UTC with whole seconds, or `None` if the time can't be written (past 9999).
#[must_use]
pub fn rfc3339(time: SystemTime) -> Option<String> {
    use std::fmt::Write;
    let mut out = String::new();
    write!(out, "{}", humantime::format_rfc3339_seconds(time)).ok()?;
    Some(out)
}

pub struct Service {
    store: Arc<dyn Store>,
    verifier: Arc<dyn PinVerifier>,
    pool: HashPool,
    /// Retrievals admitted at once: those hashing plus those queued, whether they wait for
    /// a hash permit or for their id's lock. Taken before the retrieval task is spawned.
    admission: Arc<Semaphore>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdSource>,
    stripes: Stripes,
    policy: Policy,
}

pub struct Parts {
    pub store: Arc<dyn Store>,
    pub verifier: Arc<dyn PinVerifier>,
    pub pool: HashPool,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdSource>,
    pub policy: Policy,
}

fn storage_unavailable(_: StoreError) -> ApiError {
    ApiError::ServiceUnavailable("Storage is unavailable")
}

impl Service {
    #[must_use]
    pub fn new(parts: Parts) -> Arc<Self> {
        let admitted = parts.pool.concurrency + parts.pool.max_waiting;
        Arc::new(Self {
            store: parts.store,
            verifier: parts.verifier,
            admission: Arc::new(Semaphore::new(admitted)),
            pool: parts.pool,
            clock: parts.clock,
            ids: parts.ids,
            stripes: Stripes::new(),
            policy: parts.policy,
        })
    }

    /// Validates and stores a new message.
    pub async fn create(&self, blob: Vec<u8>, pin: String, ttl: &str) -> Result<Created, ApiError> {
        if !is_valid_pin(&pin) {
            return Err(ApiError::InvalidPin);
        }
        let ttl = ttl
            .parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| ApiError::BadRequest("TTL must be a whole number of seconds"))?;
        if ttl < self.policy.min_ttl || ttl > self.policy.max_ttl {
            return Err(ApiError::BadRequest("TTL is outside the allowed range"));
        }
        if blob.is_empty() {
            return Err(ApiError::BadRequest("The blob is empty"));
        }
        if blob.len() > self.policy.max_payload_bytes {
            return Err(ApiError::PayloadTooLarge);
        }

        let mut salt_bytes = [0u8; PIN_SALT_BYTES];
        SystemEntropy
            .fill(&mut salt_bytes)
            .map_err(|_| ApiError::Internal)?;
        let pin_salt = STANDARD.encode(salt_bytes);
        let verifier = Arc::clone(&self.verifier);
        let salt = pin_salt.clone();
        let pin_verifier = self.pool.run(move || verifier.hash(&pin, &salt)).await?;

        // Compute and format the expiry before anything is stored, so an impossible time is
        // a refusal and never a failure after the commit.
        let now = self.clock.now();
        let expires_at = now
            .checked_add(ttl)
            .ok_or(ApiError::BadRequest("TTL is outside the allowed range"))?;
        let expires_at_text =
            rfc3339(expires_at).ok_or(ApiError::BadRequest("TTL is outside the allowed range"))?;
        let mut blob = Some(blob);
        for _ in 0..MAX_ID_ATTEMPTS {
            let id = self.ids.next_id().map_err(|_| ApiError::Internal)?;
            let message = NewMessage {
                id: id.clone(),
                blob: blob.take().expect("blob is returned on IdTaken"),
                pin_salt: pin_salt.clone(),
                pin_verifier: pin_verifier.clone(),
                expires_at,
                protocol_version: PROTOCOL_VERSION,
            };
            match self.store.insert(message, now) {
                Ok(()) => {
                    return Ok(Created {
                        id,
                        expires_at: expires_at_text,
                    });
                }
                Err(InsertError::IdTaken(returned)) => blob = Some(returned),
                Err(InsertError::Full) => {
                    return Err(ApiError::ServiceUnavailable("Server storage is full"));
                }
                Err(InsertError::Storage) => return Err(storage_unavailable(StoreError)),
            }
        }
        Err(ApiError::ServiceUnavailable(
            "Unable to allocate a message id",
        ))
    }

    /// Checks the PIN and consumes the message (APP.md retrieval order). Admission is
    /// bounded before anything waits: a retrieval takes a slot first (503 when none is
    /// free) and holds it in its own task until it finishes, so dropping the returned
    /// future (a client disconnect) never skips a commit and never frees a slot early.
    pub async fn retrieve(self: &Arc<Self>, id: String, pin: String) -> Result<Vec<u8>, ApiError> {
        if !is_valid_pin(&pin) {
            return Err(ApiError::InvalidPin);
        }
        let slot = Arc::clone(&self.admission)
            .try_acquire_owned()
            .map_err(|_| ApiError::ServiceUnavailable("The server is busy, try again shortly"))?;
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let _slot = slot;
            this.retrieve_now(&id, pin).await
        })
        .await
        .map_err(|_| ApiError::Internal)?
    }

    async fn retrieve_now(&self, id: &str, pin: String) -> Result<Vec<u8>, ApiError> {
        let _serialized = self.stripes.lock(id).await;
        let meta = self
            .store
            .meta(id)
            .map_err(storage_unavailable)?
            .ok_or(ApiError::Unavailable)?;
        // Every action below is bound to this row's generation: if the row expires, is
        // swept, and a new message takes the id during verification, the new message is
        // never consumed or charged by this request.
        let generation = meta.generation;
        if meta.is_expired_at(self.clock.now()) {
            self.store
                .remove(id, generation)
                .map_err(storage_unavailable)?;
            return Err(ApiError::Unavailable);
        }
        let verifier = Arc::clone(&self.verifier);
        let matches = self
            .pool
            .run(move || verifier.verify(&pin, &meta.pin_salt, &meta.pin_verifier))
            .await?;
        if !matches {
            return match self
                .store
                .record_failure(id, generation, self.policy.max_failed_attempts)
                .map_err(storage_unavailable)?
            {
                Some(_) => Err(ApiError::WrongPin),
                None => Err(ApiError::Unavailable),
            };
        }
        self.store
            .consume(id, generation)
            .map_err(storage_unavailable)?
            .ok_or(ApiError::Unavailable)
    }

    /// Removes expired messages; the background sweeper calls this every minute.
    pub fn sweep(&self) -> Result<usize, StoreError> {
        self.store.sweep(self.clock.now())
    }

    #[cfg(test)]
    fn stripe_of(&self, id: &str) -> usize {
        self.stripes.index(id)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, SystemTime};

    use pinveil_core::entropy::EntropyError;

    use super::{Clock, HashPool, IdSource, MAX_ID_ATTEMPTS, Parts, Policy, Service};
    use crate::error::ApiError;
    use crate::pin::PinVerifier;
    use crate::store::{
        Attempt, InsertError, MemoryStore, MessageMeta, NewMessage, Store, StoreError,
    };

    const PIN: &str = "123456";

    struct ManualClock(Mutex<SystemTime>);

    impl ManualClock {
        fn at(secs: u64) -> Arc<Self> {
            Arc::new(Self(Mutex::new(
                SystemTime::UNIX_EPOCH + Duration::from_secs(secs),
            )))
        }
        fn advance(&self, secs: u64) {
            *self.0.lock().unwrap() += Duration::from_secs(secs);
        }
    }

    impl Clock for ManualClock {
        fn now(&self) -> SystemTime {
            *self.0.lock().unwrap()
        }
    }

    /// Fast and deterministic; the real Argon2 verifier has its own tests.
    struct FakeVerifier;

    impl PinVerifier for FakeVerifier {
        fn hash(&self, pin: &str, salt: &str) -> String {
            format!("{pin}/{salt}")
        }
        fn verify(&self, pin: &str, salt: &str, verifier: &str) -> bool {
            verifier == format!("{pin}/{salt}")
        }
    }

    const WAIT_LIMIT: Duration = Duration::from_secs(10);

    /// Every verify must meet another one inside it, so this only succeeds if two
    /// verifications overlap. Times out instead of hanging, so a failure can't wedge the
    /// test run.
    #[derive(Default)]
    struct Rendezvous {
        arrived: Mutex<usize>,
        changed: Condvar,
    }

    impl PinVerifier for Rendezvous {
        fn hash(&self, pin: &str, salt: &str) -> String {
            FakeVerifier.hash(pin, salt)
        }
        fn verify(&self, pin: &str, salt: &str, verifier: &str) -> bool {
            let mut arrived = self.arrived.lock().unwrap();
            *arrived += 1;
            self.changed.notify_all();
            let (arrived, timeout) = self
                .changed
                .wait_timeout_while(arrived, WAIT_LIMIT, |n| *n < 2)
                .unwrap();
            drop(arrived);
            !timeout.timed_out() && FakeVerifier.verify(pin, salt, verifier)
        }
    }

    /// Holds every verify until the test opens the gate (or the wait limit passes), and
    /// counts how many verifications have started.
    #[derive(Default)]
    struct Gate {
        open: Mutex<bool>,
        changed: Condvar,
        entered: AtomicUsize,
    }

    impl Gate {
        fn open(&self) {
            *self.open.lock().unwrap() = true;
            self.changed.notify_all();
        }
    }

    impl PinVerifier for Gate {
        fn hash(&self, pin: &str, salt: &str) -> String {
            FakeVerifier.hash(pin, salt)
        }
        fn verify(&self, pin: &str, salt: &str, verifier: &str) -> bool {
            self.entered.fetch_add(1, Ordering::SeqCst);
            let open = self.open.lock().unwrap();
            drop(
                self.changed
                    .wait_timeout_while(open, WAIT_LIMIT, |open| !*open)
                    .unwrap(),
            );
            FakeVerifier.verify(pin, salt, verifier)
        }
    }

    /// Polls `condition` until it holds, instead of guessing with sleeps.
    async fn until(what: &str, condition: impl Fn() -> bool) {
        let deadline = tokio::time::Instant::now() + WAIT_LIMIT;
        while !condition() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {what}"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    struct Counter(AtomicUsize);

    impl IdSource for Counter {
        fn next_id(&self) -> Result<String, EntropyError> {
            Ok(format!("id-{}", self.0.fetch_add(1, Ordering::SeqCst)))
        }
    }

    /// Always returns the same id, and counts calls.
    struct Colliding(AtomicUsize);

    impl IdSource for Colliding {
        fn next_id(&self) -> Result<String, EntropyError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok("same-id".into())
        }
    }

    /// A memory store whose consume or failure recording can be made to fail.
    struct FaultyStore {
        inner: MemoryStore,
        fail_consume: bool,
        fail_record: bool,
    }

    impl Store for FaultyStore {
        fn insert(&self, message: NewMessage, now: SystemTime) -> Result<(), InsertError> {
            self.inner.insert(message, now)
        }
        fn meta(&self, id: &str) -> Result<Option<MessageMeta>, StoreError> {
            self.inner.meta(id)
        }
        fn record_failure(
            &self,
            id: &str,
            generation: u64,
            max: u32,
        ) -> Result<Option<Attempt>, StoreError> {
            if self.fail_record {
                Err(StoreError)
            } else {
                self.inner.record_failure(id, generation, max)
            }
        }
        fn consume(&self, id: &str, generation: u64) -> Result<Option<Vec<u8>>, StoreError> {
            if self.fail_consume {
                Err(StoreError)
            } else {
                self.inner.consume(id, generation)
            }
        }
        fn remove(&self, id: &str, generation: u64) -> Result<bool, StoreError> {
            self.inner.remove(id, generation)
        }
        fn sweep(&self, now: SystemTime) -> Result<usize, StoreError> {
            self.inner.sweep(now)
        }
    }

    fn policy() -> Policy {
        Policy {
            min_ttl: Duration::from_mins(1),
            max_ttl: Duration::from_hours(24),
            max_payload_bytes: 100,
            max_failed_attempts: 3,
        }
    }

    struct Setup {
        store: Arc<dyn Store>,
        verifier: Arc<dyn PinVerifier>,
        clock: Arc<ManualClock>,
        ids: Arc<dyn IdSource>,
        pool: (usize, usize),
    }

    impl Default for Setup {
        fn default() -> Self {
            Self {
                store: Arc::new(MemoryStore::new(100, 10_000)),
                verifier: Arc::new(FakeVerifier),
                clock: ManualClock::at(1_000),
                ids: Arc::new(Counter(AtomicUsize::new(0))),
                pool: (4, 16),
            }
        }
    }

    impl Setup {
        fn build(self) -> (Arc<Service>, Arc<ManualClock>) {
            let clock = Arc::clone(&self.clock);
            let service = Service::new(Parts {
                store: self.store,
                verifier: self.verifier,
                pool: HashPool::new(self.pool.0, self.pool.1),
                clock: self.clock,
                ids: self.ids,
                policy: policy(),
            });
            (service, clock)
        }
    }

    async fn create(service: &Service) -> String {
        service
            .create(b"blob".to_vec(), PIN.into(), "600")
            .await
            .unwrap()
            .id
    }

    #[tokio::test]
    async fn http_create_validation() {
        let (service, _) = Setup::default().build();
        let cases: Vec<(&[u8], &str, &str, ApiError)> = vec![
            (b"x", "12345", "600", ApiError::InvalidPin),
            (
                b"x",
                PIN,
                "abc",
                ApiError::BadRequest("TTL must be a whole number of seconds"),
            ),
            (
                b"x",
                PIN,
                "-5",
                ApiError::BadRequest("TTL must be a whole number of seconds"),
            ),
            (
                b"x",
                PIN,
                "59",
                ApiError::BadRequest("TTL is outside the allowed range"),
            ),
            (
                b"x",
                PIN,
                "86401",
                ApiError::BadRequest("TTL is outside the allowed range"),
            ),
            (b"", PIN, "600", ApiError::BadRequest("The blob is empty")),
            (&[0; 101], PIN, "600", ApiError::PayloadTooLarge),
        ];
        for (blob, pin, ttl, expected) in cases {
            assert_eq!(
                service
                    .create(blob.to_vec(), pin.into(), ttl)
                    .await
                    .unwrap_err(),
                expected,
                "{pin} {ttl}"
            );
        }
        assert!(
            service.create(vec![0; 100], PIN.into(), "60").await.is_ok(),
            "exactly at the limits"
        );
        assert!(
            service
                .create(vec![0; 1], PIN.into(), "86400")
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn srv_ids_collision_limit() {
        let ids = Arc::new(Colliding(AtomicUsize::new(0)));
        let (service, _) = Setup {
            ids: Arc::clone(&ids) as Arc<dyn IdSource>,
            ..Setup::default()
        }
        .build();
        create(&service).await;
        ids.0.store(0, Ordering::SeqCst);
        assert_eq!(
            service
                .create(b"b".to_vec(), PIN.into(), "600")
                .await
                .unwrap_err(),
            ApiError::ServiceUnavailable("Unable to allocate a message id")
        );
        assert_eq!(ids.0.load(Ordering::SeqCst), MAX_ID_ATTEMPTS);
    }

    #[tokio::test]
    async fn inv_7_burn_sequence_then_gone() {
        let (service, _) = Setup::default().build();
        let id = create(&service).await;
        for _ in 0..3 {
            assert_eq!(
                service
                    .retrieve(id.clone(), "000000".into())
                    .await
                    .unwrap_err(),
                ApiError::WrongPin
            );
        }
        assert_eq!(
            service.retrieve(id.clone(), PIN.into()).await.unwrap_err(),
            ApiError::Unavailable
        );
    }

    #[tokio::test]
    async fn inv_6_consume_once_and_invalid_pin_counts_nothing() {
        let (service, _) = Setup::default().build();
        let id = create(&service).await;
        assert_eq!(
            service
                .retrieve(id.clone(), "12a456".into())
                .await
                .unwrap_err(),
            ApiError::InvalidPin
        );
        assert_eq!(
            service.retrieve(id.clone(), PIN.into()).await.unwrap(),
            b"blob"
        );
        assert_eq!(
            service.retrieve(id, PIN.into()).await.unwrap_err(),
            ApiError::Unavailable
        );
        assert_eq!(
            service
                .retrieve("never-existed".into(), PIN.into())
                .await
                .unwrap_err(),
            ApiError::Unavailable
        );
    }

    #[tokio::test]
    async fn inv_8_boundary_controlled_clock() {
        let (service, clock) = Setup::default().build();
        let early = create(&service).await;
        let late = create(&service).await;
        clock.advance(599);
        assert_eq!(service.retrieve(early, PIN.into()).await.unwrap(), b"blob");
        clock.advance(1);
        assert_eq!(
            service.retrieve(late, PIN.into()).await.unwrap_err(),
            ApiError::Unavailable,
            "expires_at <= now"
        );
    }

    #[tokio::test]
    async fn inv_8_sweeper_removes_expired() {
        let store = Arc::new(MemoryStore::new(100, 10_000));
        let (service, clock) = Setup {
            store: Arc::clone(&store) as Arc<dyn Store>,
            ..Setup::default()
        }
        .build();
        create(&service).await;
        assert_eq!(service.sweep(), Ok(0));
        clock.advance(600);
        assert_eq!(service.sweep(), Ok(1));
        assert_eq!(store.usage(), (0, 0));
    }

    #[tokio::test]
    async fn inv_6_commit_failure_no_bytes() {
        let store = Arc::new(FaultyStore {
            inner: MemoryStore::new(10, 1000),
            fail_consume: true,
            fail_record: true,
        });
        let (service, _) = Setup {
            store: Arc::clone(&store) as Arc<dyn Store>,
            ..Setup::default()
        }
        .build();
        let id = create(&service).await;
        assert_eq!(
            service.retrieve(id.clone(), PIN.into()).await.unwrap_err(),
            ApiError::ServiceUnavailable("Storage is unavailable"),
            "a failed consume returns no blob"
        );
        assert_eq!(
            service
                .retrieve(id.clone(), "000000".into())
                .await
                .unwrap_err(),
            ApiError::ServiceUnavailable("Storage is unavailable"),
            "a failed attempt commit is not reported as a plain wrong PIN"
        );
        assert_eq!(store.inner.meta(&id).unwrap().unwrap().failed_attempts, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn srv_serialize_different_ids_verify_concurrently() {
        let (service, _) = Setup {
            verifier: Arc::new(Rendezvous::default()),
            ..Setup::default()
        }
        .build();
        // Hashing at create doesn't use the rendezvous; pick two ids on different stripes.
        let mut ids = Vec::new();
        while ids.len() < 2 {
            let id = create(&service).await;
            if ids
                .iter()
                .all(|other: &String| service.stripe_of(other) != service.stripe_of(&id))
            {
                ids.push(id);
            }
        }
        let a = service.retrieve(ids[0].clone(), PIN.into());
        let b = service.retrieve(ids[1].clone(), PIN.into());
        let (a, b) = tokio::join!(a, b);
        assert!(
            a.is_ok() && b.is_ok(),
            "verifications of different ids must overlap: {a:?} {b:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn inv_6_parallel_same_id_releases_once() {
        let (service, _) = Setup::default().build();
        let id = create(&service).await;
        let attempts: Vec<_> = (0..8)
            .map(|_| {
                let (service, id) = (Arc::clone(&service), id.clone());
                async move { service.retrieve(id, PIN.into()).await }
            })
            .collect();
        let results = futures_join_all(attempts).await;
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert!(
            results
                .iter()
                .filter(|r| r.is_err())
                .all(|r| r.as_ref().unwrap_err() == &ApiError::Unavailable)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn srv_hash_pool_queue_full_is_503() {
        let gate = Arc::new(Gate::default());
        let (service, _) = Setup {
            verifier: Arc::clone(&gate) as Arc<dyn PinVerifier>,
            pool: (1, 1),
            ..Setup::default()
        }
        .build();
        let (first, second, third) = (
            create(&service).await,
            create(&service).await,
            create(&service).await,
        );
        let spawn = |id: String| {
            let service = Arc::clone(&service);
            tokio::spawn(async move { service.retrieve(id, PIN.into()).await })
        };
        let running = spawn(first);
        until("the first verify to hold the only permit", || {
            gate.entered.load(Ordering::SeqCst) == 1
        })
        .await;
        let queued = spawn(second);
        until("the second request to fill the queue", || {
            service.pool.waiting.load(Ordering::SeqCst) == 1
        })
        .await;
        assert_eq!(
            service.retrieve(third, PIN.into()).await.unwrap_err(),
            ApiError::ServiceUnavailable("The server is busy, try again shortly")
        );
        gate.open();
        assert!(running.await.unwrap().is_ok());
        assert!(queued.await.unwrap().is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn srv_admission_cancelled_request_still_commits_its_attempt() {
        let gate = Arc::new(Gate::default());
        let store = Arc::new(MemoryStore::new(10, 1000));
        let (service, _) = Setup {
            store: Arc::clone(&store) as Arc<dyn Store>,
            verifier: Arc::clone(&gate) as Arc<dyn PinVerifier>,
            ..Setup::default()
        }
        .build();
        let id = create(&service).await;
        let request = tokio::spawn({
            let (service, id) = (Arc::clone(&service), id.clone());
            async move { service.retrieve(id, "000000".into()).await }
        });
        until("the verify to start", || {
            gate.entered.load(Ordering::SeqCst) == 1
        })
        .await;
        request.abort(); // the client goes away mid-verify
        gate.open();
        until("the failed attempt to be committed", || {
            store
                .meta(&id)
                .unwrap()
                .is_some_and(|m| m.failed_attempts == 1)
        })
        .await;
    }

    async fn futures_join_all<F: std::future::Future + Send + 'static>(
        futures: Vec<F>,
    ) -> Vec<F::Output>
    where
        F::Output: Send + 'static,
    {
        let handles: Vec<_> = futures.into_iter().map(tokio::spawn).collect();
        let mut out = Vec::new();
        for handle in handles {
            out.push(handle.await.unwrap());
        }
        out
    }

    /// One id for every message, so a replacement reuses the id of an expired one.
    struct SameId;

    impl IdSource for SameId {
        fn next_id(&self) -> Result<String, EntropyError> {
            Ok("same-id".into())
        }
    }

    /// Starts a retrieval of "same-id" with `pin`, holds it inside verification, expires and
    /// replaces the message (by an explicit sweep, or by a create that reclaims space at the
    /// cap), then lets the stale verification finish.
    async fn stale_retrieval(
        pin: &str,
        reclaim_by_cap: bool,
    ) -> (Result<Vec<u8>, ApiError>, Arc<Service>, Arc<MemoryStore>) {
        let gate = Arc::new(Gate::default());
        let store = Arc::new(MemoryStore::new(
            if reclaim_by_cap { 1 } else { 10 },
            10_000,
        ));
        let (service, clock) = Setup {
            store: Arc::clone(&store) as Arc<dyn Store>,
            verifier: Arc::clone(&gate) as Arc<dyn PinVerifier>,
            ids: Arc::new(SameId),
            ..Setup::default()
        }
        .build();
        service
            .create(b"OLD".to_vec(), "111111".into(), "600")
            .await
            .unwrap();
        let stale = tokio::spawn({
            let (service, pin) = (Arc::clone(&service), pin.to_owned());
            async move { service.retrieve("same-id".into(), pin).await }
        });
        until("the stale verify to start", || {
            gate.entered.load(Ordering::SeqCst) == 1
        })
        .await;
        clock.advance(600);
        if !reclaim_by_cap {
            assert_eq!(service.sweep(), Ok(1));
        }
        service
            .create(b"NEW".to_vec(), "222222".into(), "600")
            .await
            .unwrap();
        gate.open();
        (stale.await.unwrap(), service, store)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn inv_6_stale_correct_pin_never_consumes_a_replacement() {
        for reclaim_by_cap in [false, true] {
            let (stale, service, _) = stale_retrieval("111111", reclaim_by_cap).await;
            assert_eq!(
                stale.unwrap_err(),
                ApiError::Unavailable,
                "reclaim_by_cap={reclaim_by_cap}"
            );
            assert_eq!(
                service
                    .retrieve("same-id".into(), "222222".into())
                    .await
                    .unwrap(),
                b"NEW"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn inv_7_stale_wrong_pin_never_charges_a_replacement() {
        for reclaim_by_cap in [false, true] {
            let (stale, _, store) = stale_retrieval("999999", reclaim_by_cap).await;
            assert_eq!(
                stale.unwrap_err(),
                ApiError::Unavailable,
                "reclaim_by_cap={reclaim_by_cap}"
            );
            assert_eq!(store.meta("same-id").unwrap().unwrap().failed_attempts, 0);
        }
    }

    /// Pool of one permit and one queue slot: two retrievals may be admitted at once.
    fn admission_setup() -> (Arc<Service>, Arc<Gate>) {
        let gate = Arc::new(Gate::default());
        let (service, _) = Setup {
            // Room for the ~1000 creates it takes to find two ids on one stripe.
            store: Arc::new(MemoryStore::new(20_000, 1_000_000)),
            verifier: Arc::clone(&gate) as Arc<dyn PinVerifier>,
            pool: (1, 1),
            ..Setup::default()
        }
        .build();
        (service, gate)
    }

    fn spawn_retrieve(
        service: &Arc<Service>,
        id: &str,
    ) -> tokio::task::JoinHandle<Result<Vec<u8>, ApiError>> {
        let (service, id) = (Arc::clone(service), id.to_owned());
        tokio::spawn(async move { service.retrieve(id, PIN.into()).await })
    }

    const BUSY: ApiError = ApiError::ServiceUnavailable("The server is busy, try again shortly");

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn srv_admission_same_id_waiters_are_bounded() {
        let (service, gate) = admission_setup();
        let id = create(&service).await;
        let first = spawn_retrieve(&service, &id);
        until("the first verify to hold the permit", || {
            gate.entered.load(Ordering::SeqCst) == 1
        })
        .await;
        let attempts: Vec<_> = (0..12).map(|_| spawn_retrieve(&service, &id)).collect();
        // Exactly one more is admitted; the other eleven are refused at once.
        until("eleven refusals", || {
            attempts.iter().filter(|a| a.is_finished()).count() == 11
        })
        .await;
        let mut still_waiting = Vec::new();
        for attempt in attempts {
            if attempt.is_finished() {
                assert_eq!(attempt.await.unwrap().unwrap_err(), BUSY);
            } else {
                still_waiting.push(attempt);
            }
        }
        assert_eq!(still_waiting.len(), 1);
        assert_eq!(service.admission.available_permits(), 0);
        gate.open();
        assert!(first.await.unwrap().is_ok());
        assert_eq!(
            still_waiting.pop().unwrap().await.unwrap().unwrap_err(),
            ApiError::Unavailable,
            "already consumed"
        );
        until("all slots to be returned", || {
            service.admission.available_permits() == 2
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn srv_admission_unrelated_ids_sharing_a_stripe_are_bounded() {
        let (service, gate) = admission_setup();
        let first = create(&service).await;
        let neighbour = loop {
            let id = create(&service).await;
            if service.stripe_of(&id) == service.stripe_of(&first) {
                break id;
            }
        };
        let other = create(&service).await;
        let running = spawn_retrieve(&service, &first);
        until("the first verify to hold the permit", || {
            gate.entered.load(Ordering::SeqCst) == 1
        })
        .await;
        let waiting = spawn_retrieve(&service, &neighbour); // admitted, waits for the shared stripe
        until("the neighbour to be admitted", || {
            service.admission.available_permits() == 0
        })
        .await;
        assert_eq!(service.retrieve(other, PIN.into()).await.unwrap_err(), BUSY);
        gate.open();
        assert!(running.await.unwrap().is_ok());
        assert!(waiting.await.unwrap().is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn srv_admission_cancelled_waiter_keeps_its_slot_until_done() {
        let (service, gate) = admission_setup();
        let id = create(&service).await;
        let running = spawn_retrieve(&service, &id);
        until("the first verify to hold the permit", || {
            gate.entered.load(Ordering::SeqCst) == 1
        })
        .await;
        let queued = spawn_retrieve(&service, &id);
        until("the second to be admitted", || {
            service.admission.available_permits() == 0
        })
        .await;
        queued.abort(); // the client leaves while queued
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            service.admission.available_permits(),
            0,
            "the detached work still counts"
        );
        assert_eq!(service.retrieve(id, PIN.into()).await.unwrap_err(), BUSY);
        gate.open();
        assert!(running.await.unwrap().is_ok());
        until("all slots to be returned", || {
            service.admission.available_permits() == 2
        })
        .await;
    }

    async fn concurrent_creates(
        service: &Arc<Service>,
        sizes: &[usize],
    ) -> Vec<(usize, Result<super::Created, ApiError>)> {
        let handles: Vec<_> = sizes
            .iter()
            .map(|&size| {
                let service = Arc::clone(service);
                tokio::spawn(async move {
                    (size, service.create(vec![1; size], PIN.into(), "60").await)
                })
            })
            .collect();
        let mut out = Vec::new();
        for handle in handles {
            out.push(handle.await.unwrap());
        }
        out
    }

    const FULL: ApiError = ApiError::ServiceUnavailable("Server storage is full");

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn srv_caps_concurrent_creates_respect_the_count_cap() {
        let store = Arc::new(MemoryStore::new(3, 10_000));
        let (service, _) = Setup {
            store: Arc::clone(&store) as Arc<dyn Store>,
            pool: (8, 32),
            ..Setup::default()
        }
        .build();
        let results = concurrent_creates(&service, &[5; 10]).await;
        assert_eq!(results.iter().filter(|(_, r)| r.is_ok()).count(), 3);
        assert!(
            results
                .iter()
                .filter(|(_, r)| r.is_err())
                .all(|(_, r)| r.as_ref().unwrap_err() == &FULL)
        );
        assert_eq!(store.usage(), (3, 15));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn srv_caps_concurrent_creates_respect_the_byte_cap() {
        let store = Arc::new(MemoryStore::new(100, 100));
        let (service, _) = Setup {
            store: Arc::clone(&store) as Arc<dyn Store>,
            pool: (8, 32),
            ..Setup::default()
        }
        .build();
        let results = concurrent_creates(&service, &[30, 40, 50, 20, 60, 30, 40, 50, 20, 60]).await;
        let stored: usize = results
            .iter()
            .filter(|(_, r)| r.is_ok())
            .map(|(size, _)| size)
            .sum();
        assert!(stored <= 100, "{stored}");
        assert!(
            results
                .iter()
                .filter(|(_, r)| r.is_err())
                .all(|(_, r)| r.as_ref().unwrap_err() == &FULL)
        );
        assert_eq!(
            store.usage().1,
            stored,
            "byte accounting matches what was stored"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn srv_caps_reclamation_under_contention() {
        let store = Arc::new(MemoryStore::new(3, 10_000));
        let (service, clock) = Setup {
            store: Arc::clone(&store) as Arc<dyn Store>,
            pool: (8, 32),
            ..Setup::default()
        }
        .build();
        let first = concurrent_creates(&service, &[5; 3]).await;
        assert!(first.iter().all(|(_, r)| r.is_ok()));
        clock.advance(60); // all three expire
        let results = concurrent_creates(&service, &[7; 10]).await;
        assert_eq!(results.iter().filter(|(_, r)| r.is_ok()).count(), 3);
        assert_eq!(
            store.usage(),
            (3, 21),
            "expired rows were reclaimed exactly once"
        );
    }
}
