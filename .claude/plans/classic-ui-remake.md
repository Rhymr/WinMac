# Remake UI to resemble the classic ("boxy") JetBrains look

## Context

Rhymr is a GTK4 + libadwaita desktop app (Rust). Its UI today reads as
"modern IDE / VS Code": rounded corners (`--radius-sm/md/lg` = 4/6/9px),
floating rounded-chip editor tabs with gaps, rounded selection *pills* with
inset margins, soft 100–150ms transitions, and a near-black dark palette
(`#1e1e1e` editor / `#2e2e2e` panels).

The user wants the pre–New-UI JetBrains ("classic" / Darcula-era) look — the
one the `alezhu/classic-ui` plugin restores, and shown in the supplied
reference screenshots (`Filetree.png`, `Jetbrains.png`, `Gutter.png`):

- Dense, **square-cornered**, hard 1px dividers between every region.
- File tree: **full-bleed flat blue selection bar** (no rounded pill), ~20px
  rows, tight indent.
- Editor tabs: **continuous rectangular strip** with an accent **underline**
  on the active tab, which visually merges into the editor background.
- Darcula-family dark palette (`#3c3f41` panels, `#2b2b2b` editor,
  `#4c4c4c`/`#2b2b2b` borders, `#a9b7c6` text) and an "IntelliJ Classic
  Light" light palette.
- Square context menus, buttons, inputs, checkboxes.

Plus attached asks:

1. **VCS change markers in the editor gutter** — a 3px bar per changed line
   vs `HEAD`: green = added, blue = modified, grey marker = deleted
   (`Gutter.png`).
2. **Classic window chrome the app doesn't have yet** — a thin top **toolbar**
   row of icon buttons and a left **tool-window stripe** (`Jetbrains.png`).
3. **Split the font** — system UI font for chrome, monospace only in the
   editor + gutter (classic IDE convention).
4. A repo **`CLAUDE.md`** (none exists yet).
5. Commit the work in **conventional-commit** style (repo carries
   `@commitlint/config-conventional`), committing this plan file alongside.
   Leave the ~1.5k lines of pre-existing uncommitted work untouched — commit
   only what this task changes.

Most of the look is driven from one place — `src/css.rs` (`PALETTE` +
`TOKENS`) feeding `var(--…)` into the SCSS files — so the restyle is largely
a palette/token/SCSS pass; the toolbar/stripe and the gutter markers are the
two pieces of real new widget code.

## Approach

### 1. Palette + tokens — `src/css.rs`  (core lever)

- **`PALETTE`** (css.rs:26): rewrite both columns.
  - Dark → Darcula: `bg-darkest #2b2b2b`, `bg-dark #3c3f41`, `bg-mid #45494a`,
    `bg-light #4e5254`, `bg-hover #353739`; `text-bright #bbbbbb`,
    `text-dim #a9b7c6`, `text-muted #808080`, `text-number #606366`;
    `border-dark #2b2b2b`, `border-light #4c4c4c`, `border-hover #5e6060`,
    `border-active #6b6b6b`; `selection-bg #2f65ca`, `selection-hover #365880`,
    `selection-active #1f4a7a`; `popover-bg-color #3c3f41`;
    `destructive #c75450` (+ hover/active/text tuned).
  - Light → IntelliJ Classic Light: `bg-darkest #ffffff`, `bg-dark #ececec`,
    `bg-mid #ffffff`, `bg-light #d9d9d9`, `bg-hover #e5e5e5`;
    `text-bright #1d1d1d`, `text-dim #2b2b2b`, `text-muted #808080`;
    `border-dark #c0c0c0`, `border-light #c0c0c0`, `border-hover #a6a6a6`;
    `selection-bg #2675bf`, `selection-hover #4080c0`, `selection-active #1c5a9e`;
    `popover-bg-color #ffffff`; `destructive #c0392b`.
  - Update the **duplicated** accent/destructive hex literals in `theme_css()`
    (css.rs:111-119) to match the new `selection-*`/`destructive` rows.
  - Add two rows for the VCS gutter (§9): `vcs-added` `#59a869`/`#4a8f3c`,
    `vcs-modified` `#4a88c7`/`#3573b8`.
- **`TOKENS`** (css.rs:67): square + snap.
  - `--radius-sm: 2px` (buttons/inputs/checkboxes — classic kept a hairline
    radius), `--radius-md: 0`, `--radius-lg: 0`.
  - `--transition-fast: 60ms linear`, `--transition-normal: 90ms linear`.
- **Font split** — `theme_css()` (css.rs:125-129) currently emits only
  `--app-font-family` / `--app-font-size` from `settings.font_family`/`_size`
  and `base.scss`'s `* {}` applies it to *every* widget. Add
  `--ui-font-family` (a fixed cross-platform system stack, e.g.
  `-apple-system, "Segoe UI", "SF Pro Text", Cantarell, "Ubuntu", sans-serif`)
  and `--ui-font-size: 12px` to `TOKENS` (theme-invariant). Keep the
  `settings`-driven `--app-font-*` for the editor only. Drop the editor
  default `font_size` 13 → 13 stays fine for mono; the *chrome* shrinks via
  the new 12px UI token.

### 2. `assets/scss/base.scss` — primitives go boxy + dense, chrome font

- `* {}`: `font-family: var(--ui-font-family); font-size: var(--ui-font-size);`
  (was `--app-font-*`). This flips every widget to the system UI font;
  `editor.scss` (§7) re-pins the SourceView + gutter back to `--app-font-*`.
- `button`: `padding: 2px 12px; min-height: 22px;` radius `var(--radius-sm)`;
  hover/active shift background only (no border-color pop).
- `entry`: `padding: 2px 6px; min-height: 22px;` radius `var(--radius-sm)`;
  `:focus` border-color → `var(--selection-bg)` (classic blue focus ring),
  not `--text-bright`.
- `listbox > row`, `frame > label`: padding → `2px 6px`.
- `.caption` → 10px. Everything else already `border-radius: 0` via `* {}`.

### 3. `assets/scss/_mixins.scss` — full-bleed flat selection variant

`list-row-hover-select` currently either recolors *all* selected descendants
(`$force-fg`) or none. The file tree needs selected-row text forced to
`--selection-fg` **except** the git-status labels (`.file-modified` /
`.file-new` / `.file-renamed`), which must keep their own color.

Add a `$fg-except-status: false` flag: when set, emit
`&:selected *:not(.file-modified):not(.file-new):not(.file-renamed) { color: var(--selection-fg); }`.

### 4. `assets/scss/file_tree.scss` — classic tool-window tree

- `.file-list` mixin call → `list-row-hover-select(var(--bg-hover),
  var(--selection-bg), $radius: none, $fg-except-status: true)` (flat
  full-width blue selection instead of the neutral-grey pill).
- Rows: `min-height: 20px`; `.file-list-row` padding `1px 6px`.
- `.file-tree-panel-header`: `padding: 3px 6px; border-bottom: 1px solid
  var(--border-dark);` flat `var(--bg-dark)`.
- `.drop-target-active { border-radius: 0; }`.

### 5. `assets/scss/notebook.scss` — continuous underlined tab strip

- `& > header`: `padding: 0;` keep `border-bottom: 1px solid var(--border-dark)`.
- `& > tabs > tab`: `border-radius: 0; margin: 0; padding: 3px 10px;
  border: none; border-right: 1px solid var(--border-dark);`
  inactive `background-color: var(--bg-dark)`.
  - `:checked` → `background-color: var(--bg-darkest);` (merges into editor)
    `box-shadow: inset 0 -2px 0 var(--selection-hover);` (active underline);
    `:backdrop` underline → `var(--border-active)`.
  - `:hover:not(:checked)` → `var(--bg-hover)`.
- Drop the now-unneeded "reserve 1px transparent border" trick.
- `.tab-close-button`: `border-radius: 0`, keep show-on-hover/checked sizing.

### 6. Remaining SCSS — square + densify

- `context_menu.scss`: `.context-menu { border-radius: 0; padding: 0;
  box-shadow: 0 2px 8px rgba(0,0,0,0.4); }`; `.context-menu-item
  { border-radius: 0; padding: 3px 12px; min-height: 22px; font-size: 12px; }`;
  separator `margin: 3px 0`.
- `layout.scss`: `headerbar { padding: 2px 8px; border-bottom: 1px solid
  var(--border-dark); }`. (Stale unused `.left-panel`/`.right-panel`/… rules:
  leave.)
- `status_bar.scss`: `min-height: 22px`; `.status-text { font-size: 11px;
  padding: 2px 8px; }`; add `.status-text:not(:first-child) { border-left:
  1px solid var(--border-dark); }` for classic segmenting.
- `settings.scss` + `welcome.scss`: mixin radius arg → `0` (flat full-bleed
  selection for the sidebar nav / category lists). `welcome.scss`: replace
  literal `border-radius: 6px/8px` with `0`; `.recent-projects-list` rows
  lose `margin-bottom: 6px`; `.project-avatar` square; `checkbutton check`
  radius → `var(--radius-sm)`.
- `dialog.scss`: list/scroll/message-box radius → `var(--radius-sm)`; rows
  `padding: 2px 8px`.
- `rhyme_search.scss`: `.rhyme-results > row { min-height: 20px; }`; keep
  hard borders.
- `editor.scss`: `.rhyme-editor { line-height: 1.5; border: none; }`
  (panel divider does the separating); `.line-numbers` keeps a hard
  `1px solid var(--border-dark)` right edge; add `.vcs-gutter { padding: 0;
  min-width: 4px; }` (see §9). The existing `sourceview` /
  `.rhyme-editor-view` / `.rhyme-editor-gutter` / `.syllable-count` /
  `.line-numbers` rules that set `font-family` keep using `var(--app-font-*)`
  — that's now the *only* place the mono font is applied, so it must stay.

### 7. GtkSourceView style schemes — Darcula / IntelliJ-Classic-Light

Editor syntax colors are a separate system (`assets/styles/rhymr.xml` dark,
`rhymr-light.xml` light; ids `rhymr` / `rhymr-light`, selected in
`editor/mod.rs:338` `scheme_id`).

- `rhymr.xml` (dark → Darcula): `text #a9b7c6/#2b2b2b`; `selection #214283`;
  `selection-unfocused #0d293e`; `cursor #bbbbbb`; `line-numbers
  #606366/#313335`; `line-numbers-border #313335`; `current-line #323232`
  (opaque grey, not a blue wash); `current-line-number #a4a3a3 on #323232`;
  `right-margin` line `#5b5b5b` bg `#2b2b2b`; `draw-spaces #5c6370`;
  `bracket-match #3b514d`.
- `rhymr-light.xml` (light → classic): `text #000000/#ffffff`; `selection
  #a6d2ff` (black fg); `selection-unfocused #d4d4d4`; `cursor #000000`;
  `line-numbers #999999/#f0f0f0`; `line-numbers-border #d0d0d0`;
  `current-line #fcfaed` (classic pale-yellow); `current-line-number
  #000000 on #fcfaed`; `right-margin` line `#ced0d6` bg `#ffffff`.

Rhyme-group highlight backgrounds (`src/rhyme/highlight.rs:33` `PALETTE`)
are already tuned for light text on dark and stay readable on `#a9b7c6`.

### 8. Classic window chrome — toolbar + tool-window stripe  (new widgets)

New file `src/app/chrome.rs`; wired into `src/app/layout.rs`
`create_main_layout` so `main_box` becomes:

```
main_box (vertical)
├─ chrome::main_toolbar()        ← NEW, .main-toolbar
├─ content row (horizontal)      ← NEW wrapper box
│  ├─ chrome::left_stripe()      ← NEW, .tool-stripe  (~26px wide)
│  └─ content_pane (existing GtkPaned)
└─ status bar (existing, extended)
```

**8a. Main toolbar — `chrome::main_toolbar(app) -> gtk::Box`**
`.main-toolbar` — a `gtk::Box` horizontal, ~28px, `border-bottom: 1px solid
var(--border-dark)`. Flat icon `Button`s (symbolic icon names already resolve
via the bundled Adwaita icon theme — the app already uses
`open-menu-symbolic` / `window-close-symbolic`), each `.activate_action` on
an **existing** `gio` action so it's functional, not decorative:
`document-new-symbolic`→`app.new`, `document-open-symbolic`→`app.open`,
`document-save-symbolic`→`app.save-all`, separator,
`emblem-shared-symbolic`→`app.git-commit`, `go-up-symbolic`→`app.git-push`,
`go-down-symbolic`→`app.git-pull`, `view-refresh-symbolic`→`app.git-fetch`,
spacer (`hexpand`), `emblem-system-symbolic`→`app.preferences`. Tooltips on
each. Actions are registered in `menu::setup_menu`, which already runs before
`create_main_layout` returns — but `setup_menu` is called from `build_ui`
*after* `create_main_layout`; reorder so actions exist first, or have the
toolbar look them up lazily via `gtk::Widget::activate_action` (resolves at
click time — simplest, no reorder).

**8b. Left tool-window stripe — `chrome::left_stripe(...) -> gtk::Box`**
`.tool-stripe` — `gtk::Box` vertical, ~26px, `border-right: 1px solid
var(--border-dark)`, `var(--bg-dark)`. Two `ToggleButton`s
`.tool-stripe-button` (icon + tooltip; GTK4 `Label` has no text rotation, so
icon-over-tooltip is the faithful-enough analog — a rotated-text custom
widget is a possible follow-up):
- **Project** (`folder-symbolic`, active by default) → toggles
  `file_tree.get_widget().set_visible()`.
- **Rhyme Search** (`system-search-symbolic`) → drives the existing
  `RhymeSearch` collapse/expand (`rhyme_search.connect_toggle` +
  `is_collapsed`, already in layout.rs:183-205).
When the file tree is hidden *and* rhyme search collapsed, also collapse the
left column: set `content_pane` (the horizontal `GtkPaned`) start child
`set_visible(false)` / position 0; restore the remembered position when
either is shown again. Mirrors the position math already in
`create_content_layout`.

**8c. Status bar segments — `src/app/layout.rs` `create_status_bar`**
Keep the `.status-bar` box; add right-aligned segment `Label`s
(`.status-text`, so §6's `:not(:first-child)` border rule segments them):
- git branch — `GitController::new(root).current_branch_name()`, refreshed
  when the file tree refreshes (add a setter like the word-count listener,
  or just read on tab switch).
- line : column of the active buffer's cursor — subscribe to the active
  `SourceBuffer`'s `cursor-position` notify via the workspace controller
  (new small listener, same shape as `set_word_count_listener`).
- keep the existing word count.

**8d. SCSS** — new `assets/scss/chrome.scss` (add to `CSS_FILES` in both
`src/css.rs:7` and `build.rs:8`): `.main-toolbar`, `.tool-stripe`,
`.tool-stripe-button` (`min-width:26px; padding:4px 0; border-radius:0;`
`:checked` → `background: var(--bg-mid); box-shadow: inset 2px 0 0
var(--selection-hover);`). Extend `status_bar.scss` per §6.

### 9. VCS change markers in the editor gutter  (new feature)

**9a. Per-line diff — `src/git/ops.rs`**

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LineChange { Added, Modified, Deleted } // Deleted: a deletion sits just above this line

/// Change type per 0-based line of `current_text` vs the file's blob in HEAD.
/// Empty when: not a repo / no HEAD / path outside repo. Untracked file → every line Added.
pub fn line_changes(repo_root: &Path, file_abs: &Path, current_text: &str)
    -> HashMap<usize, LineChange>
```

Impl with `git2` (already a dep): open repo → `head()?.peel_to_tree()?` →
`tree.get_path(rel)?` blob content as "old". Build a patch with
`git2::Patch::from_blob_and_buffer(Some(old), Some(rel), current_text.as_bytes(),
Some(rel), Some(DiffOptions::new().context_lines(0)))`. Walk
`patch.hunks()` / `patch.num_lines_in_hunk()` + `patch.line_in_hunk()`:
addition run with a paired deletion in the same hunk → `Modified`;
addition run alone → `Added`; deletion-only hunk → `Deleted` at the new-side
line. No HEAD blob for the path → mark every line `Added`.

**9b. Gutter renderer — `src/editor/vcs_gutter.rs`  (new)**

A `sourceview5::GutterRenderer` subclass via `glib::subclass` — same pattern
as `editor/completion.rs`'s `WordCompletionProvider` GObject subclass
(`GutterRendererText` only paints text, so it can't be reused). Override the
`snapshot_line` vfunc: fill a 3px rect at the cell's left edge in the change
color; `Deleted` → a small down-triangle instead of a full-height bar.
Colours come from two new `PALETTE` rows in `src/css.rs` —
`vcs-added` `#59a869`/`#4a8f3c`, `vcs-modified` `#4a88c7`/`#3573b8`
(dark/light); `Deleted` reuses `destructive`. The renderer can't read CSS
vars mid-snapshot, so `apply_settings` pushes the resolved
`gdk::RGBA` triple into it on construction and on theme change (keyed off
`Settings.theme`), same as the source-scheme swap already there. Holds an
`Rc<RefCell<HashMap<usize, LineChange>>>` it reads while drawing;
`queue_draw()` on update. Inserted into the left `Gutter` at position `-15`
(line numbers `-30`, syllables `-20`) so the bar sits just right of the
line numbers, JetBrains-style — or `-40` for the far-left edge; pick during
implementation by eye.

**9c. Wire-up — `src/editor/mod.rs`**

- New fields: `vcs_renderer`, `vcs_changes: Rc<RefCell<HashMap<usize, LineChange>>>`.
- Recompute (`git::ops::line_changes(root, path, &buffer_text)` → store →
  `renderer.queue_draw()`): in `set_path()`, inside the existing debounced
  autosave `timeout_add_local_once` (right after the write), and in
  `apply_settings`. Reuse the existing `find_git_root()` (editor/mod.rs:389).
- New setting `show_vcs_gutter: bool` (default `true`) in
  `src/setting/mod.rs` (load/save/Default) + a checkbox on the settings
  dialog "Editor" page (`src/setting/dialog.rs`, mirror `show_syllable_gutter`
  at dialog.rs:170-192), added/removed live in `apply_settings` exactly like
  the syllable renderer (editor/mod.rs:253-267).

**9d. CSS** — `.vcs-gutter` rule in `editor.scss` (§6); drawing is in Rust.

### 10. `CLAUDE.md`  (new, repo root)

Follow `/init` conventions. Cover: `cargo run` / `build` / `clippy` /
`fmt`; the runtime SCSS→CSS pipeline (`src/css.rs`, no hot-reload) and the
"three things that must stay in sync for a theme change" (PALETTE/`theme_css`,
`sync_style_manager`, the two `assets/styles/*.xml` schemes); the `src/`
domain-module layout (`app editor file git platform rhyme setting workspace`)
with **`pub mod` + fully-qualified paths, no re-exports**; the centralized
`PALETTE`/`TOKENS` theming model (+ the new `--ui-font-*` vs `--app-font-*`
split); conventional-commits / commitlint requirement.

### 11. Commits  (conventional / commitlint-valid)

`@commitlint/config-conventional` is configured (no active husky hook;
`npx --no-install commitlint` lints). **Only stage files this task touches**
— the pre-existing uncommitted work stays untouched (use explicit
`git add <path>` per commit, never `git add -A`). `cargo fmt` +
`cargo clippy --all-targets` clean before each. Footer on every commit per
the session attribution rule. Logical chunks:

1. `docs: add CLAUDE.md and UI-remake plan`  — `CLAUDE.md`, `.claude/plans/…md`
2. `refactor(ui): classic JetBrains palette, tokens, font split`  — `src/css.rs`, `base.scss`, `_mixins.scss`, `src/setting/mod.rs` (font default)
3. `style(ui): square and densify every widget`  — remaining SCSS
4. `style(editor): Darcula and classic-light source schemes`  — `assets/styles/*.xml`
5. `feat(ui): classic toolbar and tool-window stripe`  — `src/app/chrome.rs`, `src/app/mod.rs`, `src/app/layout.rs`, `assets/scss/chrome.scss`, `src/css.rs`+`build.rs` (CSS_FILES)
6. `feat(editor): VCS change markers in the gutter`  — `src/git/ops.rs`, `src/editor/vcs_gutter.rs`, `src/editor/mod.rs`, `src/setting/*`

Verify each message: `npx --no-install commitlint --from HEAD~6 --to HEAD`.

## Critical files

| File | Change |
|---|---|
| `src/css.rs` | `PALETTE` both columns + `vcs-*` rows, `TOKENS` radii/transitions + `--ui-font-*`, `theme_css()` duplicated hex, add `chrome` to `CSS_FILES` |
| `build.rs` | add `chrome` to `CSS_FILES` (kept in sync with css.rs) |
| `assets/scss/_mixins.scss` | `$fg-except-status` flag on `list-row-hover-select` |
| `assets/scss/base.scss` | `* {}` → UI font; button/entry/listbox density + square |
| `assets/scss/file_tree.scss` | full-bleed flat blue selection, 20px rows |
| `assets/scss/notebook.scss` | continuous rectangular tabs + active underline |
| `assets/scss/{context_menu,layout,status_bar,settings,welcome,dialog,rhyme_search,editor}.scss` | square + densify, drop literal radii; editor.scss re-pins mono font |
| `assets/scss/chrome.scss` *(new)* | `.main-toolbar`, `.tool-stripe`, `.tool-stripe-button` |
| `assets/styles/rhymr.xml`, `rhymr-light.xml` | Darcula / classic-light editor colors |
| `src/app/chrome.rs` *(new)* | `main_toolbar()` + `left_stripe()` builders |
| `src/app/mod.rs`, `src/app/layout.rs` | register `chrome` module; slot toolbar + stripe into `main_box`; status-bar segments |
| `src/setting/mod.rs` | `font_size` default; `show_vcs_gutter` setting |
| `src/setting/dialog.rs` | "Editor font" relabel; `show_vcs_gutter` checkbox |
| `src/git/ops.rs` | `LineChange` enum + `line_changes()` |
| `src/editor/vcs_gutter.rs` *(new)* | `GutterRenderer` subclass drawing the 3px bars |
| `src/editor/mod.rs` | own/insert/refresh the VCS renderer; recompute on save |
| `CLAUDE.md` *(new)* | codebase guide |

## Verification

1. `cargo run` in this repo (it *is* a git repo with uncommitted changes, so
   VCS markers should appear immediately). Also `cargo run` from a workspace
   folder that is **not** a git repo → chrome/toolbar/stripe still fine, no
   gutter markers, no branch segment.
2. Preferences → Appearance → Theme: toggle Dark/Light. Confirm in both:
   Darcula/classic palette, square corners everywhere, **flat full-bleed blue
   tree selection**, **continuous underlined editor tabs**, square context
   menus, ~20–22px rows, hard 1px dividers between toolbar / stripe / file
   tree / editor / rhyme search / status bar. Chrome renders in the system UI
   font; editor + gutter stay monospace.
3. Toolbar buttons fire their actions (New/Open/Save All/Commit/Push/Pull/
   Fetch/Preferences). Left stripe: "Project" toggle hides/shows the file
   tree; "Rhyme Search" toggle expands/collapses that pane; both off →
   left column collapses; toggling back restores width.
4. Status bar shows branch • line:col • word count, segmented by 1px rules;
   line:col tracks the caret, branch matches `git branch`.
5. Editor gutter: edit a tracked file → new lines show a green bar, changed
   lines blue, a deleted-line spot a grey/red marker; undo back to HEAD →
   markers clear. Brand-new untracked file → all lines green. Toggle
   Preferences → Editor → "Show VCS gutter" off/on → renderer removed/re-added
   live.
6. `cargo fmt --check` and `cargo clippy --all-targets` clean.
7. `npx --no-install commitlint --from HEAD~6 --to HEAD` passes.

## Decisions taken (from clarifying questions)

- **Pre-existing uncommitted work** → leave untouched; commit only this
  task's files (explicit `git add <path>`, never `-A`).
- **Classic chrome** → build it: functional top toolbar + left tool-window
  stripe + segmented status bar (§8). GTK4 `Label` can't rotate text, so
  stripe buttons are icon + tooltip (rotated-text custom widget = possible
  follow-up).
- **Font** → split: system UI font for all chrome, monospace only in the
  editor + gutter (§1 `--ui-font-*` vs `--app-font-*`).

## Risks / notes

- `sourceview5` `GutterRenderer` subclassing: confirm the `snapshot_line`
  vfunc is exposed by the 0.11 bindings; fallback is `GutterRendererPixbuf`
  with three pre-rendered 3px bars, or a plain `DrawingArea` overlaid on the
  gutter.
- Diff cost: `line_changes()` runs on the autosave debounce tick only (not
  every keystroke) and on one file at a time — cheap. If it ever shows up,
  cache the HEAD blob per path.
- Toolbar symbolic icons rely on the Adwaita icon theme shipped with
  GTK/libadwaita (already used for `open-menu-symbolic` etc.) — no new
  asset files.
- Adding `chrome` to `CSS_FILES` must happen in **both** `src/css.rs:7` and
  `build.rs:8` (they carry duplicate lists by design).
