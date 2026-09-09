//! Process-wide diagnostic logging.
//!
//! Rhymr has no persistent log: this is a support/debugging aid, driven
//! **per invocation** by a `-v` / `--verbose` flag (repeatable) or the
//! `RHYMR_LOG` environment variable — deliberately *not* a [`Settings`]
//! field, since it isn't a user preference to remember between launches.
//!
//! [`parse_verbosity`] is called before the GTK application is built (it
//! strips the recognised flags out of `argv` so GTK doesn't reject them),
//! then [`init`] installs a tiny [`log::Log`] that writes every enabled
//! record to stderr. Everything else in the crate logs through the `log`
//! facade macros (`error!` / `warn!` / `info!` / `debug!` / `trace!`).
//!
//! [`Settings`]: crate::setting::Settings

use log::{LevelFilter, Metadata, Record};

/// Minimal stderr logger: `[LEVEL] target: message`, one line per record,
/// filtered by `level`. No timestamps, no colour, no file — this is
/// console diagnostics, not an audit trail.
struct StderrLogger {
    level: LevelFilter,
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

/// Remove every verbosity flag from `args` and return how many `-v` steps
/// were requested (`-v` = 1, `-vv` = 2, `--verbose` = 1, and they add up).
///
/// A bare `--` ends option parsing: tokens after it are left untouched so a
/// file path that happens to look like a flag still reaches GTK.
pub fn parse_verbosity(args: &mut Vec<String>) -> u8 {
    let mut count: u8 = 0;
    let mut past_ddash = false;
    args.retain(|arg| {
        if past_ddash {
            return true;
        }
        match arg.as_str() {
            "--" => {
                past_ddash = true;
                true
            }
            "--verbose" => {
                count = count.saturating_add(1);
                false
            }
            // `-v`, `-vv`, `-vvv`, … — a dash followed only by `v`s.
            s if s.len() >= 2 && s.starts_with('-') && s[1..].chars().all(|c| c == 'v') => {
                count = count.saturating_add((s.len() - 1) as u8);
                false
            }
            _ => true,
        }
    });
    count
}

/// Map a `-v` count to a level, let `RHYMR_LOG` override it if set, and
/// install the logger. Precedence: `RHYMR_LOG` (when it names a valid
/// level) beats the CLI count; the CLI count beats the `warn` default.
///
/// Safe to call once at startup; a second call is a no-op.
pub fn init(verbosity: u8) {
    let from_count = match verbosity {
        0 => LevelFilter::Warn,
        1 => LevelFilter::Info,
        2 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };

    let level = std::env::var("RHYMR_LOG")
        .ok()
        .and_then(|v| parse_level(v.trim()))
        .unwrap_or(from_count);

    log::set_max_level(level);
    // Only fails if a logger is already set — impossible on the single
    // startup call, and a no-op we can ignore if it somehow isn't.
    let _ = log::set_boxed_logger(Box::new(StderrLogger { level }));
}

/// `off` / `error` / `warn` / `info` / `debug` / `trace`, case-insensitive.
fn parse_level(s: &str) -> Option<LevelFilter> {
    match s.to_ascii_lowercase().as_str() {
        "off" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "warn" | "warning" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(input: &[&str]) -> (u8, Vec<String>) {
        let mut args: Vec<String> = input.iter().map(|s| s.to_string()).collect();
        let count = parse_verbosity(&mut args);
        (count, args)
    }

    #[test]
    fn no_flags_leaves_argv_untouched() {
        let (count, rest) = strip(&["rhymr", "/some/folder"]);
        assert_eq!(count, 0);
        assert_eq!(rest, ["rhymr", "/some/folder"]);
    }

    #[test]
    fn counts_and_removes_short_and_long_flags() {
        let (count, rest) = strip(&["rhymr", "-v", "--verbose", "-vv", "proj"]);
        assert_eq!(count, 4);
        assert_eq!(rest, ["rhymr", "proj"]);
    }

    #[test]
    fn double_dash_ends_option_parsing() {
        let (count, rest) = strip(&["rhymr", "-v", "--", "-v", "-vv"]);
        assert_eq!(count, 1);
        assert_eq!(rest, ["rhymr", "--", "-v", "-vv"]);
    }

    #[test]
    fn unrelated_dash_v_word_is_not_a_flag() {
        // `-verbose` (single dash, letters after the v's) is left alone.
        let (count, rest) = strip(&["rhymr", "-version", "-x"]);
        assert_eq!(count, 0);
        assert_eq!(rest, ["rhymr", "-version", "-x"]);
    }

    #[test]
    fn level_names_parse_case_insensitively() {
        assert_eq!(parse_level("TRACE"), Some(LevelFilter::Trace));
        assert_eq!(parse_level("Off"), Some(LevelFilter::Off));
        assert_eq!(parse_level("warning"), Some(LevelFilter::Warn));
        assert_eq!(parse_level("nonsense"), None);
    }
}
