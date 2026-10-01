//! Color mapping and block painting shared by the playfield and HUD previews.

use ratatui::buffer::{Buffer, Cell};
use ratatui::style::{Color, Modifier, Style};

use crate::piece::Tetromino;

/// Canonical Guideline color for a tetromino.
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

const BLOCK: char = '█';
const GHOST: char = '▒';

/// How a block should be drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BlockStyle {
    Solid,
    Ghost,
    /// Greyed out (e.g. hold already used this turn).
    Disabled,
}

/// Bounds-checked cell access, so oversized layouts clip instead of panicking.
fn cell_at(buf: &mut Buffer, x: u16, y: u16) -> Option<&mut Cell> {
    let a = buf.area;
    (x >= a.left() && x < a.right() && y >= a.top() && y < a.bottom()).then(|| buf.get_mut(x, y))
}

/// Paint one solid `w`x`h` block at `(x, y)`. Terminal cells are about twice
/// as tall as wide, so callers use `w = 2 * h` for classic square blocks.
pub fn paint_block(buf: &mut Buffer, x: u16, y: u16, w: u16, h: u16, kind: Tetromino, style: BlockStyle) {
    let (ch, st) = match style {
        BlockStyle::Solid => (BLOCK, Style::default().fg(tetromino_color(kind)).add_modifier(Modifier::BOLD)),
        BlockStyle::Ghost => (GHOST, Style::default().fg(Color::DarkGray)),
        BlockStyle::Disabled => (BLOCK, Style::default().fg(Color::DarkGray)),
    };
    for row in 0..h {
        for col in 0..w {
            if let Some(cell) = cell_at(buf, x + col, y + row) {
                cell.set_char(ch).set_style(st);
            }
        }
    }
}
