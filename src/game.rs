//! Core Guideline-style Tetris gameplay: 7-bag randomizer, SRS rotation with
//! wall kicks, DAS/ARR, soft and hard drop, lock delay with movement resets,
//! hold mechanic, ghost piece, gravity scaling by level, and a scoring model
//! with combos, back-to-back tracking, and T-Spin / T-Spin mini / perfect-clear
//! detection closely following TETR.IO's modern Guideline feel.

use std::cmp;
use std::time::{Duration, Instant};

use crate::board::{Board, LockOutcome};
use crate::piece::{self, ActivePiece, Rotation, Tetromino, FIELD_COLS, FIELD_ROWS};
use crate::settings::Settings;

/// Durations used by the game loop, all in milliseconds.
pub const DT_MS: u64 = 1000 / 60;

/// Lines a 40 Lines (sprint) run must clear to finish.
pub const SPRINT_LINES: u32 = 40;

/// Length of a Blitz run.
pub const BLITZ_DURATION: Duration = Duration::from_secs(120);

/// Time-left marks at which Blitz flashes a warning, with its text.
const BLITZ_WARNINGS: [(Duration, &str); 3] = [
    (Duration::from_secs(60), "1 MINUTE LEFT"),
    (Duration::from_secs(30), "30 SECONDS LEFT"),
    (Duration::from_secs(10), "10 SECONDS LEFT"),
];

/// How long each Blitz warning stays on screen.
const BLITZ_WARNING_SHOW: Duration = Duration::from_secs(2);

/// Fast Movement: a held move repeats every this many ms, from the first
/// repeat on. Long enough that a normal tap still moves a single cell.
const FAST_SHIFT_MS: u64 = 130;
/// Without Fast Movement: delay before a held move starts repeating...
const SHIFT_DELAY_MS: u64 = 200;
/// ...and the interval between repeats after that.
const SHIFT_REPEAT_MS: u64 = 50;

/// Which ruleset a game is played under.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameMode {
    /// Endless play: levels and gravity rise every 10 lines; scored by points.
    #[default]
    Classic,
    /// Clear [`SPRINT_LINES`] lines as fast as possible at fixed level-1
    /// gravity; scored by time.
    Sprint,
    /// Score as many points as possible in [`BLITZ_DURATION`], starting from
    /// level 1 with levels rising every 10 lines.
    Blitz,
}

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
    pub mode: GameMode,

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
    pub last_lock_was_tspin: bool,

    // Run clock. Paused time is excluded and the clock freezes at game end.
    start_time: Instant,
    paused_at: Option<Instant>,
    paused_total: Duration,
    final_time: Option<Duration>,

    // Timing, DAS/ARR.
    gravity_ms: f64,
    gravity_acc: f64,
    soft_dropping: bool,
    /// Remaining ms of soft drop when the terminal sends no key-release events;
    /// key-repeat presses keep refreshing it. `None` means "until released".
    soft_drop_ttl: Option<u64>,
    /// Direction of the held left/right key, auto-repeated by `tick`. Only set
    /// when the terminal reports key releases.
    held_shift: Option<i32>,
    shift_timer: u64,
    /// True once the held move is past its initial delay.
    shift_repeating: bool,

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
    /// True when a sprint reached its line goal or a blitz ran out its clock
    /// (as opposed to topping out).
    pub completed: bool,

    // Stats of the most recent lock (used by T-Spin detection which requires
    // "the last action was a rotation").
    last_action_was_rotate: bool,
    last_rotation_kick: usize,
}

impl Game {
    pub fn new(mut settings: Settings, mode: GameMode) -> Self {
        // Sprint and Blitz always start at level 1 so results are comparable.
        let level = match mode {
            GameMode::Classic => settings.gameplay.starting_level.max(1),
            GameMode::Sprint | GameMode::Blitz => 1,
        };
        // Pin the starting level back into the setting so persisted stats replay
        // the same starting speed.
        settings.gameplay.starting_level = level;
        let mut game = Self {
            board: Board::new(),
            settings,
            mode,
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
            last_lock_was_tspin: false,
            start_time: Instant::now(),
            paused_at: None,
            paused_total: Duration::ZERO,
            final_time: None,
            gravity_ms: gravity_for_level(level),
            gravity_acc: 0.0,
            soft_dropping: false,
            soft_drop_ttl: None,
            held_shift: None,
            shift_timer: 0,
            shift_repeating: false,
            lock_timer: None,
            lock_resets: 0,
            on_ground: false,
            spawn_delay: 0,
            events: EventLog::default(),
            clear_banner_timer: 0,
            game_over: false,
            completed: false,
            last_action_was_rotate: false,
            last_rotation_kick: 0,
        };
        game.refill_bag();
        game.refill_bag();
        game.refill_queue();
        game.spawn_next();
        game
    }

    /// Reset to a fresh game keeping the same settings and mode (used by
    /// "Restart").
    pub fn reset(&mut self) {
        let settings = self.settings.clone();
        *self = Self::new(settings, self.mode);
    }

    /// Time spent playing, excluding pauses. Frozen once the game ends.
    pub fn elapsed(&self) -> Duration {
        if let Some(t) = self.final_time {
            return t;
        }
        let now = self.paused_at.unwrap_or_else(Instant::now);
        now.duration_since(self.start_time).saturating_sub(self.paused_total)
    }

    /// Stop or restart the run clock when the game is paused or resumed.
    pub fn set_paused(&mut self, paused: bool) {
        // The key may be released while the pause menu is open; don't let the
        // piece keep sliding on resume.
        if paused {
            self.held_shift = None;
        }
        match (paused, self.paused_at) {
            (true, None) => self.paused_at = Some(Instant::now()),
            (false, Some(at)) => {
                self.paused_total += at.elapsed();
                self.paused_at = None;
            }
            _ => {}
        }
    }

    /// Time left on the Blitz clock.
    pub fn blitz_remaining(&self) -> Duration {
        BLITZ_DURATION.saturating_sub(self.elapsed())
    }

    /// The time-left warning to show right now, if the Blitz clock passed one
    /// of the [`BLITZ_WARNINGS`] marks within the last [`BLITZ_WARNING_SHOW`].
    pub fn blitz_warning(&self) -> Option<&'static str> {
        if self.mode != GameMode::Blitz || self.game_over {
            return None;
        }
        let left = self.blitz_remaining();
        BLITZ_WARNINGS
            .iter()
            .find(|(at, _)| left <= *at && left + BLITZ_WARNING_SHOW > *at)
            .map(|(_, text)| *text)
    }

    /// End the game, freezing the clock.
    fn end(&mut self) {
        let t = self.elapsed();
        // The frame loop can overshoot the Blitz limit by a few ms; clamp it.
        self.final_time = Some(match self.mode {
            GameMode::Blitz => t.min(BLITZ_DURATION),
            _ => t,
        });
        self.current = None;
        self.game_over = true;
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
            self.end();
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

    /// A left/right key went down: move once now and keep repeating in `tick`
    /// until [`Game::release_shift`].
    pub fn press_shift(&mut self, dx: i32) {
        self.shift(dx);
        self.held_shift = Some(dx);
        self.shift_timer = 0;
        self.shift_repeating = false;
    }

    /// A left/right key was released.
    pub fn release_shift(&mut self, dx: i32) {
        if self.held_shift == Some(dx) {
            self.held_shift = None;
        }
    }

    /// Repeat the held move: after the initial delay, then every interval.
    fn auto_shift(&mut self) {
        let Some(dx) = self.held_shift else { return };
        let (delay, interval) = if self.settings.gameplay.fast_movement {
            (FAST_SHIFT_MS, FAST_SHIFT_MS)
        } else {
            (SHIFT_DELAY_MS, SHIFT_REPEAT_MS)
        };
        self.shift_timer += DT_MS;
        let due = if self.shift_repeating { interval } else { delay };
        if self.shift_timer >= due {
            self.shift_timer -= due;
            self.shift_repeating = true;
            self.shift(dx);
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
        self.set_soft_dropping(false);
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

    /// Start or stop soft drop. While active it lasts until explicitly stopped.
    pub fn set_soft_dropping(&mut self, v: bool) {
        if v {
            self.begin_soft_drop();
        } else {
            self.soft_dropping = false;
        }
        self.soft_drop_ttl = None;
    }

    /// Soft drop that expires on its own unless refreshed (for terminals that
    /// never report key releases).
    pub fn soft_drop_pulse(&mut self, ttl_ms: u64) {
        self.begin_soft_drop();
        self.soft_drop_ttl = Some(ttl_ms);
    }

    /// When a soft drop starts, move one row right away (so a tap is exactly
    /// one row) and restart the gravity clock. Without the reset, time banked
    /// toward the next normal-gravity step would be spent at the much faster
    /// soft-drop rate, dropping the piece many rows in a single frame.
    fn begin_soft_drop(&mut self) {
        if self.soft_dropping {
            return;
        }
        self.soft_dropping = true;
        self.gravity_acc = 0.0;
        self.soft_drop_step();
    }

    /// Advance the game by one frame (`DT_MS` milliseconds).
    pub fn tick(&mut self) {
        if self.game_over {
            return;
        }

        if self.mode == GameMode::Blitz && self.elapsed() >= BLITZ_DURATION {
            self.completed = true;
            self.end();
            return;
        }

        // Expire the transient clear banner after ~2 seconds.
        if self.clear_banner_timer > 0 {
            self.clear_banner_timer = self.clear_banner_timer.saturating_sub(DT_MS);
            if self.clear_banner_timer == 0 {
                self.events.last_clear_text = None;
            }
        }

        // Keeps charging between pieces, so a held key carries over.
        self.auto_shift();

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

        if let Some(ttl) = self.soft_drop_ttl {
            let left = ttl.saturating_sub(DT_MS);
            self.soft_drop_ttl = Some(left);
            if left == 0 {
                self.soft_dropping = false;
                self.soft_drop_ttl = None;
            }
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
        // may have changed due to gravity this tick).
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
                }
            }
        } else {
            self.lock_timer = None;
        }
    }

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
            self.end();
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
        }
        if lines > 0 && self.mode != GameMode::Sprint {
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

        self.gravity_acc = 0.0;
        self.lock_timer = None;
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

        if self.mode == GameMode::Sprint && self.total_lines >= SPRINT_LINES {
            self.completed = true;
            self.end();
            return;
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Fill the bottom row everywhere the current piece won't land, so the
    /// next hard drop clears exactly one line.
    fn prime_single_clear(game: &mut Game) {
        let ghost = game.ghost().unwrap().cells();
        let bottom = FIELD_ROWS as i32 - 1;
        for x in 0..FIELD_COLS as i32 {
            if !ghost.contains(&(x, bottom)) {
                game.board.set(x, bottom, Tetromino::I);
            }
        }
    }

    #[test]
    fn sprint_finishes_at_line_goal_and_freezes_clock() {
        let mut game = Game::new(Settings::default(), GameMode::Sprint);
        game.total_lines = SPRINT_LINES - 1;
        prime_single_clear(&mut game);
        game.hard_drop();
        assert!(game.game_over && game.completed);
        let t = game.elapsed();
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(game.elapsed(), t);
    }

    #[test]
    fn sprint_ignores_starting_level() {
        let mut settings = Settings::default();
        settings.gameplay.starting_level = 15;
        let game = Game::new(settings, GameMode::Sprint);
        assert_eq!(game.level, 1);
    }

    #[test]
    fn classic_continues_past_forty_lines() {
        let mut game = Game::new(Settings::default(), GameMode::Classic);
        game.total_lines = SPRINT_LINES - 1;
        prime_single_clear(&mut game);
        game.hard_drop();
        assert!(!game.game_over && !game.completed);
        assert_eq!(game.level, 5);
    }

    #[test]
    fn blitz_ends_when_clock_runs_out() {
        let mut game = Game::new(Settings::default(), GameMode::Blitz);
        game.tick();
        assert!(!game.game_over);
        game.start_time -= BLITZ_DURATION;
        game.tick();
        assert!(game.game_over && game.completed);
        assert_eq!(game.elapsed(), BLITZ_DURATION);
        assert_eq!(game.blitz_remaining(), Duration::ZERO);
    }

    #[test]
    fn blitz_warns_at_time_marks() {
        let mut game = Game::new(Settings::default(), GameMode::Blitz);
        assert_eq!(game.blitz_warning(), None);
        game.start_time -= Duration::from_secs(60);
        assert_eq!(game.blitz_warning(), Some("1 MINUTE LEFT"));
        game.start_time -= Duration::from_secs(3);
        assert_eq!(game.blitz_warning(), None);
        game.start_time -= Duration::from_secs(27);
        assert_eq!(game.blitz_warning(), Some("30 SECONDS LEFT"));
        game.start_time -= Duration::from_secs(20);
        assert_eq!(game.blitz_warning(), Some("10 SECONDS LEFT"));

        let mut classic = Game::new(Settings::default(), GameMode::Classic);
        classic.start_time -= Duration::from_secs(60);
        assert_eq!(classic.blitz_warning(), None);
    }

    #[test]
    fn blitz_starts_at_level_one_and_levels_up() {
        let mut settings = Settings::default();
        settings.gameplay.starting_level = 15;
        let mut game = Game::new(settings, GameMode::Blitz);
        assert_eq!(game.level, 1);
        game.total_lines = 9;
        prime_single_clear(&mut game);
        game.hard_drop();
        assert_eq!(game.level, 2);
        assert!(game.score > 0 && !game.game_over);
    }

    #[test]
    fn fast_movement_repeats_at_a_steady_rate() {
        let mut game = Game::new(Settings::default(), GameMode::Classic);
        let x0 = game.current.unwrap().x;
        game.press_shift(1);
        assert_eq!(game.current.unwrap().x, x0 + 1);
        for _ in 0..FAST_SHIFT_MS / DT_MS {
            game.tick();
        }
        assert_eq!(game.current.unwrap().x, x0 + 1);
        game.tick();
        assert_eq!(game.current.unwrap().x, x0 + 2);
        game.release_shift(1);
        for _ in 0..20 {
            game.tick();
        }
        assert_eq!(game.current.unwrap().x, x0 + 2);
    }

    #[test]
    fn normal_movement_waits_then_repeats_faster() {
        let mut settings = Settings::default();
        settings.gameplay.fast_movement = false;
        let mut game = Game::new(settings, GameMode::Classic);
        let x0 = game.current.unwrap().x;
        game.press_shift(1);
        for _ in 0..SHIFT_DELAY_MS / DT_MS {
            game.tick();
        }
        assert_eq!(game.current.unwrap().x, x0 + 1);
        game.tick();
        assert_eq!(game.current.unwrap().x, x0 + 2);
        for _ in 0..SHIFT_REPEAT_MS.div_ceil(DT_MS) {
            game.tick();
        }
        assert_eq!(game.current.unwrap().x, x0 + 3);
    }

    #[test]
    fn pausing_stops_held_movement() {
        let mut game = Game::new(Settings::default(), GameMode::Classic);
        let x0 = game.current.unwrap().x;
        game.press_shift(-1);
        game.set_paused(true);
        game.set_paused(false);
        for _ in 0..20 {
            game.tick();
        }
        assert_eq!(game.current.unwrap().x, x0 - 1);
    }

    #[test]
    fn paused_time_is_not_counted() {
        let mut game = Game::new(Settings::default(), GameMode::Sprint);
        game.set_paused(true);
        let t = game.elapsed();
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(game.elapsed(), t);
        game.set_paused(false);
        assert!(game.elapsed() < t + Duration::from_millis(5));
    }

    #[test]
    fn soft_drop_tap_moves_one_row_despite_banked_gravity() {
        let mut game = Game::new(Settings::default(), GameMode::Classic);
        let y0 = game.current.unwrap().y;
        // Most of a level-1 gravity period has elapsed when the key goes down.
        game.gravity_acc = 900.0;
        game.set_soft_dropping(true);
        game.tick();
        game.set_soft_dropping(false);
        assert_eq!(game.current.unwrap().y - y0, 1);
    }
}
