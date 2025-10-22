use anyhow::Result;
use std::{
    net::TcpListener,
    sync::{Arc, Mutex},
    time::Duration,
};

pub async fn run_server_relay(port: &str) -> Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
    listener.set_nonblocking(true)?;
    println!("🔀 Relay Battleship Server listening on port {}", port);
    println!("This server hosts games between two remote players.\n");

    let shutdown = Arc::new(Mutex::new(false));
    let shutdown_flag = shutdown.clone();

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        *shutdown_flag.lock().unwrap() = true;
        println!("\nShutting down relay server...");
    });

    // Wait for two players
    let mut players = Vec::new();

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

    // Just use the regular server logic
    crate::server::run_game_session(players.remove(0), players.remove(0), shutdown).await
}
