use anyhow::Result;
use rand::Rng;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::game_state::GameState;
use crate::types::{CellState, GRID_SIZE, Message, SHIPS};

pub async fn run_server_ai(port: &str) -> Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
    listener.set_nonblocking(true)?;
    println!("🤖 AI Battleship Server listening on port {}", port);

    let shutdown = Arc::new(Mutex::new(false));
    let shutdown_flag = shutdown.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        *shutdown_flag.lock().unwrap() = true;
        println!("\nShutting down AI server...");
    });

    // Accept one client and play against it
    let (mut stream, addr) = loop {
        if *shutdown.lock().unwrap() {
            return Ok(());
        }
        match listener.accept() {
            Ok((s, a)) => {
                s.set_nonblocking(true)?;
                break (s, a);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => {
                eprintln!("Accept error: {}", e);
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    };
    println!("Client connected: {}", addr);

    let mut reader = BufReader::new(stream.try_clone()?);

    // Generate AI's board
    let mut ai_grid = vec![vec![CellState::Empty; GRID_SIZE]; GRID_SIZE];
    let mut rng = rand::rng();

    for (len, _name) in SHIPS {
        'place: loop {
            let x = rng.random_range(0..GRID_SIZE);
            let y = rng.random_range(0..GRID_SIZE);
            let horiz = rng.random_bool(0.5);

            let can = if horiz {
                if x + len > GRID_SIZE {
                    false
                } else {
                    (0..len).all(|i| ai_grid[y][x + i] == CellState::Empty)
                }
            } else if y + len > GRID_SIZE {
                false
            } else {
                (0..len).all(|i| ai_grid[y + i][x] == CellState::Empty)
            };

            if can {
                if horiz {
                    for i in 0..len {
                        ai_grid[y][x + i] = CellState::Ship;
                    }
                } else {
                    for i in 0..len {
                        ai_grid[y + i][x] = CellState::Ship;
                    }
                }
                break 'place;
            }
        }
    }

    let mut player_grid: Option<Vec<Vec<CellState>>> = None;
    let mut ai_fired = vec![vec![false; GRID_SIZE]; GRID_SIZE];

    let mut line = String::new();
    loop {
        if *shutdown.lock().unwrap() {
            break;
        }

        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if let Ok(msg) = serde_json::from_str::<Message>(&line) {
                    match msg {
                        Message::Attack { x, y } => {
                            // Player fired at AI
                            let hit = ai_grid[y][x] == CellState::Ship;
                            if hit {
                                ai_grid[y][x] = CellState::Hit;
                            }
                            let sunk = if hit {
                                GameState::is_ship_sunk_at(&ai_grid, x, y)
                            } else {
                                false
                            };

                            let reply = Message::AttackResult { x, y, hit, sunk };
                            writeln!(stream, "{}", serde_json::to_string(&reply)?)?;

                            // Check if all AI ships are sunk
                            if GameState::all_ships_sunk(&ai_grid) {
                                writeln!(
                                    stream,
                                    "{}",
                                    serde_json::to_string(&Message::GameOver { won: true })?
                                )?;
                                println!("Player wins!");

                                // Ask if player wants to play again
                                writeln!(
                                    stream,
                                    "{}",
                                    serde_json::to_string(&Message::PlayAgainRequest)?
                                )?;
                                println!("Asking player if they want to play again...");
                                continue;
                            }

                            // AI's turn
                            if let Some(grid) = player_grid.as_mut() {
                                writeln!(
                                    stream,
                                    "{}",
                                    serde_json::to_string(&Message::OpponentTurn)?
                                )?;

                                // Find untargeted cell
                                let (sx, sy) = loop {
                                    let sx = rng.random_range(0..GRID_SIZE);
                                    let sy = rng.random_range(0..GRID_SIZE);
                                    if !ai_fired[sy][sx] {
                                        break (sx, sy);
                                    }
                                };
                                ai_fired[sy][sx] = true;

                                let ai_hit = grid[sy][sx] == CellState::Ship;
                                if ai_hit {
                                    grid[sy][sx] = CellState::Hit;
                                } else {
                                    grid[sy][sx] = CellState::Miss;
                                }

                                // Send attack to client
                                writeln!(
                                    stream,
                                    "{}",
                                    serde_json::to_string(&Message::Attack { x: sx, y: sy })?
                                )?;

                                // Check if player lost
                                if GameState::all_ships_sunk(grid) {
                                    writeln!(
                                        stream,
                                        "{}",
                                        serde_json::to_string(&Message::GameOver { won: false })?
                                    )?;
                                    println!("AI wins!");

                                    // Ask if player wants to play again
                                    writeln!(
                                        stream,
                                        "{}",
                                        serde_json::to_string(&Message::PlayAgainRequest)?
                                    )?;
                                    println!("Asking player if they want to play again...");
                                    continue;
                                }

                                // Back to player's turn
                                writeln!(stream, "{}", serde_json::to_string(&Message::YourTurn)?)?;
                            }
                        }
                        Message::PlaceShips(client_grid) => {
                            player_grid = Some(client_grid);
                            writeln!(stream, "{}", serde_json::to_string(&Message::GameStart)?)?;
                            writeln!(stream, "{}", serde_json::to_string(&Message::YourTurn)?)?;
                            println!("Game started!");
                        }
                        Message::PlayAgainResponse { wants_to_play } => {
                            if wants_to_play {
                                println!("Player wants to play again! Starting new game...");

                                // Reset AI's board
                                ai_grid = vec![vec![CellState::Empty; GRID_SIZE]; GRID_SIZE];
                                for (len, _name) in SHIPS {
                                    'place: loop {
                                        let x = rng.random_range(0..GRID_SIZE);
                                        let y = rng.random_range(0..GRID_SIZE);
                                        let horiz = rng.random_bool(0.5);

                                        let can = if horiz {
                                            if x + len > GRID_SIZE {
                                                false
                                            } else {
                                                (0..len)
                                                    .all(|i| ai_grid[y][x + i] == CellState::Empty)
                                            }
                                        } else if y + len > GRID_SIZE {
                                            false
                                        } else {
                                            (0..len).all(|i| ai_grid[y + i][x] == CellState::Empty)
                                        };

                                        if can {
                                            if horiz {
                                                for i in 0..len {
                                                    ai_grid[y][x + i] = CellState::Ship;
                                                }
                                            } else {
                                                for i in 0..len {
                                                    ai_grid[y + i][x] = CellState::Ship;
                                                }
                                            }
                                            break 'place;
                                        }
                                    }
                                }

                                // Reset AI's firing grid
                                ai_fired = vec![vec![false; GRID_SIZE]; GRID_SIZE];

                                // Reset player grid
                                player_grid = None;

                                // Notify client that new game is starting
                                let _ = writeln!(
                                    stream,
                                    "{}",
                                    serde_json::to_string(&Message::NewGameStart)?
                                );

                                println!("New game ready! Waiting for player to place ships...");
                            } else {
                                println!("Player doesn't want to play again. Ending session.");
                                break;
                            }
                        }
                        Message::Quit => {
                            println!("Player quit the game");
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(_) => break,
        }
    }

    println!("Game ended");
    Ok(())
}
