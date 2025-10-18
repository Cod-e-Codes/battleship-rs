use anyhow::Result;
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::game_state::GameState;
use crate::types::{CellState, Message, SHIPS};

struct PlayerConnection {
    stream: TcpStream,
    grid: Option<Vec<Vec<CellState>>>,
    ready: bool,
    last_stand_used: bool,
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

// Helper function to restore a random ship on a grid
fn restore_random_ship(grid: &mut [Vec<CellState>]) -> bool {
    // Find the smallest ship to restore (Destroyer = 2 cells)
    let (length, _name) = SHIPS[4]; // Destroyer is the smallest

    // Find all possible empty positions
    let mut empty_positions = Vec::new();
    for (y, row) in grid.iter().enumerate().take(10) {
        for (x, cell) in row.iter().enumerate().take(10) {
            if *cell == CellState::Empty {
                empty_positions.push((x, y));
            }
        }
    }

    if empty_positions.is_empty() {
        return false; // No space to place ship
    }

    // Try to place the ship randomly
    use rand::Rng;
    use std::collections::HashSet;
    let mut rng = rand::rng();
    let mut tried_positions = HashSet::new();

    while tried_positions.len() < empty_positions.len() {
        let random_idx = rng.random_range(0..empty_positions.len());
        let (x, y) = empty_positions[random_idx];

        if tried_positions.contains(&(x, y)) {
            continue;
        }
        tried_positions.insert((x, y));

        // Try horizontal first, then vertical
        for horizontal in [true, false] {
            if can_place_ship(grid, x, y, length, horizontal) {
                place_ship(grid, x, y, length, horizontal);
                return true;
            }
        }
    }

    false // Couldn't place ship anywhere
}

fn can_place_ship(
    grid: &[Vec<CellState>],
    x: usize,
    y: usize,
    length: usize,
    horizontal: bool,
) -> bool {
    if horizontal {
        if x + length > 10 {
            return false;
        }
        for i in 0..length {
            if grid[y][x + i] != CellState::Empty {
                return false;
            }
        }
    } else {
        if y + length > 10 {
            return false;
        }
        for i in 0..length {
            if grid[y + i][x] != CellState::Empty {
                return false;
            }
        }
    }
    true
}

fn place_ship(grid: &mut [Vec<CellState>], x: usize, y: usize, length: usize, horizontal: bool) {
    if horizontal {
        for i in 0..length {
            grid[y][x + i] = CellState::Ship;
        }
    } else {
        for i in 0..length {
            grid[y + i][x] = CellState::Ship;
        }
    }
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
        last_stand_used: false,
    };
    let mut p2 = PlayerConnection {
        stream: players.remove(0),
        grid: None,
        ready: false,
        last_stand_used: false,
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
                                    "Player 1 attacked {} - {}",
                                    crate::game_state::GameState::format_coordinate(x, y),
                                    if hit { "HIT" } else { "MISS" }
                                );

                                // Check if player 2 lost
                                if GameState::all_ships_sunk(grid) {
                                    // Check if player 2 can use Last Stand
                                    if !p2.last_stand_used {
                                        p2.last_stand_used = true;
                                        writeln!(
                                            p2.stream,
                                            "{}",
                                            serde_json::to_string(&Message::LastStandTrigger)?
                                        )?;
                                        println!("Player 2 gets Last Stand chance!");

                                        // Wait for Last Stand result with timeout
                                        let timeout_start = Instant::now();
                                        let mut last_stand_result = None;

                                        while timeout_start.elapsed() < Duration::from_secs(10) {
                                            let mut line = String::new();
                                            match p2_reader.read_line(&mut line) {
                                                Ok(0) => break,
                                                Ok(_) => {
                                                    if let Ok(Message::LastStandResult {
                                                        success,
                                                    }) = serde_json::from_str::<Message>(&line)
                                                    {
                                                        last_stand_result = Some(success);
                                                        break;
                                                    }
                                                }
                                                Err(ref e)
                                                    if e.kind()
                                                        == std::io::ErrorKind::WouldBlock =>
                                                {
                                                    std::thread::sleep(Duration::from_millis(10));
                                                }
                                                Err(_) => break,
                                            }
                                        }

                                        if let Some(success) = last_stand_result
                                            && success
                                            && let Some(ref mut grid) = p2.grid
                                            && restore_random_ship(grid)
                                        {
                                            println!(
                                                "Player 2 Last Stand successful! Ship restored!"
                                            );
                                            continue; // Continue the game loop
                                        }

                                        println!("Player 2 Last Stand failed or timed out");
                                    }

                                    // Send game over messages
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
                        Message::CardUsed {
                            card,
                            target_x: _target_x,
                            target_y: _target_y,
                        } => {
                            println!("Player 1 used card: {:?}", card);
                            // Handle card effects
                            match card {
                                crate::types::PowerUp::Shield => {
                                    // Shield effect - handled in game logic
                                    let _ = writeln!(
                                        p1.stream,
                                        "{}",
                                        serde_json::to_string(&Message::CardEffect {
                                            effect_type: "shield_activated".to_string(),
                                            data: "".to_string(),
                                        })?
                                    );
                                }
                                crate::types::PowerUp::Radar => {
                                    // Radar effect - reveal enemy ship positions
                                    if let Some(ref grid) = p2.grid {
                                        let mut revealed_positions = Vec::new();
                                        for (y, row) in grid.iter().enumerate() {
                                            for (x, cell) in row.iter().enumerate() {
                                                if *cell == crate::types::CellState::Ship {
                                                    revealed_positions.push((x, y));
                                                }
                                            }
                                        }

                                        // Send up to 2 revealed positions
                                        let reveal_count = revealed_positions.len().min(2);
                                        for (x, y) in revealed_positions.iter().take(reveal_count) {
                                            let _ = writeln!(
                                                p1.stream,
                                                "{}",
                                                serde_json::to_string(&Message::CardEffect {
                                                    effect_type: "radar_reveal".to_string(),
                                                    data: format!("{},{}", x, y),
                                                })?
                                            );
                                        }
                                    }
                                }
                                crate::types::PowerUp::Repair => {
                                    // Repair effect - restore one hit to ship
                                    let _ = writeln!(
                                        p1.stream,
                                        "{}",
                                        serde_json::to_string(&Message::CardEffect {
                                            effect_type: "repair".to_string(),
                                            data: "".to_string(),
                                        })?
                                    );
                                }
                                crate::types::PowerUp::MissileStrike => {
                                    // Missile strike - hit 2 random enemy positions
                                    if let Some(ref mut grid) = p2.grid {
                                        let mut targets = Vec::new();
                                        for (y, row) in grid.iter().enumerate().take(10) {
                                            for (x, cell) in row.iter().enumerate().take(10) {
                                                if *cell == crate::types::CellState::Empty
                                                    || *cell == crate::types::CellState::Ship
                                                {
                                                    targets.push((x, y));
                                                }
                                            }
                                        }

                                        // Strike 2 random positions
                                        use rand::Rng;
                                        let mut rng = rand::rng();
                                        for _ in 0..2 {
                                            if !targets.is_empty() {
                                                let random_idx = rng.random_range(0..targets.len());
                                                let (x, y) = targets.remove(random_idx);
                                                let was_ship =
                                                    grid[y][x] == crate::types::CellState::Ship;
                                                grid[y][x] = if was_ship {
                                                    crate::types::CellState::Hit
                                                } else {
                                                    crate::types::CellState::Miss
                                                };

                                                let _ = writeln!(
                                                    p1.stream,
                                                    "{}",
                                                    serde_json::to_string(&Message::CardEffect {
                                                        effect_type: "missile_strike".to_string(),
                                                        data: format!("{},{}", x, y),
                                                    })?
                                                );
                                            }
                                        }

                                        // Send updated grid state to player 1
                                        let _ = writeln!(
                                            p1.stream,
                                            "{}",
                                            serde_json::to_string(&Message::GridUpdate {
                                                grid: grid.clone(),
                                            })?
                                        );
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
                                    "Player 2 attacked {} - {}",
                                    crate::game_state::GameState::format_coordinate(x, y),
                                    if hit { "HIT" } else { "MISS" }
                                );

                                // Check if player 1 lost
                                if GameState::all_ships_sunk(grid) {
                                    // Check if player 1 can use Last Stand
                                    if !p1.last_stand_used {
                                        p1.last_stand_used = true;
                                        writeln!(
                                            p1.stream,
                                            "{}",
                                            serde_json::to_string(&Message::LastStandTrigger)?
                                        )?;
                                        println!("Player 1 gets Last Stand chance!");

                                        // Wait for Last Stand result with timeout
                                        let timeout_start = Instant::now();
                                        let mut last_stand_result = None;

                                        while timeout_start.elapsed() < Duration::from_secs(10) {
                                            let mut line = String::new();
                                            match p1_reader.read_line(&mut line) {
                                                Ok(0) => break,
                                                Ok(_) => {
                                                    if let Ok(Message::LastStandResult {
                                                        success,
                                                    }) = serde_json::from_str::<Message>(&line)
                                                    {
                                                        last_stand_result = Some(success);
                                                        break;
                                                    }
                                                }
                                                Err(ref e)
                                                    if e.kind()
                                                        == std::io::ErrorKind::WouldBlock =>
                                                {
                                                    std::thread::sleep(Duration::from_millis(10));
                                                }
                                                Err(_) => break,
                                            }
                                        }

                                        if let Some(success) = last_stand_result
                                            && success
                                            && let Some(ref mut grid) = p1.grid
                                            && restore_random_ship(grid)
                                        {
                                            println!(
                                                "Player 1 Last Stand successful! Ship restored!"
                                            );
                                            continue; // Continue the game loop
                                        }

                                        println!("Player 1 Last Stand failed or timed out");
                                    }

                                    // Send game over messages
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
                        Message::CardUsed {
                            card,
                            target_x: _target_x,
                            target_y: _target_y,
                        } => {
                            println!("Player 2 used card: {:?}", card);
                            // Handle card effects
                            match card {
                                crate::types::PowerUp::Shield => {
                                    let _ = writeln!(
                                        p2.stream,
                                        "{}",
                                        serde_json::to_string(&Message::CardEffect {
                                            effect_type: "shield_activated".to_string(),
                                            data: "".to_string(),
                                        })?
                                    );
                                }
                                crate::types::PowerUp::Radar => {
                                    // Radar effect - reveal enemy ship positions
                                    if let Some(ref grid) = p1.grid {
                                        let mut revealed_positions = Vec::new();
                                        for (y, row) in grid.iter().enumerate() {
                                            for (x, cell) in row.iter().enumerate() {
                                                if *cell == crate::types::CellState::Ship {
                                                    revealed_positions.push((x, y));
                                                }
                                            }
                                        }

                                        // Send up to 2 revealed positions
                                        let reveal_count = revealed_positions.len().min(2);
                                        for (x, y) in revealed_positions.iter().take(reveal_count) {
                                            let _ = writeln!(
                                                p2.stream,
                                                "{}",
                                                serde_json::to_string(&Message::CardEffect {
                                                    effect_type: "radar_reveal".to_string(),
                                                    data: format!("{},{}", x, y),
                                                })?
                                            );
                                        }
                                    }
                                }
                                crate::types::PowerUp::Repair => {
                                    let _ = writeln!(
                                        p2.stream,
                                        "{}",
                                        serde_json::to_string(&Message::CardEffect {
                                            effect_type: "repair".to_string(),
                                            data: "".to_string(),
                                        })?
                                    );
                                }
                                crate::types::PowerUp::MissileStrike => {
                                    // Missile strike - hit 2 random enemy positions
                                    if let Some(ref mut grid) = p1.grid {
                                        let mut targets = Vec::new();
                                        for (y, row) in grid.iter().enumerate().take(10) {
                                            for (x, cell) in row.iter().enumerate().take(10) {
                                                if *cell == crate::types::CellState::Empty
                                                    || *cell == crate::types::CellState::Ship
                                                {
                                                    targets.push((x, y));
                                                }
                                            }
                                        }

                                        // Strike 2 random positions
                                        use rand::Rng;
                                        let mut rng = rand::rng();
                                        for _ in 0..2 {
                                            if !targets.is_empty() {
                                                let random_idx = rng.random_range(0..targets.len());
                                                let (x, y) = targets.remove(random_idx);
                                                let was_ship =
                                                    grid[y][x] == crate::types::CellState::Ship;
                                                grid[y][x] = if was_ship {
                                                    crate::types::CellState::Hit
                                                } else {
                                                    crate::types::CellState::Miss
                                                };

                                                let _ = writeln!(
                                                    p2.stream,
                                                    "{}",
                                                    serde_json::to_string(&Message::CardEffect {
                                                        effect_type: "missile_strike".to_string(),
                                                        data: format!("{},{}", x, y),
                                                    })?
                                                );
                                            }
                                        }

                                        // Send updated grid state to player 2
                                        let _ = writeln!(
                                            p2.stream,
                                            "{}",
                                            serde_json::to_string(&Message::GridUpdate {
                                                grid: grid.clone(),
                                            })?
                                        );
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

                // Reset Last Stand usage for new game
                p1.last_stand_used = false;
                p2.last_stand_used = false;

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
