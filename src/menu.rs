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
    Settings,
    Controls,
    Pause,
    GameOver,
}

impl MenuKind {
    pub fn len(self) -> usize {
        match self {
            Self::Main => 4,
            Self::Settings => 3,
            Self::Controls => 9,
            Self::Pause => 4,
            Self::GameOver => 3,
        }
    }
}

/// Interactive menu state. Holds the selected index, a ratatui `ListState`
/// (for future direct list widgets), the kind of menu currently shown, and
/// an optional "pending binding" index for control rebinding. The `controls`
/// snapshot is edited live during rebinding and committed back to settings
/// by the caller save loop.
#[derive(Clone, Debug)]
pub struct Menu {
    kind: MenuKind,
    selected: usize,
    list_state: ListState,
    /// If `Some(i)`, the next key captured becomes the new binding for the i-th
    /// control entry (see `Settings::CONTROL_FIELDS`).
    pending_binding: Option<usize>,
    /// A live clone of the controls being edited, committed to `Settings`
    /// whenever they change.
    controls: Controls,
}

impl Menu {
    pub fn new(kind: MenuKind, settings: &Settings) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            kind,
            selected: 0,
            list_state,
            pending_binding: None,
            controls: settings.controls.clone(),
        }
    }

    pub fn kind(&self) -> MenuKind {
        self.kind
    }

    pub fn set_kind(&mut self, kind: MenuKind, settings: &Settings) {
        self.kind = kind;
        self.selected = 0;
        self.pending_binding = None;
        self.controls = settings.controls.clone();
        self.list_state.select(Some(0));
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn list_state(&self) -> &ListState {
        &self.list_state
    }

    pub fn pending_binding(&self) -> Option<usize> {
        self.pending_binding
    }

    pub fn active_controls(&self) -> &Controls {
        &self.controls
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
        self.pending_binding = Some(self.selected);
        true
    }

    /// Cancel any in-progress rebinding.
    pub fn cancel_rebind(&mut self) {
        self.pending_binding = None;
    }

    /// If a rebinding is in progress, capture `ev` as the new binding for the
    /// selected control. Returns `true` if a binding was captured (so the
    /// caller can persist settings). `Esc` cancels the rebind instead.
    pub fn capture_binding(&mut self, ev: &KeyEvent) -> BindingCapture {
        let Some(idx) = self.pending_binding else {
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
        input::set_binding(action, &mut self.controls, label);
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
    /// A new binding was captured; contains the updated controls so the caller
    /// can persist them.
    Captured(Controls),
}