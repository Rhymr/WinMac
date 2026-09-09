use crate::platform::NoteRow;
use std::process::Command;

/// ASCII unit / record separators — a note's title or plain-text body will
/// never contain these, so no escaping is needed. Must match `notes::cache`.
const US: char = '\u{1f}';
const RS: char = '\u{1e}';

/// Read every Apple Note as `(folder, title, body)`, grouped by the folder
/// it lives in. Runs `osascript` synchronously — call it off the GTK main
/// thread (see `crate::notes::sync`).
pub fn fetch_apple_notes() -> Result<Vec<NoteRow>, String> {
    // AppleScript can't write raw control chars in a string literal, so
    // build the separators from `ASCII character`.
    let script = r#"
        tell application "Notes"
            set us to (ASCII character 31)
            set rs to (ASCII character 30)
            set out to ""
            repeat with f in folders
                set fname to name of f
                repeat with n in notes of f
                    set out to out & fname & us & (name of n) & us & (plaintext of n) & rs
                end repeat
            end repeat
            return out
        end tell
        "#;

    log::debug!("apple-notes: running osascript to read Notes");
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("running osascript: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        log::error!("apple-notes: osascript exited {}: {stderr}", output.status);
        return Err(format!("osascript exited {}: {stderr}", output.status));
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let rows = raw
        .split(RS)
        .filter(|record| !record.is_empty())
        .filter_map(|record| {
            let mut parts = record.splitn(3, US);
            let folder = parts.next()?.trim().replace('/', "_");
            let title = parts.next()?.trim().replace('/', "_");
            let body = parts.next().unwrap_or("").trim().to_string();
            if title.is_empty() {
                return None;
            }
            Some((folder, title, body))
        })
        .collect();

    Ok(rows)
}
