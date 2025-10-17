use crate::game_state::GameState;
use crate::types::{CellState, GRID_SIZE, GamePhase, Message, SHIPS};
use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc;

pub fn handle_key_event(
    state: &mut GameState,
    key: KeyEvent,
    tx: &mpsc::UnboundedSender<Message>,
) -> bool {
    match state.phase {
        GamePhase::Placing => match key.code {
            KeyCode::Up => state.cursor.1 = state.cursor.1.saturating_sub(1),
            KeyCode::Down => state.cursor.1 = (state.cursor.1 + 1).min(GRID_SIZE - 1),
            KeyCode::Left => state.cursor.0 = state.cursor.0.saturating_sub(1),
            KeyCode::Right => state.cursor.0 = (state.cursor.0 + 1).min(GRID_SIZE - 1),
            KeyCode::Char('r') | KeyCode::Char('R') => {
                state.placing_horizontal = !state.placing_horizontal;
            }
            KeyCode::Enter => {
                if state.placing_ship_idx < SHIPS.len() {
                    let (length, name) = SHIPS[state.placing_ship_idx];
                    let (x, y) = state.cursor;
                    if state.can_place_ship(x, y, length, state.placing_horizontal) {
                        state.place_ship(x, y, length, state.placing_horizontal);
                        state.messages.push(format!("{} placed!", name));
                        state.placing_ship_idx += 1;

                        if state.placing_ship_idx >= SHIPS.len() {
                            state
                                .messages
                                .push("All ships placed! Waiting for opponent...".to_string());
                            state.phase = GamePhase::WaitingForOpponent;
                            let _ = tx.send(Message::PlaceShips(state.own_grid.clone()));
                        } else {
                            state.messages.push(format!(
                                "Place {} (length {})",
                                SHIPS[state.placing_ship_idx].1, SHIPS[state.placing_ship_idx].0
                            ));
                        }
                    }
                }
            }
            KeyCode::Char('q') => return true,
            _ => {}
        },
        GamePhase::YourTurn => match key.code {
            KeyCode::Up => state.cursor.1 = state.cursor.1.saturating_sub(1),
            KeyCode::Down => state.cursor.1 = (state.cursor.1 + 1).min(GRID_SIZE - 1),
            KeyCode::Left => state.cursor.0 = state.cursor.0.saturating_sub(1),
            KeyCode::Right => state.cursor.0 = (state.cursor.0 + 1).min(GRID_SIZE - 1),
            KeyCode::Enter => {
                let (x, y) = state.cursor;
                if state.enemy_grid[y][x] == CellState::Empty {
                    let _ = tx.send(Message::Attack { x, y });
                    state.phase = GamePhase::OpponentTurn;
                    state.messages.push(format!("Firing at ({}, {})...", x, y));
                }
            }
            KeyCode::Char('q') => return true,
            _ => {}
        },
        GamePhase::GameOver => {
            if key.code == KeyCode::Char('q') {
                return true;
            }
        }
        GamePhase::WaitingForOpponent | GamePhase::OpponentTurn => {
            if key.code == KeyCode::Char('q') {
                return true;
            }
        }
    }
    false
}
