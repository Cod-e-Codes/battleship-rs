use anyhow::Result;
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::game_state::GameState;
use crate::types::{CellState, Message};

struct PlayerConnection {
    stream: TcpStream,
    grid: Option<Vec<Vec<CellState>>>,
    ready: bool,
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
                                    game_over = true;
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
                                    game_over = true;
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

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    println!("Game ended");
    Ok(())
}
