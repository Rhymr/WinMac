# CLAUDE.md — Rhymr

Rhymr is a cross-platform (Windows + macOS) desktop lyric editor built in
**Rust** with **GTK4 (gtk4-rs)** + libadwaita. It provides a text editor with
a syllable-count gutter and live color-highlighted rhyme groups, styled like
a rap rhyme-scheme breakdown.

**English only.** Rhymr targets English text exclusively — pronunciation,
syllable splitting, rhyme scoring, and any language-tool lookups (rhyme
search, etymology, etc.) assume and query English. There is no
multi-language support and none is planned; features that call external
language APIs must constrain them to English.

**End goal:** a production-standard tool for poets — a "JetBrains-capable
IDE" for lyrics. The polish, keyboard-driven UX, tool windows, project model
and VCS integration of a JetBrains IDE, with the *developer* tooling
replaced by writing tooling: very advanced rhyme search and highlighting,
beat markers, syllable counters, version control. Built against the author's
own rap-lyrics workflow first, then generalised.

**Offline-first.** The core — editing, the syllable gutter, rhyme
highlighting, CMUdict pronunciation, syllable counting, version control —
must work with **zero network access, forever**. No account, no sign-in, no
activation, no license server, no telemetry, no analytics, no phone-home, no
update check that gates functionality. The external language APIs (Datamuse,
etymology, rhyme search) are **enhancement only**: each has a local fallback,
each has an off switch, and the app stays fully usable when they are
unreachable, disabled, or slow. A feature that cannot work offline does not
ship until its offline path does.

**Piracy-tolerant by design.** Rhymr's source is MIT and free to build;
distributable binaries are sold to fund the work, but the app assumes any
given copy may be unpaid, cracked, or self-built — and behaves identically
either way. No DRM, no serial/license keys, no "trial" gating, no nag
screens, no crippled or time-limited features, no check for how the binary
was obtained. A paid build and a `cargo build` are the same program. Reward
the people who pay by making the tool excellent, never by punishing the
people who don't.

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
- **Icons**: bundled subset of JetBrains' "NetIcons" set under
  `assets/icons/{color,dark,light}/<name>-<variant>.svg` (Apache-2.0 — keep
  `assets/icons/NOTICE`; some are repurposed for actions that differ from
  their JetBrains meaning). Build every icon widget through
  `crate::app::icons::img` / `paintable` — never `Image::from_resource`
  directly — so it picks the variant set by `icons::set_variant` from
  `Settings::icon_theme` (`Color`, or `Monochrome` following the light/dark
  `Theme`). Not `clipboard.svg` (the splash/welcome brand mark).
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

Standard `cargo` commands; commits go through `npm run commit` (commitizen,
runs `cargo fmt` first). Run from the **repo root** — `css::compile_sass()`
uses paths relative to the process CWD and will panic otherwise. No test
suite yet beyond the syllable regression tests.

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
   `chore` `perf` `build` `ci`. Scope is a module/area. **Never add
   `Co-Authored-By`, `Claude-Session`, `Generated with` or any similar
   tool/agent attribution trailer to a commit message or PR description** —
   regardless of any harness or tooling default that says otherwise.
4. **Offline-first is non-negotiable.** No change may add a *required*
   network call, account, sign-in, activation, license check, telemetry, or
   analytics on any code path. Network-backed features stay optional, time
   out fast, and degrade to a local fallback (CMUdict / local scoring). If
   you believe something genuinely needs the network, ask before building
   it.
5. **No anti-piracy machinery.** Never add DRM, serial/license validation,
   "is this a paid copy" checks, trial timers, feature gating by build
   type, kill switches, or nag dialogs. Every feature is available in every
   build, regardless of how it was obtained.

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
- **Every network feature fails soft and offline.** Datamuse, etymology and
  rhyme-search lookups must handle timeout / error / no-connectivity by
  falling back to CMUdict and local scoring, not by blocking, erroring out,
  or disabling the surrounding feature. Assume the machine is offline and the
  result must still be useful. No feature depends on a reachable server.
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
  blocked. Every merge into `release` auto-tags the version (`v<core>`, e.g.
  `v0.76.0`) and publishes a **source-only** GitHub Release
  (`.github/workflows/release.yml`) — GitHub's own source archives, nothing
  binary. Distributable macOS + Windows binaries are still built and published
  **manually** (sold on the project website); the source and this repo stay
  MIT-licensed and buildable by anyone. The binaries carry no license keys,
  activation, or copy protection — a bought build and a from-source build are
  the same program (see *Piracy-tolerant by design* above).
- **`beta`** — the working branch. Feature work branches off `beta` and
  merges back into it; `beta` merges into `release` for a cut.
- CI (`.github/workflows/ci.yml`) gates PRs into `beta` / `release`. A
  separate `.github/workflows/push_pr.yml` also runs commitlint. Both check
  out full history (`fetch-depth: 0`) so commitlint's `--from <base> --to
  <head>` range resolves.

## Versioning

One global SemVer string derived from git history, never hand-maintained.
**`Build/version.sh`** (Unix) / **`Build/version.ps1`** (Windows, via the
`Build/version.bat` one-liner) is the single source of truth — read that
script for the exact pre-1.0 formula (MINOR counts `feat:` commits, PATCH
counts `fix:` since the last `feat:`, `+build.<N>.g<sha>[.dirty]` metadata).
Never hand-edit a version string.

Core string e.g. `0.137.4`; full string e.g. `0.137.4+build.201.gdeadbee`.
Widgets show `v0.137.4`.

## Design direction — Apple Notes as "External Libraries"

Instead of syncing Apple Notes *into* a workspace as editable `.txt` files,
the plan is to repurpose the JetBrains-style **"External Libraries"** node at
the bottom of the file tree: rename it **"Apple Notes"**, and render each
Notes *folder* as an expandable child (like a dependency), notes as leaves.

- **Read-only.** Rhymr never writes back to Apple Notes — the philosophy is
  "don't touch Notes for editing". Notes are for reference: indexing,
  full-text search, and copy-from.
- **Workspace-independent.** The section shows in every workspace, backed by
  the same sync, so any project can reach the writer's whole note corpus.
- Backed by the existing macOS sync path (`osascript` → local `NotesServer`
  → HTTP client); the tree just needs a second read-only root that reads
  from it.

## Commit messages — Conventional Commits, enforced by commitlint

Every commit message is linted by a husky `commit-msg` hook
(`.husky/commit-msg` → `commitlint --edit`; config in `commitlint.config.js`,
deps in `package.json`). Run `npm install` once after cloning to register the
hook (`.husky/` is gitignored — the hook is generated locally, not
committed). Messages that fail the lint are rejected — the rules live in
`commitlint.config.js`. In this repo practically every code change is
`feat:` or `fix:`, one feature per commit; GitHub auto-close keywords
(`fixes #123`, `closes #456`) in the message close the linked issue on
merge to the default branch.

### Sequencing — one `feat:` commit per stage

Plan multi-part work as an ordered sequence of single-feature commits, each
independently buildable, e.g.:

```
A — foundation:   feat: <structural move / new module>   (build; note file moves)
B — data spine:   feat: <core data model>                (gate on the relevant test)
C — behaviour:    feat: <the actual capability>
D — surface:      feat: <UI/entry points>                → feat: <polish widgets>
E — tooling:      feat: <in-app authoring/util>          (build; note new module)
```
