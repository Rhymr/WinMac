use crate::platform::NoteRow;

/// Apple Notes has no Windows equivalent to shell out to. Returns an empty
/// snapshot so the "Apple Notes" source cleanly reports `Unavailable`
/// rather than the app failing to build on Windows.
pub fn fetch_apple_notes() -> Result<Vec<NoteRow>, String> {
    Ok(Vec::new())
}
