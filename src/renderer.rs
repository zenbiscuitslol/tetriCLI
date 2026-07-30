//! Full-screen `ratatui` rendering for all application states: splash, menus,
//! settings, gameplay, pause, game-over, and statistics.
//!
//! Rendering is fully buffered through `ratatui::Frame`. The renderer never
//! reads input or mutates game state.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListState, Paragraph, Widget};
use ratatui::Frame;

use crate::app::Screen;
use crate::board::Board;
use crate::game::{Game, SpecialKind};
use crate::menu::Menu;
use crate::piece::{ActivePiece, FIELD_COLS, FIELD_ROWS, HIDDEN_ROWS};
use crate::settings::Settings;
use crate::ui::{colored, tetromino_color, BLOCK, GHOST};

/// Render a given screen into `frame` using `app` state.
pub fn render(frame: &mut ratatui::Frame, screen: &Screen, menu: &Menu, game: Option<&Game>, settings: &Settings, fps: Option<f64>) {
    match screen {
        Screen::Splash => render_splash(frame),
        Screen::MainMenu => render_main_menu(frame, menu),
        Screen::Settings => render_settings(frame, menu),
        Screen::Controls => render_controls(frame, menu),
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

/// Splash screen with a big ASCII-art logo.
fn render_splash(frame: &mut ratatui::Frame) {
    let area = frame.size();
    let logo = [
        "████████╗███████╗██████╗ ███████╗██╗████████╗██╗██████╗",
        "╚══██╔══╝██╔════╝██╔══██╗██╔════╝██║╚══██╔══╝██║██╔══██╗",
        "   ██║   █████╗  ██████╔╝███████╗██║   ██║   ██║██████╔╝",
        "   ██║   ██╔══╝  ██╔══██╗╚════██║██║   ██║   ██║██╔══██╗",
        "   ██║   ███████╗██║  ██║███████║██║   ██║   ██║██║  ██║",
        "   ╚═╝   ╚══════╝╚═╝  ╚═╝╚══════╝╚═╝   ╚═╝   ╚═╝╚═╝  ╚═╝",
    ];
    let subtitle = "a terminal Tetris";
    let prompt = "Press any key to continue…";

    let logo_h = logo.len() as u16;
    let center_v = area.height / 2;
    let top = center_v.saturating_sub(logo_h / 2 + 4);
    let left = area.width.saturating_sub(logo[0].chars().count() as u16) / 2;

    let style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let sub_style = Style::default().fg(Color::DarkGray);
    let prompt_style = Style::default().fg(Color::Gray);

    for (i, row) in logo.iter().enumerate() {
        let t = Line::from(Span::styled((*row).to_string(), style));
        frame.render_widget(Paragraph::new(t).alignment(Alignment::Left), Rect::new(left, top + i as u16, area.width, 1));
    }
    let sub_w = subtitle.len() as u16;
    frame.render_widget(
        Paragraph::new(Span::styled(subtitle.to_string(), sub_style)).alignment(Alignment::Center),
        Rect::new(area.width.saturating_sub(sub_w) / 2, top + logo_h + 2, sub_w, 1),
    );
    let p_w = prompt.len() as u16;
    frame.render_widget(
        Paragraph::new(Span::styled(prompt.to_string(), prompt_style)).alignment(Alignment::Center),
        Rect::new(area.width.saturating_sub(p_w) / 2, top + logo_h + 5, p_w, 1),
    );
}

/// The main menu: a centered box with TETRIS title and vertical list.
fn render_main_menu(frame: &mut ratatui::Frame, menu: &Menu) {
    let area = frame.size();
    let title_lines = [
        "══════════════════════",
        "        TETRIS        ",
        "══════════════════════",
    ];

    let menu_items = ["Play", "Settings", "Statistics", "Quit"];
    let list_state = menu.list_state();
    let selected = menu.selected();

    block_centered(frame, area, area.width.min(30), 16, |inner| {
        let title_h = title_lines.len() as u16;
        let mut y = inner.y;
        for line in &title_lines {
            let w = line.len() as u16;
            let x = inner.x + (inner.width.saturating_sub(w)) / 2;
            frame.render_widget(
                Paragraph::new(Span::styled(
                    (*line).to_string(),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ))
                .alignment(Alignment::Center),
                Rect::new(x, y, w, 1),
            );
            y += 1;
        }
        y += 1; // spacer

        let line_h = 1u16;
        let gap = 1u16;
        let total_h = (menu_items.len() as u16) * (line_h + gap);
        let mut cur_y = y + (inner.height - title_h - 1).saturating_sub(total_h) / 2;
        for (i, item) in menu_items.iter().enumerate() {
            let is_sel = i == selected;
            let prefix = if is_sel { "> " } else { "  " };
            let text = format!("{prefix}{item}");
            let style = if is_sel {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            let w = text.len() as u16;
            let x = inner.x + (inner.width.saturating_sub(w)) / 2;
            frame.render_widget(
                Paragraph::new(Span::styled(text, style)).alignment(Alignment::Left),
                Rect::new(x, cur_y, w, 1),
            );
            cur_y += line_h + gap;
        }
        let _ = list_state;
    });
}

fn render_settings(frame: &mut ratatui::Frame, menu: &Menu) {
    let area = frame.size();
    let chosen = menu.selected();
    let items = ["Controls", "Gameplay", "Back"];
    render_simple_menu(frame, area, "Settings", &items, chosen);
}

fn render_controls(frame: &mut ratatui::Frame, menu: &Menu) {
    let area = frame.size();
    let fields = Settings::CONTROL_FIELDS;
    let chosen = menu.selected();
    let controls = menu.active_controls();
    let pending = menu.pending_binding();

    let width = area.width.min(40);
    let height = (fields.len() as u16) * 2 + 6;
    block_centered(frame, area, width, height, |inner| {
        let header = Line::from(Span::styled(
            "Controls".to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
        frame.render_widget(
            Paragraph::new(header).alignment(Alignment::Center),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        let footer_y = inner.bottom_left().y.saturating_sub(1);
        let middle_top = inner.y + 1;
        let middle_h = (footer_y).saturating_sub(middle_top);
        if let Some(pending_idx) = pending {
            let prompt = "Press any key…  (Esc to cancel)";
            let p_y = middle_top + pending_idx as u16 * 2;
            if p_y < inner.bottom_left().y {
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        prompt.to_string(),
                        Style::default().fg(Color::Yellow),
                    ))
                    .alignment(Alignment::Left),
                    Rect::new(inner.x, p_y + 1, inner.width, 1),
                );
            }
        }
        for (i, (label, field)) in fields.iter().enumerate() {
            let binding = match *field {
                "move_left" => &controls.move_left,
                "move_right" => &controls.move_right,
                "soft_drop" => &controls.soft_drop,
                "hard_drop" => &controls.hard_drop,
                "rotate_cw" => &controls.rotate_cw,
                "rotate_ccw" => &controls.rotate_ccw,
                "hold" => &controls.hold,
                "pause" => &controls.pause,
                "restart" => &controls.restart,
                _ => "",
            };
            let pretty = crate::input::pretty_label(binding);

            let is_sel = i == chosen;
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
            let row_y = middle_top + i as u16 * 2;
            if row_y >= footer_y {
                break;
            }
            let side_w = inner.width / 2;
            frame.render_widget(
                Paragraph::new(Span::styled((*label).to_string(), label_style)).alignment(Alignment::Left),
                Rect::new(inner.x, row_y, side_w, 1),
            );
            if pending == Some(i) {
                let p = "_____";
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        p.to_string(),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    ))
                    .alignment(Alignment::Right),
                    Rect::new(inner.x + side_w, row_y, side_w, 1),
                );
            } else {
                let bw = pretty.len() as u16;
                frame.render_widget(
                    Paragraph::new(Span::styled(pretty, binding_style)).alignment(Alignment::Right),
                    Rect::new(inner.x + inner.width.saturating_sub(bw), row_y, bw, 1),
                );
            }
        }
        let _ = middle_h;
        let footer = "<Enter> rebind   <Esc> back";
        frame.render_widget(
            Paragraph::new(Span::styled(
                footer.to_string(),
                Style::default().fg(Color::DarkGray),
            ))
            .alignment(Alignment::Center),
            Rect::new(inner.x, footer_y, inner.width, 1),
        );
    });
}

fn render_stats(frame: &mut ratatui::Frame, menu: &Menu, settings: &Settings) {
    let area = frame.size();
    let s = &settings.stats;
    let rows = [
        ("Games played", s.games_played.to_string()),
        ("Total lines cleared", s.total_lines.to_string()),
        ("Best score", s.best_score.to_string()),
        ("Best level", s.best_level.to_string()),
        ("Total playtime", format_playtime(s.total_playtime_secs)),
    ];

    let width = area.width.min(40);
    let height = rows.len() as u16 + 6;
    block_centered(frame, area, width, height, |inner| {
        let header = Line::from(Span::styled(
            "Statistics".to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
        frame.render_widget(
            Paragraph::new(header).alignment(Alignment::Center),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        let mut y = inner.y + 2;
        let side_w = inner.width / 2;
        for (k, v) in &rows {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    (*k).to_string(),
                    Style::default().fg(Color::Gray),
                ))
                .alignment(Alignment::Left),
                Rect::new(inner.x, y, side_w, 1),
            );
            let vw = v.len() as u16;
            frame.render_widget(
                Paragraph::new(Span::styled(
                    (*v).clone(),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ))
                .alignment(Alignment::Right),
                Rect::new(inner.x + inner.width.saturating_sub(vw), y, vw, 1),
            );
            y += 1;
        }
        y = inner.bottom_left().y.saturating_sub(1);
        frame.render_widget(
            Paragraph::new(Span::styled(
                "<Esc> back".to_string(),
                Style::default().fg(Color::DarkGray),
            ))
            .alignment(Alignment::Center),
            Rect::new(inner.x, y, inner.width, 1),
        );
    });
    let _ = menu;
}

/// In-game rendering: play field + HUD (hold, next, score, combo, b2b, time).
fn render_game(frame: &mut ratatui::Frame, game: &Game) {
    let area = frame.size();

    // Compute geometry: a playfield that's 2*COLS wide (each cell is 2 spaces).
    let cell_w = 2u16;
    let field_w = (FIELD_COLS as u16) * cell_w;
    let hud_w = 20u16;
    let gap = 1u16;
    let total_w = field_w + gap + hud_w + 2; // borders
    let total_h = (ROWS_VISIBLE as u16) + 2;

    let layout_left = area.width.saturating_sub(total_w) / 2;
    let layout_top = area.height.saturating_sub(total_h) / 2;

    let playfield = Rect::new(layout_left, layout_top, field_w + 2, total_h);
    let hud = Rect::new(playfield.right() + gap, layout_top, hud_w, total_h);

    render_playfield(frame, game, playfield);
    render_hud(frame, game, hud);
}

const ROWS_VISIBLE: usize = FIELD_ROWS - HIDDEN_ROWS;

fn render_playfield(frame: &mut ratatui::Frame, game: &Game, area: Rect) {
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(Span::styled(
        " TETRIS ".to_string(),
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(block.clone(), area);
    let inner = block.inner(area);

    let cell_w = 2u16;
    let cell_h = 1u16;

    let rows_visible = ROWS_VISIBLE as u16;
    let cols = FIELD_COLS as u16;

    // Locked cells.
    let grid = game.board.cells();
    for r in 0..ROWS_VISIBLE {
        let view_row = grid[r + HIDDEN_ROWS];
        for c in 0..cols {
            if let Some(kind) = view_row[c].0 {
                let x = inner.x + c as u16 * cell_w;
                let y = inner.y + r as u16 * cell_h;
                paint_cell(frame, x, y, kind, BLOCK, false);
            }
        }
    }

    // Ghost piece.
    if let Some(ghost) = game.ghost() {
        let cur = game.current;
        for (x, y) in ghost.cells() {
            if y < 0 {
                continue;
            }
            // Skip overlap with current piece cells when drawing ghost.
            let overlaps = cur.map(|c| {
                c.cells().iter().any(|cc| cc.0 == x && cc.1 == y)
            }).unwrap_or(false);
            if overlaps {
                continue;
            }
            if let Some(vr) = Board::visible_row(y as usize) {
                if vr < ROWS_VISIBLE {
                    let gx = inner.x + (x as u16) * cell_w;
                    let gy = inner.y + vr as u16 * cell_h;
                    paint_cell(frame, gx, gy, ghost.kind, GHOST, true);
                }
            }
        }
    }

    // Current piece.
    if let Some(piece) = game.current {
        for (x, y) in piece.cells() {
            if y < 0 {
                continue;
            }
            if let Some(vr) = Board::visible_row(y as usize) {
                if vr < ROWS_VISIBLE {
                    let px = inner.x + (x as u16) * cell_w;
                    let py = inner.y + vr as u16 * cell_h;
                    paint_cell(frame, px, py, piece.kind, BLOCK, false);
                }
            }
        }
    }
}

fn paint_cell(frame: &mut ratatui::Frame, x: u16, y: u16, kind: crate::piece::Tetromino, glyph: &str, dim: bool) {
    let color = tetromino_color(kind);
    let style = if dim {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    };
    let span = Span::styled(glyph.repeat(2), style);
    frame.render_widget(Paragraph::new(Line::from(span)), Rect::new(x, y, 2, 1));
}

fn render_hud(frame: &mut ratatui::Frame, game: &Game, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // hold
            Constraint::Length(7), // next
            Constraint::Length(9), // level/score/lines
            Constraint::Length(6), // combo/b2b/time
        ])
        .split(area);

    // Hold
    let hold_block = Block::default().borders(Borders::ALL).title(Span::styled(" Hold ", Style::default().fg(Color::Cyan)));
    frame.render_widget(hold_block.clone(), chunks[0]);
    if let Some(kind) = game.hold {
        preview_minipiece(frame, hold_block.inner(chunks[0]), kind, game.hold_used);
    }

    // Next
    let next_block = Block::default().borders(Borders::ALL).title(Span::styled(" Next ", Style::default().fg(Color::Cyan)));
    frame.render_widget(next_block.clone(), chunks[1]);
    let next_inner = next_block.inner(chunks[1]);
    let slot_h = 3u16;
    for (i, kind) in game.queue.iter().take(4).enumerate() {
        let slot = Rect::new(next_inner.x, next_inner.y + (i as u16) * slot_h, next_inner.width, slot_h);
        preview_minipiece(frame, slot, *kind, false);
    }

    // Stats panel (score/level/lines)
    let stats_block = Block::default().borders(Borders::ALL).title(Span::styled(" Stats ", Style::default().fg(Color::Cyan)));
    frame.render_widget(stats_block.clone(), chunks[2]);
    let inner = stats_block.inner(chunks[2]);
    let rows = [
        ("Score", game.score.to_string()),
        ("Level", game.level.to_string()),
        ("Lines", game.total_lines.to_string()),
    ];
    let mut y = inner.y;
    for (k, v) in &rows {
        frame.render_widget(
            Paragraph::new(Span::styled((*k).to_string(), Style::default().fg(Color::Gray))),
            Rect::new(inner.x, y, inner.width, 1),
        );
        let vw = v.len() as u16;
        frame.render_widget(
            Paragraph::new(Span::styled(
                (*v).clone(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right),
            Rect::new(inner.x + inner.width.saturating_sub(vw), y, vw, 1),
        );
        y += 1;
    }

    // Combo / B2B / time
    let extra_block = Block::default().borders(Borders::ALL).title(Span::styled(" Info ", Style::default().fg(Color::Cyan)));
    frame.render_widget(extra_block.clone(), chunks[3]);
    let inner = extra_block.inner(chunks[3]);
    let combo = game.combo.map(|c| c.max(1)).unwrap_or(0);
    let rows = [
        ("Combo", if combo > 1 { format!("{combo}x") } else { "—".into() }),
        ("Back-to-back", if game.back_to_back { "YES" } else { "—" }),
        ("Time", format_playtime(game.start_time.elapsed().as_secs_f64())),
    ];
    let mut y = inner.y;
    for (k, v) in &rows {
        frame.render_widget(
            Paragraph::new(Span::styled((*k).to_string(), Style::default().fg(Color::Gray))),
            Rect::new(inner.x, y, inner.width, 1),
        );
        let vw = v.len() as u16;
        frame.render_widget(
            Paragraph::new(Span::styled(
                (*v).clone(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right),
            Rect::new(inner.x + inner.width.saturating_sub(vw), y, vw, 1),
        );
        y += 1;
    }

    // Transient last-clear banner, if any.
    if let Some(text) = &game.events.last_clear_text {
        let style = if matches!(game.events.last_special, SpecialKind::PerfectClear) {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if game.events.last_special.is_tspin() {
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        };
        let bw = text.len() as u16;
        let banner = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
        frame.render_widget(
            Paragraph::new(Span::styled(text.clone(), style)).alignment(Alignment::Center),
            banner,
        );
        let _ = bw;
    }
}

/// Render a 4x2 mini-preview of a tetromino in a small slot.
fn preview_minipiece(frame: &mut ratatui::Frame, area: Rect, kind: crate::piece::Tetromino, used: bool) {
    let cells = piece_preview_cells(kind);
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;
    for &(x, y) in &cells {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let w = (max_x - min_x + 1) as u16;
    let h = (max_y - min_y + 1) as u16;
    let origin_x = area.x + (area.width.saturating_sub(w * 2)) / 2;
    let origin_y = area.y + (area.height.saturating_sub(h)) / 2;
    let color = tetromino_color(kind);
    let style = if used {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    };

    // First clear the slot area to avoid artifacts.
    frame.render_widget(Clear, area);

    for &(x, y) in &cells {
        let px = origin_x + (x - min_x) as u16 * 2;
        let py = origin_y + (y - min_y) as u16;
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(BLOCK.repeat(2), style))),
            Rect::new(px, py, 2, 1),
        );
    }

    // Decorative dot under empty mini (used hold dims).
    if used {
        for (x, y) in &[(0i32, 0)] {
            let _ = (x, y, origin_x, origin_y);
        }
    }

    let _ = colored;
}

/// Cells in the 4x4 canonical preview for the convenience of the HUD.
fn piece_preview_cells(kind: crate::piece::Tetromino) -> [(i32, i32); 4] {
    use crate::piece::Rotation;
    let piece = ActivePiece {
        kind,
        rot: Rotation::Spawn,
        x: 0,
        y: 0,
    };
    piece.cells()
}

fn render_pause_overlay(frame: &mut ratatui::Frame, menu: &Menu) {
    let area = frame.size();
    let items = ["Resume", "Restart", "Main Menu", "Quit"];
    let selected = menu.selected();
    overlay_menu(frame, area, "Paused", &items, selected, 20, 10);
}

fn render_game_over_overlay(frame: &mut ratatui::Frame, game: &Game, menu: &Menu) {
    let _ = game;
    let area = frame.size();
    let items = ["Play Again", "Main Menu", "Quit"];
    let selected = menu.selected();
    overlay_menu(frame, area, "Game Over", &items, selected, 22, 10);
}

fn render_fps(frame: &mut ratatui::Frame, fps: f64) {
    let text = format!("{fps:.0} FPS");
    frame.render_widget(
        Paragraph::new(Span::styled(
            text,
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Right),
        Rect::new(0, 0, frame.size().width, 1),
    );
}

/// A centered bordered box with `title`; `inner_fn` is called with its inner
/// rect to render content.
fn block_centered<F: FnOnce(Rect)>(frame: &mut ratatui::Frame, area: Rect, w: u16, h: u16, inner_fn: F) {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let rect = Rect::new(x, y, w, h);
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(block.clone(), rect);
    inner_fn(block.inner(rect));
}

fn render_simple_menu(frame: &mut ratatui::Frame, area: Rect, title: &str, items: &[&str], selected: usize) {
    let h = items.len() as u16 + 4;
    let w = 22u16;
    block_centered(frame, area, w, h, |inner| {
        let header = Line::from(Span::styled(
            title.to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
        frame.render_widget(
            Paragraph::new(header).alignment(Alignment::Center),
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
            let tw = text.len() as u16;
            let tx = inner.x + (inner.width.saturating_sub(tw)) / 2;
            frame.render_widget(
                Paragraph::new(Span::styled(text, style)),
                Rect::new(tx, y, tw, 1),
            );
            y += 1;
        }
        let footer_y = inner.bottom_left().y.saturating_sub(1);
        frame.render_widget(
            Paragraph::new(Span::styled(
                "<↑/↓> navigate   <Enter> select   <Esc> back".to_string(),
                Style::default().fg(Color::DarkGray),
            ))
            .alignment(Alignment::Center),
            Rect::new(inner.x, footer_y, inner.width, 1),
        );
    });
}

/// Overlay menu (semi-transparent black backdrop + centered menu box).
fn overlay_menu(frame: &mut ratatui::Frame, area: Rect, title: &str, items: &[&str], selected: usize, w: u16, h: u16) {
    frame.render_widget(Clear, area);
    render_simple_menu_with_title(frame, area, title, items, selected, w, h);
}

fn render_simple_menu_with_title(frame: &mut ratatui::Frame, area: Rect, title: &str, items: &[&str], selected: usize, w: u16, h: u16) {
    block_centered(frame, area, w, h, |inner| {
        let header = Line::from(Span::styled(
            title.to_string(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
        frame.render_widget(
            Paragraph::new(header).alignment(Alignment::Center),
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
            let tw = text.len() as u16;
            let tx = inner.x + (inner.width.saturating_sub(tw)) / 2;
            frame.render_widget(
                Paragraph::new(Span::styled(text, style)),
                Rect::new(tx, y, tw, 1),
            );
            y += 1;
        }
    });
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