//! Platform shims. macOS is fully supported; other targets get inert
//! fallbacks so the rest of the app compiles and runs.

use std::path::PathBuf;

/// One Apple Note as `(folder, title, plain-text body)`.
pub type NoteRow = (String, String, String);

/// The program + args to launch for the embedded terminal, resolved
/// per-OS. `portable-pty` handles the actual PTY difference (ConPTY vs
/// openpty), so the shell choice is the only OS-specific terminal bit.
pub type ShellSpec = (PathBuf, Vec<String>);

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{default_shell, fetch_apple_notes};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{default_shell, fetch_apple_notes};

/// Apple Notes only exist on macOS — every other target returns an empty
/// snapshot, so the "Apple Notes" source reports `Unavailable`.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn fetch_apple_notes() -> Result<Vec<NoteRow>, String> {
    Ok(Vec::new())
}

/// Fallback shell for targets without a dedicated shim.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn default_shell() -> ShellSpec {
    let shell = std::env::var_os("SHELL")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/bin/sh"));
    (shell, Vec::new())
}
