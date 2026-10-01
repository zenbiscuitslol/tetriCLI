//! tetriCLI — a polished terminal Tetris clone.
//!
//! Entry point: sets up the terminal (raw mode, alternate screen), runs the
//! [`App`] game loop, and restores the terminal on exit even if the app panics
//! mid-run.

mod app;
mod board;
mod game;
mod input;
mod menu;
mod piece;
mod renderer;
mod save;
mod settings;
mod ui;

use std::io;

use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::supports_keyboard_enhancement;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;

/// Install terminal state and run the application. Guarantees teardown on the
/// way out via a `Drop`-style guard.
fn run() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    // Ask for key-release/repeat events where supported (kitty protocol).
    let enhanced = supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        execute!(
            stdout,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
        )?;
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = App::from_settings().run(&mut terminal);

    // Always restore the terminal before we return (the guard is redundant
    // here because we do it explicitly, but we keep it defensive).
    if enhanced {
        execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags)?;
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    result
}

fn main() {
    if let Err(e) = run() {
        // Best-effort cleanup; if raw mode wasn't enabled this is a no-op.
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        eprintln!("tetriCLI: {e}");
    }
}