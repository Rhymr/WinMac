//! A `GtkWidget` that paints a single line of text rotated 90° (reading
//! bottom-to-top) — GTK4's `GtkLabel` dropped `set_angle`, and the classic
//! JetBrains tool-window stripe buttons need vertical labels.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene};
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct VerticalLabel {
        pub text: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VerticalLabel {
        const NAME: &'static str = "RhymrVerticalLabel";
        type Type = super::VerticalLabel;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for VerticalLabel {}

    impl WidgetImpl for VerticalLabel {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let layout = self.obj().create_pango_layout(Some(&self.text.borrow()));
            let (w, h) = layout.pixel_size();
            // Rotated -90°: on-screen width comes from the text's height,
            // on-screen height from the text's width. A little breathing room.
            let size = match orientation {
                gtk::Orientation::Horizontal => h + 6,
                _ => w + 10,
            };
            (size, size, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let text = self.text.borrow();
            if text.is_empty() {
                return;
            }
            let layout = obj.create_pango_layout(Some(&text));
            let (tw, th) = layout.pixel_size();
            let (tw, th) = (tw as f32, th as f32);
            let (wa, ha) = (obj.width() as f32, obj.height() as f32);

            snapshot.save();
            // Place the layout origin so that, after a -90° rotation, the
            // text is centred and runs from the bottom edge upward.
            snapshot.translate(&graphene::Point::new((wa - th) / 2.0, (ha + tw) / 2.0));
            snapshot.rotate(-90.0);
            snapshot.append_layout(&layout, &obj.color());
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    pub struct VerticalLabel(ObjectSubclass<imp::VerticalLabel>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl VerticalLabel {
    pub fn new(text: &str) -> Self {
        let obj: Self = glib::Object::builder().build();
        *obj.imp().text.borrow_mut() = text.to_string();
        obj
    }
}
