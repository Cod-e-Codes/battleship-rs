use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::game_state::GameState;
use crate::types::{CellState, GRID_SIZE, GamePhase, SHIPS};

pub fn draw_ui(f: &mut Frame, state: &GameState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(8),
        ])
        .split(f.area());

    // Title + status line
    let status_text = match state.phase {
        GamePhase::Placing if state.placing_ship_idx < SHIPS.len() => {
            let (len, name) = SHIPS[state.placing_ship_idx];
            format!(
                "Placing: {} (len {}) | Ships left: {}",
                name,
                len,
                SHIPS.len() - state.placing_ship_idx
            )
        }
        _ => format!(
            "Ships placed: {} / {}",
            state.placing_ship_idx.min(SHIPS.len()),
            SHIPS.len()
        ),
    };
    let title = Paragraph::new(format!("🚢 BATTLESHIP 🚢\n{}", status_text))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Game area - adjust layout based on side panel visibility
    let game_area = if state.show_side_panel {
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(17), // Side panel area (half of previous 35%)
                Constraint::Percentage(83), // Main game area
            ])
            .split(chunks[1]);

        let game_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_chunks[1]);

        // Draw side panel first (left side)
        draw_side_panel(f, main_chunks[0], state);

        // Own grid
        draw_grid(
            f,
            game_chunks[0],
            &state.own_grid,
            "Your Fleet",
            state,
            true,
        );
        // Enemy grid
        draw_grid(
            f,
            game_chunks[1],
            &state.enemy_grid,
            "Enemy Waters",
            state,
            false,
        );

        chunks[2] // Return messages area
    } else {
        let game_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        // Own grid
        draw_grid(
            f,
            game_chunks[0],
            &state.own_grid,
            "Your Fleet",
            state,
            true,
        );
        // Enemy grid
        draw_grid(
            f,
            game_chunks[1],
            &state.enemy_grid,
            "Enemy Waters",
            state,
            false,
        );

        chunks[2] // Return messages area
    };

    // Messages
    let msg_items: Vec<ListItem> = state
        .messages
        .iter()
        .rev()
        .take(5)
        .map(|m| ListItem::new(m.clone()))
        .collect();
    let msgs = List::new(msg_items).block(Block::default().borders(Borders::ALL).title("Messages"));
    f.render_widget(msgs, game_area);
}

fn draw_grid(
    f: &mut Frame,
    area: Rect,
    grid: &[Vec<CellState>],
    title: &str,
    state: &GameState,
    is_own: bool,
) {
    // Determine if this grid should be highlighted based on whose turn it is
    let should_highlight = match state.phase {
        GamePhase::YourTurn => !is_own, // Highlight enemy grid when it's your turn
        GamePhase::OpponentTurn => is_own, // Highlight own grid when it's opponent's turn
        _ => false,                     // No highlighting during placing or other phases
    };

    let border_style = if should_highlight {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cell_width = (inner.width.saturating_sub(2)) / (GRID_SIZE as u16 + 1);
    let cell_height = (inner.height.saturating_sub(1)) / (GRID_SIZE as u16 + 1);

    if cell_width < 2 || cell_height < 1 {
        return;
    }

    // Draw grid
    for (y, _row) in grid.iter().enumerate().take(GRID_SIZE) {
        for x in 0..GRID_SIZE {
            let cell_x = inner.x + 1 + (x as u16 + 1) * cell_width;
            let cell_y = inner.y + 1 + (y as u16) * cell_height;

            let cell_rect = Rect::new(cell_x, cell_y, cell_width, cell_height);

            let (symbol, style) = match grid[y][x] {
                CellState::Empty => ("~", Style::default().fg(Color::Blue)),
                CellState::Ship => {
                    if is_own {
                        ("■", Style::default().fg(Color::Green))
                    } else {
                        ("~", Style::default().fg(Color::Blue))
                    }
                }
                CellState::Hit => (
                    "X",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                CellState::Miss => ("·", Style::default().fg(Color::DarkGray)),
            };

            let mut cell_style = style;
            // Show cursor on appropriate grid based on phase
            if state.cursor == (x, y) {
                match state.phase {
                    GamePhase::Placing => {
                        if is_own {
                            cell_style = cell_style.bg(Color::Yellow);
                        }
                    }
                    GamePhase::YourTurn => {
                        if !is_own {
                            cell_style = cell_style.bg(Color::Yellow);
                        }
                    }
                    _ => {}
                }
            }

            // Show preview for ship placement
            if is_own && state.phase == GamePhase::Placing && state.placing_ship_idx < SHIPS.len() {
                let (length, _) = SHIPS[state.placing_ship_idx];
                let (cx, cy) = state.cursor;
                let in_preview =
                    (state.placing_horizontal && y == cy && x >= cx && x < cx + length)
                        || (!state.placing_horizontal && x == cx && y >= cy && y < cy + length);
                if in_preview {
                    let valid = state.can_place_ship(cx, cy, length, state.placing_horizontal);
                    cell_style = if valid {
                        Style::default().fg(Color::LightGreen).bg(Color::DarkGray)
                    } else {
                        Style::default().fg(Color::Red).bg(Color::DarkGray)
                    };
                }
            }

            let cell = Paragraph::new(symbol)
                .style(cell_style)
                .alignment(Alignment::Center);
            f.render_widget(cell, cell_rect);
        }
    }

    // Draw coordinates
    for i in 0..GRID_SIZE {
        let x_label = Paragraph::new(format!("{}", i)).alignment(Alignment::Center);
        let x_rect = Rect::new(
            inner.x + 1 + (i as u16 + 1) * cell_width,
            inner.y,
            cell_width,
            1,
        );
        f.render_widget(x_label, x_rect);

        let y_label = Paragraph::new(format!("{}", i)).alignment(Alignment::Center);
        let y_rect = Rect::new(
            inner.x,
            inner.y + 1 + i as u16 * cell_height,
            cell_width,
            cell_height,
        );
        f.render_widget(y_label, y_rect);
    }
}

fn draw_side_panel(f: &mut Frame, area: Rect, state: &GameState) {
    // Note: Ship status should be updated before drawing
    // This is handled in the client when receiving attack results

    let panel_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12), // Ship status
            Constraint::Length(8),  // Stats
            Constraint::Min(0),     // Spacer
        ])
        .split(area);

    // Ship Status Section
    let ship_lines: Vec<String> = state
        .ship_status
        .iter()
        .map(|ship| {
            let ship_visual = "■".repeat(ship.length);

            if ship.sunk {
                format!("{}  ~~{}~~", ship_visual, ship.name)
            } else {
                format!("{}  {}", ship_visual, ship.name)
            }
        })
        .collect();

    let ship_status_text = ship_lines.join("\n");
    let ship_block = Block::default()
        .borders(Borders::ALL)
        .title("🚢 Your Fleet")
        .title_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );

    let ship_para = Paragraph::new(ship_status_text)
        .style(Style::default().fg(Color::White))
        .block(ship_block);
    f.render_widget(ship_para, panel_chunks[0]);

    // Stats Section
    let accuracy = state.get_accuracy();
    let avg_time = state.get_avg_turn_time();
    let ships_sunk = state.get_ships_sunk();

    let stats_text = format!(
        "Turns: {} | Avg Time: {:.1}s\n\
        Accuracy: {:.0}% | Sunk: {}/5\n\
        Shots: {} | Hits: {}",
        state.turn_count, avg_time, accuracy, ships_sunk, state.total_shots, state.total_hits
    );

    let stats_block = Block::default()
        .borders(Borders::ALL)
        .title("📊 Statistics")
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let stats_para = Paragraph::new(stats_text)
        .style(Style::default().fg(Color::White))
        .block(stats_block);
    f.render_widget(stats_para, panel_chunks[1]);

    // Help text
    let help_text = "Press 'S' to toggle\nthis side panel";
    let help_para = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help_para, panel_chunks[2]);
}
