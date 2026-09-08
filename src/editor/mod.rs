pub mod completion;
pub mod stat;
pub mod vcs_gutter;

use crate::app::context_menu::{ContextMenu, hint};
use crate::rhyme::highlight::RhymeHighlight;
use crate::setting::Settings;
use completion::WordCompletionProvider;
use gtk::gdk;
use gtk::prelude::*;
use gtk::{EventSequenceState, Frame, GestureClick, PropagationPhase, ScrolledWindow};
use sourceview5::GutterRendererText;
use sourceview5::prelude::{BufferExt, GutterRendererExt, GutterRendererTextExt, ViewExt};
use sourceview5::{Buffer as SourceBuffer, Completion, Gutter, View as SourceView};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

/// How long to wait after the last keystroke before writing to disk.
const AUTOSAVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(600);

/// How long to wait after the last keystroke before recomputing the VCS
/// gutter's per-line diff vs HEAD.
const VCS_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(400);

pub struct TextEditor {
    frame: Frame,
    source_view: SourceView,
    buffer: SourceBuffer,
    current_path: Rc<RefCell<Option<PathBuf>>>,
    // Has this tab been edited since it was opened, and does that edit not
    // yet have a successful autosave behind it? Used by "Close Unmodified
    // Tabs" (see workspace::Workspace) — set on the first real edit after
    // load, cleared once the debounced autosave actually writes to disk.
    modified: Rc<Cell<bool>>,
    gutter: Gutter,
    syllable_renderer: RefCell<Option<GutterRendererText>>,
    vcs_renderer: Rc<RefCell<Option<vcs_gutter::VcsGutterRenderer>>>,
    completion: Completion,
    word_provider: RefCell<Option<WordCompletionProvider>>,
    rhyme_highlight: RefCell<Option<RhymeHighlight>>,
}

impl Default for TextEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEditor {
    pub fn new() -> Self {
        let settings = crate::setting::Settings::load();

        // Create the source buffer and view
        let buffer = SourceBuffer::new(None);
        let source_view = SourceView::builder()
            .buffer(&buffer)
            .monospace(true)
            .show_line_numbers(true)
            .show_line_marks(true)
            .tab_width(settings.tab_width)
            .auto_indent(settings.auto_indent)
            .indent_width(settings.tab_width as i32)
            .highlight_current_line(true)
            .background_pattern(sourceview5::BackgroundPatternType::None)
            .smart_backspace(true)
            .smart_home_end(sourceview5::SmartHomeEndType::After)
            .hexpand(true)
            .vexpand(true)
            .build();

        // Save `AUTOSAVE_DEBOUNCE` after the last keystroke, so typing
        // doesn't hit the disk on every character. `current_path` is unset
        // until `set_path()` is called (after the tab's initial content is
        // loaded), so the programmatic `set_text()` calls below don't
        // trigger a spurious save.
        let current_path: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));

        // Guards against a save that was already in flight when a newer
        // keystroke reschedules another one — only the latest write should
        // actually land.
        let save_generation = Rc::new(Cell::new(0u64));
        let modified = Rc::new(Cell::new(false));
        let vcs_renderer: Rc<RefCell<Option<vcs_gutter::VcsGutterRenderer>>> =
            Rc::new(RefCell::new(None));

        let buffer_clone = buffer.clone();
        let path_ref = current_path.clone();
        let generation_ref = save_generation.clone();
        let modified_ref = modified.clone();
        buffer_clone.connect_changed(move |buf| {
            let Some(path) = path_ref.borrow().clone() else {
                return;
            };
            modified_ref.set(true);
            let text = buf
                .text(&buf.start_iter(), &buf.end_iter(), false)
                .to_string();

            let this_generation = generation_ref.get() + 1;
            generation_ref.set(this_generation);

            let generation_for_timeout = generation_ref.clone();
            let modified_for_timeout = modified_ref.clone();
            glib::timeout_add_local_once(AUTOSAVE_DEBOUNCE, move || {
                // A newer edit came in while this was waiting — let that one win.
                if generation_for_timeout.get() != this_generation {
                    return;
                }
                if let Err(e) = std::fs::write(&path, text) {
                    eprintln!("Auto-save failed for {path:?}: {e}");
                    return;
                }
                modified_for_timeout.set(false);
                if crate::setting::Settings::load().git_autostage
                    && let Some(root) = find_git_root(&path)
                {
                    crate::git::ops::stage_all_changes(&root);
                }
            });
        });

        // VCS gutter: recompute the per-line diff vs HEAD on its own short
        // debounce after edits (independent of the autosave one above). The
        // handler is a no-op whenever the renderer isn't attached (the
        // "Show VCS gutter" setting is off).
        {
            let buffer_for_vcs = buffer.clone();
            let path_for_vcs = current_path.clone();
            let renderer_for_vcs = vcs_renderer.clone();
            let vcs_generation = Rc::new(Cell::new(0u64));
            buffer.connect_changed(move |_| {
                if renderer_for_vcs.borrow().is_none() {
                    return;
                }
                let this_generation = vcs_generation.get() + 1;
                vcs_generation.set(this_generation);

                let buffer_for_vcs = buffer_for_vcs.clone();
                let path_for_vcs = path_for_vcs.clone();
                let renderer_for_vcs = renderer_for_vcs.clone();
                let vcs_generation = vcs_generation.clone();
                glib::timeout_add_local_once(VCS_DEBOUNCE, move || {
                    if vcs_generation.get() != this_generation {
                        return;
                    }
                    recompute_vcs(&buffer_for_vcs, &path_for_vcs, &renderer_for_vcs);
                });
            });
        }

        // Disable bracket matching
        buffer.set_highlight_matching_brackets(false);

        let scheme_manager = sourceview5::StyleSchemeManager::default();
        scheme_manager.append_search_path("assets/styles");
        let style_scheme = scheme_manager
            .scheme(scheme_id(settings.theme))
            .expect("Failed to load rhymr style scheme");
        buffer.set_style_scheme(Some(&style_scheme));

        // Configure the gutter
        let gutter = ViewExt::gutter(&source_view, gtk::TextWindowType::Left);
        gutter.set_css_classes(&["rhyme-editor-gutter"]);

        let completion = ViewExt::completion(&source_view);

        // Add custom CSS classes for the editor
        source_view.set_css_classes(&["rhyme-editor-view", "rhyme-editor"]);

        // Add scrolling support
        let scroll = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&source_view)
            .build();

        let frame = Frame::builder()
            .child(&scroll)
            .css_classes(vec!["rhyme-editor-frame"])
            .build();

        let editor = Self {
            frame,
            source_view,
            buffer,
            current_path,
            modified,
            gutter,
            syllable_renderer: RefCell::new(None),
            vcs_renderer,
            completion,
            word_provider: RefCell::new(None),
            rhyme_highlight: RefCell::new(None),
        };

        editor.setup_context_menu();

        // Set initial empty state
        editor.set_text("");
        editor.apply_settings(&settings);

        editor
    }

    /// Right-click Cut/Copy/Paste/Delete/Select All, styled to match every
    /// other menu in the app instead of GtkTextView's native popup (which,
    /// under this app's CSS reset, renders with no visible background at
    /// all — see base.scss's `* { background: none; }`). A capture-phase
    /// `GestureClick` intercepts the press before GtkText's own internal
    /// click gesture (bubble phase) gets a chance to open that native menu,
    /// and claims the sequence so it never does.
    fn setup_context_menu(&self) {
        let gesture = GestureClick::new();
        gesture.set_button(gdk::BUTTON_SECONDARY);
        gesture.set_propagation_phase(PropagationPhase::Capture);

        let frame = self.frame.clone();
        let source_view = self.source_view.clone();
        let buffer = self.buffer.clone();
        gesture.connect_pressed(move |gesture, _n_press, x, y| {
            gesture.set_state(EventSequenceState::Claimed);

            let menu = ContextMenu::new(&frame);
            let has_selection = buffer.has_selection();

            let sv = source_view.clone();
            let cut_btn = menu.add_item("Cut", Some(hint::CUT), None, move || {
                sv.emit_cut_clipboard();
            });
            cut_btn.set_sensitive(has_selection);

            let sv = source_view.clone();
            let copy_btn = menu.add_item("Copy", Some(hint::COPY), None, move || {
                sv.emit_copy_clipboard();
            });
            copy_btn.set_sensitive(has_selection);

            let sv = source_view.clone();
            menu.add_item("Paste", Some(hint::PASTE), None, move || {
                sv.emit_paste_clipboard();
            });

            let buffer_for_delete = buffer.clone();
            let delete_btn = menu.add_item(
                "Delete",
                Some(hint::DELETE),
                Some("destructive-menu-item"),
                move || {
                    buffer_for_delete.delete_selection(true, true);
                },
            );
            delete_btn.set_sensitive(has_selection);

            menu.add_separator();

            let buffer_for_select_all = buffer.clone();
            menu.add_item("Select All", Some(hint::SELECT_ALL), None, move || {
                let (start, end) = (
                    buffer_for_select_all.start_iter(),
                    buffer_for_select_all.end_iter(),
                );
                buffer_for_select_all.select_range(&start, &end);
            });

            menu.popup_at(&source_view, x, y);
        });
        self.source_view.add_controller(gesture);
    }

    /// Toggle the syllable gutter, completion provider, and rhyme
    /// highlighting to match `settings` (adding/removing each live rather
    /// than requiring the tab to be reopened), and update tab
    /// width/auto-indent, which are plain `SourceView` properties.
    pub fn apply_settings(&self, settings: &Settings) {
        self.source_view.set_tab_width(settings.tab_width);
        self.source_view.set_indent_width(settings.tab_width as i32);
        self.source_view.set_auto_indent(settings.auto_indent);

        // Font family/size are applied app-wide via the `--app-font-*` CSS
        // variables (see crate::css and base.scss's `* {}` rule) — only the
        // GtkSourceView style scheme (syntax/background colors, separate
        // from the app-wide CSS palette) needs switching here to follow
        // light/dark live.
        if let Some(scheme) =
            sourceview5::StyleSchemeManager::default().scheme(scheme_id(settings.theme))
        {
            self.buffer.set_style_scheme(Some(&scheme));
        }

        let mut renderer_slot = self.syllable_renderer.borrow_mut();
        match (renderer_slot.is_some(), settings.show_syllable_gutter) {
            (false, true) => {
                let renderer = create_syllable_renderer(&self.buffer);
                self.gutter.insert(&renderer, -20); // Position right after line numbers (-30)
                *renderer_slot = Some(renderer);
            }
            (true, false) => {
                if let Some(renderer) = renderer_slot.take() {
                    self.gutter.remove(&renderer);
                }
            }
            _ => {}
        }
        drop(renderer_slot);

        // VCS gutter — same add/remove-live pattern as the syllable renderer,
        // plus a colour refresh so the bars follow a live theme switch.
        let vcs_colors = vcs_colors(settings.theme);
        let mut vcs_slot = self.vcs_renderer.borrow_mut();
        match (vcs_slot.is_some(), settings.show_vcs_gutter) {
            (false, true) => {
                let renderer = vcs_gutter::VcsGutterRenderer::new();
                renderer.set_colors(vcs_colors.0, vcs_colors.1, vcs_colors.2);
                // Rightmost in the gutter — after line numbers (-30) and the
                // syllable count (-20) — so the change bar sits flush against
                // the text edge, JetBrains-style.
                self.gutter.insert(&renderer, 10);
                *vcs_slot = Some(renderer);
            }
            (true, false) => {
                if let Some(renderer) = vcs_slot.take() {
                    self.gutter.remove(&renderer);
                }
            }
            (true, true) => {
                if let Some(renderer) = vcs_slot.as_ref() {
                    renderer.set_colors(vcs_colors.0, vcs_colors.1, vcs_colors.2);
                }
            }
            _ => {}
        }
        drop(vcs_slot);
        recompute_vcs(&self.buffer, &self.current_path, &self.vcs_renderer);

        let mut provider_slot = self.word_provider.borrow_mut();
        match (provider_slot.is_some(), settings.word_completion) {
            (false, true) => {
                let provider = WordCompletionProvider::new();
                self.completion.add_provider(&provider);
                *provider_slot = Some(provider);
            }
            (true, false) => {
                if let Some(provider) = provider_slot.take() {
                    self.completion.remove_provider(&provider);
                }
            }
            _ => {}
        }
        drop(provider_slot);

        let mut rhyme_slot = self.rhyme_highlight.borrow_mut();
        match (rhyme_slot.is_some(), settings.rhyme_highlighting) {
            (false, true) => {
                *rhyme_slot = Some(crate::rhyme::highlight::attach(&self.buffer));
            }
            (true, false) => {
                if let Some(handle) = rhyme_slot.take() {
                    handle.detach();
                }
            }
            _ => {}
        }
    }

    pub fn set_text(&self, text: &str) {
        self.buffer.set_text(text);
        self.source_view.grab_focus();
    }

    pub fn get_text(&self) -> String {
        self.buffer
            .text(&self.buffer.start_iter(), &self.buffer.end_iter(), false)
            .to_string()
    }

    /// Associate this editor with the file it should auto-save to.
    /// Deliberately separate from construction/`set_text()`, so loading a
    /// tab's initial content never triggers a spurious save.
    pub fn set_path(&self, path: PathBuf) {
        self.current_path.replace(Some(path));
        recompute_vcs(&self.buffer, &self.current_path, &self.vcs_renderer);
    }

    pub fn get_widget(&self) -> &Frame {
        &self.frame
    }

    /// Has this tab been edited since it was opened, with that edit not yet
    /// written to disk by the autosave debounce? Used by "Close Unmodified
    /// Tabs" in the tab context menu (see workspace::Workspace).
    pub fn is_modified(&self) -> bool {
        self.modified.get()
    }

    /// Notify `f` on every buffer edit — used by the status bar's live
    /// word-count listener, independent of the autosave debounce above.
    pub fn connect_changed(&self, f: impl Fn() + 'static) {
        self.buffer.connect_changed(move |_| f());
    }

    /// Notify `f` whenever the caret moves — the status bar's line:col
    /// readout.
    pub fn connect_cursor_notify(&self, f: impl Fn() + 'static) {
        self.buffer.connect_cursor_position_notify(move |_| f());
    }
}

/// The GtkSourceView style scheme id (see assets/styles/*.xml) matching
/// `theme` — kept in one place so `new()` and `apply_settings()` can't
/// drift onto different scheme names for the same theme.
fn scheme_id(theme: crate::setting::Theme) -> &'static str {
    match theme {
        crate::setting::Theme::Dark => "rhymr",
        crate::setting::Theme::Light => "rhymr-light",
    }
}

impl Clone for TextEditor {
    fn clone(&self) -> Self {
        let editor = TextEditor::new();
        editor.set_text(&self.get_text());
        if let Some(path) = self.current_path.borrow().clone() {
            editor.set_path(path);
        }
        editor
    }
}

/// Builds a gutter renderer that shows each line's syllable count, live —
/// separate from `TextEditor::new()` so `apply_settings` can add this back
/// after the setting was toggled off and back on again.
fn create_syllable_renderer(buffer: &SourceBuffer) -> GutterRendererText {
    let syllable_renderer = GutterRendererText::new();
    syllable_renderer.set_css_classes(&["syllable-count"]);
    syllable_renderer.set_xalign(0.5);
    syllable_renderer.set_yalign(0.5);

    let buffer_clone = buffer.clone();
    syllable_renderer.connect_query_data(move |renderer, _line_obj, line_num| {
        if let Some(iter) = buffer_clone.iter_at_line(line_num as i32) {
            let end_iter = if let Some(next_iter) = buffer_clone.iter_at_line(line_num as i32 + 1) {
                next_iter
            } else {
                buffer_clone.end_iter()
            };

            let line = buffer_clone.text(&iter, &end_iter, false);
            if !line.trim().is_empty() {
                let syllables = crate::editor::stat::count_syllables(&line);
                renderer.set_text(&syllables.to_string());
            } else {
                renderer.set_text("");
            }
        }
    });

    syllable_renderer
}

/// Walk up from a file's directory looking for the workspace's `.git` —
/// TextEditor doesn't otherwise know the workspace root.
fn find_git_root(file_path: &std::path::Path) -> Option<PathBuf> {
    let mut dir = file_path.parent();
    while let Some(current) = dir {
        if current.join(".git").is_dir() {
            return Some(current.to_path_buf());
        }
        dir = current.parent();
    }
    None
}

/// Recompute the VCS gutter's per-line diff from the buffer's current text
/// and hand it to the renderer, if one is attached. Clears it when there's
/// no path or no repo.
fn recompute_vcs(
    buffer: &SourceBuffer,
    path: &Rc<RefCell<Option<PathBuf>>>,
    renderer: &Rc<RefCell<Option<vcs_gutter::VcsGutterRenderer>>>,
) {
    let Some(renderer) = renderer.borrow().clone() else {
        return;
    };
    let Some(path) = path.borrow().clone() else {
        renderer.set_changes(std::collections::HashMap::new());
        return;
    };
    let Some(root) = find_git_root(&path) else {
        renderer.set_changes(std::collections::HashMap::new());
        return;
    };
    let text = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string();
    renderer.set_changes(crate::git::ops::line_changes(&root, &path, &text));
}

/// The (added, modified, deleted) colour triple for the VCS gutter bars,
/// matching the `--vcs-*` / `--destructive` palette rows in `crate::css`.
fn vcs_colors(theme: crate::setting::Theme) -> (gdk::RGBA, gdk::RGBA, gdk::RGBA) {
    let (added, modified, deleted) = match theme {
        crate::setting::Theme::Dark => ("#59a869", "#4a88c7", "#c75450"),
        crate::setting::Theme::Light => ("#4a8f3c", "#3573b8", "#c0392b"),
    };
    let parse = |hex: &str| {
        hex.parse::<gdk::RGBA>()
            .unwrap_or_else(|_| gdk::RGBA::new(0.5, 0.5, 0.5, 1.0))
    };
    (parse(added), parse(modified), parse(deleted))
}
