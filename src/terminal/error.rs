//! Typed failure for the embedded terminal, hand-rolled in the same style
//! as [`crate::source::model::SourceError`] (no `thiserror`).

use std::fmt;

/// Something went wrong starting or driving a terminal session.
#[derive(Debug)]
pub enum TerminalError {
    /// The shell process could not be spawned.
    Spawn(String),
    /// PTY open / read / write failure.
    Io(String),
}

impl fmt::Display for TerminalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TerminalError::Spawn(m) => write!(f, "could not start the shell: {m}"),
            TerminalError::Io(m) => write!(f, "terminal i/o error: {m}"),
        }
    }
}

impl std::error::Error for TerminalError {}
