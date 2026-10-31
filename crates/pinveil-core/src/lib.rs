//! Message format, crypto, and validation shared by `pinveil-server` and the `pinveil` CLI.
//!
//! Phase 0 skeleton: only the protocol version exists so far.

/// The envelope schema version this build writes (APP.md, "Outer envelope").
pub const PROTOCOL_VERSION: u32 = 2;

#[cfg(test)]
mod tests {
    use super::PROTOCOL_VERSION;

    #[test]
    fn protocol_version_is_two() {
        assert_eq!(PROTOCOL_VERSION, 2);
    }
}
