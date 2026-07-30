//! The application state machine, game-loop driver, and input dispatch.
//!
//! [`App`] owns the settings, the live [`Game`], the current menu state, and
//! the timer logic needed to step the game at 60 FPS without busy waiting. It
//! is the glue between `input`, `game`, `menu`, and `renderer`.

use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};

use crate::game::{DaspDirection, Game};
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
    last_tick: Instant,
    fps_avg: f64,
    fps_samples: f64,
    last_fps_update: Instant,
    }

const TICK_MS: u64 = crate::game::DT_MS;
const TICK_DURATION: Duration = Duration::from_millis(TICK_MS);

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
            last_tick: Instant::now(),
            fps_avg: 0.0,
            fps_samples: 0.0,
            last_fps_update: Instant::now(),
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
        self.last_tick = Instant::now();
        self.last_fps_update = Instant::now();
        loop {
            self.render(terminal)?;
            let timeout = TICK_DURATION
                .checked_sub(self.last_tick.elapsed())
                .unwrap_or(TICK_DURATION);
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(ev) if ev.kind == KeyEventKind::Press => {
                        let should_quit = self.handle_key(ev);
                        if should_quit {
                            return Ok(());
                        }
                    }
                    Event::Key(ev) if ev.kind == KeyEventKind::Release => {
                        self.handle_key_release(ev);
                    }
                    Event::Resize(..) => {
                        // Ratatui handles redraw automatically on next render.
                    }
                    _ => {}
                }
            }
            self.tick();
        }
    }

    fn render(&mut self, terminal: &mut ratatui::Terminal<impl ratatui::backend::Backend>) -> std::io::Result<()> {
        terminal.draw(|frame| {
            crate::renderer::render(frame, &self.screen, &self.menu, self.game.as_ref(), &self.settings, self.fps());
        })?;
        Ok(())
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick);
        if elapsed >= TICK_DURATION {
            self.last_tick = now;
            if self.screen == Screen::Gameplay {
                if let Some(game) = self.game.as_mut() {
                    game.tick();
                    if game.game_over {
                        self.settings.stats.games_played += 1;
                        self.settings.stats.total_lines += game.total_lines as u64;
                        self.settings.stats.best_score = self.settings.stats.best_score.max(game.score);
                        self.settings.stats.best_level = self.settings.stats.best_level.max(game.level);
                        self.settings.stats.total_playtime_secs += game.start_time.elapsed().as_secs_f64();
                        save::save(&self.settings).ok();
                        self.enter(Screen::GameOver);
                        self.menu.set_kind(MenuKind::GameOver, &self.settings);
                    }
                }
            }
            // FPS smoothing.
            self.fps_samples += 1.0;
            if now.duration_since(self.last_fps_update) >= Duration::from_millis(500) {
                let interval = now.duration_since(self.last_fps_update).as_secs_f64().max(0.001);
                self.fps_avg = self.fps_samples / interval;
                self.fps_samples = 0.0;
                self.last_fps_update = now;
            }
        }
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
            if let Some(action) = self.match_gameplay_action(&ev) {
                self.dispatch_gameplay_action(action);
                return false;
            }
            // In-game unbound movement keys still low-level drive DAS/ARR; for
            // bound keys we already returned above. Unreachable in practice.
            return false;
        }

        match self.screen {
            Screen::MainMenu => self.handle_main_menu(ev),
            Screen::Settings => self.handle_settings_menu(ev),
            Screen::Controls => self.handle_controls_menu(ev),
            Screen::Paused => self.handle_pause_menu(ev),
            Screen::GameOver => self.handle_game_over_menu(ev),
            Screen::Stats => self.handle_stats_screen(ev),
            Screen::Splash | Screen::Gameplay => false,
        }
    }

    fn navigate_menu(&mut self, ev: KeyEvent, len: usize) -> bool {
        match ev.code {
            KeyCode::Up => self.menu.up(),
            KeyCode::Down => self.menu.down(),
            KeyCode::Enter => return true,
            KeyCode::Char('k') => self.menu.up(),
            KeyCode::Char('j') => self.menu.down(),
            _ => {}
        }
        let _ = len;
        false
    }

    fn handle_main_menu(&mut self, ev: KeyEvent) -> bool {
        let _ = self.navigate_menu(ev, MenuKind::Main.len());
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
        let _ = self.navigate_menu(ev, MenuKind::Settings.len());
        if ev.code == KeyCode::Enter {
            match self.menu.selected() {
                0 => {
                    self.enter(Screen::Controls);
                    self.menu.set_kind(MenuKind::Controls, &self.settings);
                }
                1 => {
                    // Gameplay submenu: cycle starting level directly for now.
                    let lvl = self.settings.gameplay.starting_level;
                    self.settings.gameplay.starting_level = (lvl % 20) + 1;
                    save::save(&self.settings).ok();
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
        let _ = self.navigate_menu(ev, MenuKind::Controls.len());
        if ev.code == KeyCode::Enter {
            self.menu.start_rebind();
        } else if ev.code == KeyCode::Esc {
            self.go_back();
            self.menu.set_kind(MenuKind::Settings, &self.settings);
        }
        false
    }

    fn handle_pause_menu(&mut self, ev: KeyEvent) -> bool {
        let _ = self.navigate_menu(ev, MenuKind::Pause.len());
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
        let _ = self.navigate_menu(ev, MenuKind::GameOver.len());
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
                game.das_release(DaspDirection::Right);
                game.das_start(DaspDirection::Left);
            }
            Action::MoveRight => {
                game.shift(1);
                game.das_release(DaspDirection::Left);
                game.das_start(DaspDirection::Right);
            }
            Action::SoftDrop => {
                game.set_soft_dropping(true);
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
                self.enter(Screen::Paused);
                self.menu.set_kind(MenuKind::Pause, &self.settings);
            }
            Action::Restart => {
                game.reset();
            }
        }
    }

    #[allow(dead_code)]
    fn clear_das_left(&self, game: &mut Game) {
        game.das_release(DaspDirection::Left);
    }

    #[allow(dead_code)]
    fn clear_das_right(&self, game: &mut Game) {
        game.das_release(DaspDirection::Right);
    }

    /// Handle a key release: stops DAS in the released direction and ends soft
    /// drop if the soft-drop key was let go.
    fn handle_key_release(&mut self, ev: KeyEvent) {
        if self.screen != Screen::Gameplay {
            return;
        }
        let Some(game) = self.game.as_mut() else { return };
        let left = &self.settings.controls.move_left;
        let right = &self.settings.controls.move_right;
        let soft = &self.settings.controls.soft_drop;
        if crate::input::matches_binding(&ev, left) {
            game.das_release(DaspDirection::Left);
        }
        if crate::input::matches_binding(&ev, right) {
            game.das_release(DaspDirection::Right);
        }
        if crate::input::matches_binding(&ev, soft) {
            game.set_soft_dropping(false);
        }
    }
}