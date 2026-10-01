//! The application state machine, game-loop driver, and input dispatch.
//!
//! [`App`] owns the settings, the live [`Game`], the current menu state, and
//! the timer logic needed to step the game at 60 FPS without busy waiting. It
//! is the glue between `input`, `game`, `menu`, and `renderer`.

use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::game::Game;
use crate::input::Action;
use crate::menu::{BindingCapture, Menu, MenuKind};
use crate::save;
use crate::settings::Settings;

/// Top-level application screen / mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Splash,
    MainMenu,
    Settings,
    Controls,
    GameplaySettings,
    Gameplay,
    Paused,
    GameOver,
    Stats,
}

/// The application. Run via [`App::run`].
pub struct App {
    screen: Screen,
    prev_screen: Screen,
    screen_stack: Vec<Screen>,
    settings: Settings,
    menu: Menu,
    game: Option<Game>,
    fps_avg: f64,
    fps_samples: f64,
    last_fps_update: Instant,
    /// Whether the terminal reports key-release events (kitty protocol).
    has_key_release: bool,
}

const TICK_MS: u64 = crate::game::DT_MS;
const TICK_DURATION: Duration = Duration::from_millis(TICK_MS);
/// Soft-drop lifetime per key event when the terminal reports no releases.
/// Longer than a typical key-repeat interval, shorter than a noticeable lag.
const SOFT_DROP_PULSE_MS: u64 = 120;

impl App {
    /// Construct using on-disk settings (creating defaults if absent).
    pub fn from_settings() -> Self {
        let settings = save::load_or_create();
        let menu = Menu::new(MenuKind::Main, &settings);
        Self {
            screen: Screen::Splash,
            prev_screen: Screen::Splash,
            screen_stack: Vec::new(),
            settings,
            game: None,
            menu,
            fps_avg: 0.0,
            fps_samples: 0.0,
            last_fps_update: Instant::now(),
            has_key_release: false,
        }
    }

    #[allow(dead_code)]
    pub fn screen(&self) -> Screen {
        self.screen
    }

    #[allow(dead_code)]
    pub fn menu(&self) -> &Menu {
        &self.menu
    }

    #[allow(dead_code)]
    pub fn game(&self) -> Option<&Game> {
        self.game.as_ref()
    }

    #[allow(dead_code)]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn fps(&self) -> Option<f64> {
        if self.settings.gameplay.show_fps && self.screen == Screen::Gameplay {
            if self.fps_avg > 0.0 {
                Some(self.fps_avg)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Run the event loop. Returns when the user quits. Any terminal setup
    /// errors are returned to `main` for display.
    pub fn run(&mut self, terminal: &mut ratatui::Terminal<impl ratatui::backend::Backend>) -> std::io::Result<()> {
        self.has_key_release = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
        self.last_fps_update = Instant::now();
        let mut next_tick = Instant::now() + TICK_DURATION;
        loop {
            // Sleep until the next tick or an input event, whichever is first,
            // then drain every queued event before simulating and drawing.
            let mut timeout = next_tick.saturating_duration_since(Instant::now());
            while event::poll(timeout)? {
                if self.handle_event(event::read()?) {
                    return Ok(());
                }
                timeout = Duration::ZERO;
            }

            // Fixed-timestep simulation: catch up on missed ticks (bounded so a
            // long stall can't trigger a spiral of death).
            let now = Instant::now();
            let mut steps = 0;
            while now >= next_tick && steps < 5 {
                self.tick();
                next_tick += TICK_DURATION;
                steps += 1;
            }
            if now >= next_tick {
                next_tick = now + TICK_DURATION;
            }

            self.render(terminal)?;
        }
    }

    /// Dispatch one terminal event. Returns `true` if the app should quit.
    fn handle_event(&mut self, ev: Event) -> bool {
        match ev {
            Event::Key(key) => match key.kind {
                KeyEventKind::Press => return self.handle_key(key),
                // Auto-repeat keeps soft drop alive and re-fires movement keys.
                KeyEventKind::Repeat => {
                    if self.screen == Screen::Gameplay {
                        if let Some(a @ (Action::MoveLeft | Action::MoveRight | Action::SoftDrop)) =
                            self.match_gameplay_action(&key)
                        {
                            self.dispatch_gameplay_action(a);
                        }
                    }
                }
                KeyEventKind::Release => self.handle_key_release(key),
            },
            _ => {}
        }
        false
    }

    fn render(&mut self, terminal: &mut ratatui::Terminal<impl ratatui::backend::Backend>) -> std::io::Result<()> {
        terminal.draw(|frame| {
            crate::renderer::render(frame, &self.screen, &self.menu, self.game.as_ref(), &self.settings, self.fps());
        })?;
        Ok(())
    }

    /// Advance the simulation by one fixed step.
    fn tick(&mut self) {
        if self.screen == Screen::Gameplay {
            if let Some(game) = self.game.as_mut() {
                game.tick();
                if game.game_over {
                    self.record_game_over();
                }
            }
        }

        // FPS smoothing (counts simulation-synchronised frames).
        let now = Instant::now();
        self.fps_samples += 1.0;
        let interval = now.duration_since(self.last_fps_update);
        if interval >= Duration::from_millis(500) {
            self.fps_avg = self.fps_samples / interval.as_secs_f64();
            self.fps_samples = 0.0;
            self.last_fps_update = now;
        }
    }

    fn record_game_over(&mut self) {
        let Some(game) = self.game.as_ref() else { return };
        let stats = &mut self.settings.stats;
        stats.games_played += 1;
        stats.total_lines += game.total_lines as u64;
        stats.best_score = stats.best_score.max(game.score);
        stats.best_level = stats.best_level.max(game.level);
        stats.total_playtime_secs += game.start_time.elapsed().as_secs_f64();
        save::save(&self.settings).ok();
        self.enter(Screen::GameOver);
        self.menu.set_kind(MenuKind::GameOver, &self.settings);
    }

    fn enter(&mut self, screen: Screen) {
        self.screen_stack.push(self.screen);
        self.prev_screen = self.screen;
        self.screen = screen;
    }

    fn go_back(&mut self) {
        if let Some(prev) = self.screen_stack.pop() {
            self.screen = prev;
        } else {
            self.screen = Screen::MainMenu;
        }
    }

    /// Process a key event for the current screen. Returns `true` if the app
    /// should quit.
    fn handle_key(&mut self, ev: KeyEvent) -> bool {
        // Ctrl+C / Ctrl+D quit from anywhere, including mid-game.
        if ev.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(ev.code, KeyCode::Char('c' | 'd' | 'C' | 'D'))
        {
            return true;
        }
        // Splash: any key goes to the main menu.
        if self.screen == Screen::Splash {
            self.screen = Screen::MainMenu;
            self.menu.set_kind(MenuKind::Main, &self.settings);
            return false;
        }
        // Check for an in-progress control rebinding first — it consumes the key.
        match self.menu.capture_binding(&ev) {
            BindingCapture::Captured(controls) => {
                self.settings.controls = controls;
                save::save(&self.settings).ok();
                return false;
            }
            BindingCapture::Cancelled => return false,
            BindingCapture::None => {}
        }

        // Gameplay actions are bound keys first; pause key leaves gameplay.
        if self.screen == Screen::Gameplay {
            let action = self.match_gameplay_action(&ev).or(match ev.code {
                KeyCode::Esc => Some(Action::Pause),
                _ => None,
            });
            if let Some(action) = action {
                self.dispatch_gameplay_action(action);
            }
            return false;
        }

        match self.screen {
            Screen::MainMenu => self.handle_main_menu(ev),
            Screen::Settings => self.handle_settings_menu(ev),
            Screen::Controls => self.handle_controls_menu(ev),
            Screen::GameplaySettings => self.handle_gameplay_settings_menu(ev),
            Screen::Paused => self.handle_pause_menu(ev),
            Screen::GameOver => self.handle_game_over_menu(ev),
            Screen::Stats => self.handle_stats_screen(ev),
            Screen::Splash | Screen::Gameplay => false,
        }
    }

    fn navigate_menu(&mut self, ev: KeyEvent) {
        match ev.code {
            KeyCode::Up => self.menu.up(),
            KeyCode::Down => self.menu.down(),
            KeyCode::Char('k') => self.menu.up(),
            KeyCode::Char('j') => self.menu.down(),
            _ => {}
        }
    }

    fn handle_main_menu(&mut self, ev: KeyEvent) -> bool {
        self.navigate_menu(ev);
        if ev.code == KeyCode::Enter {
            match self.menu.selected() {
                0 => self.start_game(),
                1 => {
                    self.enter(Screen::Settings);
                    self.menu.set_kind(MenuKind::Settings, &self.settings);
                }
                2 => {
                    self.enter(Screen::Stats);
                }
                3 => return true,
                _ => {}
            }
        }
        if ev.code == KeyCode::Esc {
            // Esc on the main menu is also a quit affordance.
            return true;
        }
        false
    }

    fn handle_settings_menu(&mut self, ev: KeyEvent) -> bool {
        self.navigate_menu(ev);
        if ev.code == KeyCode::Enter {
            match self.menu.selected() {
                0 => {
                    self.enter(Screen::Controls);
                    self.menu.set_kind(MenuKind::Controls, &self.settings);
                }
                1 => {
                    self.enter(Screen::GameplaySettings);
                    self.menu.set_kind(MenuKind::Gameplay, &self.settings);
                }
                2 => self.go_back(),
                _ => {}
            }
        }
        if ev.code == KeyCode::Esc {
            self.go_back();
        }
        false
    }

    fn handle_controls_menu(&mut self, ev: KeyEvent) -> bool {
        self.navigate_menu(ev);
        if ev.code == KeyCode::Enter {
            self.menu.start_rebind();
        } else if ev.code == KeyCode::Esc {
            self.go_back();
            self.menu.set_kind(MenuKind::Settings, &self.settings);
        }
        false
    }

    /// Gameplay settings submenu: two cyclable options (Starting Level and
    /// Grid Scale). Up/Down selects an option; Left/Right or Enter cycles its
    /// value; Esc returns to the Settings menu.
    fn handle_gameplay_settings_menu(&mut self, ev: KeyEvent) -> bool {
        self.navigate_menu(ev);
        let idx = self.menu.selected();
        match ev.code {
            KeyCode::Left => self.cycle_gameplay_option(idx, false),
            KeyCode::Right | KeyCode::Enter => self.cycle_gameplay_option(idx, true),
            KeyCode::Esc => {
                self.go_back();
                self.menu.set_kind(MenuKind::Settings, &self.settings);
            }
            _ => {}
        }
        false
    }

    /// Cycle the gameplay option at `idx` forward (`up`) or backward.
    fn cycle_gameplay_option(&mut self, idx: usize, up: bool) {
        match idx {
            0 => {
                let lvl = self.settings.gameplay.starting_level;
                self.settings.gameplay.starting_level = if up {
                    (lvl % 20) + 1
                } else {
                    if lvl <= 1 { 20 } else { lvl - 1 }
                };
            }
            1 => {
                let scale = self.settings.gameplay.grid_scale.clamp(1, 4);
                self.settings.gameplay.grid_scale = if up {
                    if scale >= 4 { 1 } else { scale + 1 }
                } else {
                    if scale <= 1 { 4 } else { scale - 1 }
                };
            }
            _ => {}
        }
        save::save(&self.settings).ok();
    }

    fn handle_pause_menu(&mut self, ev: KeyEvent) -> bool {
        self.navigate_menu(ev);
        if ev.code == KeyCode::Enter {
            match self.menu.selected() {
                0 => self.screen = Screen::Gameplay,
                1 => self.start_game(),
                2 => {
                    self.screen_stack.clear();
                    self.screen = Screen::MainMenu;
                    self.menu.set_kind(MenuKind::Main, &self.settings);
                    self.game = None;
                }
                3 => return true,
                _ => {}
            }
        }
        if ev.code == KeyCode::Esc {
            self.screen = Screen::Gameplay;
        }
        false
    }

    fn handle_game_over_menu(&mut self, ev: KeyEvent) -> bool {
        self.navigate_menu(ev);
        if ev.code == KeyCode::Enter {
            match self.menu.selected() {
                0 => self.start_game(),
                1 => {
                    self.screen_stack.clear();
                    self.screen = Screen::MainMenu;
                    self.menu.set_kind(MenuKind::Main, &self.settings);
                    self.game = None;
                }
                2 => return true,
                _ => {}
            }
        }
        if ev.code == KeyCode::Esc {
            self.screen_stack.clear();
            self.screen = Screen::MainMenu;
            self.menu.set_kind(MenuKind::Main, &self.settings);
            self.game = None;
        }
        false
    }

    fn handle_stats_screen(&mut self, ev: KeyEvent) -> bool {
        if ev.code == KeyCode::Esc || ev.code == KeyCode::Enter {
            self.go_back();
        }
        false
    }

    fn start_game(&mut self) {
        let game = Game::new(self.settings.clone());
        self.game = Some(game);
        self.screen_stack.clear();
        self.screen = Screen::Gameplay;
    }

    /// Identify which gameplay action `ev` maps to, if any.
    fn match_gameplay_action(&self, ev: &KeyEvent) -> Option<Action> {
        for (action, field) in Action::BINDINGS {
            let binding = match field {
                "move_left" => &self.settings.controls.move_left,
                "move_right" => &self.settings.controls.move_right,
                "soft_drop" => &self.settings.controls.soft_drop,
                "hard_drop" => &self.settings.controls.hard_drop,
                "rotate_cw" => &self.settings.controls.rotate_cw,
                "rotate_ccw" => &self.settings.controls.rotate_ccw,
                "hold" => &self.settings.controls.hold,
                "pause" => &self.settings.controls.pause,
                "restart" => &self.settings.controls.restart,
                _ => continue,
            };
            if crate::input::matches_binding(ev, binding) {
                // Avoid the same key mapping to multiple actions; first match wins.
                return Some(action);
            }
        }
        None
    }

    fn dispatch_gameplay_action(&mut self, action: Action) {
        let Some(game) = self.game.as_mut() else { return };
        match action {
            Action::MoveLeft => {
                game.shift(-1);
            }
            Action::MoveRight => {
                game.shift(1);
            }
            Action::SoftDrop => {
                if self.has_key_release {
                    game.set_soft_dropping(true);
                } else {
                    game.soft_drop_pulse(SOFT_DROP_PULSE_MS);
                }
            }
            Action::HardDrop => {
                game.hard_drop();
            }
            Action::RotateCw => {
                game.rotate(true);
            }
            Action::RotateCcw => {
                game.rotate(false);
            }
            Action::Hold => {
                game.try_hold();
            }
            Action::Pause => {
                self.screen = Screen::Paused;
                self.menu.set_kind(MenuKind::Pause, &self.settings);
            }
            Action::Restart => {
                game.reset();
            }
        }
    }

    /// Handle a key release: ends soft drop if the soft-drop key was let go.
    fn handle_key_release(&mut self, ev: KeyEvent) {
        if self.screen != Screen::Gameplay {
            return;
        }
        let Some(game) = self.game.as_mut() else { return };
        let soft = &self.settings.controls.soft_drop;
        if crate::input::matches_binding(&ev, soft) {
            game.set_soft_dropping(false);
        }
    }
}