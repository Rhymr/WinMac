//! Platform shims. macOS is fully supported; other targets get inert
//! fallbacks so the rest of the app compiles and runs.

/// One Apple Note as `(folder, title, plain-text body)`.
pub type NoteRow = (String, String, String);

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::fetch_apple_notes;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::fetch_apple_notes;

/// Apple Notes only exist on macOS — every other target returns an empty
/// snapshot, so the "Apple Notes" source reports `Unavailable`.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn fetch_apple_notes() -> Result<Vec<NoteRow>, String> {
    Ok(Vec::new())
}
