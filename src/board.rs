//! The play field: a 10-column wide, 22-row tall grid (two hidden rows above
//! the visible 20). Stores a `Cell` per position and handles collision queries,
//! line clearing, and lock-in.

use crate::piece::{ActivePiece, Tetromino, FIELD_COLS, FIELD_ROWS, HIDDEN_ROWS};

/// An empty cell or one filled with a tetromino whose color this carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Cell(pub Option<Tetromino>);

impl Cell {
    pub const EMPTY: Cell = Cell(None);
    pub fn is_empty(self) -> bool {
        self.0.is_none()
    }
}

/// The Tetris play field.
#[derive(Clone, Debug)]
pub struct Board {
    /// Row-major: `[row][col]`, row 0 is the top hidden row.
    cells: Vec<Vec<Cell>>,
    /// Tracks the highest row containing any locked block (for fast top-out
    /// detection and perfect-clear checks). Bounds: `0..=FIELD_ROWS`.
    highest_row: usize,
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl Board {
    pub fn new() -> Self {
        Self {
            cells: vec![vec![Cell::EMPTY; FIELD_COLS]; FIELD_ROWS],
            highest_row: FIELD_ROWS,
        }
    }

    /// Returns `true` if the cell at `(x, y)` is occupied by a locked block.
    /// Out-of-range vertical cells above the field are treated as empty.
    pub fn filled(&self, x: i32, y: i32) -> bool {
        if y < 0 {
            return false;
        }
        if x < 0 || x >= FIELD_COLS as i32 || y >= FIELD_ROWS as i32 {
            return false;
        }
        !self.cells[y as usize][x as usize].is_empty()
    }

    /// Stamp `piece` into the field. Cells above the top hidden rows that would
    /// land above the field (y < 0) are recorded as a top-out condition by the
    /// caller; here they are simply skipped.
    pub fn lock(&mut self, piece: ActivePiece) -> LockOutcome {
        let mut above_top = false;
        for (x, y) in piece.cells() {
            if y < 0 {
                above_top = true;
                continue;
            }
            if (x as usize) < FIELD_COLS && (y as usize) < FIELD_ROWS {
                self.cells[y as usize][x as usize] = Cell(Some(piece.kind));
                if y < self.highest_row as i32 {
                    self.highest_row = y as usize;
                }
            }
        }
        if above_top {
            LockOutcome::TopOut
        } else {
            LockOutcome::Locked
        }
    }

    /// Clear all full rows. Returns the indices of cleared rows (ascending) and
    /// shifts the field down, maintaining the two hidden rows at the top.
    pub fn clear_lines(&mut self) -> Vec<usize> {
        let mut cleared: Vec<usize> = Vec::new();
        for y in 0..FIELD_ROWS {
            if self.cells[y].iter().all(|c| !c.is_empty()) {
                cleared.push(y);
            }
        }
        if cleared.is_empty() {
            return cleared;
        }
        // Compact: keep non-full rows, then prepend empty rows at the top.
        let mut surviving: Vec<Vec<Cell>> = Vec::with_capacity(FIELD_ROWS);
        for y in 0..FIELD_ROWS {
            if !cleared.contains(&y) {
                surviving.push(std::mem::take(&mut self.cells[y]));
            }
        }
        let removed = FIELD_ROWS - surviving.len();
        // Empty top rows (including hidden rows).
        let mut new_rows = vec![vec![Cell::EMPTY; FIELD_COLS]; removed];
        new_rows.append(&mut surviving);
        self.cells = new_rows;

        // Recompute highest row lazily.
        self.highest_row = self.topmost_filled_row().unwrap_or(FIELD_ROWS);
        cleared
    }

    fn topmost_filled_row(&self) -> Option<usize> {
        self.cells.iter().position(|row| row.iter().any(|c| !c.is_empty()))
    }

    /// The number of the topmost filled row (FIELD_ROWS if the board is empty).
    #[allow(dead_code)]
    pub fn highest_filled_row(&self) -> usize {
        self.highest_row
    }

    /// True when the entire field is empty (the perfect-clear precondition).
    pub fn is_empty(&self) -> bool {
        self.highest_row == FIELD_ROWS
    }

    /// True when any block rests inside the spawn rows (immediate top-out on
    /// next spawn / block-out detection helper).
    pub fn blocked_at_spawn(&self, piece: ActivePiece) -> bool {
        crate::piece::collides(piece, &|x, y| self.filled(x, y))
    }

    /// Read-only access to the raw cell grid for rendering.
    pub fn cells(&self) -> &[Vec<Cell>] {
        &self.cells
    }

    /// Convert an absolute field row index to a visible row index, or `None`
    /// if it falls within the hidden rows.
    pub fn visible_row(abs_row: usize) -> Option<usize> {
        if abs_row < HIDDEN_ROWS {
            None
        } else {
            Some(abs_row - HIDDEN_ROWS)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockOutcome {
    /// The piece locked entirely within the field.
    Locked,
    /// Part of the piece locked above the top of the field (game over).
    TopOut,
}