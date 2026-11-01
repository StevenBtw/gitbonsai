//! The crate's error type. Every message names the fix.

/// Something that stopped a tree from growing.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The config, or a flag that overrides it, is invalid. Exit code 2.
    #[error("invalid config: {0}")]
    Config(String),
    /// The snapshot can't be read or breaks its invariants.
    #[error("invalid snapshot: {0}")]
    Snapshot(String),
    /// Collection from GitHub failed. No partial snapshot is ever written.
    #[error("collection failed: {0}")]
    Collect(String),
    /// Rasterizing the PNG failed.
    #[error("rendering failed: {0}")]
    Render(String),
    /// Reading or writing a file failed.
    #[error("{context}: {source}")]
    Io {
        /// What was being done, naming the path.
        context: String,
        /// The underlying error.
        source: std::io::Error,
    },
}

impl Error {
    /// The process exit code: 2 for invalid usage or config, 1 otherwise.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Config(_) => 2,
            _ => 1,
        }
    }

    /// An I/O error with what was being done.
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}
