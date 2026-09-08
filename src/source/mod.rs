//! **External text sources** — read-only providers of a document corpus
//! the writer keeps outside the project (Apple Notes today; Google Docs and
//! others later, see issue #2). Each registered source renders as its own
//! labelled, collapsible tree at the bottom of the file panel
//! (`crate::app::source_panel`), workspace-independent.
//!
//! Rhymr never writes back to a source. Interactions are: browse, open a
//! document in a read-only editor buffer, copy its text/title, refresh.

pub mod apple_notes;
pub mod model;
mod registry;

pub use model::{DocId, SourceDoc, SourceError, SourceFolder, SourceStatus, SourceTree};
pub use registry::SourceRegistry;

/// A read-only external provider of text documents. Implementations must be
/// cheap to construct; the heavy work is in [`load`](TextSource::load) /
/// [`document_text`](TextSource::document_text), both of which the panel
/// calls on a worker thread — hence the `Send + Sync` bound.
pub trait TextSource: Send + Sync {
    /// Stable machine id, e.g. `"apple-notes"`.
    fn id(&self) -> &'static str;

    /// Human label for the tree's root row, e.g. `"Apple Notes"`.
    fn label(&self) -> &str;

    /// Bundled icon slug (`crate::app::icons`) for the root and folder rows.
    fn icon(&self) -> &'static str;

    /// Whether the source can be loaded right now.
    fn status(&self) -> SourceStatus;

    /// Point the source at the current workspace so any per-workspace cache
    /// (kept under `<workspace>/.rhymr/`) lands in the right place. Default:
    /// ignored.
    fn set_workspace(&self, _root: Option<&std::path::Path>) {}

    /// A possibly-stale tree available *instantly* from a local cache, so
    /// the panel can paint before [`load`](TextSource::load) finishes.
    /// Default: an empty tree.
    fn cached(&self) -> SourceTree {
        SourceTree::default()
    }

    /// Fetch the current folder/document tree. Blocking — never call on the
    /// GTK main thread.
    fn load(&self) -> Result<SourceTree, SourceError>;

    /// Full plain text of one document, for opening in a read-only buffer
    /// and for "Copy text". Blocking — never call on the GTK main thread.
    fn document_text(&self, id: &DocId) -> Result<String, SourceError>;
}
