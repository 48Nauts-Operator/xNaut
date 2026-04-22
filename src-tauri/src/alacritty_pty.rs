// ABOUTME: Alacritty-backed terminal emulation. Wraps a PTY with alacritty_terminal's
// ABOUTME: parser to fix Unicode width bugs that cause TUI apps to render incorrectly.

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{cell::Cell, Config, Term};
use alacritty_terminal::vte::ansi::{Color, Rgb};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Dimensions for a terminal grid.
#[derive(Debug, Clone, Copy)]
pub struct GridSize {
    pub cols: usize,
    pub lines: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// No-op event listener — we don't need any callbacks for now (bell, title change, etc.).
#[derive(Clone)]
pub struct NoOpListener;

impl EventListener for NoOpListener {
    fn send_event(&self, _event: Event) {}
}

/// Wraps an alacritty Term and its parser so we can feed bytes from a PTY
/// and read out the cell grid.
pub struct AlacrittyTerm {
    term: Arc<Mutex<Term<NoOpListener>>>,
    parser: Arc<Mutex<alacritty_terminal::vte::ansi::Processor>>,
}

impl AlacrittyTerm {
    pub fn new(cols: u16, rows: u16) -> Self {
        let size = GridSize {
            cols: cols as usize,
            lines: rows as usize,
        };
        let config = Config::default();
        let term = Term::new(config, &size, NoOpListener);
        Self {
            term: Arc::new(Mutex::new(term)),
            parser: Arc::new(Mutex::new(
                alacritty_terminal::vte::ansi::Processor::new(),
            )),
        }
    }

    /// Feed a chunk of bytes from the PTY into the parser.
    pub async fn feed(&self, bytes: &[u8]) {
        let mut term = self.term.lock().await;
        let mut parser = self.parser.lock().await;
        parser.advance(&mut *term, bytes);
    }

    /// Resize the terminal grid. Takes &self so it works through Arc.
    pub async fn resize(&self, cols: u16, rows: u16) {
        let size = GridSize {
            cols: cols as usize,
            lines: rows as usize,
        };
        let mut term = self.term.lock().await;
        term.resize(size);
    }

    /// Snapshot the current visible grid as a serializable structure.
    pub async fn snapshot(&self) -> GridSnapshot {
        let term = self.term.lock().await;
        let cols = term.columns();
        let lines = term.screen_lines();
        let grid = term.grid();
        let cursor_point = grid.cursor.point;

        let mut rows = Vec::with_capacity(lines);
        for line in 0..lines as i32 {
            let mut cells = Vec::with_capacity(cols);
            for col in 0..cols {
                let point = Point::new(Line(line), Column(col));
                let cell = &grid[point];
                cells.push(serialize_cell(cell));
            }
            rows.push(GridRow { cells });
        }

        GridSnapshot {
            cols,
            rows: lines,
            cursor_row: cursor_point.line.0,
            cursor_col: cursor_point.column.0,
            grid: rows,
        }
    }
}

/// Serialized cell with character + color + style attrs.
#[derive(Debug, Clone, Serialize)]
pub struct SerCell {
    /// The character at this cell. Space if empty.
    pub c: String,
    /// Foreground color as `#rrggbb` or named color index.
    pub fg: String,
    /// Background color as `#rrggbb` or named color index.
    pub bg: String,
    /// Style flags packed into a u16: bold=1, italic=2, underline=4, etc.
    pub flags: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct GridRow {
    pub cells: Vec<SerCell>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GridSnapshot {
    pub cols: usize,
    pub rows: usize,
    pub cursor_row: i32,
    pub cursor_col: usize,
    pub grid: Vec<GridRow>,
}

fn serialize_cell(cell: &Cell) -> SerCell {
    SerCell {
        c: cell.c.to_string(),
        fg: color_to_string(&cell.fg),
        bg: color_to_string(&cell.bg),
        flags: pack_flags(cell.flags),
    }
}

fn color_to_string(color: &Color) -> String {
    match color {
        Color::Named(named) => format!("named:{:?}", named),
        Color::Spec(Rgb { r, g, b }) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        Color::Indexed(idx) => format!("idx:{}", idx),
    }
}

fn pack_flags(flags: Flags) -> u16 {
    let mut packed = 0u16;
    if flags.contains(Flags::BOLD) {
        packed |= 1;
    }
    if flags.contains(Flags::ITALIC) {
        packed |= 2;
    }
    if flags.contains(Flags::UNDERLINE) {
        packed |= 4;
    }
    if flags.contains(Flags::STRIKEOUT) {
        packed |= 8;
    }
    if flags.contains(Flags::INVERSE) {
        packed |= 16;
    }
    if flags.contains(Flags::DIM) {
        packed |= 32;
    }
    packed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn smoke_create_and_feed_plain_text() {
        let term = AlacrittyTerm::new(80, 24);
        term.feed(b"Hello, world!").await;
        let snapshot = term.snapshot().await;
        assert_eq!(snapshot.cols, 80);
        assert_eq!(snapshot.rows, 24);
        // First 13 cells should contain "Hello, world!"
        let actual: String = snapshot.grid[0].cells[..13]
            .iter()
            .map(|c| c.c.clone())
            .collect();
        assert_eq!(actual, "Hello, world!");
    }

    #[tokio::test]
    async fn handles_unicode_width_correctly() {
        // The em-dash and other wide chars should not corrupt later positions.
        let term = AlacrittyTerm::new(80, 24);
        term.feed(b"a\xe2\x80\x94b").await; // a—b
        let snapshot = term.snapshot().await;
        assert_eq!(snapshot.grid[0].cells[0].c, "a");
        assert_eq!(snapshot.grid[0].cells[1].c, "—");
        assert_eq!(snapshot.grid[0].cells[2].c, "b");
    }

    #[tokio::test]
    async fn ansi_color_escape_sequences_apply() {
        // Red foreground, then text, then reset
        let term = AlacrittyTerm::new(80, 24);
        term.feed(b"\x1b[31mERR\x1b[0m").await;
        let snapshot = term.snapshot().await;
        assert_eq!(snapshot.grid[0].cells[0].c, "E");
        // Color should be Red (named color)
        assert!(snapshot.grid[0].cells[0].fg.contains("Red"));
    }
}
