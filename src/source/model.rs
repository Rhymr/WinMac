//! Platform-agnostic model every [`TextSource`](super::TextSource) reports
//! its contents in — one representation, shared by Apple Notes today and
//! any future backend.

use std::fmt;

/// Opaque, source-defined identifier for a single document (an Apple Note's
/// title, a Drive file id, …).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DocId(pub String);

/// One document leaf in a source tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDoc {
    pub id: DocId,
    pub title: String,
}

/// A folder in a source tree — folders nest, documents are leaves.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SourceFolder {
    pub name: String,
    pub folders: Vec<SourceFolder>,
    pub docs: Vec<SourceDoc>,
}

/// The whole tree a source exposes: top-level folders plus any documents
/// that sit loose at the root.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SourceTree {
    pub folders: Vec<SourceFolder>,
    pub docs: Vec<SourceDoc>,
}

impl SourceTree {
    /// `true` when there is nothing to show.
    pub fn is_empty(&self) -> bool {
        self.folders.is_empty() && self.docs.is_empty()
    }
}

/// Whether a source is usable right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceStatus {
    /// Configured and ready to load.
    Ready,
    /// Needs the user to sign in / grant access before it can load.
    NeedsAuth,
    /// Can't be used on this platform or in this environment.
    Unavailable(String),
}

/// Typed failure from a [`TextSource`](super::TextSource) operation.
#[derive(Debug)]
pub enum SourceError {
    /// Sign-in / permission problem.
    Auth(String),
    /// Network / transport problem.
    Network(String),
    /// The source returned data we couldn't understand.
    Parse(String),
    /// Local I/O (spawning a helper, reading a cache, …).
    Io(String),
    /// The operation isn't supported on this platform.
    Unsupported,
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::Auth(m) => write!(f, "authentication failed: {m}"),
            SourceError::Network(m) => write!(f, "network error: {m}"),
            SourceError::Parse(m) => write!(f, "could not parse source data: {m}"),
            SourceError::Io(m) => write!(f, "i/o error: {m}"),
            SourceError::Unsupported => write!(f, "not supported on this platform"),
        }
    }
}

impl std::error::Error for SourceError {}
