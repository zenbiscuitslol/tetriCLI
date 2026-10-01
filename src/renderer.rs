//! Full-screen `ratatui` rendering for all application states: splash, menus,
//! settings, gameplay, pause, game-over, and statistics.
//!
//! Rendering is fully buffered through `ratatui::Frame`. The renderer never
//! reads input or mutates game state.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::Screen;
use crate::board::Board;
use crate::game::{Game, SpecialKind};
use crate::menu::Menu;
use crate::piece::{ActivePiece, Rotation, FIELD_COLS, FIELD_ROWS, HIDDEN_ROWS};
use crate::settings::Settings;
use crate::piece::Tetromino;
use crate::ui::{paint_block, BlockStyle};

pub const ROWS_VISIBLE: usize = FIELD_ROWS - HIDDEN_ROWS;

/// Render a given screen into `frame` using app state.
pub fn render(
    frame: &mut Frame,
    screen: &Screen,
    menu: &Menu,
    game: Option<&Game>,
    settings: &Settings,
    fps: Option<f64>,
) {
    match screen {
        Screen::Splash => render_splash(frame),
        Screen::MainMenu => render_main_menu(frame, menu),
        Screen::Settings => render_settings(frame, menu),
        Screen::Controls => render_controls(frame, menu),
        Screen::GameplaySettings => render_gameplay_settings(frame, menu, settings),
        Screen::Gameplay => {
            if let Some(game) = game {
                render_game(frame, game);
            }
        }
        Screen::Paused => {
            if let Some(game) = game {
                render_game(frame, game);
                render_pause_overlay(frame, menu);
            }
        }
        Screen::GameOver => {
            if let Some(game) = game {
                render_game(frame, game);
                render_game_over_overlay(frame, game, menu);
            }
        }
        Screen::Stats => render_stats(frame, menu, settings),
    }
    if let Some(fps) = fps {
        render_fps(frame, fps);
    }
}

// ---------------------------------------------------------------------------
// Splash
// ---------------------------------------------------------------------------

fn render_splash(frame: &mut Frame) {
    let area = frame.size();
    let logo = [
        "████████ ███████╗ ████████ ███████╗ ██╗      ███████╗ ██╗      ██████╗",
        " ══██╔═ ██╔════╝  ══██╔═ ██╔════╝ ██║      ██╔════╝ ██║      ╚══██╔══",
        "   ██║   ██████╗     ██║   ██████╗  ██║      ██║      ██║         ██║",
        "   ██║   ██╔═══╝     ██║   ██╔══██╗ ██║      ██║      ██║         ██║",
        "   ██║   ███████╗    ██║   ██║  ██║ ██║      ╚██████╗ ███████╗ ██████║",
        "   ╚═╝   ╚══════╝    ╚═╝   ╚═╝  ╚═╝ ╚═╝       ╚═════╝ ╚══════╝ ╚═════╝",
    ];
    let subtitle = "a terminal Tetris";
    let prompt = "Press any key to continue…";

    let logo_h = logo.len() as u16;
    let center_v = area.height / 2;
    let top = center_v.saturating_sub(logo_h / 2 + 4);
    let left = area.width.saturating_sub(logo[0].chars().count() as u16) / 2;

    let style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    for (i, row) in logo.iter().enumerate() {
        frame.render_widget(
            Paragraph::new(Span::styled((*row).to_string(), style)).alignment(Alignment::Left),
            Rect::new(left, top + i as u16, area.width, 1),
        );
    }
    frame.render_widget(
        Paragraph::new(Span::styled(subtitle.to_string(), Style::default().fg(Color::DarkGray)))
            .alignment(Alignment::Center),
        Rect::new(0, top + logo_h + 2, area.width, 1),
    );
    frame.render_widget(
        Paragraph::new(Span::styled(prompt.to_string(), Style::default().fg(Color::Gray)))
            .alignment(Alignment::Center),
        Rect::new(0, top + logo_h + 5, area.width, 1),
    );
}

// ---------------------------------------------------------------------------
// Main menu
// ---------------------------------------------------------------------------

fn render_main_menu(frame: &mut Frame, menu: &Menu) {
    let area = frame.size();
    let title_lines = ["══════════════════════", "       tetriCLI       ", "══════════════════════"];
    let items = ["Play", "Settings", "Statistics", "Quit"];
    let selected = menu.selected();

    let inner = block_centered(frame, area, area.width.min(30), 16);

    // Title.
    let mut y = inner.y;
    for line in &title_lines {
        frame.render_widget(
            Paragraph::new(Span::styled(
                (*line).to_string(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
    }

    // Menu items.
    let title_h = title_lines.len() as u16;
    let line_h = 1u16;
    let gap = 1u16;
    let total_h = (items.len() as u16) * (line_h + gap);
    let mut cur_y = y + 1 + (inner.height - title_h - 1).saturating_sub(total_h) / 2;
    for (i, item) in items.iter().enumerate() {
        let is_sel = i == selected;
        let prefix = if is_sel { "> " } else { "  " };
        let text = format!("{prefix}{item}");
        let style = if is_sel {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(text, style)).alignment(Alignment::Center),
            Rect::new(inner.x, cur_y, inner.width, 1),
        );
        cur_y += line_h + gap;
    }
}

// ---------------------------------------------------------------------------
// Settings menu
// ---------------------------------------------------------------------------

fn render_settings(frame: &mut Frame, menu: &Menu) {
    let area = frame.size();
    let items = ["Controls", "Gameplay", "Back"];
    render_simple_menu(frame, area, "Settings", &items, menu.selected());
}

// ---------------------------------------------------------------------------
// Gameplay settings (cyclable options) menu
// ---------------------------------------------------------------------------

fn render_gameplay_settings(frame: &mut Frame, menu: &Menu, settings: &Settings) {
    let area = frame.size();
    let selected = menu.selected();
    let level = settings.gameplay.starting_level;
    let scale = settings.gameplay.grid_scale.clamp(1, 4);

    let rows: [(&str, String); 2] = [
        ("Starting Level", level.to_string()),
        ("Grid Scale", format!("{scale}x")),
    ];

    let width = area.width.min(34);
    let height = rows.len() as u16 * 2 + 6;
    let inner = block_centered(frame, area, width, height);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "Gameplay".to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let footer_y = inner.bottom().saturating_sub(1);
    let side_w = inner.width / 2;
    for (i, (label, value)) in rows.iter().enumerate() {
        let row_y = inner.y + 1 + i as u16 * 2;
        if row_y >= footer_y {
            break;
        }
        let is_sel = i == selected;
        let label_style = if is_sel {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let value_style = if is_sel {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let prefix = if is_sel { "> " } else { "  " };
        frame.render_widget(
            Paragraph::new(Span::styled(format!("{prefix}{label}"), label_style))
                .alignment(Alignment::Left),
            Rect::new(inner.x, row_y, side_w, 1),
        );
        frame.render_widget(
            Paragraph::new(Span::styled(value.clone(), value_style)).alignment(Alignment::Right),
            Rect::new(inner.x + side_w, row_y, inner.width - side_w, 1),
        );
    }

    frame.render_widget(
        Paragraph::new(Span::styled(
            "<↑/↓> navigate   <←/→> change   <Esc> back".to_string(),
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center),
        Rect::new(inner.x, footer_y, inner.width, 1),
    );
}

// ---------------------------------------------------------------------------
// Controls (rebinding) menu
// ---------------------------------------------------------------------------

fn render_controls(frame: &mut Frame, menu: &Menu) {
    let area = frame.size();
    let fields = Settings::CONTROL_FIELDS;
    let selected = menu.selected();
    let controls = menu.active_controls();
    let pending = menu.pending_binding();

    let width = area.width.min(44);
    let height = (fields.len() as u16) * 2 + 6;
    let inner = block_centered(frame, area, width, height);

    // Header.
    frame.render_widget(
        Paragraph::new(Span::styled(
            "Controls".to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let footer_y = inner.bottom().saturating_sub(1);

    for (i, (label, field)) in fields.iter().enumerate() {
        let binding = field_value(controls, field);
        let pretty = crate::input::pretty_label(binding);

        let is_sel = i == selected;
        let label_style = if is_sel {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let binding_style = if is_sel {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let row_y = inner.y + 1 + i as u16 * 2;
        if row_y >= footer_y {
            break;
        }

        let side_w = inner.width / 2;
        frame.render_widget(
            Paragraph::new(Span::styled((*label).to_string(), label_style)).alignment(Alignment::Left),
            Rect::new(inner.x, row_y, side_w, 1),
        );
        if pending == Some(i) {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    "Press any key…".to_string(),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ))
                .alignment(Alignment::Right),
                Rect::new(inner.x + side_w, row_y, side_w, 1),
            );
        } else {
            frame.render_widget(
                Paragraph::new(Span::styled(pretty, binding_style)).alignment(Alignment::Right),
                Rect::new(inner.x + side_w, row_y, side_w, 1),
            );
        }
    }

    frame.render_widget(
        Paragraph::new(Span::styled(
            "<Enter> rebind   <Esc> back".to_string(),
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center),
        Rect::new(inner.x, footer_y, inner.width, 1),
    );
}

fn field_value<'a>(c: &'a crate::settings::Controls, field: &str) -> &'a str {
    match field {
        "move_left" => &c.move_left,
        "move_right" => &c.move_right,
        "soft_drop" => &c.soft_drop,
        "hard_drop" => &c.hard_drop,
        "rotate_cw" => &c.rotate_cw,
        "rotate_ccw" => &c.rotate_ccw,
        "hold" => &c.hold,
        "pause" => &c.pause,
        "restart" => &c.restart,
        _ => "",
    }
}

// ---------------------------------------------------------------------------
// Statistics screen
// ---------------------------------------------------------------------------

fn render_stats(frame: &mut Frame, menu: &Menu, settings: &Settings) {
    let _ = menu;
    let area = frame.size();
    let s = &settings.stats;
    let rows: [(&str, String); 5] = [
        ("Games played", s.games_played.to_string()),
        ("Total lines cleared", s.total_lines.to_string()),
        ("Best score", s.best_score.to_string()),
        ("Best level", s.best_level.to_string()),
        ("Total playtime", format_playtime(s.total_playtime_secs)),
    ];

    let width = area.width.min(40);
    let height = rows.len() as u16 + 6;
    let inner = block_centered(frame, area, width, height);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "Statistics".to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let mut y = inner.y + 2;
    let side_w = inner.width / 2;
    for (k, v) in &rows {
        frame.render_widget(
            Paragraph::new(Span::styled((*k).to_string(), Style::default().fg(Color::Gray)))
                .alignment(Alignment::Left),
            Rect::new(inner.x, y, side_w, 1),
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                v.clone(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right),
            Rect::new(inner.x + side_w, y, inner.width - side_w, 1),
        );
        y += 1;
    }

    let footer_y = inner.bottom().saturating_sub(1);
    frame.render_widget(
        Paragraph::new(Span::styled(
            "<Esc> back".to_string(),
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center),
        Rect::new(inner.x, footer_y, inner.width, 1),
    );
}

// ---------------------------------------------------------------------------
// Gameplay
// ---------------------------------------------------------------------------

fn render_game(frame: &mut Frame, game: &Game) {
    let area = frame.size();

    let (cell_w, cell_h) = cell_dims(game, area);
    let field_w = FIELD_COLS as u16 * cell_w;
    let field_h = ROWS_VISIBLE as u16 * cell_h;
    let hud_w = 20u16;
    let gap = 1u16;
    let total_w = field_w + 2 + gap + hud_w + 2;
    let total_h = field_h + 2;

    let layout_left = area.width.saturating_sub(total_w) / 2;
    let layout_top = area.height.saturating_sub(total_h) / 2;

    // Clip to the terminal so undersized windows degrade instead of panicking.
    let playfield = Rect::new(layout_left, layout_top, field_w + 2, total_h).intersection(area);
    let hud = Rect::new(layout_left + field_w + 2 + gap, layout_top, hud_w, total_h).intersection(area);

    render_playfield(frame, game, playfield, cell_w, cell_h);
    render_hud(frame, game, hud);
}

fn cell_dims(game: &Game, area: Rect) -> (u16, u16) {
    let scale = game.settings.gameplay.grid_scale.clamp(1, 4) as u16;
    let hud_w = 20u16;
    let gap = 1u16;
    let margin = 4u16;
    let max_field_inner_w = area.width.saturating_sub(hud_w + gap + margin + 2);
    let max_field_inner_h = area.height.saturating_sub(margin + 2);
    let max_cell_w = (max_field_inner_w / FIELD_COLS as u16).max(1);
    let max_cell_h = (max_field_inner_h / ROWS_VISIBLE as u16).max(1);
    let cell_w = max_cell_w.min(scale * 2).max(1);
    let cell_h = max_cell_h.min(scale).max(1);
    (cell_w, cell_h)
}

fn render_playfield(frame: &mut Frame, game: &Game, area: Rect, cell_w: u16, cell_h: u16) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            " tetriCLI ".to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(block.clone(), area);
    let inner = block.inner(area);

    let grid = game.board.cells();
    let cell_origin = |col: usize, vis_row: usize| {
        (inner.x + col as u16 * cell_w, inner.y + vis_row as u16 * cell_h)
    };

    // Locked cells, with a faint dot grid in the empty ones.
    let buf = frame.buffer_mut();
    for r in 0..ROWS_VISIBLE {
        let view_row = &grid[r + HIDDEN_ROWS];
        for c in 0..FIELD_COLS {
            let (x, y) = cell_origin(c, r);
            match view_row[c].0 {
                Some(kind) => paint_block(buf, x, y, cell_w, cell_h, kind, BlockStyle::Solid),
                None => {}
            }
        }
    }

    // Ghost piece (skipping cells the live piece already covers).
    if game.settings.gameplay.show_ghost {
        if let (Some(ghost), Some(cur)) = (game.ghost(), game.current) {
            let cur_cells = cur.cells();
            for (x, y) in ghost.cells() {
                if cur_cells.contains(&(x, y)) {
                    continue;
                }
                if let Some(vr) = visible_row(y) {
                    let (px, py) = cell_origin(x as usize, vr);
                    paint_block(buf, px, py, cell_w, cell_h, ghost.kind, BlockStyle::Ghost);
                }
            }
        }
    }

    // Current piece.
    if let Some(piece) = game.current {
        for (x, y) in piece.cells() {
            if let Some(vr) = visible_row(y) {
                let (px, py) = cell_origin(x as usize, vr);
                paint_block(buf, px, py, cell_w, cell_h, piece.kind, BlockStyle::Solid);
            }
        }
    }
}

/// Map a board row to a visible-row index, if it is on screen.
fn visible_row(y: i32) -> Option<usize> {
    if y < 0 {
        return None;
    }
    Board::visible_row(y as usize).filter(|&vr| vr < ROWS_VISIBLE)
}

fn render_hud(frame: &mut Frame, game: &Game, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(9),
            Constraint::Min(5),
            Constraint::Length(5),
        ])
        .split(area);

    // Hold.
    let hold_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Hold ", Style::default().fg(Color::Cyan)));
    frame.render_widget(hold_block.clone(), chunks[0]);
    if let Some(kind) = game.hold {
        preview_minipiece(frame, hold_block.inner(chunks[0]), kind, game.hold_used);
    }

    // Next.
    let next_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Next ", Style::default().fg(Color::Cyan)));
    frame.render_widget(next_block.clone(), chunks[1]);
    let next_inner = next_block.inner(chunks[1]);
    let slot_h = 2u16;
    for (i, kind) in game.queue.iter().take(4).enumerate() {
        let slot = Rect::new(next_inner.x, next_inner.y + i as u16 * slot_h, next_inner.width, slot_h);
        if slot.bottom() > next_inner.bottom() {
            break;
        }
        preview_minipiece(frame, slot, *kind, false);
    }

    // Stats panel.
    let stats_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Stats ", Style::default().fg(Color::Cyan)));
    frame.render_widget(stats_block.clone(), chunks[2]);
    let inner = stats_block.inner(chunks[2]);
    let rows = [
        ("Score", game.score.to_string()),
        ("Level", game.level.to_string()),
        ("Lines", game.total_lines.to_string()),
    ];
    let mut y = inner.y;
    let side_w = inner.width / 2;
    for (k, v) in &rows {
        frame.render_widget(
            Paragraph::new(Span::styled((*k).to_string(), Style::default().fg(Color::Gray))),
            Rect::new(inner.x, y, side_w, 1),
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                v.clone(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right),
            Rect::new(inner.x + side_w, y, inner.width - side_w, 1),
        );
        y += 1;
    }

    // Info (combo/b2b/time).
    let info_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Info ", Style::default().fg(Color::Cyan)));
    frame.render_widget(info_block.clone(), chunks[3]);
    let inner = info_block.inner(chunks[3]);
    let combo = game.combo.map(|c| c.max(1)).unwrap_or(0);
    let rows = [
        ("Combo", if combo > 1 { format!("{combo}x") } else { "—".to_string() }),
        ("B2B", if game.back_to_back { "YES".to_string() } else { "—" .to_string() }),
        ("Time", format_playtime(game.start_time.elapsed().as_secs_f64())),
    ];
    let mut y = inner.y;
    let side_w = inner.width / 2;
    for (k, v) in &rows {
        frame.render_widget(
            Paragraph::new(Span::styled((*k).to_string(), Style::default().fg(Color::Gray))),
            Rect::new(inner.x, y, side_w, 1),
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                v.clone(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right),
            Rect::new(inner.x + side_w, y, inner.width - side_w, 1),
        );
        y += 1;
    }

    // Last-clear banner.
    if let Some(text) = &game.events.last_clear_text {
        let style = if matches!(game.events.last_special, SpecialKind::PerfectClear) {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if game.events.last_special.is_tspin() {
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(text.clone(), style)).alignment(Alignment::Center),
            Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
        );
    }
}

/// Render a mini-preview of a tetromino (2x1 terminal cells per block),
/// centered in `area`.
fn preview_minipiece(frame: &mut Frame, area: Rect, kind: Tetromino, used: bool) {
    let cells = ActivePiece { kind, rot: Rotation::Spawn, x: 0, y: 0 }.cells();
    let min_x = cells.iter().map(|c| c.0).min().unwrap_or(0);
    let min_y = cells.iter().map(|c| c.1).min().unwrap_or(0);
    let max_x = cells.iter().map(|c| c.0).max().unwrap_or(0);
    let max_y = cells.iter().map(|c| c.1).max().unwrap_or(0);
    let pw = (max_x - min_x + 1) as u16 * 2;
    let ph = (max_y - min_y + 1) as u16;
    let origin_x = area.x + area.width.saturating_sub(pw) / 2;
    let origin_y = area.y + area.height.saturating_sub(ph) / 2;

    frame.render_widget(Clear, area);

    let style = if used { BlockStyle::Disabled } else { BlockStyle::Solid };
    let buf = frame.buffer_mut();
    for &(x, y) in &cells {
        let px = origin_x + (x - min_x) as u16 * 2;
        let py = origin_y + (y - min_y) as u16;
        paint_block(buf, px, py, 2, 1, kind, style);
    }
}

// ---------------------------------------------------------------------------
// Pause / Game-over overlays
// ---------------------------------------------------------------------------

fn render_pause_overlay(frame: &mut Frame, menu: &Menu) {
    let area = frame.size();
    let items = ["Resume", "Restart", "Main Menu", "Quit"];
    overlay_menu(frame, area, "Paused", &items, menu.selected(), 22, 10);
}

fn render_game_over_overlay(frame: &mut Frame, game: &Game, menu: &Menu) {
    let area = frame.size();
    let items = ["Play Again", "Main Menu", "Quit"];
    let title = format!("Game Over  Score: {}", game.score);
    overlay_menu(frame, area, &title, &items, menu.selected(), 28, 10);
}

fn overlay_menu(frame: &mut Frame, area: Rect, title: &str, items: &[&str], selected: usize, w: u16, h: u16) {
    frame.render_widget(Clear, area);
    render_simple_menu_with_title(frame, area, title, items, selected, w, h);
}

// ---------------------------------------------------------------------------
// Generic menu helpers
// ---------------------------------------------------------------------------

fn render_simple_menu(frame: &mut Frame, area: Rect, title: &str, items: &[&str], selected: usize) {
    let h = items.len() as u16 + 4;
    let w = 24u16;
    let inner = block_centered(frame, area, w, h);

    frame.render_widget(
        Paragraph::new(Span::styled(
            title.to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let mut y = inner.y + 2;
    for (i, item) in items.iter().enumerate() {
        let is_sel = i == selected;
        let prefix = if is_sel { "> " } else { "  " };
        let text = format!("{prefix}{item}");
        let style = if is_sel {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(text, style)).alignment(Alignment::Center),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
    }

    let footer_y = inner.bottom().saturating_sub(1);
    frame.render_widget(
        Paragraph::new(Span::styled(
            "<↑/↓> navigate   <Enter> select   <Esc> back".to_string(),
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center),
        Rect::new(inner.x, footer_y, inner.width, 1),
    );
}

fn render_simple_menu_with_title(frame: &mut Frame, area: Rect, title: &str, items: &[&str], selected: usize, w: u16, h: u16) {
    let inner = block_centered(frame, area, w, h);

    frame.render_widget(
        Paragraph::new(Span::styled(
            title.to_string(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let mut y = inner.y + 2;
    for (i, item) in items.iter().enumerate() {
        let is_sel = i == selected;
        let prefix = if is_sel { "> " } else { "  " };
        let text = format!("{prefix}{item}");
        let style = if is_sel {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(text, style)).alignment(Alignment::Center),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
    }
}

// ---------------------------------------------------------------------------
// FPS counter
// ---------------------------------------------------------------------------

fn render_fps(frame: &mut Frame, fps: f64) {
    frame.render_widget(
        Paragraph::new(Span::styled(format!("{fps:.0} FPS"), Style::default().fg(Color::DarkGray)))
            .alignment(Alignment::Right),
        Rect::new(0, 0, frame.size().width, 1),
    );
}

// ---------------------------------------------------------------------------
// Shared layout helpers
// ---------------------------------------------------------------------------

/// A centered bordered box. Renders the border and returns the inner content
/// rect. The caller renders content into the returned rect directly.
fn block_centered(frame: &mut Frame, area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let rect = Rect::new(x, y, w, h);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(block.clone(), rect);
    block.inner(rect)
}

/// Format seconds as M:SS or H:MM:SS.
fn format_playtime(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Gameplay must render without panicking at any scale or terminal size,
    /// including ones too small to fit the full layout.
    #[test]
    fn render_game_fits_any_size() {
        for scale in 1..=4 {
            for (w, h) in [(120, 50), (80, 24), (60, 22), (40, 12)] {
                let mut settings = Settings::default();
                settings.gameplay.grid_scale = scale;
                let mut game = Game::new(settings);
                game.hard_drop();
                let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
                term.draw(|f| render_game(f, &game)).unwrap();
            }
        }
    }
}
