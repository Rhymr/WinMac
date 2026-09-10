//! Draws an `alacritty_terminal` grid into a cairo context for the
//! Terminal tool window. Uses cairo's monospace "toy" text API — adequate
//! for a fixed-pitch terminal and keeps the dependency surface to `cairo`
//! alone (no pango / pangocairo).

use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, CursorShape};
use gtk::cairo;

use super::SharedTerm;

/// Fixed cell geometry for the current font.
#[derive(Clone, Copy)]
pub struct Metrics {
    pub cell_w: f64,
    pub cell_h: f64,
    /// Baseline offset from the top of a cell.
    pub baseline: f64,
}

/// A resolved 16-colour + fg/bg/cursor palette for one theme.
struct Palette {
    fg: (f64, f64, f64),
    bg: (f64, f64, f64),
    cursor: (f64, f64, f64),
    ansi: [(f64, f64, f64); 16],
}

/// Select the monospace face + size on `ctx`. Must be re-run every draw —
/// a cairo context is fresh per frame.
pub fn select_font(ctx: &cairo::Context, family: &str, size_px: f64, bold: bool) {
    ctx.select_font_face(
        family,
        cairo::FontSlant::Normal,
        if bold {
            cairo::FontWeight::Bold
        } else {
            cairo::FontWeight::Normal
        },
    );
    ctx.set_font_size(size_px);
}

/// Measure one cell for `family` at `size_px`.
pub fn metrics(ctx: &cairo::Context, family: &str, size_px: f64) -> Metrics {
    select_font(ctx, family, size_px, false);
    let fe = ctx.font_extents().ok();
    let te = ctx.text_extents("M").ok();
    let cell_h = fe
        .as_ref()
        .map(|e| e.height())
        .unwrap_or(size_px * 1.3)
        .ceil()
        .max(1.0);
    let cell_w = te
        .as_ref()
        .map(|e| e.x_advance())
        .filter(|w| *w > 0.0)
        .unwrap_or(size_px * 0.6)
        .ceil()
        .max(1.0);
    let baseline = fe.as_ref().map(|e| e.ascent()).unwrap_or(size_px);
    Metrics {
        cell_w,
        cell_h,
        baseline,
    }
}

/// Paint the terminal into `ctx` (a `size` = `(w, h)` px area).
pub fn draw(
    term: &SharedTerm,
    ctx: &cairo::Context,
    family: &str,
    size_px: f64,
    m: Metrics,
    dark: bool,
    size: (i32, i32),
) {
    let (w, h) = size;
    if w <= 0 || h <= 0 {
        return;
    }
    let pal = palette(dark);

    // Whole-area background first.
    set_src(ctx, pal.bg);
    ctx.rectangle(0.0, 0.0, f64::from(w), f64::from(h));
    let _ = ctx.fill();

    let Ok(term) = term.lock() else { return };
    let content = term.renderable_content();
    let show_cursor =
        content.mode.contains(TermMode::SHOW_CURSOR) && content.cursor.shape != CursorShape::Hidden;
    let cursor_col = content.cursor.point.column.0;
    let cursor_line = content.cursor.point.line.0;
    let selection = content.selection;
    let sel_bg = if dark {
        byte_rgb(0x2f, 0x65, 0xca)
    } else {
        byte_rgb(0xb4, 0xd7, 0xff)
    };

    for indexed in content.display_iter {
        let cell = indexed.cell;
        let col = indexed.point.column.0;
        let line = indexed.point.line.0;
        if line < 0 {
            continue;
        }
        let x = col as f64 * m.cell_w;
        let y = line as f64 * m.cell_h;

        let selected = selection
            .as_ref()
            .is_some_and(|s| s.contains(indexed.point));
        let mut fg = resolve(cell.fg, &pal, true);
        let mut bg = resolve(cell.bg, &pal, false);
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if selected {
            bg = sel_bg;
        }

        if bg != pal.bg {
            set_src(ctx, bg);
            ctx.rectangle(x, y, m.cell_w, m.cell_h);
            let _ = ctx.fill();
        }

        if cell.c != ' ' && cell.c != '\0' {
            select_font(ctx, family, size_px, cell.flags.contains(Flags::BOLD));
            set_src(ctx, fg);
            ctx.move_to(x, y + m.baseline);
            let mut b = [0u8; 4];
            let _ = ctx.show_text(cell.c.encode_utf8(&mut b));
        }
    }

    if show_cursor && cursor_line >= 0 {
        let x = cursor_col as f64 * m.cell_w;
        let y = cursor_line as f64 * m.cell_h;
        set_src(ctx, pal.cursor);
        match content.cursor.shape {
            CursorShape::Beam => ctx.rectangle(x, y, 2.0, m.cell_h),
            CursorShape::Underline => ctx.rectangle(x, y + m.cell_h - 2.0, m.cell_w, 2.0),
            _ => {
                // Block: outline so the glyph under it stays visible.
                ctx.set_line_width(1.0);
                ctx.rectangle(x + 0.5, y + 0.5, m.cell_w - 1.0, m.cell_h - 1.0);
                let _ = ctx.stroke();
                return;
            }
        }
        let _ = ctx.fill();
    }
}

fn set_src(ctx: &cairo::Context, (r, g, b): (f64, f64, f64)) {
    ctx.set_source_rgb(r, g, b);
}

/// Resolve an alacritty [`Color`] to an RGB triple in 0.0..=1.0.
fn resolve(color: Color, pal: &Palette, is_fg: bool) -> (f64, f64, f64) {
    match color {
        Color::Spec(rgb) => byte_rgb(rgb.r, rgb.g, rgb.b),
        Color::Indexed(i) => indexed(i, pal),
        Color::Named(named) => match named as usize {
            n @ 0..=15 => pal.ansi[n],
            256 => pal.fg,
            257 => pal.bg,
            258 => pal.cursor,
            n @ 259..=266 => pal.ansi[n - 259],
            267 | 268 => pal.fg,
            _ if is_fg => pal.fg,
            _ => pal.bg,
        },
    }
}

/// xterm 256-colour index → RGB.
fn indexed(i: u8, pal: &Palette) -> (f64, f64, f64) {
    match i {
        0..=15 => pal.ansi[i as usize],
        16..=231 => {
            let i = i - 16;
            let step = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            byte_rgb(step(i / 36), step((i / 6) % 6), step(i % 6))
        }
        232..=255 => {
            let v = 8 + (i - 232) * 10;
            byte_rgb(v, v, v)
        }
    }
}

fn byte_rgb(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    )
}

/// The palette per theme. The 16 ANSI colours are a standard set shared by
/// both themes; only fg / bg / cursor track light vs dark, mirroring the
/// editor scheme (kept in hex here, like `editor::mod::vcs_colors`).
fn palette(dark: bool) -> Palette {
    const ANSI: [(u8, u8, u8); 16] = [
        (0x2b, 0x2b, 0x2b), // black
        (0xc7, 0x54, 0x50), // red
        (0x6a, 0x87, 0x59), // green
        (0xbb, 0xb5, 0x29), // yellow
        (0x6a, 0x8b, 0xdf), // blue
        (0xb0, 0x82, 0xc5), // magenta
        (0x4a, 0x88, 0xc7), // cyan
        (0xbb, 0xbb, 0xbb), // white
        (0x60, 0x63, 0x66), // bright black
        (0xd1, 0x6a, 0x66), // bright red
        (0x7f, 0xa8, 0x66), // bright green
        (0xd6, 0xcf, 0x5b), // bright yellow
        (0x82, 0xa2, 0xe6), // bright blue
        (0xc7, 0x9c, 0xe6), // bright magenta
        (0x5c, 0xb0, 0xd6), // bright cyan
        (0xef, 0xef, 0xef), // bright white
    ];
    let ansi = ANSI.map(|(r, g, b)| byte_rgb(r, g, b));
    if dark {
        Palette {
            fg: byte_rgb(0xa9, 0xb7, 0xc6),
            bg: byte_rgb(0x2b, 0x2b, 0x2b),
            cursor: byte_rgb(0xce, 0xd0, 0xce),
            ansi,
        }
    } else {
        Palette {
            fg: byte_rgb(0x1d, 0x1d, 0x1d),
            bg: byte_rgb(0xff, 0xff, 0xff),
            cursor: byte_rgb(0x30, 0x30, 0x30),
            ansi,
        }
    }
}
