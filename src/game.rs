//! Core Guideline-style Tetris gameplay: 7-bag randomizer, SRS rotation with
//! wall kicks, DAS/ARR, soft and hard drop, lock delay with movement resets,
//! hold mechanic, ghost piece, gravity scaling by level, and a scoring model
//! with combos, back-to-back tracking, and T-Spin / T-Spin mini / perfect-clear
//! detection closely following TETR.IO's modern Guideline feel.

use std::cmp;

use crate::board::{Board, LockOutcome};
use crate::piece::{self, ActivePiece, Rotation, Tetromino, FIELD_COLS, FIELD_ROWS};
use crate::settings::Settings;

/// Durations used by the game loop, all in milliseconds.
pub const DT_MS: u64 = 1000 / 60;

/// A compact per-frame snapshot of triggers the renderer/UX cares about, used to
/// surface transient feedback (recent clear text, spin banners) without
/// threading separate event channels.
#[derive(Clone, Debug, Default)]
pub struct EventLog {
    pub last_clear_text: Option<String>,
    pub last_b2b: bool,
    pub last_combo: u32,
    pub last_special: SpecialKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpecialKind {
    #[default]
    None,
    Single,
    Double,
    Triple,
    Tetris,
    TSpin,
    TSpinSingle,
    TSpinDouble,
    TSpinTriple,
    TSpinMini,
    PerfectClear,
}

impl SpecialKind {
    pub fn is_tspin(self) -> bool {
        matches!(
            self,
            SpecialKind::TSpin
                | SpecialKind::TSpinSingle
                | SpecialKind::TSpinDouble
                | SpecialKind::TSpinTriple
                | SpecialKind::TSpinMini
        )
    }

    #[allow(dead_code)]
    pub fn is_difficult(self) -> bool {
        // Tetris and any T-Spin (single/double/triple; mini does NOT count for
        // back-to-back) are "difficult" line clears that maintain back-to-back.
        matches!(
            self,
            SpecialKind::Tetris
                | SpecialKind::TSpinSingle
                | SpecialKind::TSpinDouble
                | SpecialKind::TSpinTriple
        )
    }

    #[allow(dead_code)]
    pub fn lines(self) -> u32 {
        match self {
            SpecialKind::Single | SpecialKind::TSpinSingle => 1,
            SpecialKind::Double => 2,
            SpecialKind::TSpinDouble => 2,
            SpecialKind::Triple | SpecialKind::TSpinTriple => 3,
            SpecialKind::Tetris => 4,
            _ => 0,
        }
    }
}

/// The live game state.
pub struct Game {
    pub board: Board,
    pub settings: Settings,

    // Piece logistics.
    pub bag: Vec<Tetromino>,
    pub queue: Vec<Tetromino>,
    pub current: Option<ActivePiece>,
    pub hold: Option<Tetromino>,
    pub hold_used: bool,

    // Scoring / progress.
    pub score: u64,
    pub level: u32,
    pub total_lines: u32,
    pub combo: Option<u32>,
    pub back_to_back: bool,
    pub pieces_placed: u64,
    pub start_time: std::time::Instant,
    pub last_lock_was_tspin: bool,

    // Timing, DAS/ARR.
    gravity_ms: f64,
    gravity_acc: f64,
    // Retained for settings.json backward-compatibility; DAS auto-shift was
    // removed in favour of one-cell-per-press movement.
    #[allow(dead_code)]
    das_dir: DaspDirection,
    #[allow(dead_code)]
    das_timer: u64,
    #[allow(dead_code)]
    arr_timer: u64,
    soft_dropping: bool,

    // Lock delay.
    lock_timer: Option<u64>,
    lock_resets: u32,
    on_ground: bool,

    // Spawn delay (ARE-like short grace before a new piece appears).
    spawn_delay: u64,

    // Per-frame event log for the HUD.
    pub events: EventLog,
    /// Countdown (ms) for the transient clear banner shown on the HUD.
    clear_banner_timer: u64,

    // True once the current piece's spawn fully atop hidden rows would overlap
    // the field — handles block-out / lock-out top-out conditions.
    pub game_over: bool,

    // Stats of the most recent lock (used by T-Spin detection which requires
    // "the last action was a rotation").
    last_action_was_rotate: bool,
    last_rotation_kick: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DaspDirection {
    None,
    Left,
    Right,
}

impl Game {
    pub fn new(mut settings: Settings) -> Self {
        let level = settings.gameplay.starting_level.max(1);
        // Pin the starting level back into the setting so persisted stats replay
        // the same starting speed.
        settings.gameplay.starting_level = level;
        let mut game = Self {
            board: Board::new(),
            settings,
            bag: Vec::new(),
            queue: Vec::new(),
            current: None,
            hold: None,
            hold_used: false,
            score: 0,
            level,
            total_lines: 0,
            combo: None,
            back_to_back: false,
            pieces_placed: 0,
            start_time: std::time::Instant::now(),
            last_lock_was_tspin: false,
            gravity_ms: gravity_for_level(level),
            gravity_acc: 0.0,
            das_dir: DaspDirection::None,
            das_timer: 0,
            arr_timer: 0,
            soft_dropping: false,
            lock_timer: None,
            lock_resets: 0,
            on_ground: false,
            spawn_delay: 0,
            events: EventLog::default(),
            clear_banner_timer: 0,
            game_over: false,
            last_action_was_rotate: false,
            last_rotation_kick: 0,
        };
        game.refill_bag();
        game.refill_bag();
        game.refill_queue();
        game.spawn_next();
        game
    }

    /// Reset to a fresh game keeping the same settings (used by "Restart").
    pub fn reset(&mut self) {
        let settings = self.settings.clone();
        *self = Self::new(settings);
    }

    /// Refill the bag with all 7 tetrominoes in a random order.
    fn refill_bag(&mut self) {
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        let mut next: [Tetromino; 7] = Tetromino::ALL;
        next.shuffle(&mut rng);
        self.bag.extend_from_slice(&next);
    }

    /// Ensure at least 5 pieces are queued (visual "next" depth).
    fn refill_queue(&mut self) {
        const QUEUE_DEPTH: usize = 5;
        while self.queue.len() < QUEUE_DEPTH {
            if self.bag.is_empty() {
                self.refill_bag();
            }
            // bag is guaranteed non-empty here
            let kind = self.bag.remove(self.bag.len() - 1);
            self.queue.push(kind);
        }
    }

    /// Spawn the next piece from the queue. Returns whether spawn succeeded
    /// (a piece overlapping blocks on spawn is immediate game over).
    fn spawn_next(&mut self) -> bool {
        self.refill_queue();
        let kind = self.queue.remove(0);
        self.refill_queue();
        self.spawn_kind(kind)
    }

    fn spawn_kind(&mut self, kind: Tetromino) -> bool {
        let piece = ActivePiece::new(kind);
        self.last_action_was_rotate = false;
        self.last_rotation_kick = 0;
        self.on_ground = false;
        self.lock_timer = None;
        self.lock_resets = 0;
        self.spawn_delay = 0;
        self.gravity_acc = 0.0;
        if self.board.blocked_at_spawn(piece) {
            self.current = None;
            self.game_over = true;
            return false;
        }
        self.current = Some(piece);
        true
    }

    /// Attempt a hold swap. The held piece (if any) spawns immediately; the
    /// current piece is put on hold. Only one hold per piece is allowed.
    pub fn try_hold(&mut self) {
        if self.hold_used || self.game_over {
            return;
        }
        let Some(cur) = self.current else { return };
        let stored = self.hold.replace(cur.kind);
        self.hold_used = true;
        if let Some(prev) = stored {
            // A held piece always spawns in rotation 0.
            self.spawn_kind(prev);
        } else {
            self.spawn_next();
        }
    }

    /// Move the current piece horizontally by `dx` (0 allowed for reset
    /// bookkeeping). Returns whether it moved.
    pub fn shift(&mut self, dx: i32) -> bool {
        let Some(piece) = self.current else { return false };
        let candidate = piece.moved(dx, 0);
        if !collides(&self.board, candidate) {
            self.current = Some(candidate);
            self.last_action_was_rotate = false;
            self.maybe_reset_lock();
            true
        } else {
            false
        }
    }

    /// Drop the piece one row if possible. Returns whether it moved.
    pub fn soft_drop_step(&mut self) -> bool {
        let Some(piece) = self.current else { return false };
        let candidate = piece.moved(0, 1);
        if !collides(&self.board, candidate) {
            self.current = Some(candidate);
            self.last_action_was_rotate = false;
            // Soft drop awards 1 point per cell dropped.
            self.score += 1;
            true
        } else {
            false
        }
    }

    /// Hard drop: instantly drop to the floor and lock immediately.
    pub fn hard_drop(&mut self) {
        let Some(piece) = self.current else { return };
        let (dropped, grounded) = self.drop_distance(piece);
        let new_piece = piece.moved(0, dropped);
        self.current = Some(new_piece);
        // Hard drop awards 2 points per cell dropped.
        self.score += 2 * dropped as u64;
        if dropped == 0 && !grounded {
            // Shouldn't normally happen, but be defensive.
            return;
        }
        self.last_action_was_rotate = false;
        self.lock_current(true);
    }

    /// Compute how many rows `piece` can fall before resting and whether it
    /// ended grounded.
    fn drop_distance(&self, piece: ActivePiece) -> (i32, bool) {
        let mut dropped = 0i32;
        loop {
            let candidate = piece.moved(0, dropped + 1);
            if collides(&self.board, candidate) {
                return (dropped, true);
            }
            dropped += 1;
        }
    }

    /// Rotate the current piece clockwise (true) or counter-clockwise (false).
    pub fn rotate(&mut self, cw: bool) {
        let Some(piece) = self.current else { return };
        let to = if cw { piece.rot.cw() } else { piece.rot.ccw() };
        let result = piece::try_rotate(piece, to, &|x, y| self.board.filled(x, y));
        if result.success {
            self.current = Some(result.piece);
            self.last_action_was_rotate = true;
            self.last_rotation_kick = result.kick_index;
            self.maybe_reset_lock();
        }
    }

    /// Begin DAS charging in `dir`. Retained for backward compatibility; the
    /// auto-shift loop (`update_das`) was removed so this now does nothing
    /// useful but keeps the public API stable.
    #[allow(dead_code)]
    pub fn das_start(&mut self, dir: DaspDirection) {
        if dir == self.das_dir {
            return;
        }
        self.das_dir = dir;
        self.das_timer = 0;
        self.arr_timer = 0;
    }

    /// Release DAS (a move key was lifted).
    #[allow(dead_code)]
    pub fn das_release(&mut self, dir: DaspDirection) {
        if self.das_dir == dir {
            self.das_dir = DaspDirection::None;
            self.das_timer = 0;
            self.arr_timer = 0;
        }
    }

    pub fn set_soft_dropping(&mut self, v: bool) {
        self.soft_dropping = v;
    }

    /// Advance the game by one frame (`DT_MS` milliseconds).
    pub fn tick(&mut self) {
        if self.game_over {
            return;
        }

        // Expire the transient clear banner after ~2 seconds.
        if self.clear_banner_timer > 0 {
            self.clear_banner_timer = self.clear_banner_timer.saturating_sub(DT_MS);
            if self.clear_banner_timer == 0 {
                self.events.last_clear_text = None;
            }
        }

        if self.spawn_delay > 0 {
            self.spawn_delay = self.spawn_delay.saturating_sub(DT_MS);
            if self.spawn_delay == 0 {
                self.spawn_next();
            }
            return;
        }
        if self.current.is_none() {
            return;
        }

        // Soft drop gravity override.
        let g = if self.soft_dropping {
            (self.gravity_ms / self.settings.gameplay.soft_drop_factor.max(1) as f64).max(1.0)
        } else {
            self.gravity_ms
        };

        self.gravity_acc += DT_MS as f64;
        while self.gravity_acc >= g {
            self.gravity_acc -= g;
            if !self.soft_drop_step() {
                self.gravity_acc = 0.0;
                break;
            }
        }

        // Recompute grounded state using the *current* piece position (which
        // may have changed due to DAS or gravity this tick).
        let grounded = self
            .current
            .map(|p| self.is_grounded(p))
            .unwrap_or(false);
        self.on_ground = grounded;
        if grounded {
            if self.lock_timer.is_none() {
                self.lock_timer = Some(0);
            }
            if let Some(t) = self.lock_timer.as_mut() {
                *t += DT_MS;
            }
            if let Some(t) = self.lock_timer {
                if t >= self.settings.gameplay.lock_delay_ms {
                    self.lock_current(false);
                    return;
                }
            }
        } else {
            self.lock_timer = None;
        }
    }

    // DAS/ARR auto-shift was removed so that each move-key press shifts the
    // piece by exactly one cell. `das_start` / `das_release` below remain as
    // harmless no-ops so existing callers and the persisted `das`/`arr` settings
    // stay valid for backward compatibility.

    fn is_grounded(&self, piece: ActivePiece) -> bool {
        collides(&self.board, piece.moved(0, 1))
    }

    /// When grounded and a movement/rotation occurs, reset the lock timer up to
    /// `max_lock_resets` times (TETR.IO/Guideline "move reset" behavior).
    fn maybe_reset_lock(&mut self) {
        let grounded = self
            .current
            .map(|p| self.is_grounded(p))
            .unwrap_or(false);
        if grounded && self.lock_resets < self.settings.gameplay.max_lock_resets {
            self.lock_timer = Some(0);
            self.lock_resets += 1;
        }
    }

    /// Lock the current piece into the field, compute line clears and scoring,
    /// then either spawn the next piece or enter spawn delay.
    fn lock_current(&mut self, from_hard_drop: bool) {
        let Some(piece) = self.current.take() else { return };

        // Detect T-Spin before locking (last action was a successful rotation
        // and the piece is a T).
        let kind = detect_spin(&self.board, piece, self.last_action_was_rotate, self.last_rotation_kick);

        let outcome = self.board.lock(piece);
        if matches!(outcome, LockOutcome::TopOut) {
            self.game_over = true;
            return;
        }
        self.pieces_placed += 1;
        self.hold_used = false;

        let cleared = self.board.clear_lines();
        let lines = cleared.len() as u32;

        // Perfect clear: the field becomes empty after a line clear.
        let perfect_clear = lines > 0 && self.board.is_empty();

        // Score the lock.
        self.score_lines(lines, kind, perfect_clear);

        if lines > 0 {
            self.total_lines += lines;
            // Level progression: every 10 lines.
            let new_level = cmp::max(
                self.settings.gameplay.starting_level,
                self.settings.gameplay.starting_level + self.total_lines / 10,
            );
            if new_level != self.level {
                self.level = new_level;
                self.gravity_ms = gravity_for_level(self.level);
            }
        }

        self.last_lock_was_tspin = kind.is_tspin();
        self.last_action_was_rotate = false;

        // Build a HUD event summary.
        let special = combine_special(lines, kind, perfect_clear);
        self.events.last_clear_text = describe_clear(special, lines, perfect_clear);
        if self.events.last_clear_text.is_some() {
            self.clear_banner_timer = 2000; // 2 second display
        }
        self.events.last_b2b = self.back_to_back;
        self.events.last_combo = self.combo.unwrap_or(0);
        self.events.last_special = special;

        // A short spawn delay (ARE-like grace) keeps the next piece from spawning
        // on the same frame, matching TETR.IO feel.
        self.spawn_delay = if from_hard_drop { 0 } else { 16 };
        if self.spawn_delay == 0 {
            self.spawn_next();
        }
    }

    /// Advance combo / B2B state and add to the score for a line clear (or
    /// non-clear lock).
    fn score_lines(&mut self, lines: u32, spin: SpecialKind, perfect_clear: bool) {
        let gp = &self.settings.gameplay;
        let _ = gp;

        let base_level = self.level.max(1) as u64;

        if lines == 0 {
            // No clear breaks the combo (but not B2B).
            if spin.is_tspin() {
                // T-Spin no-lines still counts as a T-Spin (and is "difficult").
                let pts = 400 * base_level;
                self.score += pts;
                if self.back_to_back {
                    self.score += pts / 2;
                }
                self.back_to_back = true;
            }
            self.combo = None;
            return;
        }

        // Base line-clear scores (Guideline-ish, level-scaled).
        let base: u64 = match (lines, spin.is_tspin()) {
            (1, true) => 800,  // T-Spin Single
            (2, true) => 1200, // T-Spin Double
            (3, true) => 1600, // T-Spin Triple
            (1, false) => 100,  // Single
            (2, false) => 300,  // Double
            (3, false) => 500,  // Triple
            (4, false) => 800, // Tetris
            _ => 0,
        };
        let base = if matches!(spin, SpecialKind::TSpinMini) && lines == 1 {
            // T-Spin mini single: 200 * level
            200
        } else {
            base
        };
        let mut pts = base * base_level;

        // Perfect clear bonus (Guideline-scaled).
        if perfect_clear {
            pts += match lines {
                1 => 800 * base_level,
                2 => 1200 * base_level,
                3 => 1800 * base_level,
                4 => 2000 * base_level,
                _ => 0,
            };
        }

        // Back-to-back bonus: T-Spin / Tetris chains.
        let difficult = spin.is_tspin() || lines == 4;
        if difficult && self.back_to_back {
            pts = pts + pts / 2; // x1.5
        }
        if difficult {
            self.back_to_back = true;
        } else if lines > 0 {
            self.back_to_back = false;
        }

        // Combo: consecutive line clears.
        let combo = self.combo.unwrap_or(0);
        if lines > 0 {
            let new_combo = (combo + 1).max(1);
            self.combo = Some(new_combo);
            // Combo bonus: 50 * combo * level.
            if new_combo > 1 {
                pts += 50 * (new_combo.saturating_sub(1)) as u64 * base_level;
            }
        } else {
            self.combo = None;
        }

        self.score += pts;
    }

    /// Compute the ghost piece (dropped position of the current piece).
    pub fn ghost(&self) -> Option<ActivePiece> {
        let piece = self.current?;
        let (dropped, _) = self.drop_distance(piece);
        Some(piece.moved(0, dropped))
    }
}

fn collides(board: &Board, piece: ActivePiece) -> bool {
    piece::collides(piece, &|x, y| board.filled(x, y))
}

/// Gravity period in milliseconds per level, following a Guideline-style curve
/// that sharpens toward 20G at high levels.
fn gravity_for_level(level: u32) -> f64 {
    // Points: level 1 ~= 1000ms/cell; near-20G by ~19-20.
    let table = [
        1000.0, // 1
        793.0,  // 2
        617.0,  // 3
        472.0,  // 4
        360.0,  // 5
        270.0,  // 6
        207.0,  // 7
        158.0,  // 8
        123.0,  // 9
        94.0,   // 10
        70.0,   // 11
        52.0,   // 12
        39.0,   // 13
        29.0,   // 14
        22.0,   // 15
        16.0,   // 16
        12.0,   // 17
        9.0,    // 18
        6.0,    // 19
    ];
    let idx = (level.saturating_sub(1)) as usize;
    if idx < table.len() {
        table[idx]
    } else {
        // 20G: a fraction-of-a-frame drop.
        1.0
    }
}

/// T-Spin / T-Spin mini detection using the 3-corner rule.
///
/// A T-Spin requires that the piece is a T, the last action was a rotation, and
/// at least 3 of the 4 diagonals around the T's center are occupied (minimax
/// variant: at least two of those filled corners must be on the side that faces
/// the direction of the last successful kick / the T's facing). A "mini" is
/// distinguished by the kicks that were used and which corners are filled.
fn detect_spin(
    board: &Board,
    piece: ActivePiece,
    last_was_rotate: bool,
    last_kick: usize,
) -> SpecialKind {
    if piece.kind != Tetromino::T || !last_was_rotate {
        return SpecialKind::None;
    }

    let (cx, cy) = t_center(piece);
    // Four diagonal corners around the T center.
    let corners = [
        (cx - 1, cy - 1),
        (cx + 1, cy - 1),
        (cx - 1, cy + 1),
        (cx + 1, cy + 1),
    ];
    let filled: [bool; 4] = [
        corner_filled(board, corners[0].0, corners[0].1),
        corner_filled(board, corners[1].0, corners[1].1),
        corner_filled(board, corners[2].0, corners[2].1),
        corner_filled(board, corners[3].0, corners[3].1),
    ];
    let total: u8 = filled.iter().map(|b| *b as u8).sum();

    if total < 3 {
        return SpecialKind::None;
    }

    // For mini detection we need the "front" pair (the two corners on the side
    // the T points toward). If at least one of the front corners is filled it's
    // a full T-Spin; otherwise it's a mini (unless a "TST"-style kick — the
    // fifth SRS kick with offset (±1, ±2) — was used, which promotes a mini to
    // a full T-Spin).
    let (front_a, front_b) = front_corners(piece.rot);
    let one_in_front = filled[front_a] || filled[front_b];
    let promoted_by_kick = last_kick == 4;

    if one_in_front || promoted_by_kick {
        // Line count is resolved by the caller in `combine_special`.
        SpecialKind::TSpin
    } else {
        SpecialKind::TSpinMini
    }
}

/// The (x, y) board coordinate of the T piece's rotational center.
fn t_center(piece: ActivePiece) -> (i32, i32) {
    // In the canonical 3x3 box the center cell is (1, 1).
    (piece.x + 1, piece.y + 1)
}

/// Returns the two indices into the `corners` array that correspond to the
/// "front" pair (the side the T points toward), by rotation.
fn front_corners(rot: Rotation) -> (usize, usize) {
    match rot {
        Rotation::Spawn => (2, 3),          // points down: front is bottom pair
        Rotation::Clockwise => (1, 3),     // points left: front is left pair
        Rotation::OneEighty => (0, 1),       // points up: front is top pair
        Rotation::CounterClockwise => (0, 2), // points right: front is right pair
    }
}

fn corner_filled(board: &Board, x: i32, y: i32) -> bool {
    // Treat the walls and floor as filled for the corner rule (Standard).
    if x < 0 || x >= FIELD_COLS as i32 || y >= FIELD_ROWS as i32 {
        return true;
    }
    if y < 0 {
        return false;
    }
    board.filled(x, y)
}

/// Combine spin kind + lines + perfect-clear flag into a final `SpecialKind`.
fn combine_special(lines: u32, spin: SpecialKind, perfect_clear: bool) -> SpecialKind {
    if perfect_clear {
        return SpecialKind::PerfectClear;
    }
    match (lines, spin) {
        (0, SpecialKind::TSpin) => SpecialKind::TSpin,
        (1, SpecialKind::TSpin) => SpecialKind::TSpinSingle,
        (2, SpecialKind::TSpin) => SpecialKind::TSpinDouble,
        (3, SpecialKind::TSpin) => SpecialKind::TSpinTriple,
        (0, SpecialKind::TSpinMini) => SpecialKind::TSpinMini,
        (1, SpecialKind::TSpinMini) => SpecialKind::TSpinMini, // mini single
        (_, SpecialKind::TSpinMini) => SpecialKind::TSpin,     // mini double+ promotes
        (4, _) => SpecialKind::Tetris,
        (3, _) => SpecialKind::Triple,
        (2, _) => SpecialKind::Double,
        (1, _) => SpecialKind::Single,
        _ => SpecialKind::None,
    }
}

fn describe_clear(special: SpecialKind, lines: u32, perfect: bool) -> Option<String> {
    if perfect && lines > 0 {
        return Some(match lines {
            4 => "PERFECT CLEAR (TETRIS)!".into(),
            _ => "PERFECT CLEAR!".into(),
        });
    }
    let s = match special {
        SpecialKind::TSpinMini => "T-SPIN MINI",
        SpecialKind::TSpin => "T-SPIN",
        SpecialKind::TSpinSingle => "T-SPIN SINGLE",
        SpecialKind::TSpinDouble => "T-SPIN DOUBLE",
        SpecialKind::TSpinTriple => "T-SPIN TRIPLE",
        SpecialKind::Tetris => "TETRIS",
        SpecialKind::Triple => "TRIPLE",
        SpecialKind::Double => "DOUBLE",
        SpecialKind::Single => "SINGLE",
        SpecialKind::PerfectClear => "PERFECT CLEAR!",
        SpecialKind::None => return None,
    };
    Some(s.into())
}