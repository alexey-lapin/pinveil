//! Errors and exit codes (APP.md, "Secrets and streams"). Every failure belongs to one class,
//! and each class has a fixed exit code. Messages never contain the share link's fragment or
//! the PIN.

use std::fmt;
use std::process::ExitCode;

/// A failure class. The numbers are part of the CLI's interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitClass {
    /// Bad arguments, a bad profile, or a required terminal that isn't there.
    Usage,
    /// The server couldn't be reached, TLS failed, or its config was missing or invalid.
    Network,
    /// The server refused the request: wrong PIN, rate limit, full store, too large.
    Rejected,
    /// The message doesn't exist: never created, expired, opened, or burned.
    Unavailable,
    /// The message arrived but can't be opened: wrong link, tampering, or a bad payload.
    Decryption,
    /// Writing the output failed, or the user quit at a file collision.
    Output,
    /// Anything else, such as a failing random number generator.
    Internal,
}

impl ExitClass {
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Internal => 1,
            Self::Usage => 2,
            Self::Network => 3,
            Self::Rejected => 4,
            Self::Unavailable => 5,
            Self::Decryption => 6,
            Self::Output => 7,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    pub class: ExitClass,
    pub message: String,
}

impl CliError {
    pub fn new(class: ExitClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ExitClass::Usage, message)
    }

    #[must_use]
    pub fn exit_code(&self) -> ExitCode {
        ExitCode::from(self.class.code())
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}
