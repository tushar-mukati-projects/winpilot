use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidArgument,
    NotFound,
    Ambiguous,
    StaleRef,
    Obscured,
    Unsupported,
    Timeout,
    Denied,
    ElevatedTarget,
    NeedsUser,
    UserActive,
    StoppedByUser,
    Busy,
    TargetNotResponding,
    Cancelled,
    Internal,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidArgument => "invalid_argument",
            Self::NotFound => "not_found",
            Self::Ambiguous => "ambiguous",
            Self::StaleRef => "stale_ref",
            Self::Obscured => "obscured",
            Self::Unsupported => "unsupported",
            Self::Timeout => "timeout",
            Self::Denied => "denied",
            Self::ElevatedTarget => "elevated_target",
            Self::NeedsUser => "needs_user",
            Self::UserActive => "user_active",
            Self::StoppedByUser => "stopped_by_user",
            Self::Busy => "busy",
            Self::TargetNotResponding => "target_not_responding",
            Self::Cancelled => "cancelled",
            Self::Internal => "internal",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unsupported, message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Timeout, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

pub trait Context<T> {
    fn context(self, what: &str) -> Result<T>;
}

impl<T, E: fmt::Display> Context<T> for std::result::Result<T, E> {
    fn context(self, what: &str) -> Result<T> {
        self.map_err(|error| Error::internal(format!("{what}: {error}")))
    }
}

impl<T> Context<T> for Option<T> {
    fn context(self, what: &str) -> Result<T> {
        self.ok_or_else(|| Error::not_found(what.to_string()))
    }
}
