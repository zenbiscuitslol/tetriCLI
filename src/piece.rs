//! Tetromino definitions, the SRS rotation system, and wall-kick tables.
//!
//! All cell offsets use board coordinates: `x` grows to the right and `y` grows
//! downward (row 0 is the top of the field). Spawn positions follow the Guideline
//! convention where the piece's bounding box is placed with its left edge at
//! column 3.

use serde::{Deserialize, Serialize};

pub const COLS: usize = 10;
pub const ROWS: usize = 20;
/// Visible row count. We keep two hidden rows above the visible field (rows -2
/// and -1) so that pieces can spawn cleanly and lock partially above the top.
pub const HIDDEN_ROWS: usize = 2;
pub const FIELD_ROWS: usize = ROWS + HIDDEN_ROWS;
pub const FIELD_COLS: usize = COLS;

/// The seven standard tetrominoes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tetromino {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

impl Tetromino {
    /// All variants in the natural Guideline order (used by the 7-bag).
    pub const ALL: [Self; 7] = [
        Self::I,
        Self::O,
        Self::T,
        Self::S,
        Self::Z,
        Self::J,
        Self::L,
    ];

    /// Returns the four cells of `self` in rotation `rot`, using the canonical
    /// SRS bounding-box coordinates (top-left origin of the box is column 0,
    /// row 0). These offsets are *not* yet shifted to the spawn column.
    pub fn cells(self, rot: Rotation) -> &'static [(i32, i32)] {
        SHAPES[self as usize][rot as usize]
    }

    /// A short, uppercase identifier used by the HUD and stats.
    #[allow(dead_code)]
    pub fn letter(self) -> char {
        match self {
            Self::I => 'I',
            Self::O => 'O',
            Self::T => 'T',
            Self::S => 'S',
            Self::Z => 'Z',
            Self::J => 'J',
            Self::L => 'L',
        }
    }

    /// Spawn column (left edge of the bounding box).
    pub fn spawn_x(self) -> i32 {
        match self {
            Self::O => 3,
            Self::I => 3,
            _ => 3,
        }
    }

    /// Spawn row (top edge of the bounding box). Most pieces spawn with the box
    /// beginning at hidden row 0; the empty top portion of rotation 0 simply keeps
    /// them visually centered.
    pub fn spawn_y(self) -> i32 {
        0
    }
}

/// The four SRS rotation states. Clockwise increments the index (wrapping),
/// counter-clockwise decrements it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Rotation {
    Spawn = 0,
    Clockwise = 1,
    OneEighty = 2,
    CounterClockwise = 3,
}

impl Rotation {
    /// Rotate clockwise, wrapping around at 3.
    pub fn cw(self) -> Self {
        match self {
            Self::Spawn => Self::Clockwise,
            Self::Clockwise => Self::OneEighty,
            Self::OneEighty => Self::CounterClockwise,
            Self::CounterClockwise => Self::Spawn,
        }
    }

    /// Rotate counter-clockwise, wrapping around at 3.
    pub fn ccw(self) -> Self {
        match self {
            Self::Spawn => Self::CounterClockwise,
            Self::Clockwise => Self::Spawn,
            Self::OneEighty => Self::Clockwise,
            Self::CounterClockwise => Self::OneEighty,
        }
    }

    /// Rotate by `self` -> 180.
    #[allow(dead_code)]
    pub fn one_eighty(self) -> Self {
        match self {
            Self::Spawn => Self::OneEighty,
            Self::Clockwise => Self::CounterClockwise,
            Self::OneEighty => Self::Spawn,
            Self::CounterClockwise => Self::Clockwise,
        }
    }
}

/// A live piece on the field: a tetromino type, its rotation, and the position
/// of its bounding-box's top-left corner in board coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivePiece {
    pub kind: Tetromino,
    pub rot: Rotation,
    pub x: i32,
    pub y: i32,
}

impl ActivePiece {
    pub fn new(kind: Tetromino) -> Self {
        Self {
            kind,
            rot: Rotation::Spawn,
            x: kind.spawn_x(),
            y: kind.spawn_y(),
        }
    }

    /// The absolute board coordinates of every filled cell of this piece.
    pub fn cells(&self) -> [(i32, i32); 4] {
        let base = self.kind.cells(self.rot);
        let mut out = [(0, 0); 4];
        for i in 0..4 {
            out[i] = (self.x + base[i].0, self.y + base[i].1);
        }
        out
    }

    pub fn moved(&self, dx: i32, dy: i32) -> Self {
        Self {
            kind: self.kind,
            rot: self.rot,
            x: self.x + dx,
            y: self.y + dy,
        }
    }

    #[allow(dead_code)]
    pub fn rotated(&self, rot: Rotation) -> Self {
        Self {
            kind: self.kind,
            rot,
            x: self.x,
            y: self.y,
        }
    }
}

/// SRS shape tables: `[tetromino][rotation]` -> four `(x, y)` cells inside the
/// piece's canonical bounding box.
///
/// The box is 4x4 for I, 2x2 for O (but still expressed inside a 4-wide layout
/// for uniformity), and 3x3 for the rest. Coordinates use a top-left origin.
///
/// These are the canonical Guideline SRS representations.
static SHAPES: [[&[(i32, i32)]; 4]; 7] = [
    // I
    [
        &[(0, 1), (1, 1), (2, 1), (3, 1)],
        &[(2, 0), (2, 1), (2, 2), (2, 3)],
        &[(0, 2), (1, 2), (2, 2), (3, 2)],
        &[(1, 0), (1, 1), (1, 2), (1, 3)],
    ],
    // O
    [
        &[(1, 0), (2, 0), (1, 1), (2, 1)],
        &[(1, 0), (2, 0), (1, 1), (2, 1)],
        &[(1, 0), (2, 0), (1, 1), (2, 1)],
        &[(1, 0), (2, 0), (1, 1), (2, 1)],
    ],
    // T
    [
        &[(1, 0), (0, 1), (1, 1), (2, 1)],
        &[(1, 0), (1, 1), (2, 1), (1, 2)],
        &[(0, 1), (1, 1), (2, 1), (1, 2)],
        &[(1, 0), (0, 1), (1, 1), (1, 2)],
    ],
    // S
    [
        &[(1, 0), (2, 0), (0, 1), (1, 1)],
        &[(1, 0), (1, 1), (2, 1), (2, 2)],
        &[(1, 1), (2, 1), (0, 2), (1, 2)],
        &[(0, 0), (0, 1), (1, 1), (1, 2)],
    ],
    // Z
    [
        &[(0, 0), (1, 0), (1, 1), (2, 1)],
        &[(2, 0), (1, 1), (2, 1), (1, 2)],
        &[(0, 1), (1, 1), (1, 2), (2, 2)],
        &[(1, 0), (0, 1), (1, 1), (0, 2)],
    ],
    // J
    [
        &[(0, 0), (0, 1), (1, 1), (2, 1)],
        &[(1, 0), (2, 0), (1, 1), (1, 2)],
        &[(0, 1), (1, 1), (2, 1), (2, 2)],
        &[(1, 0), (1, 1), (1, 2), (0, 2)],
    ],
    // L
    [
        &[(2, 0), (0, 1), (1, 1), (2, 1)],
        &[(1, 0), (1, 1), (1, 2), (2, 2)],
        &[(0, 1), (1, 1), (2, 1), (0, 2)],
        &[(0, 0), (1, 0), (1, 1), (1, 2)],
    ],
];

/// Wall-kick test offsets.
///
/// Each entry is `(dx, dy)` in board coordinates (y down). The SRS tables are
/// stored with y *up* in the source literature, so the y component is negated
/// here for convenience. A rotation from state `from` to state `to` tries each
/// offset in order: the identity by itself, then the kicks.
struct WallKickTable {
    /// `[from_rotation][to_rotation]` -> slice of kick offsets (identity first).
    offsets: &'static [[&'static [(i32, i32)]; 4]; 4],
}

impl WallKickTable {
    /// Get the ordered list of kick offsets to try for `from -> to`.
    fn kicks(&self, from: Rotation, to: Rotation) -> &'static [(i32, i32)] {
        self.offsets[from as usize][to as usize]
    }
}

// JLSTZ wall kicks (identity first, followed by the four "flick" tests).
// Source offsets use (dx, dy_up); stored here as (dx, -dy_up).
const JLSTZ_KICKS: [[&[(i32, i32)]; 4]; 4] = [
    // from Spawn (0)
    [
        // -> Spawn (no-op)
        &[(0, 0)],
        // -> Clockwise (1)
        &[(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
        // -> OneEighty (2)
        &[(0, 0)],
        // -> CounterClockwise (3)
        &[(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
    ],
    // from Clockwise (1)
    [
        // -> Spawn (0)
        &[(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
        // -> Clockwise (no-op)
        &[(0, 0)],
        // -> OneEighty (2)
        &[(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
        // -> CounterClockwise (3)
        &[(0, 0)],
    ],
    // from OneEighty (2)
    [
        // -> Spawn (0)
        &[(0, 0)],
        // -> Clockwise (1)
        &[(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
        // -> OneEighty (no-op)
        &[(0, 0)],
        // -> CounterClockwise (3)
        &[(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
    ],
    // from CounterClockwise (3)
    [
        // -> Spawn (0)
        &[(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
        // -> Clockwise (1)
        &[(0, 0)],
        // -> OneEighty (2)
        &[(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
        // -> CounterClockwise (no-op)
        &[(0, 0)],
    ],
];

// I wall kicks.
const I_KICKS: [[&[(i32, i32)]; 4]; 4] = [
    // from Spawn (0)
    [
        &[(0, 0)],
        &[(0, 0), (-2, 0), (1, 0), (-2, 2), (1, -1)],
        &[(0, 0)],
        &[(0, 0), (2, 0), (-1, 0), (2, -2), (-1, 2)],
    ],
    // from Clockwise (1)
    [
        &[(0, 0), (2, 0), (-1, 0), (2, -2), (-1, 2)],
        &[(0, 0)],
        &[(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
        &[(0, 0)],
    ],
    // from OneEighty (2)
    [
        &[(0, 0)],
        &[(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 2)],
        &[(0, 0)],
        &[(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
    ],
    // from CounterClockwise (3)
    [
        &[(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 2)],
        &[(0, 0)],
        &[(0, 0), (-2, 0), (1, 0), (-2, 2), (1, -1)],
        &[(0, 0)],
    ],
];

fn kicks_for(kind: Tetromino) -> &'static [[&'static [(i32, i32)]; 4]; 4] {
    match kind {
        Tetromino::I => &I_KICKS,
        _ => &JLSTZ_KICKS,
    }
}

/// The result of an attempted rotation, including which kick offset succeeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RotateResult {
    /// The resulting piece (already moved by the kick that worked), or the
    /// un-rotated piece if rotation failed.
    pub piece: ActivePiece,
    /// Whether the rotation succeeded.
    pub success: bool,
    /// Index of the kick offset that succeeded (0 = no kick / identity).
    pub kick_index: usize,
}

/// Attempt to rotate `piece` from `piece.rot` to `to` against `board`, applying
/// SRS wall kicks. Returns the resulting piece and metadata regardless of
/// whether a kick succeeded.
pub fn try_rotate(piece: ActivePiece, to: Rotation, filled: &impl Fn(i32, i32) -> bool) -> RotateResult {
    let table = kicks_for(piece.kind);
    let kicks = WallKickTable { offsets: table }.kicks(piece.rot, to);
    for (idx, &(dx, dy)) in kicks.iter().enumerate() {
        let candidate = ActivePiece {
            kind: piece.kind,
            rot: to,
            x: piece.x + dx,
            y: piece.y + dy,
        };
        if !collides(candidate, filled) {
            return RotateResult {
                piece: candidate,
                success: true,
                kick_index: idx,
            };
        }
    }
    RotateResult {
        piece,
        success: false,
        kick_index: 0,
    }
}

/// True if `piece` overlaps a filled cell or is out of horizontal bounds.
pub fn collides(piece: ActivePiece, filled: &impl Fn(i32, i32) -> bool) -> bool {
    for &(x, y) in &piece.cells() {
        if x < 0 || x >= FIELD_COLS as i32 {
            return true;
        }
        // Falling off the bottom is a collision; pieces above the top hidden
        // rows are tolerated.
        if y >= FIELD_ROWS as i32 {
            return true;
        }
        if y >= 0 && filled(x, y) {
            return true;
        }
    }
    false
}