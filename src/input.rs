use crate::game_state::GameState;
use crate::types::{CellState, GRID_SIZE, GamePhase, Message, SHIPS, SidePanelMode};
use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc;

pub fn handle_key_event(
    state: &mut GameState,
    key: KeyEvent,
    tx: &mpsc::UnboundedSender<Message>,
) -> bool {
    match state.phase {
        GamePhase::Placing => match key.code {
            KeyCode::Up => {
                state.cursor.1 = state.cursor.1.saturating_sub(1);
            }
            KeyCode::Down => {
                let max_y = if state.placing_ship_idx < SHIPS.len() && !state.placing_horizontal {
                    let (length, _) = SHIPS[state.placing_ship_idx];
                    GRID_SIZE.saturating_sub(length)
                } else {
                    GRID_SIZE - 1
                };
                state.cursor.1 = (state.cursor.1 + 1).min(max_y);
            }
            KeyCode::Left => {
                state.cursor.0 = state.cursor.0.saturating_sub(1);
            }
            KeyCode::Right => {
                let max_x = if state.placing_ship_idx < SHIPS.len() && state.placing_horizontal {
                    let (length, _) = SHIPS[state.placing_ship_idx];
                    GRID_SIZE.saturating_sub(length)
                } else {
                    GRID_SIZE - 1
                };
                state.cursor.0 = (state.cursor.0 + 1).min(max_x);
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                state.placing_horizontal = !state.placing_horizontal;

                // Adjust cursor if rotation would put ship out of bounds
                if state.placing_ship_idx < SHIPS.len() {
                    let (length, _) = SHIPS[state.placing_ship_idx];
                    if state.placing_horizontal {
                        // Now horizontal - check if ship would extend beyond right edge
                        if state.cursor.0 + length > GRID_SIZE {
                            state.cursor.0 = GRID_SIZE.saturating_sub(length);
                        }
                    } else {
                        // Now vertical - check if ship would extend beyond bottom edge
                        if state.cursor.1 + length > GRID_SIZE {
                            state.cursor.1 = GRID_SIZE.saturating_sub(length);
                        }
                    }
                }
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
            KeyCode::Char('q') => {
                let _ = tx.send(Message::Quit);
                return true;
            }
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
                    state.messages.push(format!(
                        "Firing at {}...",
                        crate::game_state::GameState::format_coordinate(x, y)
                    ));
                }
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                state.side_panel_mode = match state.side_panel_mode {
                    SidePanelMode::Statistics => SidePanelMode::Deck,
                    SidePanelMode::Deck => SidePanelMode::Statistics,
                    SidePanelMode::Hidden => SidePanelMode::Statistics,
                };
            }
            // Card usage (1-5 keys)
            KeyCode::Char('1') => {
                if state.can_use_card(0)
                    && let Some(card) = state.use_card(0)
                {
                    let _ = tx.send(Message::CardUsed {
                        card,
                        target_x: None,
                        target_y: None,
                    });
                }
            }
            KeyCode::Char('2') => {
                if state.can_use_card(1)
                    && let Some(card) = state.use_card(1)
                {
                    let _ = tx.send(Message::CardUsed {
                        card,
                        target_x: None,
                        target_y: None,
                    });
                }
            }
            KeyCode::Char('3') => {
                if state.can_use_card(2)
                    && let Some(card) = state.use_card(2)
                {
                    let _ = tx.send(Message::CardUsed {
                        card,
                        target_x: None,
                        target_y: None,
                    });
                }
            }
            KeyCode::Char('4') => {
                if state.can_use_card(3)
                    && let Some(card) = state.use_card(3)
                {
                    let _ = tx.send(Message::CardUsed {
                        card,
                        target_x: None,
                        target_y: None,
                    });
                }
            }
            KeyCode::Char('5') => {
                if state.can_use_card(4)
                    && let Some(card) = state.use_card(4)
                {
                    let _ = tx.send(Message::CardUsed {
                        card,
                        target_x: None,
                        target_y: None,
                    });
                }
            }
            KeyCode::Char('q') => {
                let _ = tx.send(Message::Quit);
                return true;
            }
            _ => {}
        },
        GamePhase::GameOver => {
            if key.code == KeyCode::Char('q') {
                let _ = tx.send(Message::Quit);
                return true;
            }
        }
        GamePhase::PlayAgainPrompt => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let _ = tx.send(Message::PlayAgainResponse {
                    wants_to_play: true,
                });
                state.messages.push("You chose to play again!".to_string());
                state.phase = GamePhase::GameOver; // Will be reset by server if both agree
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                let _ = tx.send(Message::PlayAgainResponse {
                    wants_to_play: false,
                });
                state
                    .messages
                    .push("You chose not to play again.".to_string());
                state.phase = GamePhase::GameOver;
            }
            KeyCode::Char('q') => {
                let _ = tx.send(Message::Quit);
                return true;
            }
            _ => {}
        },
        GamePhase::LastStand => match key.code {
            KeyCode::Char('.') => {
                if let Some(result) = state.check_last_stand_input('.') {
                    let _ = tx.send(Message::LastStandResult { success: result });
                    if result {
                        state
                            .messages
                            .push("Last Stand successful! Ship restored!".to_string());
                    } else {
                        state
                            .messages
                            .push("Last Stand failed! Game over!".to_string());
                    }
                }
            }
            KeyCode::Char('-') => {
                if let Some(result) = state.check_last_stand_input('-') {
                    let _ = tx.send(Message::LastStandResult { success: result });
                    if result {
                        state
                            .messages
                            .push("Last Stand successful! Ship restored!".to_string());
                    } else {
                        state
                            .messages
                            .push("Last Stand failed! Game over!".to_string());
                    }
                }
            }
            KeyCode::Char('q') => {
                let _ = tx.send(Message::Quit);
                return true;
            }
            _ => {}
        },
        GamePhase::WaitingForOpponent | GamePhase::OpponentTurn => match key.code {
            KeyCode::Char('s') | KeyCode::Char('S') => {
                state.side_panel_mode = match state.side_panel_mode {
                    SidePanelMode::Statistics => SidePanelMode::Deck,
                    SidePanelMode::Deck => SidePanelMode::Statistics,
                    SidePanelMode::Hidden => SidePanelMode::Statistics,
                };
            }
            KeyCode::Char('q') => {
                let _ = tx.send(Message::Quit);
                return true;
            }
            _ => {}
        },
    }
    false
}
