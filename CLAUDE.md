# CLAUDE.md

Guidance for working in this repo.

## What Rhymr is

A cross-platform (Windows + macOS) **desktop editor for writing lyrics and
poetry** — GTK4 + libadwaita, in Rust. It is a text editor built around a
writer's needs rather than a programmer's: a **syllable-count gutter**, and
**live rhyme-group highlighting** that color-codes the rhyme scheme across
lines the way you'd annotate a rap verse by hand.

**End goal:** a production-standard tool for poets — a "JetBrains-capable
IDE" for lyrics. Same polish, keyboard-driven UX, tool windows, VCS
integration and project model as a JetBrains IDE, but with all the
*developer* tooling replaced by writing tooling: very advanced rhyme search
and highlighting, beat markers, syllable counters, version control. The
author is building against their own rap-lyrics workflow first, then
generalising.

macOS is the fully supported target today; `src/platform/` has the Windows
shims but they're largely stubs. True win-mac parity is a goal, not a
current fact.

## Build / run / check

```sh
cargo run                       # splash → welcome picker → workspace
cargo build
cargo clippy --all-targets      # zero-warning bar
cargo fmt                       # rustfmt; commits expect formatted code
npm run commit                  # commitizen prompt; runs `cargo fmt` first
```

Run from the **repo root** — `css::compile_sass()` uses paths relative to
the process CWD and will panic otherwise.

`build.rs` compiles `assets/scss/*.scss` → `assets/css/*.css` (gitignored,
regenerated) and bundles `assets/resources.xml` into a gresource. The
checked-in CSS is **not** what styles the app — see below.

No test suite yet beyond the ~19k-word **syllable regression tests** that
lock the current syllabification behaviour.

## Styling / theming (one place, easy to get wrong)

The look is driven from `src/css.rs`:

- `PALETTE` — every themeable color as `(name, dark, light)`. Dark = classic
  Darcula, light = classic "IntelliJ Light". Single source of truth.
- `TOKENS` — theme-invariant `:root` vars: `--radius-*` (all `0` — the UI is
  deliberately boxy/classic), `--transition-*`, and the **font split**
  `--ui-font-*` (OS UI font, all chrome) vs `--app-font-*` (monospace,
  Settings-driven, editor + gutter only).
- `theme_css()` emits the active theme's `:root {}` at runtime and appends it
  to the freshly-compiled SCSS. **No hot-reload** — SCSS recompiles only at
  startup (`css::init`) and on the settings dialog's Apply/OK
  (`css::reload`).

SCSS files consume `var(--…)`; never hardcode palette colors. `_mixins.scss`
holds `list-row-hover-select` (row hover/selection; `$fg-except-status`
spares the file tree's git-status label colors through a selection). Adding a
stylesheet means adding its stem to `CSS_FILES` in **both** `src/css.rs` and
`build.rs` (duplicated on purpose).

### Three things stay in lockstep for a theme change

1. `src/css.rs` `PALETTE` / `theme_css()` — custom-widget CSS + mirrored
   libadwaita `--accent-*` / `--destructive-*` names.
2. `src/css.rs` `sync_style_manager()` — points `AdwStyleManager` at the same
   `Settings.theme` (native header-bar chrome).
3. `assets/styles/rhymr.xml` (dark) / `rhymr-light.xml` (light) — the
   GtkSourceView editor color schemes, chosen by `editor::scheme_id()`. A
   **separate** color system from the CSS palette.

`Settings.theme` (`Dark` | `Light`) drives all three; there is no
OS-theme-following.

## Module layout (`src/`)

Domain modules, each `pub mod` with fully-qualified paths — **no re-exports**
(`crate::git::ops::…`).

| dir | what |
|---|---|
| `app/` | window shell: `splash` (product splash), `welcome` (JetBrains-style project picker, with Configure/Help sidebar dropdowns), `layout` (panes + status bar), `chrome` (toolbar + tool-window stripe), `menu` (gio actions + accels), `context_menu` (shared popup-menu builder) |
| `editor/` | `TextEditor` wrapping `sourceview5::View`; gutter renderers — `vcs_gutter` (VCS change bars) + syllable count; `completion` (bundled dictionary), `stat` (syllable/word counts) |
| `file/` | `FileTree` (`tree` = model/render, `tree_menu` = actions), `ops` |
| `git/` | `ops` — `git2` wrappers: `file_statuses`, per-line `line_changes` (HEAD-blob vs buffer diff), commit/push/pull/fetch, `stage_all_changes`; `dialog` |
| `rhyme/` | `highlight` (syllable-level rhyme scoring, Hirjee & Brown; `TextTag` background coloring per rhyme group), `search` (Datamuse-backed panel), `score` |
| `platform/` | macOS / Windows shims (Apple Notes fetch on macOS) |
| `setting/` | `Settings` (flat `key=value` file under the OS config dir) + settings dialog |
| `workspace/` | `Workspace` (notebook/tabs), `WorkspaceController` (shared root path + status-bar listeners: word count, caret line:col, git branch), `manager`, `recent` |

`TextEditor` is a plain struct, not a GObject; its `Clone` impl builds a
*fresh* editor and copies text/path, so closures capture individual
`Rc`/widget clones, never `self`.

## Key subsystems

### Syllable pipeline
CMUdict phonemes → syllabification for the gutter count. A
**phoneme→letter alignment** is being built so the gutter splits *written*
words at the right letters — syllabic-consonant words ("candle") currently
misplace letters. Direction under consideration: a precomputed EM
many-to-many alignment (Phonetisaurus / m2m-aligner, or a pre-aligned
CMUdict) baked into a lookup table. The ~19k-word regression suite locks
current behaviour — expect to update it deliberately.

### Rhyme highlighting
A word's pronunciation should resolve by **layering every available source**
(Datamuse + CMUdict + advanced and simple phonetic fallbacks), not one
lookup. Scoring is syllable-level local alignment per Hirjee & Brown;
`rhyme/highlight.rs` cycles a 24-hue background palette across rhyme groups.

### Editor gutter
Renderers are inserted into the left `Gutter` at fixed priorities: VCS
change bars `-40` (leftmost), line numbers `-30`, syllable count `-20`.
`vcs_gutter::VcsGutterRenderer` is a `sourceview5::GutterRenderer` subclass
that paints a 3px bar per changed line (green add / blue modify / seam for
delete); the diff comes from `git::ops::line_changes` on a 400ms debounce
after edits, gated by the `show_vcs_gutter` setting.

### Apple Notes sync (macOS only)
`osascript` → a local `actix-web` `NotesServer` on `127.0.0.1:8080` → an
HTTP client in the UI. Used by the "Import Apple Notes as text files" option
in the New Project dialog.

### In progress
A right-click **"Find selected lyrics"** editor action (ported from a
find-my-lyrics Chrome extension): current selection → Genius `/search` API →
list of matching songs.

## Conventions

- **Commits**: Conventional Commits, enforced by `@commitlint/config-conventional`
  via husky (`npx --no-install commitlint`, or `npm run commit`). Types in
  use: `feat` `fix` `refactor` `style` `docs` `chore` `perf` `build` `ci`.
  Scope is a module/area: `feat(editor): …`, `refactor(ui): …`.
- Live-applied settings: a settings-dialog toggle should take effect on
  already-open tabs via `WorkspaceController::apply_settings` →
  `TextEditor::apply_settings` (see how the syllable / VCS gutter renderers
  add & remove themselves there).
- New chrome/widget code lives in `src/app/`; keep `src/app/mod.rs`'s
  `pub mod` list in sync.

## Branching & releases

- **`release`** — stable, protected. Only fast-forward / merge commits land
  here; direct pushes are blocked. A push to `release` triggers
  `.github/workflows/release.yml`, which builds macOS + Windows binaries,
  tags `v<version>`, and publishes a GitHub Release.
- **`beta`** — the working branch. Feature work branches off `beta` and
  merges back into it; `beta` merges into `release` for a cut.
- `.github/workflows/ci.yml` runs `fmt --check`, `clippy -D warnings`,
  `build`, and (on PRs) commitlint against `beta` / `release`.

## Versioning

One global SemVer string derived from git history, never hand-maintained.
**`Build/version.sh`** (Unix) / **`Build/version.ps1`** (Windows, via the
`Build/version.bat` one-liner) is the single source of truth. Formula,
pre-1.0:

- **MAJOR** is `0` until someone runs `git tag -a vX.Y.Z` with X ≥ 1. A
  reachable `vX.Y.Z` tag with X ≥ 1 then becomes the base and the counts
  below are taken since it.
- **MINOR** = count of every `feat:` commit reachable from HEAD (all
  ancestors, not first-parent) — so a `feat:` on an unmerged branch bumps it
  immediately.
- **PATCH** = count of `fix:` commits since the most recent `feat:` commit.
- **`+build.<N>.g<sha>[.dirty]`** metadata: `<N>` = `git rev-list --count
  HEAD`, `<sha>` = short hash, `.dirty` when the tracked tree has
  uncommitted changes.
- No `-prerelease` suffix while MAJOR is 0 (`0.x` already means unstable).
  `RHYMR_VERSION_PRERELEASE` injects one if ever needed.

Core string e.g. `0.137.4`; full string e.g. `0.137.4+build.201.gdeadbee`.
Widgets show `v0.137.4`.
