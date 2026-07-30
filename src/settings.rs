//! Strongly-typed, serde-compatible settings model and defaults.

use serde::{Deserialize, Serialize};

/// Persistent high-level game configuration. Loaded from / saved to
/// `config/settings.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub controls: Controls,
    pub gameplay: Gameplay,
    pub stats: Stats,
}

/// Rebindable controls. Each binding is the stringified name of a
/// [`crate::input::Key`](super::input::Key) value.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Controls {
    pub move_left: String,
    pub move_right: String,
    pub soft_drop: String,
    pub hard_drop: String,
    pub rotate_cw: String,
    pub rotate_ccw: String,
    pub hold: String,
    pub pause: String,
    pub restart: String,
}

/// Tunable gameplay parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gameplay {
    pub starting_level: u32,
    pub das: u64,
    pub arr: u64,
    pub soft_drop_factor: u32,
    pub lock_delay_ms: u64,
    pub max_lock_resets: u32,
    pub show_ghost: bool,
    pub show_fps: bool,
}

/// Persisted lifetime statistics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stats {
    pub games_played: u64,
    pub total_lines: u64,
    pub best_score: u64,
    pub best_level: u32,
    pub total_playtime_secs: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            controls: Controls {
                move_left: "Left".into(),
                move_right: "Right".into(),
                soft_drop: "Down".into(),
                hard_drop: "Space".into(),
                rotate_cw: "Up".into(),
                rotate_ccw: "Z".into(),
                hold: "C".into(),
                pause: "Esc".into(),
                restart: "R".into(),
            },
            gameplay: Gameplay {
                starting_level: 1,
                das: 133,
                arr: 20,
                soft_drop_factor: 20,
                lock_delay_ms: 500,
                max_lock_resets: 15,
                show_ghost: true,
                show_fps: false,
            },
            stats: Stats {
                games_played: 0,
                total_lines: 0,
                best_score: 0,
                best_level: 0,
                total_playtime_secs: 0.0,
            },
        }
    }
}

impl Settings {
    /// Returns the list of `(label, field-name)` tuples used to iterate controls
    /// in the settings UI in display order. The "field" is the serialized key.
    pub const CONTROL_FIELDS: [(&'static str, &'static str); 9] = [
        ("Move Left", "move_left"),
        ("Move Right", "move_right"),
        ("Soft Drop", "soft_drop"),
        ("Hard Drop", "hard_drop"),
        ("Rotate CW", "rotate_cw"),
        ("Rotate CCW", "rotate_ccw"),
        ("Hold Piece", "hold"),
        ("Pause", "pause"),
        ("Restart", "restart"),
    ];
}