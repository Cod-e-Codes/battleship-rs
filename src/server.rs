use anyhow::Result;
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::game_state::GameState;
use crate::types::{CellState, Message};

struct PlayerConnection {
    stream: TcpStream,
    grid: Option<Vec<Vec<CellState>>>,
    ready: bool,
}

#[derive(Debug)]
enum PlayAgainState {
    None,
    WaitingForResponses {
        p1_response: Option<bool>,
        p2_response: Option<bool>,
        timeout_start: Instant,
    },
    Timeout,
    BothAgreed,
    OneDeclined,
}

pub async fn run_server(port: &str) -> Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
    listener.set_nonblocking(true)?;
    println!("🚢 Battleship Server listening on port {}", port);
    println!("Waiting for 2 players to connect...\n");

    let shutdown = Arc::new(Mutex::new(false));
    let shutdown_flag = shutdown.clone();

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        *shutdown_flag.lock().unwrap() = true;
        println!("\nShutting down server...");
    });

    // Wait for two players
    let mut players: Vec<TcpStream> = Vec::new();

    while players.len() < 2 {
        if *shutdown.lock().unwrap() {
            return Ok(());
        }

        match listener.accept() {
            Ok((stream, addr)) => {
                stream.set_nonblocking(true)?;
                println!("Player {} connected: {}", players.len() + 1, addr);
                players.push(stream);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => {
                eprintln!("Accept error: {}", e);
            }
        }
    }

    println!("\n2 players connected! Starting game...\n");

    // Create player connections
    let mut p1 = PlayerConnection {
        stream: players.remove(0),
        grid: None,
        ready: false,
    };
    let mut p2 = PlayerConnection {
        stream: players.remove(0),
        grid: None,
        ready: false,
    };

    let mut p1_reader = BufReader::new(p1.stream.try_clone()?);
    let mut p2_reader = BufReader::new(p2.stream.try_clone()?);

    // Game loop
    let mut current_turn = 0; // 0 = player 1, 1 = player 2
    let mut game_over = false;
    let mut play_again_state = PlayAgainState::None;

    while !game_over && !*shutdown.lock().unwrap() {
        // Read from both players
        let mut line = String::new();

        // Check player 1
        match p1_reader.read_line(&mut line) {
            Ok(0) => {
                println!("Player 1 disconnected");
                break;
            }
            Ok(_) => {
                if let Ok(msg) = serde_json::from_str::<Message>(&line) {
                    match msg {
                        Message::PlaceShips(grid) => {
                            p1.grid = Some(grid);
                            p1.ready = true;
                            println!("Player 1 placed ships");

                            if p2.ready {
                                // Both ready, start game
                                writeln!(
                                    p1.stream,
                                    "{}",
                                    serde_json::to_string(&Message::GameStart)?
                                )?;
                                writeln!(
                                    p2.stream,
                                    "{}",
                                    serde_json::to_string(&Message::GameStart)?
                                )?;
                                writeln!(
                                    p1.stream,
                                    "{}",
                                    serde_json::to_string(&Message::YourTurn)?
                                )?;
                                writeln!(
                                    p2.stream,
                                    "{}",
                                    serde_json::to_string(&Message::OpponentTurn)?
                                )?;
                                println!("Game started! Player 1's turn\n");
                            } else {
                                writeln!(
                                    p1.stream,
                                    "{}",
                                    serde_json::to_string(&Message::WaitingForOpponent)?
                                )?;
                            }
                        }
                        Message::Attack { x, y } if current_turn == 0 && p1.ready && p2.ready => {
                            // Player 1 attacks player 2
                            if let Some(ref mut grid) = p2.grid {
                                let hit = grid[y][x] == CellState::Ship;
                                if hit {
                                    grid[y][x] = CellState::Hit;
                                }
                                let sunk = if hit {
                                    GameState::is_ship_sunk_at(grid, x, y)
                                } else {
                                    false
                                };

                                // Send result to player 1
                                writeln!(
                                    p1.stream,
                                    "{}",
                                    serde_json::to_string(&Message::AttackResult {
                                        x,
                                        y,
                                        hit,
                                        sunk
                                    })?
                                )?;

                                // Send attack to player 2
                                writeln!(
                                    p2.stream,
                                    "{}",
                                    serde_json::to_string(&Message::Attack { x, y })?
                                )?;

                                println!(
                                    "Player 1 attacked ({}, {}) - {}",
                                    x,
                                    y,
                                    if hit { "HIT" } else { "MISS" }
                                );

                                // Check if player 2 lost
                                if GameState::all_ships_sunk(grid) {
                                    writeln!(
                                        p1.stream,
                                        "{}",
                                        serde_json::to_string(&Message::GameOver { won: true })?
                                    )?;
                                    writeln!(
                                        p2.stream,
                                        "{}",
                                        serde_json::to_string(&Message::GameOver { won: false })?
                                    )?;
                                    println!("\n🎉 Player 1 wins!");

                                    // Start play again process
                                    play_again_state = PlayAgainState::WaitingForResponses {
                                        p1_response: None,
                                        p2_response: None,
                                        timeout_start: Instant::now(),
                                    };
                                    writeln!(
                                        p1.stream,
                                        "{}",
                                        serde_json::to_string(&Message::PlayAgainRequest)?
                                    )?;
                                    writeln!(
                                        p2.stream,
                                        "{}",
                                        serde_json::to_string(&Message::PlayAgainRequest)?
                                    )?;
                                    println!("Asking both players if they want to play again...");
                                } else {
                                    // Switch turn
                                    current_turn = 1;
                                    writeln!(
                                        p1.stream,
                                        "{}",
                                        serde_json::to_string(&Message::OpponentTurn)?
                                    )?;
                                    writeln!(
                                        p2.stream,
                                        "{}",
                                        serde_json::to_string(&Message::YourTurn)?
                                    )?;
                                    println!("Player 2's turn\n");
                                }
                            }
                        }
                        Message::PlayAgainResponse { wants_to_play } => {
                            if let PlayAgainState::WaitingForResponses {
                                p1_response,
                                p2_response,
                                ..
                            } = &mut play_again_state
                            {
                                *p1_response = Some(wants_to_play);
                                println!("Player 1 play again response: {}", wants_to_play);

                                // Check if both players responded
                                if let (Some(p1_resp), Some(p2_resp)) = (p1_response, p2_response) {
                                    if *p1_resp && *p2_resp {
                                        play_again_state = PlayAgainState::BothAgreed;
                                    } else {
                                        play_again_state = PlayAgainState::OneDeclined;
                                    }
                                }
                            }
                        }
                        Message::Quit => {
                            println!("Player 1 quit the game");
                            let _ = writeln!(
                                p2.stream,
                                "{}",
                                serde_json::to_string(&Message::OpponentQuit)?
                            );
                            game_over = true;
                        }
                        _ => {}
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => {
                println!("Player 1 connection error");
                break;
            }
        }

        // Check player 2
        line.clear();
        match p2_reader.read_line(&mut line) {
            Ok(0) => {
                println!("Player 2 disconnected");
                break;
            }
            Ok(_) => {
                if let Ok(msg) = serde_json::from_str::<Message>(&line) {
                    match msg {
                        Message::PlaceShips(grid) => {
                            p2.grid = Some(grid);
                            p2.ready = true;
                            println!("Player 2 placed ships");

                            if p1.ready {
                                // Both ready, start game
                                writeln!(
                                    p1.stream,
                                    "{}",
                                    serde_json::to_string(&Message::GameStart)?
                                )?;
                                writeln!(
                                    p2.stream,
                                    "{}",
                                    serde_json::to_string(&Message::GameStart)?
                                )?;
                                writeln!(
                                    p1.stream,
                                    "{}",
                                    serde_json::to_string(&Message::YourTurn)?
                                )?;
                                writeln!(
                                    p2.stream,
                                    "{}",
                                    serde_json::to_string(&Message::OpponentTurn)?
                                )?;
                                println!("Game started! Player 1's turn\n");
                            } else {
                                writeln!(
                                    p2.stream,
                                    "{}",
                                    serde_json::to_string(&Message::WaitingForOpponent)?
                                )?;
                            }
                        }
                        Message::Attack { x, y } if current_turn == 1 && p1.ready && p2.ready => {
                            // Player 2 attacks player 1
                            if let Some(ref mut grid) = p1.grid {
                                let hit = grid[y][x] == CellState::Ship;
                                if hit {
                                    grid[y][x] = CellState::Hit;
                                }
                                let sunk = if hit {
                                    GameState::is_ship_sunk_at(grid, x, y)
                                } else {
                                    false
                                };

                                // Send result to player 2
                                writeln!(
                                    p2.stream,
                                    "{}",
                                    serde_json::to_string(&Message::AttackResult {
                                        x,
                                        y,
                                        hit,
                                        sunk
                                    })?
                                )?;

                                // Send attack to player 1
                                writeln!(
                                    p1.stream,
                                    "{}",
                                    serde_json::to_string(&Message::Attack { x, y })?
                                )?;

                                println!(
                                    "Player 2 attacked ({}, {}) - {}",
                                    x,
                                    y,
                                    if hit { "HIT" } else { "MISS" }
                                );

                                // Check if player 1 lost
                                if GameState::all_ships_sunk(grid) {
                                    writeln!(
                                        p1.stream,
                                        "{}",
                                        serde_json::to_string(&Message::GameOver { won: false })?
                                    )?;
                                    writeln!(
                                        p2.stream,
                                        "{}",
                                        serde_json::to_string(&Message::GameOver { won: true })?
                                    )?;
                                    println!("\n🎉 Player 2 wins!");

                                    // Start play again process
                                    play_again_state = PlayAgainState::WaitingForResponses {
                                        p1_response: None,
                                        p2_response: None,
                                        timeout_start: Instant::now(),
                                    };
                                    writeln!(
                                        p1.stream,
                                        "{}",
                                        serde_json::to_string(&Message::PlayAgainRequest)?
                                    )?;
                                    writeln!(
                                        p2.stream,
                                        "{}",
                                        serde_json::to_string(&Message::PlayAgainRequest)?
                                    )?;
                                    println!("Asking both players if they want to play again...");
                                } else {
                                    // Switch turn
                                    current_turn = 0;
                                    writeln!(
                                        p1.stream,
                                        "{}",
                                        serde_json::to_string(&Message::YourTurn)?
                                    )?;
                                    writeln!(
                                        p2.stream,
                                        "{}",
                                        serde_json::to_string(&Message::OpponentTurn)?
                                    )?;
                                    println!("Player 1's turn\n");
                                }
                            }
                        }
                        Message::PlayAgainResponse { wants_to_play } => {
                            if let PlayAgainState::WaitingForResponses {
                                p1_response,
                                p2_response,
                                ..
                            } = &mut play_again_state
                            {
                                *p2_response = Some(wants_to_play);
                                println!("Player 2 play again response: {}", wants_to_play);

                                // Check if both players responded
                                if let (Some(p1_resp), Some(p2_resp)) = (p1_response, p2_response) {
                                    if *p1_resp && *p2_resp {
                                        play_again_state = PlayAgainState::BothAgreed;
                                    } else {
                                        play_again_state = PlayAgainState::OneDeclined;
                                    }
                                }
                            }
                        }
                        Message::Quit => {
                            println!("Player 2 quit the game");
                            let _ = writeln!(
                                p1.stream,
                                "{}",
                                serde_json::to_string(&Message::OpponentQuit)?
                            );
                            game_over = true;
                        }
                        _ => {}
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => {
                println!("Player 2 connection error");
                break;
            }
        }

        // Handle play again state transitions
        match &mut play_again_state {
            PlayAgainState::WaitingForResponses { timeout_start, .. } => {
                if timeout_start.elapsed() > Duration::from_secs(30) {
                    println!("Play again timeout - no response from one or both players");
                    play_again_state = PlayAgainState::Timeout;
                }
            }
            PlayAgainState::BothAgreed => {
                println!("Both players want to play again! Starting new game...");

                // Reset game state
                p1.grid = None;
                p1.ready = false;
                p2.grid = None;
                p2.ready = false;
                current_turn = 0;
                play_again_state = PlayAgainState::None;

                // Notify both players that new game is starting
                let _ = writeln!(
                    p1.stream,
                    "{}",
                    serde_json::to_string(&Message::NewGameStart)?
                );
                let _ = writeln!(
                    p2.stream,
                    "{}",
                    serde_json::to_string(&Message::NewGameStart)?
                );

                println!("New game ready! Waiting for players to place ships...");
            }
            PlayAgainState::OneDeclined => {
                println!("One player declined to play again. Ending session.");
                game_over = true;
            }
            PlayAgainState::Timeout => {
                println!("Play again timeout reached. Ending session.");
                game_over = true;
            }
            PlayAgainState::None => {}
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    println!("Game ended");
    Ok(())
}
