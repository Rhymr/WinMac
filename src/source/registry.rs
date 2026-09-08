//! The one place external sources are registered. The file panel iterates
//! this; adding a backend means adding it here and nowhere else.

use super::TextSource;
use super::apple_notes::AppleNotesSource;
use std::sync::Arc;

/// Ordered set of the app's registered [`TextSource`]s.
pub struct SourceRegistry {
    sources: Vec<Arc<dyn TextSource>>,
}

impl SourceRegistry {
    /// The registry the app ships with. Apple Notes only for now — it
    /// reports `Unavailable` off macOS, so nothing renders there.
    pub fn with_defaults() -> Self {
        Self {
            sources: vec![Arc::new(AppleNotesSource::new())],
        }
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
