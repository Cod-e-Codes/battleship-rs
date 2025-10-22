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
    // Don't set nonblocking on the read stream
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    writer.set_nonblocking(true)?;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let state = Arc::new(Mutex::new(GameState::new()));
    let state_clone = state.clone();

    // Network receiver thread - use blocking reads
    tokio::spawn(async move {
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    println!("Server disconnected");
                    break;
                }
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
                                state.messages.push("Your turn!".to_string());
                            }
                            Message::OpponentTurn => {
                                state.end_turn();
                                state.phase = GamePhase::OpponentTurn;
                                state.messages.push("Opponent's turn...".to_string());
                            }
                            Message::Attack { x, y } => {
                                let hit = state.own_grid[y][x] == CellState::Ship;
                                state.own_grid[y][x] =
                                    if hit { CellState::Hit } else { CellState::Miss };
                                if hit {
                                    state.messages.push(format!(
                                        "Enemy hit your ship at {}!",
                                        crate::game_state::GameState::format_coordinate(x, y)
                                    ));
                                } else {
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
                            Message::Quit => {
                                state.messages.push("You have quit the game.".to_string());
                                state.phase = GamePhase::GameOver;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Read error: {}", e);
                    break;
                }
            }
        }
    });

    // Network sender
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let json = serde_json::to_string(&msg).unwrap() + "\n";
            match writer.write_all(json.as_bytes()) {
                Ok(_) => {
                    let _ = writer.flush();
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    let _ = writer.write_all(json.as_bytes());
                    let _ = writer.flush();
                }
                Err(e) => {
                    eprintln!("Write error: {}", e);
                    break;
                }
            }
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
