//! The app's version string — derived from git history at build time by
//! `Build/version.sh` (see CLAUDE.md § Versioning). The single source of
//! truth for anything that displays a version; never hardcode `"2026.1"` or
//! read `CARGO_PKG_VERSION` for display.

/// Core SemVer, e.g. `0.24.0`.
pub const CORE: &str = env!("RHYMR_VERSION");

/// Full string with build metadata, e.g. `0.24.0+build.201.gdeadbee`.
pub const FULL: &str = env!("RHYMR_VERSION_FULL");

/// `v`-prefixed core string for UI display, e.g. `v0.24.0`.
pub fn display() -> String {
    format!("v{CORE}")
}
