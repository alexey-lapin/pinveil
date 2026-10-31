//! Message format, crypto, and validation shared by `pinveil-server` and the `pinveil` CLI.
//!
//! The browser implements the same rules in `web/static`. The cross-client vectors in
//! `tests/vectors` keep the two implementations byte-compatible.

pub mod entropy;
pub mod generate;
pub mod protocol;
pub mod wordlist;

pub use protocol::PROTOCOL_VERSION;
