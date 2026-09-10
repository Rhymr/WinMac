//! Cross-platform PTY session + terminal emulation for the embedded
//! Terminal tool window.
//!
//! `portable-pty` (wezterm) opens the PTY — ConPTY on Windows, `openpty`
//! on Unix — and spawns the shell; only [`crate::platform::default_shell`]
//! is OS-specific. `alacritty_terminal` owns the screen grid, scrollback
//! and escape parsing. A dedicated reader thread pumps PTY bytes into the
//! grid off the GTK main thread; the UI polls [`TerminalSession::take_dirty`]
//! and repaints (see [`crate::app::terminal_panel`]).

pub mod error;
pub mod render;

use std::io::{Read, Write};
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

use error::TerminalError;

/// A grid geometry passed to `alacritty_terminal` (`Dimensions`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct GridSize {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// The `Term` shared between the reader thread (writes) and the UI thread
/// (reads, for rendering).
pub type SharedTerm = Arc<Mutex<Term<Proxy>>>;

/// `alacritty_terminal`'s event sink: forwards terminal-originated writes
/// back to the PTY, tracks the window title, flags a repaint, and notes
/// when the child exits.
#[derive(Clone)]
pub struct Proxy {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    dirty: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
    title: Arc<Mutex<String>>,
}

impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(text) => {
                if let Ok(mut w) = self.writer.lock() {
                    let _ = w.write_all(text.as_bytes());
                    let _ = w.flush();
                }
            }
            Event::Title(t) => {
                if let Ok(mut g) = self.title.lock() {
                    *g = t;
                }
            }
            Event::ResetTitle => {
                if let Ok(mut g) = self.title.lock() {
                    g.clear();
                }
            }
            Event::ChildExit(_) | Event::Exit => {
                self.exited.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
        self.dirty.store(true, Ordering::Relaxed);
    }
}

/// One running shell + its emulated screen.
pub struct TerminalSession {
    term: SharedTerm,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Box<dyn MasterPty + Send>,
    killer: Mutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
    dirty: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
    title: Arc<Mutex<String>>,
    size: Mutex<GridSize>,
}

impl TerminalSession {
    /// Open a PTY, spawn the platform shell in `cwd`, and start pumping its
    /// output into an emulated `cols`×`rows` screen.
    pub fn spawn(cwd: &Path, cols: usize, rows: usize) -> Result<Rc<Self>, TerminalError> {
        let cols = cols.max(2);
        let rows = rows.max(1);

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| TerminalError::Io(e.to_string()))?;

        let (shell, args) = crate::platform::default_shell();
        let mut cmd = CommandBuilder::new(shell);
        for a in args {
            cmd.arg(a);
        }
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| TerminalError::Spawn(e.to_string()))?;
        drop(pair.slave);
        let killer = child.clone_killer();

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| TerminalError::Io(e.to_string()))?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| TerminalError::Io(e.to_string()))?;

        let writer = Arc::new(Mutex::new(writer));
        let dirty = Arc::new(AtomicBool::new(true));
        let exited = Arc::new(AtomicBool::new(false));
        let title = Arc::new(Mutex::new(String::new()));

        let proxy = Proxy {
            writer: Arc::clone(&writer),
            dirty: Arc::clone(&dirty),
            exited: Arc::clone(&exited),
            title: Arc::clone(&title),
        };
        let mut config = Config::default();
        config.scrolling_history = crate::config::TERMINAL_SCROLLBACK;
        let term: SharedTerm = Arc::new(Mutex::new(Term::new(
            config,
            &GridSize { cols, rows },
            proxy,
        )));

        // Reader thread: PTY bytes → escape parser → grid, off the UI thread.
        {
            let term = Arc::clone(&term);
            let dirty = Arc::clone(&dirty);
            let exited = Arc::clone(&exited);
            thread::Builder::new()
                .name("terminal-reader".into())
                .spawn(move || {
                    let mut parser: Processor = Processor::new();
                    let mut buf = [0u8; crate::config::TERMINAL_READ_BUF];
                    loop {
                        match reader.read(&mut buf) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if let Ok(mut t) = term.lock() {
                                    parser.advance(&mut *t, &buf[..n]);
                                }
                                dirty.store(true, Ordering::Relaxed);
                            }
                        }
                    }
                    exited.store(true, Ordering::Relaxed);
                    dirty.store(true, Ordering::Relaxed);
                })
                .map_err(|e| TerminalError::Io(e.to_string()))?;
        }

        // `Rc`, not `Arc`: this handle lives only on the GTK main thread.
        // The reader thread holds `Arc` clones of the inner `Send + Sync`
        // fields (`term` / `dirty` / `exited`), never this wrapper.
        Ok(Rc::new(Self {
            term,
            writer,
            master: pair.master,
            killer: Mutex::new(killer),
            dirty,
            exited,
            title,
            size: Mutex::new(GridSize { cols, rows }),
        }))
    }

    /// The shared emulated screen, for the renderer.
    pub fn term(&self) -> &SharedTerm {
        &self.term
    }

    /// Take the "needs repaint" flag (clears it).
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }

    /// Whether the shell has exited.
    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::Relaxed)
    }

    /// Current OSC-set window title (empty if none).
    pub fn title(&self) -> String {
        self.title.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Send raw bytes (already terminal-encoded) to the shell.
    pub fn write(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(bytes);
            let _ = w.flush();
        }
    }

    /// Resize both the PTY and the emulated grid. A no-op if unchanged.
    pub fn resize(&self, cols: usize, rows: usize) {
        let next = GridSize {
            cols: cols.max(2),
            rows: rows.max(1),
        };
        if let Ok(mut cur) = self.size.lock() {
            if *cur == next {
                return;
            }
            *cur = next;
        }
        let _ = self.master.resize(PtySize {
            rows: next.rows as u16,
            cols: next.cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        });
        if let Ok(mut t) = self.term.lock() {
            t.resize(next);
        }
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Scroll the viewport by `lines` (positive = towards history).
    pub fn scroll(&self, lines: i32) {
        if lines == 0 {
            return;
        }
        if let Ok(mut t) = self.term.lock() {
            t.scroll_display(Scroll::Delta(lines));
        }
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// The current selection as plain text, if any.
    pub fn selection_text(&self) -> Option<String> {
        self.term
            .lock()
            .ok()
            .and_then(|t| t.selection_to_string())
            .filter(|s| !s.is_empty())
    }

    /// Kill the shell process.
    pub fn shutdown(&self) {
        if let Ok(mut k) = self.killer.lock() {
            let _ = k.kill();
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}
