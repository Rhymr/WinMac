use super::{IconTheme, Settings, Theme};
use crate::app::context_menu::ContextMenu;
use crate::workspace::controller::WorkspaceController;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, CheckButton, FontDialog, FontDialogButton, Grid, Label, ListBox,
    ListBoxRow, Orientation, SearchEntry, Separator, SpinButton, Stack, Window, pango,
};
use libadwaita::Application;
use std::cell::RefCell;
use std::rc::Rc;

/// `(stack name, sidebar label, breadcrumb parent group)` — the parent
/// group is shown before the label in the content header, JetBrains-style
/// ("Appearance & Behavior › Appearance"). An empty parent shows just the
/// label.
const CATEGORIES: [(&str, &str, &str); 5] = [
    ("appearance", "Appearance", "Appearance & Behavior"),
    ("editor", "Editor", ""),
    ("rhyme", "Rhyme Highlighting", "Editor"),
    ("completion", "Completions", "Editor"),
    ("git", "Git", "Version Control"),
];

/// A category page: a tight vertical stack of section headers and form
/// grids, on the flat content background (no inset panel).
fn settings_page() -> GtkBox {
    GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .margin_top(14)
        .margin_start(20)
        .margin_end(20)
        .css_classes(["settings-page"])
        .build()
}

/// A small bold muted sub-header above a *sub*-group of rows (only used
/// where a page has more than one group — the category name itself is
/// already in the breadcrumb header).
fn section_header(text: &str) -> Label {
    Label::builder()
        .label(text)
        .halign(Align::Start)
        .margin_top(14)
        .margin_bottom(2)
        .css_classes(["settings-section"])
        .build()
}

/// A 2-column form grid: right-aligned labels in a fixed-width column 0,
/// left-aligned controls in column 1.
fn form_grid() -> Grid {
    Grid::builder()
        .row_spacing(8)
        .column_spacing(12)
        .margin_start(4)
        .build()
}

/// Attach a `label:` / control pair at `row` of `grid`.
fn grid_field(grid: &Grid, row: i32, label_text: &str, control: &impl IsA<gtk::Widget>) {
    grid.attach(
        &Label::builder()
            .label(label_text)
            .halign(Align::End)
            .width_request(120)
            .css_classes(["settings-field-label"])
            .build(),
        0,
        row,
        1,
        1,
    );
    grid.attach(control, 1, row, 1, 1);
}

/// Attach a full-width checkbox (it carries its own label) at `row`.
fn grid_check(grid: &Grid, row: i32, check: &CheckButton) {
    grid.attach(check, 0, row, 2, 1);
}

/// A wrapped, dimmed explanatory paragraph under a group.
fn description_label(text: &str) -> Label {
    Label::builder()
        .label(text)
        .halign(Align::Start)
        .wrap(true)
        .max_width_chars(60)
        .margin_top(4)
        .css_classes(["dim-label", "caption"])
        .build()
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

    for (_, label, _) in CATEGORIES {
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

    // Breadcrumb-style header ("Appearance & Behavior › Appearance").
    let header_label = Label::builder()
        .halign(Align::Start)
        .margin_top(14)
        .margin_bottom(10)
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
    // Late-bound "widgets changed" hook — the dropdowns are built before
    // `update_apply_sensitivity` exists, so their `on_change` calls through
    // this slot, which is filled in once that closure is defined.
    let mark_dirty: Rc<RefCell<Box<dyn Fn()>>> = Rc::new(RefCell::new(Box::new(|| {})));

    let (theme_dropdown, theme_selected) = ContextMenu::select_dropdown(
        &["Dark", "Light"],
        if settings.theme == Theme::Dark { 0 } else { 1 },
        {
            let mark_dirty = mark_dirty.clone();
            move |_| (mark_dirty.borrow())()
        },
    );

    // Icon set: "Color" is the JetBrains NetIcons colour glyphs; "Monochrome"
    // is the flat grey set, which then follows the light/dark theme.
    let (icon_theme_dropdown, icon_theme_selected) = ContextMenu::select_dropdown(
        &["Color", "Monochrome"],
        if settings.icon_theme == IconTheme::Color {
            0
        } else {
            1
        },
        {
            let mark_dirty = mark_dirty.clone();
            move |_| (mark_dirty.borrow())()
        },
    );

    let font_button = FontDialogButton::builder()
        .dialog(&FontDialog::builder().title("Font").build())
        .valign(Align::Center)
        .build();
    font_button.set_use_size(true);
    font_button.set_font_desc(&pango::FontDescription::from_string(&format!(
        "{} {}",
        settings.font_family, settings.font_size
    )));

    let appearance_page = settings_page();
    let appearance_grid = form_grid();
    grid_field(&appearance_grid, 0, "Theme:", &theme_dropdown);
    grid_field(&appearance_grid, 1, "Icons:", &icon_theme_dropdown);
    grid_field(&appearance_grid, 2, "Editor font:", &font_button);
    appearance_page.append(&appearance_grid);
    stack.add_named(&appearance_page, Some("appearance"));

    let gutter_toggle = CheckButton::builder()
        .label("Show syllable count in the gutter")
        .active(settings.show_syllable_gutter)
        .build();
    let vcs_gutter_toggle = CheckButton::builder()
        .label("Show VCS change markers in the gutter")
        .active(settings.show_vcs_gutter)
        .build();
    let auto_indent_toggle = CheckButton::builder()
        .label("Auto-indent new lines")
        .active(settings.auto_indent)
        .build();
    let tab_width_spin = SpinButton::with_range(1.0, 8.0, 1.0);
    tab_width_spin.set_value(settings.tab_width as f64);

    let editor_page = settings_page();
    editor_page.append(&section_header("Gutter"));
    let gutter_grid = form_grid();
    grid_check(&gutter_grid, 0, &gutter_toggle);
    grid_check(&gutter_grid, 1, &vcs_gutter_toggle);
    editor_page.append(&gutter_grid);
    editor_page.append(&section_header("Indentation"));
    let indent_grid = form_grid();
    grid_check(&indent_grid, 0, &auto_indent_toggle);
    grid_field(&indent_grid, 1, "Tab width:", &tab_width_spin);
    editor_page.append(&indent_grid);
    stack.add_named(&editor_page, Some("editor"));

    let rhyme_toggle = CheckButton::builder()
        .label("Highlight rhyming syllables")
        .active(settings.rhyme_highlighting)
        .build();
    let rhyme_stop_at_blank_line_toggle = CheckButton::builder()
        .label("Don't match rhymes across a blank line")
        .active(settings.rhyme_stop_at_blank_line)
        .build();
    let rhyme_page = settings_page();
    let rhyme_grid = form_grid();
    grid_check(&rhyme_grid, 0, &rhyme_toggle);
    grid_check(&rhyme_grid, 1, &rhyme_stop_at_blank_line_toggle);
    rhyme_page.append(&rhyme_grid);
    rhyme_page.append(&description_label(
        "Colors the background of syllables that rhyme with another word elsewhere in the document.",
    ));
    stack.add_named(&rhyme_page, Some("rhyme"));

    let completion_toggle = CheckButton::builder()
        .label("Enable dictionary word completion")
        .active(settings.word_completion)
        .build();
    let completion_page = settings_page();
    let completion_grid = form_grid();
    grid_check(&completion_grid, 0, &completion_toggle);
    completion_page.append(&completion_grid);
    completion_page.append(&description_label(
        "Suggests words from the bundled dictionary as you type. Tab or Enter accepts a suggestion.",
    ));
    stack.add_named(&completion_page, Some("completion"));

    let git_toggle = CheckButton::builder()
        .label("Automatically stage changes when saving")
        .active(settings.git_autostage)
        .build();
    let git_page = settings_page();
    let git_grid = form_grid();
    grid_check(&git_grid, 0, &git_toggle);
    git_page.append(&git_grid);
    stack.add_named(&git_page, Some("git"));

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
            let (name, label, parent) = CATEGORIES[row.index() as usize];
            stack_for_select.set_visible_child_name(name);
            let header = if parent.is_empty() {
                label.to_string()
            } else {
                format!("{parent}  \u{203a}  {label}")
            };
            header_for_select.set_text(&header);
        }
    });
    category_list.select_row(category_list.row_at_index(0).as_ref());

    let category_list_for_search = category_list.clone();
    search_entry.connect_search_changed(move |entry| {
        let query = entry.text().to_lowercase();
        let mut index = 0;
        while let Some(row) = category_list_for_search.row_at_index(index) {
            let (_, label, _) = CATEGORIES[index as usize];
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
        let vcs_gutter_toggle = vcs_gutter_toggle.clone();
        let auto_indent_toggle = auto_indent_toggle.clone();
        let tab_width_spin = tab_width_spin.clone();
        let rhyme_toggle = rhyme_toggle.clone();
        let rhyme_stop_at_blank_line_toggle = rhyme_stop_at_blank_line_toggle.clone();
        let completion_toggle = completion_toggle.clone();
        let git_toggle = git_toggle.clone();
        let theme_selected = theme_selected.clone();
        let icon_theme_selected = icon_theme_selected.clone();
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
                show_vcs_gutter: vcs_gutter_toggle.is_active(),
                rhyme_highlighting: rhyme_toggle.is_active(),
                rhyme_stop_at_blank_line: rhyme_stop_at_blank_line_toggle.is_active(),
                word_completion: completion_toggle.is_active(),
                auto_indent: auto_indent_toggle.is_active(),
                tab_width: tab_width_spin.value() as u32,
                git_autostage: git_toggle.is_active(),
                theme: if theme_selected.get() == 0 {
                    Theme::Dark
                } else {
                    Theme::Light
                },
                icon_theme: if icon_theme_selected.get() == 0 {
                    IconTheme::Color
                } else {
                    IconTheme::Monochrome
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
        &vcs_gutter_toggle,
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
    font_button.connect_font_desc_notify(move |_| f());
    // Now that `update_apply_sensitivity` exists, point the dropdowns'
    // late-bound change hook at it.
    *mark_dirty.borrow_mut() = Box::new({
        let f = update_apply_sensitivity.clone();
        move || f()
    });

    let apply: Rc<dyn Fn()> = Rc::new({
        let read_current = read_current.clone();
        let baseline = baseline.clone();
        let update_apply_sensitivity = update_apply_sensitivity.clone();
        move || {
            let current = read_current();
            current.save();
            // Chrome/tree already on screen keep their current icons; the
            // new variant applies to widgets built after this point (new
            // tabs, a rebuilt tree) and fully on next launch.
            crate::app::icons::set_variant(&current);
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
