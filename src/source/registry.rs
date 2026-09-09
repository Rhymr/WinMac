//! The one place external sources are registered. The file panel iterates
//! this; adding a backend means adding it here and nowhere else.

use super::TextSource;
use std::sync::Arc;

/// Ordered set of the app's registered [`TextSource`]s.
pub struct SourceRegistry {
    sources: Vec<Arc<dyn TextSource>>,
}

impl SourceRegistry {
    /// The registry the app ships with. Apple Notes is the only backend
    /// today, and it only exists on macOS — so off macOS it isn't even
    /// constructed and `sources()` comes back empty (the panel then hides
    /// itself). A future non-macOS backend would be pushed unconditionally
    /// here; Apple Notes stays `cfg`-gated.
    pub fn with_defaults() -> Self {
        let sources: Vec<Arc<dyn TextSource>> = vec![
            #[cfg(target_os = "macos")]
            Arc::new(super::apple_notes::AppleNotesSource::new()),
        ];

        Self { sources }
    }

    /// Every registered source, in display order.
    pub fn sources(&self) -> &[Arc<dyn TextSource>] {
        &self.sources
    }
}

impl Default for SourceRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}
