//! Protocol constants shared by every client (APP.md, "Validation"). Operators can't change
//! these; server configuration may only set creation limits at or below the ceilings.

/// The envelope schema version this build writes and reads.
pub const PROTOCOL_VERSION: u32 = 2;
