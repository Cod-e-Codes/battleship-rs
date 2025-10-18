use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io::{self, BufRead, BufReader, Write},
    net::TcpStream,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;

use crate::game_state::GameState;
use crate::input::handle_key_event;
use crate::types::{CellState, GamePhase, Message};
use crate::ui::draw_ui;

pub async fn run_client(addr: &str) -> Result<()> {
    let stream = TcpStream::connect(addr)?;
    stream.set_nonblocking(true)?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let state = Arc::new(Mutex::new(GameState::new()));
    let state_clone = state.clone();

    // Network receiver thread
    tokio::spawn(async move {
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if let Ok(msg) = serde_json::from_str::<Message>(&line) {
                        let mut state = state_clone.lock().unwrap();
                        match msg {
                            Message::WaitingForOpponent => {
                                state
                                    .messages
                                    .push("Waiting for opponent to place ships...".to_string());
                            }
                            Message::GameStart => {
                                state.messages.push("Game starting!".to_string());
                            }
                            Message::YourTurn => {
                                state.phase = GamePhase::YourTurn;
                                state.turn_count += 1;
                                state.start_turn();

                                // Deactivate shield after one turn
                                if state.shield_active {
                                    state.deactivate_shield();
                                }

                                state.messages.push("Your turn!".to_string());
                            }
                            Message::OpponentTurn => {
                                state.end_turn();
                                state.phase = GamePhase::OpponentTurn;

                                // Clear radar reveals after one turn
                                if !state.radar_reveals.is_empty() {
                                    state.clear_radar_reveals();
                                }

                                state.messages.push("Opponent's turn...".to_string());
                            }
                            Message::Attack { x, y } => {
                                let hit = state.own_grid[y][x] == CellState::Ship;
                                let mut actual_hit = hit;

                                // Check if shield is active and this would be a hit
                                if hit && state.shield_active {
                                    // 50% chance to block damage with shield
                                    use rand::Rng;
                                    let mut rng = rand::rng();
                                    if rng.random_range(0..2) == 0 {
                                        // Shield blocked the hit
                                        actual_hit = false; // Treat as miss for game logic
                                        state.messages.push(format!(
                                            "🛡️ Shield blocked enemy attack at {}!",
                                            crate::game_state::GameState::format_coordinate(x, y)
                                        ));
                                    }
                                }

                                state.own_grid[y][x] = if actual_hit {
                                    CellState::Hit
                                } else {
                                    CellState::Miss
                                };
                                if actual_hit {
                                    state.messages.push(format!(
                                        "Enemy hit your ship at {}!",
                                        crate::game_state::GameState::format_coordinate(x, y)
                                    ));
                                } else if !hit {
                                    // Only show "missed" message if it wasn't a shield block
                                    state.messages.push(format!(
                                        "Enemy missed at {}",
                                        crate::game_state::GameState::format_coordinate(x, y)
                                    ));
                                }
                            }
                            Message::AttackResult { x, y, hit, sunk } => {
                                state.enemy_grid[y][x] =
                                    if hit { CellState::Hit } else { CellState::Miss };
                                state.record_shot(hit);
                                state.update_ship_status();

                                if hit {
                                    state.messages.push(if sunk {
                                        format!(
                                            "HIT at {}! Ship sunk!",
                                            crate::game_state::GameState::format_coordinate(x, y)
                                        )
                                    } else {
                                        format!(
                                            "HIT at {}!",
                                            crate::game_state::GameState::format_coordinate(x, y)
                                        )
                                    });

                                    // Draw a card for hitting/sinking
                                    if let Some(card) = state.draw_card() {
                                        state
                                            .messages
                                            .push(format!("🃏 Drew {} card!", card.name()));
                                    }
                                } else {
                                    state.messages.push(format!(
                                        "Miss at {}",
                                        crate::game_state::GameState::format_coordinate(x, y)
                                    ));
                                }
                            }
                            Message::GameOver { won } => {
                                state.phase = GamePhase::GameOver;
                                state.winner = Some(won);
                                state.messages.push(if won {
                                    "🎉 YOU WIN! 🎉".to_string()
                                } else {
                                    "💀 YOU LOSE! 💀".to_string()
                                });
                            }
                            Message::PlayAgainRequest => {
                                state.phase = GamePhase::PlayAgainPrompt;
                                state
                                    .messages
                                    .push("Do you want to play again? (Y/N)".to_string());
                            }
                            Message::PlayAgainResponse { wants_to_play } => {
                                if wants_to_play {
                                    state
                                        .messages
                                        .push("Opponent wants to play again!".to_string());
                                } else {
                                    state
                                        .messages
                                        .push("Opponent doesn't want to play again.".to_string());
                                }
                            }
                            Message::PlayAgainTimeout => {
                                state
                                    .messages
                                    .push("Play again timeout - ending game.".to_string());
                            }
                            Message::OpponentQuit => {
                                state
                                    .messages
                                    .push("Opponent has quit the game.".to_string());
                                state.phase = GamePhase::GameOver;
                            }
                            Message::NewGameStart => {
                                state.reset_for_new_game();
                                state
                                    .messages
                                    .push("New game starting! Place your ships.".to_string());
                            }
                            Message::LastStandTrigger => {
                                state.phase = GamePhase::LastStand;
                                state.trigger_last_stand();
                                state.messages.push("⚡ LAST STAND CHANCE! ⚡".to_string());
                            }
                            Message::LastStandResult { success } => {
                                if success {
                                    state
                                        .messages
                                        .push("Last Stand successful! Ship restored!".to_string());
                                    // Server will send updated grid state
                                } else {
                                    state
                                        .messages
                                        .push("Last Stand failed! Game over!".to_string());
                                    state.phase = GamePhase::GameOver;
                                }
                            }
                            Message::CardDrawn { card } => {
                                state.current_hand.push(card.clone());
                                state
                                    .messages
                                    .push(format!("🃏 Drew {} card!", card.name()));
                            }
                            Message::CardEffect { effect_type, data } => {
                                match effect_type.as_str() {
                                    "shield_activated" => {
                                        state.activate_shield();
                                    }
                                    "shield_expired" => {
                                        state.deactivate_shield();
                                    }
                                    "radar_reveal" => {
                                        // Parse radar reveal data
                                        if let Some((x, y)) = data.split_once(',')
                                            && let (Ok(x), Ok(y)) =
                                                (x.parse::<usize>(), y.parse::<usize>())
                                        {
                                            state.radar_reveals.push((x, y));
                                            state.messages.push(format!(
                                                "📡 Radar revealed ship at {}!",
                                                crate::game_state::GameState::format_coordinate(
                                                    x, y
                                                )
                                            ));
                                        }
                                    }
                                    "missile_strike" => {
                                        // Parse missile strike data (format: "x,y,result")
                                        // Temporary debug: log to file instead of stdout
                                        use std::fs::OpenOptions;
                                        use std::io::Write;
                                        if let Ok(mut file) = OpenOptions::new()
                                            .create(true)
                                            .append(true)
                                            .open("debug.log")
                                        {
                                            let _ = writeln!(file, "Missile data: '{}'", data);
                                        }
                                        if let Some((xy, result)) = data.split_once(',')
                                            && let Some((x_str, y_str)) = xy.split_once(',')
                                            && let (Ok(x), Ok(y)) =
                                                (x_str.parse::<usize>(), y_str.parse::<usize>())
                                        {
                                            // Debug: log successful parsing
                                            use std::fs::OpenOptions;
                                            use std::io::Write;
                                            if let Ok(mut file) = OpenOptions::new()
                                                .create(true)
                                                .append(true)
                                                .open("debug.log")
                                            {
                                                let _ = writeln!(
                                                    file,
                                                    "Parsed: x={}, y={}, result={}",
                                                    x, y, result
                                                );
                                            }
                                            // Update enemy grid based on result
                                            state.enemy_grid[y][x] = if result == "hit" {
                                                CellState::Hit
                                            } else {
                                                CellState::Miss
                                            };

                                            let result_text =
                                                if result == "hit" { "hit" } else { "missed" };
                                            state.messages.push(format!(
                                                "🚀 Missile strike {} at {}!",
                                                result_text,
                                                crate::game_state::GameState::format_coordinate(
                                                    x, y
                                                )
                                            ));
                                        }
                                    }
                                    "repair" => {
                                        if state.repair_ship() {
                                            state.messages.push("🔧 Ship repaired!".to_string());
                                        } else {
                                            state
                                                .messages
                                                .push("🔧 No damaged ships to repair!".to_string());
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            Message::GridUpdate { grid } => {
                                // Update own grid (for missile strikes)
                                state.own_grid = grid;
                                state.messages.push("Grid updated!".to_string());
                            }
                            Message::Quit => {
                                state.messages.push("You have quit the game.".to_string());
                                state.phase = GamePhase::GameOver;
                            }
                            _ => {}
                        }
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(_) => break,
            }
        }
    });

    // Network sender
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let json = serde_json::to_string(&msg).unwrap() + "\n";
            let _ = writer.write_all(json.as_bytes());
        }
    });

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| {
            let state = state.lock().unwrap();
            draw_ui(f, &state);
        })?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            let should_quit = {
                let mut state = state.lock().unwrap();
                handle_key_event(&mut state, key, &tx)
            };
            if should_quit {
                break;
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
