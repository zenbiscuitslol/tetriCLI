//! Menu navigation state for each of the game's menus, plus the control
//! rebinding flow that captures the next key as a new binding.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::ListState;

use crate::input::{self, Action};
use crate::settings::Controls;
use crate::settings::Settings;

/// Number of entries in each menu. Used to clamp selection indices.
#[derive(Clone, Copy, Debug)]
pub enum MenuKind {
    Main,
    ModeSelect,
    Settings,
    Controls,
    Gameplay,
    Pause,
    GameOver,
}

impl MenuKind {
    pub fn len(self) -> usize {
        match self {
            Self::Main => 4,
            Self::ModeSelect => 3,
            Self::Settings => 3,
            Self::Controls => 9,
            Self::Gameplay => 5,
            Self::Pause => 4,
            Self::GameOver => 3,
        }
    }
}

/// Interactive menu state. Holds the selected index, a ratatui `ListState`
/// (for future direct list widgets), the kind of menu currently shown, and
/// an optional "pending binding" slot for control rebinding. The `controls`
/// snapshot is edited live during rebinding and committed back to settings
/// by the caller save loop.
#[derive(Clone, Debug)]
pub struct Menu {
    kind: MenuKind,
    selected: usize,
    list_state: ListState,
    /// Selected binding slot in the Controls menu: 0 = primary, 1 = secondary.
    slot: usize,
    /// If `Some((i, slot))`, the next key captured becomes the new binding in
    /// `slot` for the i-th control entry (see `Settings::CONTROL_FIELDS`).
    pending_binding: Option<(usize, usize)>,
    /// A live clone of the `[primary, secondary]` controls being edited,
    /// committed to `Settings` whenever they change.
    controls: [Controls; 2],
}

impl Menu {
    pub fn new(kind: MenuKind, settings: &Settings) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            kind,
            selected: 0,
            list_state,
            slot: 0,
            pending_binding: None,
            controls: controls_of(settings),
        }
    }

    #[allow(dead_code)]
    pub fn kind(&self) -> MenuKind {
        self.kind
    }

    pub fn set_kind(&mut self, kind: MenuKind, settings: &Settings) {
        self.kind = kind;
        self.selected = 0;
        self.slot = 0;
        self.pending_binding = None;
        self.controls = controls_of(settings);
        self.list_state.select(Some(0));
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    #[allow(dead_code)]
    pub fn list_state(&self) -> &ListState {
        &self.list_state
    }

    pub fn slot(&self) -> usize {
        self.slot
    }

    pub fn pending_binding(&self) -> Option<(usize, usize)> {
        self.pending_binding
    }

    pub fn active_controls(&self) -> &[Controls; 2] {
        &self.controls
    }

    /// Select the primary (0) or secondary (1) binding column.
    pub fn select_slot(&mut self, slot: usize) {
        self.slot = slot.min(1);
    }

    /// Navigate up by one (wraps to bottom).
    pub fn up(&mut self) {
        let len = self.kind.len();
        self.selected = if self.selected == 0 { len - 1 } else { self.selected - 1 };
        self.list_state.select(Some(self.selected));
    }

    /// Navigate down by one (wraps to top).
    pub fn down(&mut self) {
        let len = self.kind.len();
        self.selected = (self.selected + 1) % len;
        self.list_state.select(Some(self.selected));
    }

    /// Begin rebinding the currently-selected control. Only meaningful when the
    /// menu kind is `Controls`. Returns `true` if rebinding started.
    pub fn start_rebind(&mut self) -> bool {
        if !matches!(self.kind, MenuKind::Controls) {
            return false;
        }
        self.pending_binding = Some((self.selected, self.slot));
        true
    }

    /// Unbind the selected control's secondary key. Primary keys can only be
    /// rebound, never cleared, so every action stays reachable. Returns the
    /// updated controls if anything changed.
    pub fn clear_secondary(&mut self) -> Option<[Controls; 2]> {
        if !matches!(self.kind, MenuKind::Controls) || self.slot != 1 {
            return None;
        }
        let action = Action::BINDINGS[self.selected].0;
        if input::binding_for(action, &self.controls[1]).is_empty() {
            return None;
        }
        input::set_binding(action, &mut self.controls[1], String::new());
        Some(self.controls.clone())
    }

    /// Cancel any in-progress rebinding.
    #[allow(dead_code)]
    pub fn cancel_rebind(&mut self) {
        self.pending_binding = None;
    }

    /// If a rebinding is in progress, capture `ev` as the new binding for the
    /// selected control. Returns `true` if a binding was captured (so the
    /// caller can persist settings). `Esc` cancels the rebind instead.
    pub fn capture_binding(&mut self, ev: &KeyEvent) -> BindingCapture {
        let Some((idx, slot)) = self.pending_binding else {
            return BindingCapture::None;
        };
        if ev.code == KeyCode::Esc {
            self.pending_binding = None;
            return BindingCapture::Cancelled;
        }
        // Ignore pure modifier presses that don't produce a usable label.
        let label = input::event_label(ev);
        if label.starts_with("Char(") {
            // For printable chars keep the cleaned label.
        }
        let action = Action::BINDINGS[idx].0;
        input::set_binding(action, &mut self.controls[slot], label);
        let captured = self.controls.clone();
        self.pending_binding = None;
        BindingCapture::Captured(captured)
    }
}

/// Result of attempting to capture a key for rebinding.
pub enum BindingCapture {
    /// No rebinding was in progress.
    None,
    /// The user pressed Escape and cancelled the rebinding.
    Cancelled,
    /// A new binding was captured; contains the updated `[primary, secondary]`
    /// controls so the caller can persist them.
    Captured([Controls; 2]),
}

fn controls_of(settings: &Settings) -> [Controls; 2] {
    [settings.controls.clone(), settings.controls_secondary.clone()]
}