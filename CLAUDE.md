# CLAUDE.md — Rhymr

Rhymr is a cross-platform (Windows + macOS) desktop lyric editor built in
**Rust** with **GTK4 (gtk4-rs)** + libadwaita. It provides a text editor with
a syllable-count gutter and live color-highlighted rhyme groups, styled like
a rap rhyme-scheme breakdown.

**End goal:** a production-standard tool for poets — a "JetBrains-capable
IDE" for lyrics. The polish, keyboard-driven UX, tool windows, project model
and VCS integration of a JetBrains IDE, with the *developer* tooling
replaced by writing tooling: very advanced rhyme search and highlighting,
beat markers, syllable counters, version control. Built against the author's
own rap-lyrics workflow first, then generalised.

## Architecture

- **UI**: gtk4-rs. Tabbed editor (`TextEditor` widgets in a `Notebook`), file
  tree, rhyme-search panel, macOS Apple Notes sync panel. Plus classic
  JetBrains-style chrome: a top toolbar, a left tool-window stripe, a product
  splash, and a JetBrains-style welcome window.
- **Styling**: SCSS compiled to CSS **at runtime on launch** via
  `css::compile_sass`. Edit the `.scss`, never the generated `.css`. The look
  is driven from `src/css.rs` — `PALETTE` (every themeable color as
  `(name, dark, light)`; dark = classic Darcula, light = classic IntelliJ
  Light) and `TOKENS` (theme-invariant `:root` vars: all `--radius-*` are `0`
  — the UI is deliberately boxy/classic; plus the font split, `--ui-font-*`
  for chrome vs the Settings-driven monospace `--app-font-*` for the editor +
  gutter only). No hot-reload; SCSS recompiles at startup (`css::init`) and
  on the settings dialog's Apply/OK (`css::reload`). A new stylesheet's stem
  goes in `CSS_FILES` in **both** `src/css.rs` and `build.rs`.
- **Resources**: GResources compiled at build time via `build.rs`.
- **Rhyme/pronunciation**: Datamuse API + CMUdict + advanced/simple
  fallbacks, layered — never a single lookup. Scoring is syllable-level
  local alignment (Hirjee & Brown); `rhyme/highlight.rs` cycles a 24-hue
  background palette across rhyme groups.
- **Syllables**: CMUdict phonemes + a phoneme→letter alignment for correct
  gutter splitting (syllabic-consonant words like "candle" currently
  misplace letters — direction under consideration is a precomputed EM
  many-to-many alignment baked into a lookup table). Guarded by a ~19k-word
  regression suite.
- **Editor gutter**: renderers sit at fixed priorities in the left `Gutter` —
  VCS change bars `-40` (leftmost), line numbers `-30`, syllable count `-20`.
  `editor::vcs_gutter::VcsGutterRenderer` (a `sourceview5::GutterRenderer`
  subclass) paints a 3px bar per line changed vs git HEAD; the diff comes
  from `git::ops::line_changes` on a 400ms debounce, gated by the
  `show_vcs_gutter` setting.
- **Apple Notes sync (macOS only)**: `osascript` → local actix-web
  `NotesServer` on `127.0.0.1:8080` → HTTP client in the UI.
- **Platform layer**: `src/platform/`. macOS is fully supported; Windows is a
  stub being filled in. Cross-platform parity is the goal.

### `src/` module layout

Domain modules, each a `pub mod` with fully-qualified paths — **no
re-exports** (`crate::git::ops::…`).

| dir | what |
|---|---|
| `app/` | window shell: `splash`, `welcome` (project picker + Configure/Help sidebar dropdowns), `layout` (panes + status bar), `chrome` (toolbar + tool-window stripe), `menu` (gio actions + accels), `context_menu` (shared popup-menu builder) |
| `editor/` | `TextEditor` wrapping `sourceview5::View`; gutter renderers (`vcs_gutter` + syllable count), `completion`, `stat` |
| `file/` | `FileTree` (`tree` model/render, `tree_menu` actions), `ops` |
| `git/` | `ops` — `git2` wrappers (`file_statuses`, per-line `line_changes`, commit/push/pull/fetch, `stage_all_changes`); `dialog` |
| `rhyme/` | `highlight` (rhyme scoring + `TextTag` coloring), `search` (Datamuse panel), `score` |
| `platform/` | macOS / Windows shims |
| `setting/` | `Settings` (flat `key=value` file under the OS config dir) + settings dialog |
| `workspace/` | `Workspace` (notebook/tabs), `WorkspaceController` (shared root path + status-bar listeners), `manager`, `recent` |

`TextEditor` is a plain struct, not a GObject; its `Clone` impl builds a
*fresh* editor and copies text/path, so closures capture individual
`Rc`/widget clones, never `self`.

### Theme changes — three things stay in lockstep

`Settings.theme` (`Dark` | `Light`) drives all three; there is no
OS-theme-following.

1. `src/css.rs` `PALETTE` / `theme_css()` — custom-widget CSS + the mirrored
   libadwaita `--accent-*` / `--destructive-*` names.
2. `src/css.rs` `sync_style_manager()` — points `AdwStyleManager` at the same
   theme (native header-bar chrome).
3. `assets/styles/rhymr.xml` (dark) / `rhymr-light.xml` (light) — the
   GtkSourceView editor color schemes, chosen by `editor::scheme_id()`. A
   **separate** color system from the CSS palette.

## Build / run / check

```sh
cargo run                       # splash → welcome picker → workspace
cargo build
cargo clippy --all-targets      # treat warnings as errors
cargo fmt
npm run commit                  # commitizen prompt; runs `cargo fmt` first
```

Run from the **repo root** — `css::compile_sass()` uses paths relative to
the process CWD and will panic otherwise. No test suite yet beyond the
syllable regression tests.

## Non-negotiables

1. **Do not break the syllable regression tests.** The ~19k-word suite locks
   current behavior. If a change alters output, show me the diff of affected
   cases and explain *why* before assuming the new behavior is correct.
   Never edit the expected-output fixtures to make tests pass.
2. **Never edit generated artifacts.** Change `.scss` not `.css`; change
   source not compiled GResources.
3. **Commits use the project flow.** Conventional Commits via `npm run
   commit` (runs `cargo fmt` first). Don't hand-write commit messages that
   bypass commitlint. Types in use: `feat` `fix` `refactor` `style` `docs`
   `chore` `perf` `build` `ci`. Scope is a module/area.

## Production standards

- **`cargo fmt` and `cargo clippy` clean.** Treat clippy warnings as errors.
  No `#[allow(...)]` without a one-line comment justifying it.
- **No `unwrap()` / `expect()` / `panic!` on any path that handles user
  input, file I/O, network (Datamuse/Genius/NotesServer), or parsing.**
  Return `Result` and propagate with `?`. `expect()` is acceptable only for
  genuine invariants that cannot fail at runtime, with a message stating the
  invariant.
- **Errors are typed, not stringly.** Use the project's existing error
  enum(s); add variants rather than returning `Box<dyn Error>` or ad-hoc
  strings. Preserve context.
- **No blocking work on the GTK main thread.** Network calls (Datamuse,
  Genius, NotesServer) and heavy alignment/parsing run off-thread; results
  marshalled back to the UI properly. Never `.await`-block or sleep on the UI
  thread.
- **No secrets or hardcoded hosts/ports scattered in code.** The NotesServer
  address (`127.0.0.1:8080`), API base URLs, and timeouts live in one
  config/constants module.
- **Public items get doc comments.** Every `pub` fn/struct/trait gets a `///`
  explaining intent, not restating the signature.

## Deduplication — read before writing code

**Before adding any function, type, or module, search the codebase for
existing equivalents.** Assume the thing you need may already exist under a
different name. This project has multiple pronunciation sources and platform
backends, which makes duplication easy and costly.

- **Search first.** Grep for the concept (pronunciation, phoneme, align,
  syllable, rhyme, notes, sync) before writing. If something 80% similar
  exists, extend or generalize it — don't fork it.
- **One source of truth per concept.** Pronunciation resolution (Datamuse +
  CMUdict + fallbacks) funnels through a single resolver API. Callers ask the
  resolver; they never re-implement lookup order or fallback logic. Same for
  syllable splitting and rhyme grouping — one canonical path each.
- **Platform code shares a trait, not copy-paste.** `mac` and `win`
  implementations satisfy a common trait. Shared logic lives in
  platform-agnostic code; only genuinely OS-specific bits live under
  `src/platform/*`. When filling in the Windows stub, mirror the macOS trait
  — don't clone its body.
- **Fallback layering lives in exactly one place.** Adding a new source means
  registering it with the resolver, not adding another `if let None` chain at
  a call site.
- **No parallel data models.** One representation for a word's
  pronunciation, one for a rhyme group, one for a syllable-split result.
  Extend or compose the existing one.
- **Extract on the second occurrence.** The first time logic is duplicated,
  extract it into a shared helper in the same change.

When you spot existing duplication adjacent to what I asked for, flag it and
propose a consolidation — but do the consolidation as a separate,
clearly-labeled step, not silently mixed into a feature change.

## Change discipline

- **Small, reviewable diffs.** One concern per change. Don't reformat or
  "tidy" unrelated code in the same diff.
- **Say what you're about to do.** For anything beyond a trivial edit,
  briefly state the plan and which files you'll touch before editing.
- **Match existing conventions.** Follow the module layout, naming, and
  error-handling patterns already in the file.
- **Tests travel with behavior.** New non-trivial logic gets a test. Bug
  fixes get a regression test reproducing the bug.
- **When unsure, ask.** If a change would touch the syllable alignment, the
  resolver contract, or the platform trait, confirm the approach first.
- **Live-applied settings**: a settings-dialog toggle should take effect on
  already-open tabs via `WorkspaceController::apply_settings` →
  `TextEditor::apply_settings` (see how the syllable / VCS gutter renderers
  add & remove themselves there).

## Branching & releases

- **`release`** — stable, protected. Only merges land here; direct pushes are
  blocked. A push to `release` triggers `.github/workflows/release.yml`,
  which builds macOS + Windows binaries, tags `v<version>`, and publishes a
  GitHub Release.
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
  ancestors, not first-parent) — a `feat:` on an unmerged branch bumps it
  immediately.
- **PATCH** = count of `fix:` commits since the most recent `feat:` commit.
- **`+build.<N>.g<sha>[.dirty]`** metadata: `<N>` = `git rev-list --count
  HEAD`, `<sha>` = short hash, `.dirty` when the tracked tree has
  uncommitted changes.
- No `-prerelease` suffix while MAJOR is 0 (`0.x` already means unstable).
  `RHYMR_VERSION_PRERELEASE` injects one if ever needed.

Core string e.g. `0.137.4`; full string e.g. `0.137.4+build.201.gdeadbee`.
Widgets show `v0.137.4`.
