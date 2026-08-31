use std::path::PathBuf;

/// Errors produced while discovering, loading, or invoking the desktop SDK.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("1Password desktop integration is only supported on macOS")]
    UnsupportedPlatform,

    #[error("1Password desktop application was not found in any supported location")]
    ApplicationNotFound { searched_paths: Vec<PathBuf> },

    #[error("failed to load 1Password desktop library at {path}: {message}")]
    LibraryLoad { path: PathBuf, message: String },

    #[error("1Password desktop library is missing symbol {symbol}: {message}")]
    MissingSymbol {
        symbol: &'static str,
        message: String,
    },

    #[error(
        "desktop app connection channel is closed; enable Settings > Developer > Integrate with other apps"
    )]
    ChannelClosed,

    #[error("connection to the 1Password desktop app was unexpectedly dropped")]
    ConnectionDropped,

    #[error("1Password desktop library returned code {0}")]
    TransportCode(i32),

    #[error("invalid desktop SDK protocol response: {0}")]
    Protocol(String),

    #[error("1Password operation {name} failed: {message}")]
    Remote { name: String, message: String },

    #[error("{field} must not be empty")]
    MissingConfiguration { field: &'static str },

    #[error("invalid secret-reference segment {segment:?}")]
    InvalidReferenceSegment { segment: String },

    #[error("invalid 1Password secret reference: {0}")]
    InvalidSecretReference(String),

    #[error("the 1Password client is closed")]
    ClientClosed,

    #[error("the 1Password client state lock was poisoned")]
    LockPoisoned,
}

impl Error {
    pub(crate) fn is_session_expired(&self) -> bool {
        matches!(self, Self::Remote { name, .. } if name == "DesktopSessionExpired")
    }
}

pub type Result<T> = std::result::Result<T, Error>;
