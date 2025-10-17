mod client;
mod game_state;
mod input;
mod server;
mod server_ai;
mod types;
mod ui;

use anyhow::Result;
use client::run_client;
use server::run_server;
use server_ai::run_server_ai;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("🚢 BATTLESHIP - Networked Terminal Game\n");
        println!("Usage:");
        println!("  Two-player server: {} server <port>", args[0]);
        println!("  AI opponent:       {} server-ai <port>", args[0]);
        println!("  Client:            {} client <host:port>", args[0]);
        println!("\nExamples:");
        println!("  # Start a server for two players");
        println!("  {} server 8080", args[0]);
        println!();
        println!("  # Connect as player 1");
        println!("  {} client 127.0.0.1:8080", args[0]);
        println!();
        println!("  # Connect as player 2 (in another terminal)");
        println!("  {} client 127.0.0.1:8080", args[0]);
        println!();
        println!("  # Or play against AI");
        println!("  {} server-ai 8080", args[0]);
        println!("  {} client 127.0.0.1:8080", args[0]);
        println!();
        println!("SSH Tunneling for remote play:");
        println!("  On server machine: {} server 8080", args[0]);
        println!("  On client machine: ssh -L 8080:localhost:8080 user@server-ip");
        println!("  Then connect: {} client 127.0.0.1:8080", args[0]);
        return Ok(());
    }

    match args[1].as_str() {
        "server" => {
            let port = args.get(2).map(|s| s.as_str()).unwrap_or("8080");
            run_server(port).await
        }
        "server-ai" => {
            let port = args.get(2).map(|s| s.as_str()).unwrap_or("8080");
            run_server_ai(port).await
        }
        "client" => {
            let addr = args.get(2).map(|s| s.as_str()).unwrap_or("127.0.0.1:8080");
            run_client(addr).await
        }
        _ => {
            println!("Invalid command. Use 'server', 'server-ai', or 'client'");
            println!("Run without arguments for help");
            Ok(())
        }
    }
}
