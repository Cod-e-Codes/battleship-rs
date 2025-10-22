use anyhow::Result;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;

use crate::types::Message;

// Track game state
struct GameState {
    player1_ready: bool,
    player2_ready: bool,
    game_started: bool,
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

    let players: Arc<Mutex<HashMap<usize, mpsc::UnboundedSender<String>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let game_state = Arc::new(Mutex::new(GameState {
        player1_ready: false,
        player2_ready: false,
        game_started: false,
    }));

    let mut next_id = 0;
    let mut connections: Vec<(usize, TcpStream)> = Vec::new();

    loop {
        if *shutdown.lock().unwrap() {
            break;
        }

        // Accept new connections
        match listener.accept() {
            Ok((stream, addr)) => {
                stream.set_nonblocking(true)?;
                let player_id = next_id;
                next_id += 1;

                println!("Player {} connected: {}", player_id, addr);

                // Send initial message to the client
                let mut writer = stream.try_clone()?;
                let initial_msg = serde_json::to_string(&Message::WaitingForOpponent)? + "\n";
                writer.write_all(initial_msg.as_bytes())?;

                connections.push((player_id, stream));

                // If we have 2 players, start the relay
                if connections.len() == 2 {
                    println!("\n2 players connected! Starting relay...\n");

                    let (id1, stream1) = connections.remove(0);
                    let (id2, stream2) = connections.remove(0);

                    let players_clone1 = players.clone();
                    let players_clone2 = players.clone();
                    let shutdown_clone1 = shutdown.clone();
                    let shutdown_clone2 = shutdown.clone();
                    let game_state_clone1 = game_state.clone();
                    let game_state_clone2 = game_state.clone();

                    // Create channels for each player
                    let (tx1, mut rx1) = mpsc::unbounded_channel();
                    let (tx2, mut rx2) = mpsc::unbounded_channel();

                    players.lock().unwrap().insert(id1, tx1.clone());
                    players.lock().unwrap().insert(id2, tx2.clone());

                    // Spawn sender task for player 1
                    let mut writer1 = stream1.try_clone()?;
                    tokio::spawn(async move {
                        while let Some(msg) = rx1.recv().await {
                            if writer1.write_all(msg.as_bytes()).is_err() {
                                break;
                            }
                        }
                    });

                    // Spawn sender task for player 2
                    let mut writer2 = stream2.try_clone()?;
                    tokio::spawn(async move {
                        while let Some(msg) = rx2.recv().await {
                            if writer2.write_all(msg.as_bytes()).is_err() {
                                break;
                            }
                        }
                    });

                    // Spawn receiver task for player 1
                    tokio::spawn(async move {
                        let mut reader = BufReader::new(stream1);
                        let mut line = String::new();
                        loop {
                            if *shutdown_clone1.lock().unwrap() {
                                break;
                            }

                            line.clear();
                            match reader.read_line(&mut line) {
                                Ok(0) => {
                                    println!("Player {} disconnected", id1);
                                    break;
                                }
                                Ok(_) => {
                                    // Parse message first to handle game coordination
                                    if let Ok(msg) = serde_json::from_str::<Message>(&line) {
                                        match msg {
                                            Message::PlaceShips(_) => {
                                                println!("Player {} placed ships", id1);

                                                // Update game state
                                                let mut gs = game_state_clone1.lock().unwrap();
                                                gs.player1_ready = true;

                                                // Relay to player 2
                                                if let Some(tx) =
                                                    players_clone1.lock().unwrap().get(&id2)
                                                {
                                                    let _ = tx.send(line.clone());
                                                }

                                                // Check if both ready
                                                if gs.player1_ready
                                                    && gs.player2_ready
                                                    && !gs.game_started
                                                {
                                                    gs.game_started = true;
                                                    println!(
                                                        "Both players ready! Starting game..."
                                                    );

                                                    // Send GameStart to both
                                                    if let Some(tx) =
                                                        players_clone1.lock().unwrap().get(&id1)
                                                    {
                                                        let msg = serde_json::to_string(
                                                            &Message::GameStart,
                                                        )
                                                        .unwrap()
                                                            + "\n";
                                                        let _ = tx.send(msg);
                                                    }
                                                    if let Some(tx) =
                                                        players_clone1.lock().unwrap().get(&id2)
                                                    {
                                                        let msg = serde_json::to_string(
                                                            &Message::GameStart,
                                                        )
                                                        .unwrap()
                                                            + "\n";
                                                        let _ = tx.send(msg);
                                                    }

                                                    // Send YourTurn to player 1, OpponentTurn to player 2
                                                    if let Some(tx) =
                                                        players_clone1.lock().unwrap().get(&id1)
                                                    {
                                                        let msg = serde_json::to_string(
                                                            &Message::YourTurn,
                                                        )
                                                        .unwrap()
                                                            + "\n";
                                                        let _ = tx.send(msg);
                                                    }
                                                    if let Some(tx) =
                                                        players_clone1.lock().unwrap().get(&id2)
                                                    {
                                                        let msg = serde_json::to_string(
                                                            &Message::OpponentTurn,
                                                        )
                                                        .unwrap()
                                                            + "\n";
                                                        let _ = tx.send(msg);
                                                    }

                                                    println!("Player 1's turn\n");
                                                }
                                            }
                                            Message::Attack { x, y } => {
                                                println!(
                                                    "Player {} attacked {}",
                                                    id1,
                                                    crate::game_state::GameState::format_coordinate(
                                                        x, y
                                                    )
                                                );
                                                // Relay to player 2
                                                if let Some(tx) =
                                                    players_clone1.lock().unwrap().get(&id2)
                                                {
                                                    let _ = tx.send(line.clone());
                                                }
                                            }
                                            Message::GameOver { won } => {
                                                println!(
                                                    "Game over! Player {} {}",
                                                    id1,
                                                    if won { "won" } else { "lost" }
                                                );
                                                // Relay to player 2
                                                if let Some(tx) =
                                                    players_clone1.lock().unwrap().get(&id2)
                                                {
                                                    let _ = tx.send(line.clone());
                                                }
                                            }
                                            _ => {
                                                // Relay all other messages
                                                if let Some(tx) =
                                                    players_clone1.lock().unwrap().get(&id2)
                                                {
                                                    let _ = tx.send(line.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                    tokio::time::sleep(Duration::from_millis(10)).await;
                                }
                                Err(_) => {
                                    println!("Player {} connection error", id1);
                                    break;
                                }
                            }
                        }
                        players_clone1.lock().unwrap().remove(&id1);
                    });

                    // Spawn receiver task for player 2
                    tokio::spawn(async move {
                        let mut reader = BufReader::new(stream2);
                        let mut line = String::new();
                        loop {
                            if *shutdown_clone2.lock().unwrap() {
                                break;
                            }

                            line.clear();
                            match reader.read_line(&mut line) {
                                Ok(0) => {
                                    println!("Player {} disconnected", id2);
                                    break;
                                }
                                Ok(_) => {
                                    // Parse message first to handle game coordination
                                    if let Ok(msg) = serde_json::from_str::<Message>(&line) {
                                        match msg {
                                            Message::PlaceShips(_) => {
                                                println!("Player {} placed ships", id2);

                                                // Update game state
                                                let mut gs = game_state_clone2.lock().unwrap();
                                                gs.player2_ready = true;

                                                // Relay to player 1
                                                if let Some(tx) =
                                                    players_clone2.lock().unwrap().get(&id1)
                                                {
                                                    let _ = tx.send(line.clone());
                                                }

                                                // Check if both ready
                                                if gs.player1_ready
                                                    && gs.player2_ready
                                                    && !gs.game_started
                                                {
                                                    gs.game_started = true;
                                                    println!(
                                                        "Both players ready! Starting game..."
                                                    );

                                                    // Send GameStart to both
                                                    if let Some(tx) =
                                                        players_clone2.lock().unwrap().get(&id1)
                                                    {
                                                        let msg = serde_json::to_string(
                                                            &Message::GameStart,
                                                        )
                                                        .unwrap()
                                                            + "\n";
                                                        let _ = tx.send(msg);
                                                    }
                                                    if let Some(tx) =
                                                        players_clone2.lock().unwrap().get(&id2)
                                                    {
                                                        let msg = serde_json::to_string(
                                                            &Message::GameStart,
                                                        )
                                                        .unwrap()
                                                            + "\n";
                                                        let _ = tx.send(msg);
                                                    }

                                                    // Send YourTurn to player 1, OpponentTurn to player 2
                                                    if let Some(tx) =
                                                        players_clone2.lock().unwrap().get(&id1)
                                                    {
                                                        let msg = serde_json::to_string(
                                                            &Message::YourTurn,
                                                        )
                                                        .unwrap()
                                                            + "\n";
                                                        let _ = tx.send(msg);
                                                    }
                                                    if let Some(tx) =
                                                        players_clone2.lock().unwrap().get(&id2)
                                                    {
                                                        let msg = serde_json::to_string(
                                                            &Message::OpponentTurn,
                                                        )
                                                        .unwrap()
                                                            + "\n";
                                                        let _ = tx.send(msg);
                                                    }

                                                    println!("Player 1's turn\n");
                                                }
                                            }
                                            Message::Attack { x, y } => {
                                                println!(
                                                    "Player {} attacked {}",
                                                    id2,
                                                    crate::game_state::GameState::format_coordinate(
                                                        x, y
                                                    )
                                                );
                                                // Relay to player 1
                                                if let Some(tx) =
                                                    players_clone2.lock().unwrap().get(&id1)
                                                {
                                                    let _ = tx.send(line.clone());
                                                }
                                            }
                                            Message::GameOver { won } => {
                                                println!(
                                                    "Game over! Player {} {}",
                                                    id2,
                                                    if won { "won" } else { "lost" }
                                                );
                                                // Relay to player 1
                                                if let Some(tx) =
                                                    players_clone2.lock().unwrap().get(&id1)
                                                {
                                                    let _ = tx.send(line.clone());
                                                }
                                            }
                                            _ => {
                                                // Relay all other messages
                                                if let Some(tx) =
                                                    players_clone2.lock().unwrap().get(&id1)
                                                {
                                                    let _ = tx.send(line.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                    tokio::time::sleep(Duration::from_millis(10)).await;
                                }
                                Err(_) => {
                                    println!("Player {} connection error", id2);
                                    break;
                                }
                            }
                        }
                        players_clone2.lock().unwrap().remove(&id2);
                    });

                    // Wait for game to end
                    println!("Relay active. Waiting for game to complete...\n");
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => {
                eprintln!("Accept error: {}", e);
            }
        }
    }

    println!("Relay server shut down");
    Ok(())
}
