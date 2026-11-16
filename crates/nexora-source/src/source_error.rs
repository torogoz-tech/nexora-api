#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SourceError {
    #[error("source destination already exists")]
    DestinationExists,
    #[error("source contains an unsupported symbolic link or submodule")]
    UnsupportedEntry,
    #[error("invalid source reader configuration")]
    InvalidConfiguration,
    #[error("source reader host is not supported")]
    UnsupportedHost,
    #[error("invalid Git object identity or content length")]
    InvalidObject,
    #[error("source process unavailable")]
    Unavailable,
    #[error("source operation timed out")]
    TimedOut,
    #[error("malformed Git tree metadata")]
    MalformedTree,
    #[error("unsafe or ambiguous source path")]
    UnsafePath,
    #[error("source exceeds configured limits")]
    LimitExceeded,
}
