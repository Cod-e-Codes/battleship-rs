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

struct RelayState {
    p1: PlayerConnection,
    p2: PlayerConnection,
    current_turn: usize, // 0 = player 1, 1 = player 2
    play_again_state: PlayAgainState,
}

pub async fn run_server_relay(port: &str) -> Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
    listener.set_nonblocking(true)?;
    println!("🔀 Relay Battleship Server listening on port {}", port);
    println!("This server relays messages between two players.\n");

    let shutdown = Arc::new(Mutex::new(false));
    let shutdown_flag = shutdown.clone();

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        *shutdown_flag.lock().unwrap() = true;
        println!("\nShutting down relay server...");
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

    let stream1 = players.remove(0);
    let stream2 = players.remove(0);

    let mut p1_reader = BufReader::new(stream1.try_clone()?);
    let mut p2_reader = BufReader::new(stream2.try_clone()?);
    let mut p1_writer = stream1;
    let mut p2_writer = stream2;

    let relay_state = Arc::new(Mutex::new(RelayState {
        p1: PlayerConnection {
            grid: None,
            ready: false,
        },
        p2: PlayerConnection {
            grid: None,
            ready: false,
        },
        current_turn: 0,
        play_again_state: PlayAgainState::None,
    }));

    let mut game_over = false;

    while !game_over && !*shutdown.lock().unwrap() {
        let mut line = String::new();

        // Check player 1
        match p1_reader.read_line(&mut line) {
            Ok(0) => {
                println!("Player 1 disconnected");
                break;
            }
            Ok(_) => {
                if let Ok(msg) = serde_json::from_str::<Message>(&line) {
                    let mut state = relay_state.lock().unwrap();
                    match msg {
                        Message::PlaceShips(grid) => {
                            state.p1.grid = Some(grid);
                            state.p1.ready = true;
                            println!("Player 1 placed ships");

                            if state.p2.ready {
                                writeln!(
                                    p1_writer,
                                    "{}",
                                    serde_json::to_string(&Message::GameStart)?
                                )?;
                                writeln!(
                                    p2_writer,
                                    "{}",
                                    serde_json::to_string(&Message::GameStart)?
                                )?;
                                writeln!(
                                    p1_writer,
                                    "{}",
                                    serde_json::to_string(&Message::YourTurn)?
                                )?;
                                writeln!(
                                    p2_writer,
                                    "{}",
                                    serde_json::to_string(&Message::OpponentTurn)?
                                )?;
                                println!("Game started! Player 1's turn\n");
                            } else {
                                writeln!(
                                    p1_writer,
                                    "{}",
                                    serde_json::to_string(&Message::WaitingForOpponent)?
                                )?;
                            }
                        }
                        Message::Attack { x, y }
                            if state.current_turn == 0 && state.p1.ready && state.p2.ready =>
                        {
                            // Player 1 attacks player 2
                            if let Some(ref mut grid) = state.p2.grid {
                                let hit = grid[y][x] == CellState::Ship;
                                if hit {
                                    grid[y][x] = CellState::Hit;
                                }
                                let sunk = if hit {
                                    GameState::is_ship_sunk_at(grid, x, y)
                                } else {
                                    false
                                };

                                writeln!(
                                    p1_writer,
                                    "{}",
                                    serde_json::to_string(&Message::AttackResult {
                                        x,
                                        y,
                                        hit,
                                        sunk
                                    })?
                                )?;

                                writeln!(
                                    p2_writer,
                                    "{}",
                                    serde_json::to_string(&Message::Attack { x, y })?
                                )?;

                                println!(
                                    "Player 1 attacked {} - {}",
                                    GameState::format_coordinate(x, y),
                                    if hit { "HIT" } else { "MISS" }
                                );

                                if GameState::all_ships_sunk(grid) {
                                    writeln!(
                                        p1_writer,
                                        "{}",
                                        serde_json::to_string(&Message::GameOver { won: true })?
                                    )?;
                                    writeln!(
                                        p2_writer,
                                        "{}",
                                        serde_json::to_string(&Message::GameOver { won: false })?
                                    )?;
                                    println!("\n🎉 Player 1 wins!");

                                    state.play_again_state = PlayAgainState::WaitingForResponses {
                                        p1_response: None,
                                        p2_response: None,
                                        timeout_start: Instant::now(),
                                    };
                                    writeln!(
                                        p1_writer,
                                        "{}",
                                        serde_json::to_string(&Message::PlayAgainRequest)?
                                    )?;
                                    writeln!(
                                        p2_writer,
                                        "{}",
                                        serde_json::to_string(&Message::PlayAgainRequest)?
                                    )?;
                                    println!("Asking both players if they want to play again...");
                                } else {
                                    state.current_turn = 1;
                                    writeln!(
                                        p1_writer,
                                        "{}",
                                        serde_json::to_string(&Message::OpponentTurn)?
                                    )?;
                                    writeln!(
                                        p2_writer,
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
                            } = &mut state.play_again_state
                            {
                                *p1_response = Some(wants_to_play);
                                println!("Player 1 play again response: {}", wants_to_play);

                                if let (Some(p1_resp), Some(p2_resp)) = (p1_response, p2_response) {
                                    if *p1_resp && *p2_resp {
                                        state.play_again_state = PlayAgainState::BothAgreed;
                                    } else {
                                        state.play_again_state = PlayAgainState::OneDeclined;
                                    }
                                }
                            }
                        }
                        Message::Quit => {
                            println!("Player 1 quit the game");
                            let _ = writeln!(
                                p2_writer,
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
                    let mut state = relay_state.lock().unwrap();
                    match msg {
                        Message::PlaceShips(grid) => {
                            state.p2.grid = Some(grid);
                            state.p2.ready = true;
                            println!("Player 2 placed ships");

                            if state.p1.ready {
                                writeln!(
                                    p1_writer,
                                    "{}",
                                    serde_json::to_string(&Message::GameStart)?
                                )?;
                                writeln!(
                                    p2_writer,
                                    "{}",
                                    serde_json::to_string(&Message::GameStart)?
                                )?;
                                writeln!(
                                    p1_writer,
                                    "{}",
                                    serde_json::to_string(&Message::YourTurn)?
                                )?;
                                writeln!(
                                    p2_writer,
                                    "{}",
                                    serde_json::to_string(&Message::OpponentTurn)?
                                )?;
                                println!("Game started! Player 1's turn\n");
                            } else {
                                writeln!(
                                    p2_writer,
                                    "{}",
                                    serde_json::to_string(&Message::WaitingForOpponent)?
                                )?;
                            }
                        }
                        Message::Attack { x, y }
                            if state.current_turn == 1 && state.p1.ready && state.p2.ready =>
                        {
                            // Player 2 attacks player 1
                            if let Some(ref mut grid) = state.p1.grid {
                                let hit = grid[y][x] == CellState::Ship;
                                if hit {
                                    grid[y][x] = CellState::Hit;
                                }
                                let sunk = if hit {
                                    GameState::is_ship_sunk_at(grid, x, y)
                                } else {
                                    false
                                };

                                writeln!(
                                    p2_writer,
                                    "{}",
                                    serde_json::to_string(&Message::AttackResult {
                                        x,
                                        y,
                                        hit,
                                        sunk
                                    })?
                                )?;

                                writeln!(
                                    p1_writer,
                                    "{}",
                                    serde_json::to_string(&Message::Attack { x, y })?
                                )?;

                                println!(
                                    "Player 2 attacked {} - {}",
                                    GameState::format_coordinate(x, y),
                                    if hit { "HIT" } else { "MISS" }
                                );

                                if GameState::all_ships_sunk(grid) {
                                    writeln!(
                                        p1_writer,
                                        "{}",
                                        serde_json::to_string(&Message::GameOver { won: false })?
                                    )?;
                                    writeln!(
                                        p2_writer,
                                        "{}",
                                        serde_json::to_string(&Message::GameOver { won: true })?
                                    )?;
                                    println!("\n🎉 Player 2 wins!");

                                    state.play_again_state = PlayAgainState::WaitingForResponses {
                                        p1_response: None,
                                        p2_response: None,
                                        timeout_start: Instant::now(),
                                    };
                                    writeln!(
                                        p1_writer,
                                        "{}",
                                        serde_json::to_string(&Message::PlayAgainRequest)?
                                    )?;
                                    writeln!(
                                        p2_writer,
                                        "{}",
                                        serde_json::to_string(&Message::PlayAgainRequest)?
                                    )?;
                                    println!("Asking both players if they want to play again...");
                                } else {
                                    state.current_turn = 0;
                                    writeln!(
                                        p1_writer,
                                        "{}",
                                        serde_json::to_string(&Message::YourTurn)?
                                    )?;
                                    writeln!(
                                        p2_writer,
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
                            } = &mut state.play_again_state
                            {
                                *p2_response = Some(wants_to_play);
                                println!("Player 2 play again response: {}", wants_to_play);

                                if let (Some(p1_resp), Some(p2_resp)) = (p1_response, p2_response) {
                                    if *p1_resp && *p2_resp {
                                        state.play_again_state = PlayAgainState::BothAgreed;
                                    } else {
                                        state.play_again_state = PlayAgainState::OneDeclined;
                                    }
                                }
                            }
                        }
                        Message::Quit => {
                            println!("Player 2 quit the game");
                            let _ = writeln!(
                                p1_writer,
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
        {
            let mut state = relay_state.lock().unwrap();
            match &mut state.play_again_state {
                PlayAgainState::WaitingForResponses { timeout_start, .. } => {
                    if timeout_start.elapsed() > Duration::from_secs(30) {
                        println!("Play again timeout - no response from one or both players");
                        state.play_again_state = PlayAgainState::Timeout;
                    }
                }
                PlayAgainState::BothAgreed => {
                    println!("Both players want to play again! Starting new game...");

                    state.p1.grid = None;
                    state.p1.ready = false;
                    state.p2.grid = None;
                    state.p2.ready = false;
                    state.current_turn = 0;
                    state.play_again_state = PlayAgainState::None;

                    let _ = writeln!(
                        p1_writer,
                        "{}",
                        serde_json::to_string(&Message::NewGameStart)?
                    );
                    let _ = writeln!(
                        p2_writer,
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
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    println!("Game ended");
    Ok(())
}
