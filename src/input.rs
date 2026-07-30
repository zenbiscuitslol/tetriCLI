//! Input handling: translating `crossterm` key events into stable string labels
//! used both for storing rebindable controls and for matching pressed keys.
//!
//! Stored bindings are opaque strings produced by [`event_label`]. The labels
//! are stable, human-readable, and round-trip losslessly through serde.
//!
//! Menu navigation (arrow keys, Enter, Escape) intentionally uses fixed keys
//! independent of the rebindable gameplay controls.

use crossterm::event::{KeyCode, KeyEvent};

/// A stable, human-readable label for a key event. Used as the serialized form
/// of a control binding.
///
/// Examples: `"Left"`, `"Down"`, `"Up"`, `"Space"`, `"Esc"`, `"Enter"`, `"Z"`,
/// `"1"`, `"F2"`.
pub fn event_label(ev: &KeyEvent) -> String {
    code_label(ev.code)
}

/// The label for a [`KeyCode`], ignoring modifiers. Modifier-only keys are
/// reported with their own names so binding to them still works.
pub fn code_label(code: KeyCode) -> String {
    match code {
        KeyCode::Left => "Left".into(),
        KeyCode::Right => "Right".into(),
        KeyCode::Up => "Up".into(),
        KeyCode::Down => "Down".into(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Esc => "Esc".into(),
        KeyCode::Backspace => "Backspace".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::Delete => "Delete".into(),
        KeyCode::Insert => "Insert".into(),
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        KeyCode::PageUp => "PageUp".into(),
        KeyCode::PageDown => "PageDown".into(),
        KeyCode::Char(c) => {
            if c == ' ' {
                "Space".into()
            } else if c.is_alphanumeric() {
                c.to_ascii_uppercase().to_string()
            } else {
                format!("Char({})", c)
            }
        }
        KeyCode::F(n) => format!("F{n}"),
        other => format!("{other:?}"),
    }
}

/// True if a pressed key event matches the stored binding string.
pub fn matches_binding(ev: &KeyEvent, binding: &str) -> bool {
    event_label(ev).eq_ignore_ascii_case(binding)
}

/// Pretty-print a binding label for the settings UI (e.g. `"Space"` -> `"Space"`,
/// `"Char(,)"` -> `","`).
pub fn pretty_label(label: &str) -> String {
    if let Some(rest) = label.strip_prefix("Char(") {
        if let Some(inner) = rest.strip_suffix(')') {
            return inner.to_string();
        }
    }
    label.to_string()
}

/// A high-level gameplay action the player can perform.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    MoveLeft,
    MoveRight,
    SoftDrop,
    HardDrop,
    RotateCw,
    RotateCcw,
    Hold,
    Pause,
    Restart,
}

impl Action {
    /// Iterate the gameplay actions in binding-order, paired with the field
    /// name used in [`crate::settings::Controls`].
    pub const BINDINGS: [(Self, &'static str); 9] = [
        (Self::MoveLeft, "move_left"),
        (Self::MoveRight, "move_right"),
        (Self::SoftDrop, "soft_drop"),
        (Self::HardDrop, "hard_drop"),
        (Self::RotateCw, "rotate_cw"),
        (Self::RotateCcw, "rotate_ccw"),
        (Self::Hold, "hold"),
        (Self::Pause, "pause"),
        (Self::Restart, "restart"),
    ];

    /// Human-readable name for this action.
    #[allow(dead_code)]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MoveLeft => "Move Left",
            Self::MoveRight => "Move Right",
            Self::SoftDrop => "Soft Drop",
            Self::HardDrop => "Hard Drop",
            Self::RotateCw => "Rotate CW",
            Self::RotateCcw => "Rotate CCW",
            Self::Hold => "Hold Piece",
            Self::Pause => "Pause",
            Self::Restart => "Restart",
        }
    }
}

/// Look up the binding string for `action` inside `controls`, returning it as
/// a borrowed slice where possible via a match.
#[allow(dead_code)]
pub fn binding_for<'a>(action: Action, controls: &'a crate::settings::Controls) -> &'a str {
    match action {
        Action::MoveLeft => &controls.move_left,
        Action::MoveRight => &controls.move_right,
        Action::SoftDrop => &controls.soft_drop,
        Action::HardDrop => &controls.hard_drop,
        Action::RotateCw => &controls.rotate_cw,
        Action::RotateCcw => &controls.rotate_ccw,
        Action::Hold => &controls.hold,
        Action::Pause => &controls.pause,
        Action::Restart => &controls.restart,
    }
}

/// Set the binding string for `action` in `controls`.
pub fn set_binding(action: Action, controls: &mut crate::settings::Controls, value: String) {
    match action {
        Action::MoveLeft => controls.move_left = value,
        Action::MoveRight => controls.move_right = value,
        Action::SoftDrop => controls.soft_drop = value,
        Action::HardDrop => controls.hard_drop = value,
        Action::RotateCw => controls.rotate_cw = value,
        Action::RotateCcw => controls.rotate_ccw = value,
        Action::Hold => controls.hold = value,
        Action::Pause => controls.pause = value,
        Action::Restart => controls.restart = value,
    }
}