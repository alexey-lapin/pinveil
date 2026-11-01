//! Fixed-window rate limiting per client address and bucket (APP.md, "Rate limiting").
//!
//! State is bounded: at most `max_tracked_keys` counters, and expired counters leave
//! through a queue in window order, so cleanup per request is amortized O(1). When the
//! cap is reached, new addresses are refused rather than evicting live counters, because
//! eviction would let a flood of fresh addresses reset everyone's counts.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ipnet::IpNet;

use crate::config::RateLimitConfig;

/// Longest `X-Forwarded-For` value parsed; longer values fall back to the peer address.
const MAX_FORWARDED_BYTES: usize = 1024;
/// Most addresses parsed from one `X-Forwarded-For` value.
const MAX_FORWARDED_ENTRIES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    Create,
    Retrieve,
}

type Key = (Bucket, IpAddr);

struct Window {
    started: Instant,
    count: u32,
}

#[derive(Default)]
struct Inner {
    windows: HashMap<Key, Window>,
    /// `(window start, key)` in start order. Entries whose window was later restarted
    /// are skipped when they reach the front.
    expiry: VecDeque<(Instant, Key)>,
}

pub struct RateLimiter {
    inner: Mutex<Inner>,
    window: Duration,
    create_limit: u32,
    retrieve_limit: u32,
    max_keys: usize,
}

impl RateLimiter {
    #[must_use]
    pub fn new(config: &RateLimitConfig) -> Self {
        Self {
            inner: Mutex::default(),
            window: config.window,
            create_limit: config.create_per_window,
            retrieve_limit: config.retrieve_per_window,
            max_keys: config.max_tracked_keys,
        }
    }

    /// Counts a request and returns whether it may proceed.
    pub fn allow(&self, bucket: Bucket, client: IpAddr, now: Instant) -> bool {
        let limit = match bucket {
            Bucket::Create => self.create_limit,
            Bucket::Retrieve => self.retrieve_limit,
        };
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Inner { windows, expiry } = &mut *inner;
        while let Some(&(started, key)) = expiry.front() {
            if started + self.window > now {
                break;
            }
            expiry.pop_front();
            if windows.get(&key).is_some_and(|w| w.started == started) {
                windows.remove(&key);
            }
        }
        let key = (bucket, client);
        if let Some(window) = windows.get_mut(&key) {
            if window.count >= limit {
                return false;
            }
            window.count += 1;
            return true;
        }
        if windows.len() >= self.max_keys {
            return false;
        }
        windows.insert(
            key,
            Window {
                started: now,
                count: 1,
            },
        );
        expiry.push_back((now, key));
        true
    }

    /// Tracked counters, for tests.
    #[must_use]
    pub fn tracked(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .windows
            .len()
    }
}

/// The address to count a request against. The socket peer, unless the peer is a trusted
/// proxy; then `X-Forwarded-For` is walked from the right, skipping trusted proxies, to
/// the first untrusted address. A missing, malformed, or over-long header falls back to
/// the peer, so it can never mint a fresh bucket.
#[must_use]
pub fn client_address(peer: IpAddr, forwarded_for: &[&str], trusted: &[IpNet]) -> IpAddr {
    let is_trusted = |ip: &IpAddr| trusted.iter().any(|net| net.contains(ip));
    if !is_trusted(&peer) {
        return peer;
    }
    let [header] = forwarded_for else {
        return peer;
    };
    if header.len() > MAX_FORWARDED_BYTES {
        return peer;
    }
    let entries: Vec<&str> = header.split(',').map(str::trim).collect();
    if entries.len() > MAX_FORWARDED_ENTRIES {
        return peer;
    }
    let Ok(chain) = entries
        .iter()
        .map(|e| e.parse::<IpAddr>())
        .collect::<Result<Vec<_>, _>>()
    else {
        return peer;
    };
    chain
        .into_iter()
        .rev()
        .find(|ip| !is_trusted(ip))
        .unwrap_or(peer)
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;
    use std::time::{Duration, Instant};

    use ipnet::IpNet;

    use super::{Bucket, RateLimiter, client_address};
    use crate::config::RateLimitConfig;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    fn limiter(per_window: u32, max_keys: usize) -> RateLimiter {
        RateLimiter::new(&RateLimitConfig {
            window: Duration::from_mins(1),
            create_per_window: per_window,
            retrieve_per_window: per_window + 1,
            max_tracked_keys: max_keys,
        })
    }

    #[test]
    fn srv_ratelimit_fixed_window_per_bucket_and_key() {
        let limiter = limiter(2, 100);
        let t0 = Instant::now();
        let a = ip("1.1.1.1");
        assert!(limiter.allow(Bucket::Create, a, t0));
        assert!(limiter.allow(Bucket::Create, a, t0));
        assert!(!limiter.allow(Bucket::Create, a, t0));
        assert!(
            limiter.allow(Bucket::Create, ip("2.2.2.2"), t0),
            "keys are independent"
        );
        assert!(
            limiter.allow(Bucket::Retrieve, a, t0),
            "buckets are independent"
        );
        assert!(!limiter.allow(Bucket::Create, a, t0 + Duration::from_secs(59)));
        assert!(
            limiter.allow(Bucket::Create, a, t0 + Duration::from_mins(1)),
            "a new window"
        );
    }

    #[test]
    fn srv_ratelimit_key_cap_fails_closed_and_frees_on_expiry() {
        let limiter = limiter(5, 2);
        let t0 = Instant::now();
        assert!(limiter.allow(Bucket::Create, ip("1.1.1.1"), t0));
        assert!(limiter.allow(Bucket::Create, ip("2.2.2.2"), t0));
        assert!(
            !limiter.allow(Bucket::Create, ip("3.3.3.3"), t0),
            "new keys are refused at the cap"
        );
        assert!(
            limiter.allow(Bucket::Create, ip("1.1.1.1"), t0),
            "existing keys keep working"
        );
        assert_eq!(limiter.tracked(), 2);
        assert!(limiter.allow(Bucket::Create, ip("3.3.3.3"), t0 + Duration::from_mins(1)));
        assert_eq!(limiter.tracked(), 1, "expired counters left the map");
    }

    #[test]
    fn srv_ratelimit_state_stays_bounded_under_a_flood() {
        let limiter = limiter(1, 1000);
        let t0 = Instant::now();
        for i in 0..100_000u32 {
            let addr = IpAddr::from(i.to_be_bytes());
            limiter.allow(
                Bucket::Retrieve,
                addr,
                t0 + Duration::from_millis(u64::from(i)),
            );
        }
        assert!(limiter.tracked() <= 1000);
    }

    #[test]
    fn srv_ratelimit_client_address_rules() {
        let trusted: Vec<IpNet> = vec![
            "127.0.0.1/32".parse().unwrap(),
            "10.0.0.0/8".parse().unwrap(),
        ];
        let peer = ip("127.0.0.1");
        let cases: &[(&[&str], &str)] = &[
            (&[], "127.0.0.1"),
            (&["1.1.1.1"], "1.1.1.1"),
            (&["6.6.6.6, 1.1.1.1"], "1.1.1.1"),
            (&["1.1.1.1, 10.0.0.5"], "1.1.1.1"),
            (&["10.0.0.9, 10.0.0.5"], "127.0.0.1"),
            (&["not-an-ip"], "127.0.0.1"),
            (&["1.1.1.1:443"], "127.0.0.1"),
            (&["1.1.1.1", "2.2.2.2"], "127.0.0.1"),
            (&["::1, 2001:db8::7"], "2001:db8::7"),
        ];
        for (header, expected) in cases {
            assert_eq!(
                client_address(peer, header, &trusted),
                ip(expected),
                "{header:?}"
            );
        }
        let long = vec!["1.1.1.1"; 33].join(",");
        assert_eq!(client_address(peer, &[long.as_str()], &trusted), peer);
        let huge = format!("{}1.1.1.1", " ".repeat(1100));
        assert_eq!(client_address(peer, &[huge.as_str()], &trusted), peer);
        // An untrusted peer's header is ignored entirely.
        assert_eq!(
            client_address(ip("5.5.5.5"), &["1.1.1.1"], &trusted),
            ip("5.5.5.5")
        );
        assert_eq!(
            client_address(peer, &["1.1.1.1"], &[]),
            peer,
            "no trusted proxies by default"
        );
    }
}
