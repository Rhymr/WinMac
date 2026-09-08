//! A `GtkSourceGutterRenderer` subclass that paints a thin colored bar in
//! the editor gutter next to each line changed vs git HEAD — green added,
//! blue modified, red/seam deleted, JetBrains-style.
//!
//! The per-line diff itself is computed elsewhere
//! ([`crate::git::ops::line_changes`]); this only draws. Colors are pushed
//! in by the caller (resolved from the active theme) since a renderer can't
//! read CSS custom properties mid-snapshot.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, prelude::*};
use sourceview5::subclass::prelude::*;

use crate::git::ops::LineChange;

/// Bar width in px. Kept narrow so it reads as a margin accent, not a column.
const BAR_WIDTH: f32 = 3.0;
/// Height of the little marker drawn for a pure deletion seam.
const SEAM_HEIGHT: f32 = 3.0;

mod imp {
    use super::*;

    pub struct VcsGutterRenderer {
        pub changes: RefCell<HashMap<usize, LineChange>>,
        pub added: Cell<gdk::RGBA>,
        pub modified: Cell<gdk::RGBA>,
        pub deleted: Cell<gdk::RGBA>,
    }

    impl Default for VcsGutterRenderer {
        fn default() -> Self {
            let clear = gdk::RGBA::new(0.0, 0.0, 0.0, 0.0);
            Self {
                changes: RefCell::new(HashMap::new()),
                added: Cell::new(clear),
                modified: Cell::new(clear),
                deleted: Cell::new(clear),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VcsGutterRenderer {
        const NAME: &'static str = "RhymrVcsGutterRenderer";
        type Type = super::VcsGutterRenderer;
        type ParentType = sourceview5::GutterRenderer;
    }

    impl ObjectImpl for VcsGutterRenderer {}

    impl WidgetImpl for VcsGutterRenderer {}

    impl GutterRendererImpl for VcsGutterRenderer {
        fn snapshot_line(
            &self,
            snapshot: &gtk::Snapshot,
            lines: &sourceview5::GutterLines,
            line: u32,
        ) {
            let Some(change) = self.changes.borrow().get(&(line as usize)).copied() else {
                return;
            };

            let (y, height) =
                lines.line_yrange(line, sourceview5::GutterRendererAlignmentMode::Cell);
            let (y, height) = (y as f32, height as f32);

            // Flush against the gutter's right edge, so the bar butts up
            // against the text (JetBrains-style) rather than sitting far left.
            let x = (self.obj().width() as f32 - BAR_WIDTH).max(0.0);
            let rect = match change {
                LineChange::Deleted => {
                    graphene::Rect::new(x - 1.0, y, BAR_WIDTH + 1.0, SEAM_HEIGHT)
                }
                _ => graphene::Rect::new(x, y, BAR_WIDTH, height),
            };
            let color = match change {
                LineChange::Added => self.added.get(),
                LineChange::Modified => self.modified.get(),
                LineChange::Deleted => self.deleted.get(),
            };
            snapshot.append_color(&color, &rect);
        }
    }
}

glib::wrapper! {
    pub struct VcsGutterRenderer(ObjectSubclass<imp::VcsGutterRenderer>)
        @extends sourceview5::GutterRenderer, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for VcsGutterRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl VcsGutterRenderer {
    pub fn new() -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.set_size_request((BAR_WIDTH as i32) + 1, -1);
        obj.add_css_class("vcs-gutter");
        obj
    }

    /// Set the three change colors (already resolved from the theme).
    pub fn set_colors(&self, added: gdk::RGBA, modified: gdk::RGBA, deleted: gdk::RGBA) {
        let imp = self.imp();
        imp.added.set(added);
        imp.modified.set(modified);
        imp.deleted.set(deleted);
        self.queue_draw();
    }

    /// Replace the per-line change map and repaint.
    pub fn set_changes(&self, changes: HashMap<usize, LineChange>) {
        *self.imp().changes.borrow_mut() = changes;
        self.queue_draw();
    }
}
