//! Widget-level UI rendering helpers: block (box) drawing, color mapping for
//! tetrominoes, and a few small composite widgets shared by the renderer and
//! menus.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::piece::Tetromino;

/// Returns the canonical Guideline cell color for a tetromino.
pub fn tetromino_color(kind: Tetromino) -> Color {
    match kind {
        Tetromino::I => Color::Cyan,
        Tetromino::O => Color::Yellow,
        Tetromino::T => Color::Magenta,
        Tetromino::S => Color::Green,
        Tetromino::Z => Color::Red,
        Tetromino::J => Color::Blue,
        Tetromino::L => Color::LightYellow,
    }
}

/// The block glyph used for filled cells (a full block plus a subtle inset).
pub const BLOCK: &str = "█";

/// A single-character, dimmer glyph used for ghost pieces.
pub const GHOST: &str = "▒";

/// A centered title bar for bordered widgets.
#[allow(dead_code)]
pub fn title<'a>(text: &'a str) -> Line<'a> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    ))
}

/// Style a string with the tetromino's color (used for the hold/next previews).
#[allow(dead_code)]
pub fn colored(text: &str, kind: Tetromino) -> Span<'_> {
    Span::styled(text.to_string(), Style::default().fg(tetromino_color(kind)))
}