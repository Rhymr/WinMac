use super::spec::{CATEGORY_TREE, CategoryId, LiveApply, SettingKind, SettingValue};
use super::{SPECS, Settings};
use crate::app::context_menu::ContextMenu;
use crate::workspace::controller::WorkspaceController;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, CheckButton, ColorDialog, ColorDialogButton, Entry,
    EventSequenceState, FontDialog, FontDialogButton, GestureClick, Grid, Label, ListBox,
    ListBoxRow, Orientation, ScrolledWindow, SearchEntry, Separator, SpinButton, Stack, Window,
    gdk, pango,
};
use libadwaita::Application;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

/// Per-depth indent for a child row in the category tree.
const CAT_INDENT_PX: i32 = 14;

/// Late-bound callback slots: the value dropdowns and the tree chevrons are
/// built before the closures they need exist, so they call through one of
/// these, filled in once those closures are defined.
type DirtyHook = Rc<RefCell<Box<dyn Fn()>>>;
type ToggleHook = Rc<RefCell<Box<dyn Fn(CategoryId)>>>;

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

/// Decimal places a float spin button should show for a given step
/// (0.05 -> 2, 0.1 -> 1, 1.0 -> 0), capped at 4.
fn decimals_for(step: f64) -> u32 {
    let mut places = 0;
    let mut scaled = step.abs();
    while places < 4 && (scaled.fract() > 1e-9) {
        scaled *= 10.0;
        places += 1;
    }
    places
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

// ===========================================================================
// Category tree helpers (data from `spec::CATEGORY_TREE`)
// ===========================================================================

fn node(id: CategoryId) -> Option<&'static super::spec::CategoryNode> {
    CATEGORY_TREE.iter().find(|n| n.id == id)
}

fn has_children(id: CategoryId) -> bool {
    CATEGORY_TREE.iter().any(|n| n.parent == Some(id))
}

/// Number of parent hops to a top-level node.
fn depth(id: CategoryId) -> i32 {
    let mut d = 0;
    let mut cur = node(id).and_then(|n| n.parent);
    while let Some(pid) = cur {
        d += 1;
        cur = node(pid).and_then(|n| n.parent);
    }
    d
}

/// The `Stack` child name for a category — also its search anchor.
fn stack_name(id: CategoryId) -> &'static str {
    match id {
        CategoryId::Appearance => "appearance",
        CategoryId::AppearanceWindow => "window",
        CategoryId::ColorScheme => "colorscheme",
        CategoryId::EditorGeneral => "editor",
        CategoryId::EditorRhyme => "rhyme",
        CategoryId::EditorCompletion => "completion",
        CategoryId::EditorFileTree => "filetree",
        CategoryId::VersionControlGit => "git",
        CategoryId::ToolsNetwork => "sources",
    }
}

/// "Appearance & Behavior  ›  Appearance" — the group prefix (for a
/// top-level node) or the parent-label chain, then this node's label.
fn breadcrumb(id: CategoryId) -> String {
    let Some(n) = node(id) else {
        return String::new();
    };
    let mut parts: Vec<&str> = Vec::new();
    if n.parent.is_none() && !n.breadcrumb_parent.is_empty() {
        parts.push(n.breadcrumb_parent);
    }
    let mut chain: Vec<&str> = Vec::new();
    let mut cur = n.parent;
    while let Some(pid) = cur {
        if let Some(p) = node(pid) {
            chain.push(p.label);
            cur = p.parent;
        } else {
            break;
        }
    }
    chain.reverse();
    parts.extend(chain);
    parts.push(n.label);
    parts.join("  \u{203a}  ")
}

/// Does `id`'s label, one of its settings' label/description/group, or any
/// descendant match the lowercased `query`?
fn category_matches(id: CategoryId, query: &str) -> bool {
    if let Some(n) = node(id)
        && n.label.to_lowercase().contains(query)
    {
        return true;
    }
    if SPECS.iter().any(|s| {
        s.category == id
            && (s.label.to_lowercase().contains(query)
                || s.description.to_lowercase().contains(query)
                || s.group.to_lowercase().contains(query))
    }) {
        return true;
    }
    CATEGORY_TREE
        .iter()
        .any(|n| n.parent == Some(id) && category_matches(n.id, query))
}

/// Rebuild `list`'s rows from `CATEGORY_TREE`, hiding rows under a collapsed
/// ancestor (ignored while `query` is non-empty — search shows every match,
/// fully expanded). Returns the `CategoryId` at each row index.
fn build_category_rows(
    list: &ListBox,
    collapsed: &HashSet<CategoryId>,
    query: &str,
    toggle: &Rc<dyn Fn(CategoryId)>,
) -> Vec<CategoryId> {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let mut shown = Vec::new();
    for n in CATEGORY_TREE {
        if query.is_empty() {
            let mut hidden = false;
            let mut cur = n.parent;
            while let Some(pid) = cur {
                if collapsed.contains(&pid) {
                    hidden = true;
                    break;
                }
                cur = node(pid).and_then(|p| p.parent);
            }
            if hidden {
                continue;
            }
        } else if !category_matches(n.id, query) {
            continue;
        }

        let hbox = GtkBox::new(Orientation::Horizontal, 4);
        hbox.set_margin_start(12 + CAT_INDENT_PX * depth(n.id));
        hbox.set_margin_top(6);
        hbox.set_margin_bottom(6);
        hbox.set_margin_end(12);

        if has_children(n.id) {
            let expanded = !query.is_empty() || !collapsed.contains(&n.id);
            let chevron = Label::new(Some(if expanded { "\u{25be}" } else { "\u{25b8}" }));
            chevron.add_css_class("dir-chevron");
            let gesture = GestureClick::new();
            let toggle = toggle.clone();
            let id = n.id;
            gesture.connect_released(move |g, _, _, _| {
                g.set_state(EventSequenceState::Claimed);
                toggle(id);
            });
            chevron.add_controller(gesture);
            hbox.append(&chevron);
        } else {
            let spacer = Label::new(None);
            spacer.set_width_request(12);
            hbox.append(&spacer);
        }

        hbox.append(&Label::builder().label(n.label).halign(Align::Start).build());

        let row = ListBoxRow::new();
        row.set_child(Some(&hbox));
        list.append(&row);
        shown.push(n.id);
    }
    shown
}

// ===========================================================================
// Setting widgets: one bound control per setting, read back generically
// ===========================================================================

enum BoundWidget {
    Check(CheckButton),
    Spin(SpinButton),
    Entry(Entry),
    Enum {
        selected: Rc<Cell<usize>>,
        values: &'static [&'static str],
    },
    /// Answers for both the family key and its paired [`SettingKind::FontSize`].
    Font(FontDialogButton),
}

#[derive(Default)]
struct SettingWidgets(HashMap<&'static str, BoundWidget>);

impl SettingWidgets {
    fn font_button(&self) -> Option<&FontDialogButton> {
        self.0.values().find_map(|w| match w {
            BoundWidget::Font(fb) => Some(fb),
            _ => None,
        })
    }

    /// The control's current value as a [`SettingValue`] of the shape
    /// `kind` expects, or `None` when no widget is bound for `key`.
    fn value(&self, key: &str, kind: SettingKind) -> Option<SettingValue> {
        if let SettingKind::FontSize { min, max } = kind {
            let desc = self.font_button()?.font_desc().unwrap_or_default();
            let pt = if desc.size() > 0 {
                (desc.size() / pango::SCALE) as i64
            } else {
                i64::from(Settings::default().font_size)
            };
            return Some(SettingValue::Int(pt.clamp(min, max)));
        }
        match self.0.get(key)? {
            BoundWidget::Check(cb) => Some(SettingValue::Bool(cb.is_active())),
            BoundWidget::Spin(sb) => Some(match kind {
                SettingKind::Float { .. } => SettingValue::Float(sb.value()),
                _ => SettingValue::Int(sb.value().round() as i64),
            }),
            BoundWidget::Entry(e) => Some(SettingValue::Text(e.text().to_string())),
            BoundWidget::Enum { selected, values } => {
                let idx = selected.get().min(values.len().saturating_sub(1));
                Some(SettingValue::Text(values[idx].to_string()))
            }
            BoundWidget::Font(fb) => {
                let desc = fb.font_desc().unwrap_or_default();
                let family = desc
                    .family()
                    .map(|f| f.to_string())
                    .unwrap_or_else(|| Settings::default().font_family);
                Some(SettingValue::Text(family))
            }
        }
    }
}

/// Build one category's page from every [`SPECS`] entry in that category:
/// a checkbox for `Bool`, a spin button for `Int`, a value dropdown for
/// `Enum`, a font picker for `Font` (its `FontSize` sibling rides along).
/// Section headers come from the specs' `group`; a `description` renders
/// under the control, and a `LiveApply::Restart` setting gets a note.
fn build_page(
    cat: CategoryId,
    settings: &Settings,
    widgets: &mut SettingWidgets,
    mark_dirty: &DirtyHook,
) -> GtkBox {
    let page = settings_page();
    let mut grid = form_grid();
    let mut group: Option<&str> = None;
    let mut row = 0;

    for spec in SPECS.iter().filter(|s| s.category == cat) {
        if matches!(spec.kind, SettingKind::FontSize { .. }) {
            continue; // owned by the paired Font widget
        }
        if group != Some(spec.group) {
            group = Some(spec.group);
            if !spec.group.is_empty() {
                page.append(&section_header(spec.group));
            }
            grid = form_grid();
            page.append(&grid);
            row = 0;
        }

        match spec.kind {
            SettingKind::Bool => {
                let active = matches!((spec.get)(settings), SettingValue::Bool(true));
                let check = CheckButton::builder()
                    .label(spec.label)
                    .active(active)
                    .build();
                grid_check(&grid, row, &check);
                widgets.0.insert(spec.key, BoundWidget::Check(check));
            }
            SettingKind::Int { min, max, step } => {
                let spin = SpinButton::with_range(min as f64, max as f64, step.max(1) as f64);
                if let SettingValue::Int(n) = (spec.get)(settings) {
                    spin.set_value(n as f64);
                }
                grid_field(&grid, row, &format!("{}:", spec.label), &spin);
                widgets.0.insert(spec.key, BoundWidget::Spin(spin));
            }
            SettingKind::Float { min, max, step } => {
                let spin = SpinButton::with_range(min, max, step);
                spin.set_digits(decimals_for(step));
                if let SettingValue::Float(f) = (spec.get)(settings) {
                    spin.set_value(f);
                }
                grid_field(&grid, row, &format!("{}:", spec.label), &spin);
                widgets.0.insert(spec.key, BoundWidget::Spin(spin));
            }
            SettingKind::Enum { values, labels } => {
                let initial = match (spec.get)(settings) {
                    SettingValue::Text(t) => values.iter().position(|v| **v == *t).unwrap_or(0),
                    _ => 0,
                };
                let (button, selected) = ContextMenu::select_dropdown(labels, initial, {
                    let mark_dirty = mark_dirty.clone();
                    move |_| (mark_dirty.borrow())()
                });
                grid_field(&grid, row, &format!("{}:", spec.label), &button);
                widgets
                    .0
                    .insert(spec.key, BoundWidget::Enum { selected, values });
            }
            SettingKind::Font => {
                let font_button = FontDialogButton::builder()
                    .dialog(&FontDialog::builder().title("Font").build())
                    .valign(Align::Center)
                    .build();
                font_button.set_use_size(true);
                let family = match (spec.get)(settings) {
                    SettingValue::Text(t) => t,
                    _ => Settings::default().font_family,
                };
                font_button.set_font_desc(&pango::FontDescription::from_string(&format!(
                    "{} {}",
                    family, settings.font_size
                )));
                grid_field(&grid, row, &format!("{}:", spec.label), &font_button);
                widgets.0.insert(spec.key, BoundWidget::Font(font_button));
            }
            SettingKind::Text => {
                let entry = Entry::builder().hexpand(true).build();
                if let SettingValue::Text(t) = (spec.get)(settings) {
                    entry.set_text(&t);
                }
                grid_field(&grid, row, &format!("{}:", spec.label), &entry);
                widgets.0.insert(spec.key, BoundWidget::Entry(entry));
            }
            SettingKind::FontSize { .. } => {}
        }

        if !spec.description.is_empty() {
            page.append(&description_label(spec.description));
        }
        if spec.live == LiveApply::Restart {
            page.append(&description_label("(restart required)"));
        }
        row += 1;
    }
    page
}

/// `#rrggbb` for a `gdk::RGBA` (alpha dropped).
fn hex_from_rgba(c: &gdk::RGBA) -> String {
    let to = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        to(c.red()),
        to(c.green()),
        to(c.blue())
    )
}

/// The Color Scheme page: a scrollable list of every `css::PALETTE` entry
/// with a colour picker and a per-row Reset, plus a "Reset all" button.
/// Edits accumulate in `overrides` (the `palette.<name>` map); a value
/// equal to the current theme's default is dropped, so Reset really clears
/// the override rather than pinning the default.
fn build_color_scheme_page(
    settings: &Settings,
    overrides: &Rc<RefCell<BTreeMap<String, String>>>,
    mark_dirty: &DirtyHook,
) -> ScrolledWindow {
    let page = settings_page();
    let theme = settings.theme;

    let reset_all = Button::builder()
        .label("Reset all colours to theme default")
        .halign(Align::Start)
        .build();
    page.append(&reset_all);
    page.append(&description_label(
        "Overrides apply on top of the current theme and are saved as palette.<name> lines.",
    ));

    let grid = form_grid();
    grid.set_margin_top(8);
    page.append(&grid);

    // (picker, default hex) per palette entry — reused by the reset wiring.
    let mut pickers: Vec<(ColorDialogButton, &'static str, String)> = Vec::new();

    for (i, (name, _, _)) in crate::css::PALETTE.iter().enumerate() {
        let row = i as i32;
        let default_hex = crate::css::palette_default(name, theme)
            .unwrap_or("#000000")
            .to_string();
        let current_hex = overrides
            .borrow()
            .get(*name)
            .cloned()
            .unwrap_or_else(|| default_hex.clone());

        let picker = ColorDialogButton::builder()
            .dialog(&ColorDialog::builder().with_alpha(false).build())
            .valign(Align::Center)
            .build();
        if let Ok(rgba) = gdk::RGBA::parse(&current_hex) {
            picker.set_rgba(&rgba);
        }
        let reset = Button::builder()
            .label("Reset")
            .css_classes(["flat"])
            .build();

        grid_field(&grid, row, name, &picker);
        grid.attach(&reset, 2, row, 1, 1);

        // Picker edits -> the overrides map (default value clears it).
        picker.connect_rgba_notify({
            let overrides = overrides.clone();
            let mark_dirty = mark_dirty.clone();
            let name = *name;
            let default_hex = default_hex.clone();
            move |p| {
                let hex = hex_from_rgba(&p.rgba());
                {
                    let mut map = overrides.borrow_mut();
                    if hex.eq_ignore_ascii_case(&default_hex) {
                        map.remove(name);
                    } else {
                        map.insert(name.to_string(), hex);
                    }
                }
                (mark_dirty.borrow())();
            }
        });

        reset.connect_clicked({
            let picker = picker.clone();
            let default_hex = default_hex.clone();
            move |_| {
                if let Ok(rgba) = gdk::RGBA::parse(&default_hex) {
                    picker.set_rgba(&rgba); // fires rgba_notify -> map cleared
                }
            }
        });

        pickers.push((picker, name, default_hex));
    }

    reset_all.connect_clicked({
        let overrides = overrides.clone();
        let mark_dirty = mark_dirty.clone();
        move |_| {
            overrides.borrow_mut().clear();
            for (picker, _, default_hex) in &pickers {
                if let Ok(rgba) = gdk::RGBA::parse(default_hex) {
                    picker.set_rgba(&rgba);
                }
            }
            (mark_dirty.borrow())();
        }
    });

    ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&page)
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
    // Sidebar: search + collapsible category tree
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

    sidebar.append(&search_entry);
    sidebar.append(&category_list);

    // ==========================================
    // Content: breadcrumb header + one Stack page per category
    // ==========================================
    let content = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .css_classes(vec!["settings-content"])
        .build();

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

    // Late-bound "widgets changed" hook — the value dropdowns are built
    // before `update_apply_sensitivity` exists, so their `on_change` calls
    // through this slot, filled in once that closure is defined.
    let mark_dirty: DirtyHook = Rc::new(RefCell::new(Box::new(|| {})));

    // Editor color-scheme overrides, edited on the Color Scheme page and
    // merged back into `Settings` by `read_current`.
    let overrides: Rc<RefCell<BTreeMap<String, String>>> =
        Rc::new(RefCell::new(settings.palette_overrides.clone()));

    let mut widgets = SettingWidgets::default();
    for n in CATEGORY_TREE {
        if n.id == CategoryId::ColorScheme {
            let page = build_color_scheme_page(&settings, &overrides, &mark_dirty);
            stack.add_named(&page, Some(stack_name(n.id)));
        } else {
            let page = build_page(n.id, &settings, &mut widgets, &mark_dirty);
            stack.add_named(&page, Some(stack_name(n.id)));
        }
    }
    let widgets = Rc::new(widgets);

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
    let collapsed: Rc<RefCell<HashSet<CategoryId>>> = Rc::new(RefCell::new(HashSet::new()));
    let visible_cats: Rc<RefCell<Vec<CategoryId>>> = Rc::new(RefCell::new(Vec::new()));
    let selected_cat = Rc::new(Cell::new(CATEGORY_TREE[0].id));

    // Chevron clicks call this through a slot (it needs `rebuild`, which in
    // turn needs it — same cycle as `mark_dirty`).
    let toggle_slot: ToggleHook = Rc::new(RefCell::new(Box::new(|_| {})));
    let toggle: Rc<dyn Fn(CategoryId)> = Rc::new({
        let toggle_slot = toggle_slot.clone();
        move |id| (toggle_slot.borrow())(id)
    });

    let rebuild: Rc<dyn Fn()> = Rc::new({
        let list = category_list.clone();
        let collapsed = collapsed.clone();
        let search_entry = search_entry.clone();
        let toggle = toggle.clone();
        let visible_cats = visible_cats.clone();
        let selected_cat = selected_cat.clone();
        move || {
            let query = search_entry.text().to_lowercase();
            let shown = build_category_rows(&list, &collapsed.borrow(), &query, &toggle);
            let want = shown
                .iter()
                .position(|c| *c == selected_cat.get())
                .unwrap_or(0);
            visible_cats.replace(shown);
            list.select_row(list.row_at_index(want as i32).as_ref());
        }
    });

    *toggle_slot.borrow_mut() = Box::new({
        let collapsed = collapsed.clone();
        let rebuild = rebuild.clone();
        move |id| {
            {
                let mut c = collapsed.borrow_mut();
                if !c.remove(&id) {
                    c.insert(id);
                }
            }
            rebuild();
        }
    });

    category_list.connect_row_selected({
        let stack = stack.clone();
        let header_label = header_label.clone();
        let visible_cats = visible_cats.clone();
        let selected_cat = selected_cat.clone();
        move |_, row| {
            let Some(row) = row else { return };
            let idx = row.index();
            if idx < 0 {
                return;
            }
            let Some(&id) = visible_cats.borrow().get(idx as usize) else {
                return;
            };
            selected_cat.set(id);
            stack.set_visible_child_name(stack_name(id));
            header_label.set_text(&breadcrumb(id));
        }
    });

    search_entry.connect_search_changed({
        let rebuild = rebuild.clone();
        move |_| rebuild()
    });

    rebuild();

    let dialog_for_cancel = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_for_cancel.close();
    });

    // What's currently saved on disk — Apply is enabled only once a widget
    // diverges from this, and it's refreshed after each save. Seeding
    // `read_current` from a clone of it also carries `Settings`' private
    // `unknown` passthrough (keys a newer build wrote) through the
    // dirty-check and the save.
    let baseline = Rc::new(RefCell::new(settings));

    // One place that turns the dialog's widgets back into a `Settings`.
    let read_current: Rc<dyn Fn() -> Settings> = Rc::new({
        let widgets = widgets.clone();
        let baseline = baseline.clone();
        let overrides = overrides.clone();
        move || {
            let mut current = baseline.borrow().clone();
            for spec in SPECS {
                if let Some(value) = widgets.value(spec.key, spec.kind) {
                    (spec.set)(&mut current, value);
                }
            }
            current.palette_overrides = overrides.borrow().clone();
            current
        }
    });

    apply_btn.set_sensitive(false);
    let update_apply_sensitivity: Rc<dyn Fn()> = Rc::new({
        let apply_btn = apply_btn.clone();
        let read_current = read_current.clone();
        let baseline = baseline.clone();
        move || {
            apply_btn.set_sensitive(read_current() != *baseline.borrow());
        }
    });

    for w in widgets.0.values() {
        let f = update_apply_sensitivity.clone();
        match w {
            BoundWidget::Check(cb) => {
                cb.connect_toggled(move |_| f());
            }
            BoundWidget::Spin(sb) => {
                sb.connect_value_changed(move |_| f());
            }
            BoundWidget::Font(fb) => {
                fb.connect_font_desc_notify(move |_| f());
            }
            BoundWidget::Entry(e) => {
                e.connect_changed(move |_| f());
            }
            // Value dropdowns route through `mark_dirty` (set below).
            BoundWidget::Enum { .. } => {}
        }
    }
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
