//! Randomness behind a small trait, so production code uses the operating system's CSPRNG
//! and tests can replay fixed bytes. There is no fallback: if the system source fails,
//! the operation fails (APP.md, "Keys").

/// A source of random bytes.
pub trait Entropy {
    /// Fills `buf` entirely with random bytes.
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), EntropyError>;
}

/// The operating system's CSPRNG.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemEntropy;

impl Entropy for SystemEntropy {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), EntropyError> {
        getrandom::fill(buf).map_err(|_| EntropyError)
    }
}

/// The entropy source failed. Creation must abort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the system random number generator failed")]
pub struct EntropyError;

/// A uniformly distributed integer in `0..n`, by rejection sampling over big-endian `u32`
/// draws. Draws at or above the largest multiple of `n` are discarded, so there is no
/// modulo bias. The browser uses the same algorithm.
///
/// # Panics
///
/// If `n` is zero.
pub fn uniform_below(entropy: &mut impl Entropy, n: u32) -> Result<u32, EntropyError> {
    assert!(n > 0, "uniform_below needs a positive bound");
    let range = 1u64 << 32;
    let limit = range - range % u64::from(n);
    loop {
        let mut bytes = [0u8; 4];
        entropy.fill(&mut bytes)?;
        let value = u32::from_be_bytes(bytes);
        if u64::from(value) < limit {
            return Ok(value % n);
        }
    }
}

/// Test entropy that replays fixed bytes and fails once they run out.
#[cfg(test)]
pub(crate) struct ReplayEntropy {
    bytes: std::collections::VecDeque<u8>,
}

#[cfg(test)]
impl ReplayEntropy {
    pub(crate) fn new(bytes: impl IntoIterator<Item = u8>) -> Self {
        Self {
            bytes: bytes.into_iter().collect(),
        }
    }

    /// Replays the given big-endian `u32` draws.
    pub(crate) fn of_u32s(values: &[u32]) -> Self {
        Self::new(values.iter().flat_map(|v| v.to_be_bytes()))
    }
}

#[cfg(test)]
impl Entropy for ReplayEntropy {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), EntropyError> {
        if self.bytes.len() < buf.len() {
            return Err(EntropyError);
        }
        for byte in buf.iter_mut() {
            *byte = self.bytes.pop_front().ok_or(EntropyError)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{EntropyError, ReplayEntropy, SystemEntropy, uniform_below};

    #[test]
    fn gen_rejection_boundary_for_7776() {
        // floor(2^32 / 7776) * 7776 = 4_294_964_736
        let limit = 4_294_964_736_u32;
        let mut entropy = ReplayEntropy::of_u32s(&[limit - 1]);
        assert_eq!(uniform_below(&mut entropy, 7776), Ok(7775));
        let mut entropy = ReplayEntropy::of_u32s(&[limit, u32::MAX, 7]);
        assert_eq!(
            uniform_below(&mut entropy, 7776),
            Ok(7),
            "draws at or above the limit are discarded"
        );
    }

    #[test]
    fn gen_entropy_failure_aborts() {
        let mut empty = ReplayEntropy::new([]);
        assert_eq!(uniform_below(&mut empty, 10), Err(EntropyError));
        let mut only_rejects = ReplayEntropy::of_u32s(&[u32::MAX]);
        assert_eq!(uniform_below(&mut only_rejects, 7776), Err(EntropyError));
    }

    #[test]
    fn system_entropy_produces_values_in_range() {
        for _ in 0..1000 {
            assert!(uniform_below(&mut SystemEntropy, 7776).unwrap() < 7776);
        }
    }
}
