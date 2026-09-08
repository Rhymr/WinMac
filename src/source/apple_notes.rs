//! Apple Notes as a [`TextSource`]. macOS only — reads via `osascript`
//! (`crate::platform::fetch_apple_notes`), keeps a JSON snapshot in the
//! workspace's `.rhymr/` for instant startup, and serves note bodies
//! straight from the last load.

use super::TextSource;
use super::model::{DocId, SourceDoc, SourceError, SourceFolder, SourceStatus, SourceTree};
use crate::platform::NoteRow;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

/// A document id is `folder\x1ftitle` so notes with the same title in
/// different folders stay distinct.
fn doc_id(folder: &str, title: &str) -> DocId {
    DocId(format!("{folder}\u{1f}{title}"))
}

#[derive(Serialize, Deserialize, Default)]
struct Snapshot {
    notes: Vec<SnapNote>,
}

#[derive(Serialize, Deserialize)]
struct SnapNote {
    folder: String,
    title: String,
    body: String,
}

pub struct AppleNotesSource {
    /// Bodies from the most recent [`load`](TextSource::load) /
    /// [`cached`](TextSource::cached), keyed by [`DocId`] — `document_text`
    /// serves from here so opening a note needs no second `osascript` call.
    /// `Mutex` (not `RefCell`) because the panel calls this from a worker
    /// thread.
    bodies: Mutex<HashMap<DocId, String>>,
    /// Current workspace root — the JSON snapshot lives in its `.rhymr/`.
    workspace: Mutex<Option<PathBuf>>,
}

impl AppleNotesSource {
    pub fn new() -> Self {
        Self {
            bodies: Mutex::new(HashMap::new()),
            workspace: Mutex::new(None),
        }
    }

    fn cache_path(&self) -> Option<PathBuf> {
        let scope = crate::setting::Settings::load().notes_cache_scope;
        let guard = self
            .workspace
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        crate::config::apple_notes_cache(guard.as_deref(), scope)
    }

    /// Turn flat `(folder, title, body)` rows into a one-level tree,
    /// remembering every body for `document_text`.
    fn ingest(&self, rows: Vec<NoteRow>) -> SourceTree {
        let mut folders: Vec<SourceFolder> = Vec::new();
        let mut bodies = self.bodies.lock().unwrap_or_else(PoisonError::into_inner);
        bodies.clear();

        for (folder, title, body) in rows {
            let folder = if folder.trim().is_empty() {
                "Notes".to_string()
            } else {
                folder.trim().to_string()
            };
            let title = if title.trim().is_empty() {
                "Untitled".to_string()
            } else {
                title.trim().to_string()
            };
            let id = doc_id(&folder, &title);
            bodies.insert(id.clone(), body);

            let entry = match folders.iter_mut().find(|f| f.name == folder) {
                Some(existing) => existing,
                None => {
                    folders.push(SourceFolder {
                        name: folder,
                        ..Default::default()
                    });
                    folders
                        .last_mut()
                        .expect("just pushed a folder, so last_mut is Some")
                }
            };
            entry.docs.push(SourceDoc { id, title });
        }

        for folder in &mut folders {
            folder.docs.sort_by_key(|d| d.title.to_lowercase());
        }
        folders.sort_by_key(|f| f.name.to_lowercase());

        SourceTree {
            folders,
            docs: Vec::new(),
        }
    }

    fn read_snapshot(&self) -> Vec<NoteRow> {
        let Some(path) = self.cache_path() else {
            return Vec::new();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Vec::new();
        };
        let snapshot: Snapshot = serde_json::from_str(&text).unwrap_or_default();
        snapshot
            .notes
            .into_iter()
            .map(|n| (n.folder, n.title, n.body))
            .collect()
    }

    fn write_snapshot(&self, rows: &[NoteRow]) {
        let Some(path) = self.cache_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let snapshot = Snapshot {
            notes: rows
                .iter()
                .map(|(folder, title, body)| SnapNote {
                    folder: folder.clone(),
                    title: title.clone(),
                    body: body.clone(),
                })
                .collect(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&snapshot) {
            let _ = std::fs::write(&path, json);
        }
    }
}

impl Default for AppleNotesSource {
    fn default() -> Self {
        Self::new()
    }
}

impl TextSource for AppleNotesSource {
    fn id(&self) -> &'static str {
        "apple-notes"
    }

    fn label(&self) -> &str {
        "Apple Notes"
    }

    fn icon(&self) -> &'static str {
        "folder-ios"
    }

    fn status(&self) -> SourceStatus {
        if cfg!(target_os = "macos") {
            SourceStatus::Ready
        } else {
            SourceStatus::Unavailable("Apple Notes is macOS only".to_string())
        }
    }

    fn set_workspace(&self, root: Option<&Path>) {
        *self
            .workspace
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = root.map(Path::to_path_buf);
    }

    fn cached(&self) -> SourceTree {
        let rows = self.read_snapshot();
        self.ingest(rows)
    }

    fn load(&self) -> Result<SourceTree, SourceError> {
        if !cfg!(target_os = "macos") {
            return Err(SourceError::Unsupported);
        }
        let rows = crate::platform::fetch_apple_notes().map_err(|e| {
            log::warn!("apple-notes: fetch failed: {e}");
            SourceError::Io(e)
        })?;
        log::debug!("apple-notes: fetched {} notes", rows.len());
        self.write_snapshot(&rows);
        Ok(self.ingest(rows))
    }

    fn document_text(&self, id: &DocId) -> Result<String, SourceError> {
        if let Some(body) = self
            .bodies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
        {
            return Ok(body.clone());
        }
        // Only a stale snapshot was cached — do a fresh load and retry.
        log::debug!("apple-notes: body cache miss for {id:?}, reloading");
        self.load()?;
        self.bodies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
            .ok_or_else(|| {
                log::warn!("apple-notes: note {id:?} not found after reload");
                SourceError::Parse(format!("note {id:?} not found"))
            })
    }
}
