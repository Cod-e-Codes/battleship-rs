use anyhow::Result;
use rand::Rng;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::game_state::GameState;
use crate::types::{CellState, GRID_SIZE, Message, PowerUp, SHIPS};

fn ai_draw_card(ai_hand: &mut Vec<PowerUp>, rng: &mut impl rand::Rng) {
    if ai_hand.len() < 5 {
        let cards = [
            PowerUp::Shield,
            PowerUp::Radar,
            PowerUp::Repair,
            PowerUp::MissileStrike,
        ];
        let card = cards[rng.random_range(0..cards.len())].clone();
        ai_hand.push(card);
    }
}

// Helper function to check if a ship at position is fully sunk
fn is_ship_fully_sunk(grid: &[Vec<CellState>], x: usize, y: usize) -> bool {
    // Check if this position is part of a ship
    if grid[y][x] != CellState::Hit {
        return false;
    }

    // Check horizontal ship
    let mut is_horizontal = false;
    if (x > 0 && matches!(grid[y][x - 1], CellState::Ship | CellState::Hit))
        || (x + 1 < GRID_SIZE && matches!(grid[y][x + 1], CellState::Ship | CellState::Hit))
    {
        is_horizontal = true;
    }

    if is_horizontal {
        // Find leftmost part of ship
        let mut left = x as isize;
        while left > 0 && matches!(grid[y][left as usize - 1], CellState::Ship | CellState::Hit) {
            left -= 1;
        }
        // Find rightmost part of ship
        let mut right = x;
        while right < GRID_SIZE - 1
            && matches!(grid[y][right + 1], CellState::Ship | CellState::Hit)
        {
            right += 1;
        }
        // Check if all parts are Hit (fully sunk)
        for i in left as usize..=right {
            if grid[y][i] == CellState::Ship {
                return false; // Still has undamaged parts
            }
        }
        true
    } else {
        // Check vertical ship
        let mut top = y as isize;
        while top > 0 && matches!(grid[top as usize - 1][x], CellState::Ship | CellState::Hit) {
            top -= 1;
        }
        let mut bottom = y;
        while bottom < GRID_SIZE - 1
            && matches!(grid[bottom + 1][x], CellState::Ship | CellState::Hit)
        {
            bottom += 1;
        }
        // Check if all parts are Hit (fully sunk)
        for row in grid.iter().take(bottom + 1).skip(top as usize) {
            if row[x] == CellState::Ship {
                return false; // Still has undamaged parts
            }
        }
        true
    }
}

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

    let mut ai_hand: Vec<PowerUp> = Vec::new();
    let mut ai_shield_active = false;

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
                            let mut actual_hit = hit;

                            // Check if AI shield is active
                            if hit && ai_shield_active && rng.random_range(0..2) == 0 {
                                actual_hit = false;
                                println!("🛡️ AI Shield blocked player attack at ({}, {})!", x, y);
                            }

                            // Update grid
                            if actual_hit {
                                ai_grid[y][x] = CellState::Hit;
                            } else {
                                ai_grid[y][x] = CellState::Miss;
                            }

                            let sunk = if actual_hit {
                                GameState::is_ship_sunk_at(&ai_grid, x, y)
                            } else {
                                false
                            };

                            let reply = Message::AttackResult {
                                x,
                                y,
                                hit: actual_hit,
                                sunk,
                            };
                            writeln!(stream, "{}", serde_json::to_string(&reply)?)?;

                            if actual_hit {
                                ai_draw_card(&mut ai_hand, &mut rng);

                                // Player draws card
                                let cards = [
                                    PowerUp::Shield,
                                    PowerUp::Radar,
                                    PowerUp::Repair,
                                    PowerUp::MissileStrike,
                                ];
                                let card = cards[rng.random_range(0..cards.len())].clone();
                                let _ = writeln!(
                                    stream,
                                    "{}",
                                    serde_json::to_string(&Message::CardDrawn { card })?
                                );
                            }

                            // Check if all AI ships are sunk
                            if GameState::all_ships_sunk(&ai_grid) {
                                writeln!(
                                    stream,
                                    "{}",
                                    serde_json::to_string(&Message::GameOver { won: true })?
                                )?;
                                println!("Player wins! All AI ships sunk.");
                                writeln!(
                                    stream,
                                    "{}",
                                    serde_json::to_string(&Message::PlayAgainRequest)?
                                )?;
                                continue;
                            }

                            // AI's turn
                            if let Some(grid) = player_grid.as_mut() {
                                writeln!(
                                    stream,
                                    "{}",
                                    serde_json::to_string(&Message::OpponentTurn)?
                                )?;

                                // Deactivate AI shield
                                if ai_shield_active {
                                    ai_shield_active = false;
                                    println!("🛡️ AI Shield expired!");
                                }

                                // AI uses cards strategically (only use repair if there's a valid target)
                                if !ai_hand.is_empty() {
                                    let card_index = rng.random_range(0..ai_hand.len());
                                    let card = ai_hand[card_index].clone();

                                    // Special handling for Repair - only use if valid
                                    if matches!(card, PowerUp::Repair) {
                                        // Check if there's a damaged ship that's not fully sunk
                                        let has_valid_repair_target =
                                            ai_grid.iter().enumerate().any(|(y, row)| {
                                                row.iter().enumerate().any(|(x, &cell)| {
                                                    cell == CellState::Hit
                                                        && !is_ship_fully_sunk(&ai_grid, x, y)
                                                })
                                            });

                                        if !has_valid_repair_target {
                                            // Don't use repair card, skip to next turn
                                            println!("🤖 AI has Repair but no valid targets");
                                        } else {
                                            // Remove and use the card
                                            ai_hand.remove(card_index);

                                            // Find a valid repair target first
                                            let mut repair_target = None;
                                            for (y, row) in
                                                ai_grid.iter().enumerate().take(GRID_SIZE)
                                            {
                                                for (x, &cell) in
                                                    row.iter().enumerate().take(GRID_SIZE)
                                                {
                                                    if cell == CellState::Hit
                                                        && !is_ship_fully_sunk(&ai_grid, x, y)
                                                    {
                                                        repair_target = Some((x, y));
                                                        break;
                                                    }
                                                }
                                                if repair_target.is_some() {
                                                    break;
                                                }
                                            }

                                            // Apply repair if target found
                                            if let Some((x, y)) = repair_target {
                                                ai_grid[y][x] = CellState::Ship;
                                                println!("🤖 AI used Repair at ({}, {})!", x, y);
                                            }
                                        }
                                    } else {
                                        // Use other cards normally
                                        ai_hand.remove(card_index);

                                        match card {
                                            PowerUp::Shield => {
                                                ai_shield_active = true;
                                                println!("🤖 AI used Shield!");
                                            }
                                            PowerUp::Radar => {
                                                println!("🤖 AI used Radar!");
                                            }
                                            PowerUp::MissileStrike => {
                                                let mut targets = Vec::new();
                                                for (y, row) in
                                                    grid.iter().enumerate().take(GRID_SIZE)
                                                {
                                                    for (x, cell) in
                                                        row.iter().enumerate().take(GRID_SIZE)
                                                    {
                                                        if *cell == CellState::Empty
                                                            || *cell == CellState::Ship
                                                        {
                                                            targets.push((x, y));
                                                        }
                                                    }
                                                }
                                                for _ in 0..2 {
                                                    if !targets.is_empty() {
                                                        let random_idx =
                                                            rng.random_range(0..targets.len());
                                                        let (x, y) = targets.remove(random_idx);
                                                        let was_ship =
                                                            grid[y][x] == CellState::Ship;
                                                        grid[y][x] = if was_ship {
                                                            CellState::Hit
                                                        } else {
                                                            CellState::Miss
                                                        };
                                                    }
                                                }
                                                println!("🤖 AI used Missile Strike!");
                                            }
                                            _ => {}
                                        }
                                    }
                                }

                                // AI attacks
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

                                writeln!(
                                    stream,
                                    "{}",
                                    serde_json::to_string(&Message::Attack { x: sx, y: sy })?
                                )?;

                                if GameState::all_ships_sunk(grid) {
                                    writeln!(
                                        stream,
                                        "{}",
                                        serde_json::to_string(&Message::GameOver { won: false })?
                                    )?;
                                    println!("AI wins!");
                                    writeln!(
                                        stream,
                                        "{}",
                                        serde_json::to_string(&Message::PlayAgainRequest)?
                                    )?;
                                    continue;
                                }

                                writeln!(stream, "{}", serde_json::to_string(&Message::YourTurn)?)?;
                            }
                        }
                        Message::CardUsed { card, .. } => {
                            println!("Player used card: {:?}", card);
                            match card {
                                PowerUp::Shield => {
                                    let _ = writeln!(
                                        stream,
                                        "{}",
                                        serde_json::to_string(&Message::CardEffect {
                                            effect_type: "shield_activated".to_string(),
                                            data: "".to_string(),
                                        })?
                                    );
                                }
                                PowerUp::Radar => {
                                    // Reveal actual unhit ship positions
                                    let mut revealed_positions = Vec::new();
                                    for (y, row) in ai_grid.iter().enumerate() {
                                        for (x, cell) in row.iter().enumerate() {
                                            if *cell == CellState::Ship {
                                                revealed_positions.push((x, y));
                                            }
                                        }
                                    }

                                    use rand::seq::SliceRandom;
                                    revealed_positions.shuffle(&mut rng);

                                    let reveal_count = revealed_positions.len().min(2);
                                    for (x, y) in revealed_positions.iter().take(reveal_count) {
                                        let _ = writeln!(
                                            stream,
                                            "{}",
                                            serde_json::to_string(&Message::CardEffect {
                                                effect_type: "radar_reveal".to_string(),
                                                data: format!("{},{}", x, y),
                                            })?
                                        );
                                    }
                                    println!(
                                        "Player used Radar! Revealed {} positions",
                                        reveal_count
                                    );
                                }
                                PowerUp::Repair => {
                                    let _ = writeln!(
                                        stream,
                                        "{}",
                                        serde_json::to_string(&Message::CardEffect {
                                            effect_type: "repair".to_string(),
                                            data: "".to_string(),
                                        })?
                                    );
                                }
                                PowerUp::MissileStrike => {
                                    let mut targets = Vec::new();
                                    for (y, row) in ai_grid.iter().enumerate() {
                                        for (x, cell) in row.iter().enumerate() {
                                            if *cell == CellState::Empty || *cell == CellState::Ship
                                            {
                                                targets.push((x, y));
                                            }
                                        }
                                    }

                                    for _ in 0..2 {
                                        if !targets.is_empty() {
                                            let random_idx = rng.random_range(0..targets.len());
                                            let (x, y) = targets.remove(random_idx);
                                            let was_ship = ai_grid[y][x] == CellState::Ship;

                                            if was_ship {
                                                ai_grid[y][x] = CellState::Hit;
                                            } else {
                                                ai_grid[y][x] = CellState::Miss;
                                            }

                                            let _ = writeln!(
                                                stream,
                                                "{}",
                                                serde_json::to_string(&Message::CardEffect {
                                                    effect_type: "missile_strike".to_string(),
                                                    data: format!(
                                                        "{},{},{}",
                                                        x,
                                                        y,
                                                        if was_ship { "hit" } else { "miss" }
                                                    ),
                                                })?
                                            );
                                        }
                                    }

                                    // Check for victory after missile strike
                                    if GameState::all_ships_sunk(&ai_grid) {
                                        writeln!(
                                            stream,
                                            "{}",
                                            serde_json::to_string(&Message::GameOver {
                                                won: true
                                            })?
                                        )?;
                                        println!("Player wins! All AI ships sunk.");
                                        writeln!(
                                            stream,
                                            "{}",
                                            serde_json::to_string(&Message::PlayAgainRequest)?
                                        )?;
                                    }
                                }
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

                                // Reset everything
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

                                ai_fired = vec![vec![false; GRID_SIZE]; GRID_SIZE];
                                ai_hand.clear();
                                ai_shield_active = false;
                                player_grid = None;

                                let _ = writeln!(
                                    stream,
                                    "{}",
                                    serde_json::to_string(&Message::NewGameStart)?
                                );
                                println!("New game ready!");
                            } else {
                                println!("Player doesn't want to play again.");
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
