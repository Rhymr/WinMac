use super::{Settings, Theme};
use crate::workspace::controller::WorkspaceController;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, CheckButton, DropDown, FontDialog, FontDialogButton, Label,
    ListBox, ListBoxRow, Orientation, SearchEntry, Separator, SpinButton, Stack, Window, pango,
};
use libadwaita::Application;
use std::cell::RefCell;
use std::rc::Rc;

const CATEGORIES: [(&str, &str); 5] = [
    ("appearance", "Appearance"),
    ("editor", "Editor"),
    ("rhyme", "Rhyme Highlighting"),
    ("completion", "Completions"),
    ("git", "Git"),
];

/// The two widgets `page()` doesn't cover: a field row (label + control)
/// used by the Appearance category, styled like the rest of the dialog's
/// left-aligned, margin-matched form rows.
fn field_row(label_text: &str, control: &impl IsA<gtk::Widget>) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 10);
    row.append(
        &Label::builder()
            .label(label_text)
            .halign(Align::Start)
            .width_chars(12)
            .build(),
    );
    row.append(control);
    row
}

/// A category page: a checkbox toggle plus an optional description label
/// underneath it, both left-aligned with the same margins.
fn page(toggle: &CheckButton, description: Option<&str>) -> GtkBox {
    let page = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(10)
        .margin_top(20)
        .margin_start(24)
        .margin_end(24)
        .build();
    page.append(toggle);
    if let Some(text) = description {
        let desc = Label::builder()
            .label(text)
            .halign(Align::Start)
            .wrap(true)
            .max_width_chars(60)
            .css_classes(vec!["dim-label", "caption"])
            .build();
        page.append(&desc);
    }
    page
}

/// `controller` is `None` when opened from the welcome screen (no
/// workspace loaded yet, so there's nothing to live-apply to) and `Some`
/// when opened from an already-open workspace's File menu.
pub fn show_settings_dialog(app: &Application, controller: Option<Rc<WorkspaceController>>) {
    let Some(parent) = app.active_window() else {
        return;
    };

    let settings = Settings::load();

    let dialog = Window::builder()
        .title("Settings — Rhymr")
        .transient_for(&parent)
        .modal(true)
        .default_width(820)
        .default_height(560)
        .css_classes(vec!["settings-window"])
        .build();

    let root = GtkBox::new(Orientation::Vertical, 0);

    // ==========================================
    // Sidebar: search + category list
    // ==========================================
    let sidebar = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .css_classes(vec!["settings-sidebar"])
        .build();
    sidebar.set_width_request(220);

    let search_entry = SearchEntry::builder()
        .placeholder_text("Search")
        .margin_top(12)
        .margin_bottom(8)
        .margin_start(12)
        .margin_end(12)
        .build();

    let category_list = ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .css_classes(vec!["settings-category-list"])
        .build();

    for (_, label) in CATEGORIES {
        let row = ListBoxRow::new();
        let row_label = Label::builder()
            .label(label)
            .halign(Align::Start)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(12)
            .margin_end(12)
            .build();
        row.set_child(Some(&row_label));
        category_list.append(&row);
    }

    sidebar.append(&search_entry);
    sidebar.append(&category_list);

    // ==========================================
    // Content: header + the selected category's page
    // ==========================================
    let content = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .css_classes(vec!["settings-content"])
        .build();

    let header_label = Label::builder()
        .halign(Align::Start)
        .margin_top(16)
        .margin_bottom(12)
        .margin_start(20)
        .css_classes(vec!["settings-header-title"])
        .build();
    content.append(&header_label);
    content.append(&Separator::new(Orientation::Horizontal));

    let stack = Stack::new();
    stack.set_vexpand(true);

    // ==========================================
    // Appearance: theme + app-wide font (the font dialog button covers both
    // family and size in one native picker)
    // ==========================================
    let theme_dropdown = DropDown::from_strings(&["Dark", "Light"]);
    theme_dropdown.set_selected(if settings.theme == Theme::Dark { 0 } else { 1 });

    let font_button = FontDialogButton::builder()
        .dialog(&FontDialog::builder().title("Font").build())
        .valign(Align::Center)
        .build();
    font_button.set_use_size(true);
    font_button.set_font_desc(&pango::FontDescription::from_string(&format!(
        "{} {}",
        settings.font_family, settings.font_size
    )));

    let appearance_page = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(14)
        .margin_top(20)
        .margin_start(24)
        .margin_end(24)
        .build();
    appearance_page.append(&field_row("Theme:", &theme_dropdown));
    appearance_page.append(&field_row("Font:", &font_button));
    stack.add_named(&appearance_page, Some("appearance"));

    let gutter_toggle = CheckButton::builder()
        .label("Show syllable count in the gutter")
        .active(settings.show_syllable_gutter)
        .build();
    let auto_indent_toggle = CheckButton::builder()
        .label("Auto-indent new lines")
        .active(settings.auto_indent)
        .build();
    let tab_width_row = GtkBox::new(Orientation::Horizontal, 10);
    let tab_width_spin = SpinButton::with_range(1.0, 8.0, 1.0);
    tab_width_spin.set_value(settings.tab_width as f64);
    tab_width_row.append(
        &Label::builder()
            .label("Tab width:")
            .halign(Align::Start)
            .build(),
    );
    tab_width_row.append(&tab_width_spin);

    let editor_page = page(&gutter_toggle, None);
    editor_page.append(&auto_indent_toggle);
    editor_page.append(&tab_width_row);
    stack.add_named(&editor_page, Some("editor"));

    let rhyme_toggle = CheckButton::builder()
        .label("Highlight rhyming syllables")
        .active(settings.rhyme_highlighting)
        .build();
    let rhyme_stop_at_blank_line_toggle = CheckButton::builder()
        .label("Don't match rhymes across a blank line")
        .active(settings.rhyme_stop_at_blank_line)
        .build();
    let rhyme_page = page(
        &rhyme_toggle,
        Some(
            "Colors the background of syllables that rhyme with another word elsewhere in the document.",
        ),
    );
    rhyme_page.append(&rhyme_stop_at_blank_line_toggle);
    stack.add_named(&rhyme_page, Some("rhyme"));

    let completion_toggle = CheckButton::builder()
        .label("Enable dictionary word completion")
        .active(settings.word_completion)
        .build();
    stack.add_named(
        &page(
            &completion_toggle,
            Some("Suggests words from the bundled dictionary as you type. Tab or Enter accepts a suggestion."),
        ),
        Some("completion"),
    );

    let git_toggle = CheckButton::builder()
        .label("Automatically stage changes when saving")
        .active(settings.git_autostage)
        .build();
    stack.add_named(&page(&git_toggle, None), Some("git"));

    content.append(&stack);

    let main_split = GtkBox::new(Orientation::Horizontal, 0);
    main_split.set_vexpand(true);
    main_split.append(&sidebar);
    main_split.append(&content);

    // ==========================================
    // Bottom bar: Cancel / Apply / OK, right-aligned
    // ==========================================
    let footer = GtkBox::new(Orientation::Horizontal, 10);
    footer.set_halign(Align::End);
    footer.set_margin_top(12);
    footer.set_margin_bottom(12);
    footer.set_margin_start(16);
    footer.set_margin_end(16);

    let cancel_btn = Button::builder().label("Cancel").build();
    let apply_btn = Button::builder().label("Apply").build();
    let ok_btn = Button::builder()
        .label("OK")
        .css_classes(vec!["suggested-action"])
        .build();

    footer.append(&cancel_btn);
    footer.append(&apply_btn);
    footer.append(&ok_btn);

    root.append(&main_split);
    root.append(&Separator::new(Orientation::Horizontal));
    root.append(&footer);
    dialog.set_child(Some(&root));

    // ==========================================
    // Wiring
    // ==========================================
    let stack_for_select = stack.clone();
    let header_for_select = header_label.clone();
    category_list.connect_row_selected(move |_, row| {
        if let Some(row) = row {
            let (name, label) = CATEGORIES[row.index() as usize];
            stack_for_select.set_visible_child_name(name);
            header_for_select.set_text(label);
        }
    });
    category_list.select_row(category_list.row_at_index(0).as_ref());

    let category_list_for_search = category_list.clone();
    search_entry.connect_search_changed(move |entry| {
        let query = entry.text().to_lowercase();
        let mut index = 0;
        while let Some(row) = category_list_for_search.row_at_index(index) {
            let (_, label) = CATEGORIES[index as usize];
            row.set_visible(query.is_empty() || label.to_lowercase().contains(&query));
            index += 1;
        }
    });

    let dialog_for_cancel = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_for_cancel.close();
    });

    // Reads the dialog's current widget state into a `Settings` value —
    // shared by the dirty-check below and by the actual save, so there's
    // one place that knows how to turn widgets into a `Settings`.
    let read_current: Rc<dyn Fn() -> Settings> = Rc::new({
        let gutter_toggle = gutter_toggle.clone();
        let auto_indent_toggle = auto_indent_toggle.clone();
        let tab_width_spin = tab_width_spin.clone();
        let rhyme_toggle = rhyme_toggle.clone();
        let rhyme_stop_at_blank_line_toggle = rhyme_stop_at_blank_line_toggle.clone();
        let completion_toggle = completion_toggle.clone();
        let git_toggle = git_toggle.clone();
        let theme_dropdown = theme_dropdown.clone();
        let font_button = font_button.clone();
        move || {
            let font_desc = font_button.font_desc().unwrap_or_else(|| {
                pango::FontDescription::from_string(&Settings::default().font_family)
            });
            let font_family = font_desc
                .family()
                .map(|f| f.to_string())
                .unwrap_or_else(|| Settings::default().font_family);
            let font_size = if font_desc.size() > 0 {
                (font_desc.size() / pango::SCALE).max(6) as u32
            } else {
                Settings::default().font_size
            };
            Settings {
                show_syllable_gutter: gutter_toggle.is_active(),
                rhyme_highlighting: rhyme_toggle.is_active(),
                rhyme_stop_at_blank_line: rhyme_stop_at_blank_line_toggle.is_active(),
                word_completion: completion_toggle.is_active(),
                auto_indent: auto_indent_toggle.is_active(),
                tab_width: tab_width_spin.value() as u32,
                git_autostage: git_toggle.is_active(),
                theme: if theme_dropdown.selected() == 0 {
                    Theme::Dark
                } else {
                    Theme::Light
                },
                font_family,
                font_size,
            }
        }
    });

    // What's currently saved on disk — Apply is only enabled once the
    // widgets diverge from this, and it's refreshed after every save so
    // Apply goes back to disabled until something changes again.
    let baseline = Rc::new(RefCell::new(settings));

    apply_btn.set_sensitive(false);
    let update_apply_sensitivity: Rc<dyn Fn()> = Rc::new({
        let apply_btn = apply_btn.clone();
        let read_current = read_current.clone();
        let baseline = baseline.clone();
        move || {
            apply_btn.set_sensitive(read_current() != *baseline.borrow());
        }
    });

    for toggle in [
        &gutter_toggle,
        &auto_indent_toggle,
        &rhyme_toggle,
        &rhyme_stop_at_blank_line_toggle,
        &completion_toggle,
        &git_toggle,
    ] {
        let f = update_apply_sensitivity.clone();
        toggle.connect_toggled(move |_| f());
    }
    let f = update_apply_sensitivity.clone();
    tab_width_spin.connect_value_changed(move |_| f());
    let f = update_apply_sensitivity.clone();
    theme_dropdown.connect_selected_notify(move |_| f());
    let f = update_apply_sensitivity.clone();
    font_button.connect_font_desc_notify(move |_| f());

    let apply: Rc<dyn Fn()> = Rc::new({
        let read_current = read_current.clone();
        let baseline = baseline.clone();
        let update_apply_sensitivity = update_apply_sensitivity.clone();
        move || {
            let current = read_current();
            current.save();
            crate::css::reload(&current);
            crate::css::sync_style_manager(&current);
            if let Some(controller) = &controller {
                controller.apply_settings(&current);
            }
            baseline.replace(current);
            update_apply_sensitivity();
        }
    });

    let apply_for_apply = apply.clone();
    apply_btn.connect_clicked(move |_| {
        apply_for_apply();
    });

    let dialog_for_ok = dialog.clone();
    ok_btn.connect_clicked(move |_| {
        apply();
        dialog_for_ok.close();
    });

    dialog.present();
}
