use crate::types::{CellState, GRID_SIZE, GamePhase, PowerUp, SHIPS, SidePanelMode};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct ShipStatus {
    pub name: String,
    pub length: usize,
    pub hits: usize,
    pub sunk: bool,
}

pub struct GameState {
    pub own_grid: Vec<Vec<CellState>>,
    pub enemy_grid: Vec<Vec<CellState>>,
    pub phase: GamePhase,
    pub cursor: (usize, usize),
    pub placing_ship_idx: usize,
    pub placing_horizontal: bool,
    pub messages: Vec<String>,
    pub winner: Option<bool>,
    // Side panel and stats
    pub side_panel_mode: SidePanelMode,
    pub ship_status: Vec<ShipStatus>,
    pub total_shots: usize,
    pub total_hits: usize,
    pub turn_count: usize,
    pub turn_start_time: Option<Instant>,
    pub turn_times: Vec<f64>, // Store last 10 turn times
    // Play again functionality
    pub play_again_response: Option<bool>,
    pub waiting_for_play_again: bool,
    // Last Stand mechanic
    pub last_stand_used: bool,
    pub last_stand_sequence: Vec<char>,
    pub last_stand_input: Vec<char>,
    // Power-ups
    pub current_hand: Vec<PowerUp>,
    pub max_hand_size: usize,
    pub shield_active: bool,
    pub radar_reveals: Vec<(usize, usize)>, // Coordinates revealed by radar
}

impl GameState {
    pub fn new() -> Self {
        let mut ship_status = Vec::new();
        for (length, name) in SHIPS.iter() {
            ship_status.push(ShipStatus {
                name: name.to_string(),
                length: *length,
                hits: 0,
                sunk: false,
            });
        }

        Self {
            own_grid: vec![vec![CellState::Empty; GRID_SIZE]; GRID_SIZE],
            enemy_grid: vec![vec![CellState::Empty; GRID_SIZE]; GRID_SIZE],
            phase: GamePhase::Placing,
            cursor: (0, 0),
            placing_ship_idx: 0,
            placing_horizontal: true,
            messages: vec!["Place your ships! Use arrows, R to rotate, Enter to place".to_string()],
            winner: None,
            // Side panel and stats
            side_panel_mode: SidePanelMode::Statistics,
            ship_status,
            total_shots: 0,
            total_hits: 0,
            turn_count: 0,
            turn_start_time: None,
            turn_times: Vec::new(),
            // Play again functionality
            play_again_response: None,
            waiting_for_play_again: false,
            // Last Stand mechanic
            last_stand_used: false,
            last_stand_sequence: Vec::new(),
            last_stand_input: Vec::new(),
            // Power-ups
            current_hand: Vec::new(),
            max_hand_size: 5,
            shield_active: false,
            radar_reveals: Vec::new(),
        }
    }

    pub fn can_place_ship(&self, x: usize, y: usize, length: usize, horizontal: bool) -> bool {
        if horizontal {
            if x + length > GRID_SIZE {
                return false;
            }
            for i in 0..length {
                if self.own_grid[y][x + i] != CellState::Empty {
                    return false;
                }
            }
        } else {
            if y + length > GRID_SIZE {
                return false;
            }
            for i in 0..length {
                if self.own_grid[y + i][x] != CellState::Empty {
                    return false;
                }
            }
        }
        true
    }

    pub fn place_ship(&mut self, x: usize, y: usize, length: usize, horizontal: bool) {
        if horizontal {
            for i in 0..length {
                self.own_grid[y][x + i] = CellState::Ship;
            }
        } else {
            for i in 0..length {
                self.own_grid[y + i][x] = CellState::Ship;
            }
        }
    }

    pub fn all_ships_sunk(grid: &[Vec<CellState>]) -> bool {
        !grid.iter().flatten().any(|c| *c == CellState::Ship)
    }

    pub fn is_ship_sunk_at(grid: &[Vec<CellState>], x: usize, y: usize) -> bool {
        // Check if ship is horizontal or vertical
        let horiz = (x > 0 && matches!(grid[y][x - 1], CellState::Ship | CellState::Hit))
            || (x + 1 < GRID_SIZE && matches!(grid[y][x + 1], CellState::Ship | CellState::Hit));

        if horiz {
            // Check horizontal ship
            let mut lx = x as isize;
            while lx >= 0 && matches!(grid[y][lx as usize], CellState::Ship | CellState::Hit) {
                if grid[y][lx as usize] == CellState::Ship {
                    return false;
                }
                lx -= 1;
            }
            let mut rx = x + 1;
            while rx < GRID_SIZE && matches!(grid[y][rx], CellState::Ship | CellState::Hit) {
                if grid[y][rx] == CellState::Ship {
                    return false;
                }
                rx += 1;
            }
            true
        } else {
            // Check vertical ship
            let mut uy = y as isize;
            while uy >= 0 && matches!(grid[uy as usize][x], CellState::Ship | CellState::Hit) {
                if grid[uy as usize][x] == CellState::Ship {
                    return false;
                }
                uy -= 1;
            }
            let mut dy = y + 1;
            while dy < GRID_SIZE && matches!(grid[dy][x], CellState::Ship | CellState::Hit) {
                if grid[dy][x] == CellState::Ship {
                    return false;
                }
                dy += 1;
            }
            true
        }
    }

    // Statistics and overlay methods
    pub fn start_turn(&mut self) {
        self.turn_start_time = Some(Instant::now());
    }

    pub fn end_turn(&mut self) {
        if let Some(start_time) = self.turn_start_time {
            let duration = start_time.elapsed().as_secs_f64();
            self.turn_times.push(duration);
            if self.turn_times.len() > 10 {
                self.turn_times.remove(0); // Keep only last 10 turns
            }
        }
        self.turn_start_time = None;
    }

    pub fn record_shot(&mut self, hit: bool) {
        self.total_shots += 1;
        if hit {
            self.total_hits += 1;
        }
    }

    pub fn update_ship_status(&mut self) {
        // Count hits on each ship by analyzing the grid
        for ship in &mut self.ship_status {
            ship.hits = 0;
            ship.sunk = false;
        }

        // Simple approach: count all hits on own grid and distribute to ships
        // This is a simplified version - in a real implementation you'd track ship positions
        let total_hits = self
            .own_grid
            .iter()
            .flatten()
            .filter(|&&cell| cell == CellState::Hit)
            .count();

        // Distribute hits across ships (this is simplified - real implementation would track exact positions)
        let mut remaining_hits = total_hits;
        for ship in &mut self.ship_status {
            if remaining_hits >= ship.length {
                ship.hits = ship.length;
                ship.sunk = true;
                remaining_hits -= ship.length;
            } else {
                ship.hits = remaining_hits;
                remaining_hits = 0;
            }
        }
    }

    pub fn get_accuracy(&self) -> f64 {
        if self.total_shots == 0 {
            0.0
        } else {
            (self.total_hits as f64 / self.total_shots as f64) * 100.0
        }
    }

    pub fn get_avg_turn_time(&self) -> f64 {
        if self.turn_times.is_empty() {
            0.0
        } else {
            self.turn_times.iter().sum::<f64>() / self.turn_times.len() as f64
        }
    }

    pub fn get_ships_sunk(&self) -> usize {
        self.ship_status.iter().filter(|ship| ship.sunk).count()
    }

    pub fn format_coordinate(x: usize, y: usize) -> String {
        format!("{}{}", (b'A' + y as u8) as char, x + 1)
    }

    pub fn reset_for_new_game(&mut self) {
        self.own_grid = vec![vec![CellState::Empty; GRID_SIZE]; GRID_SIZE];
        self.enemy_grid = vec![vec![CellState::Empty; GRID_SIZE]; GRID_SIZE];
        self.phase = GamePhase::Placing;
        self.cursor = (0, 0);
        self.placing_ship_idx = 0;
        self.placing_horizontal = true;
        self.messages =
            vec!["Place your ships! Use arrows, R to rotate, Enter to place".to_string()];
        self.winner = None;
        self.total_shots = 0;
        self.total_hits = 0;
        self.turn_count = 0;
        self.turn_start_time = None;
        self.turn_times.clear();
        self.play_again_response = None;
        self.waiting_for_play_again = false;

        // Reset ship status
        for ship in &mut self.ship_status {
            ship.hits = 0;
            ship.sunk = false;
        }

        // Reset Last Stand state
        self.last_stand_used = false;
        self.last_stand_sequence.clear();
        self.last_stand_input.clear();

        // Reset card state
        self.current_hand.clear();
        self.shield_active = false;
        self.radar_reveals.clear();
    }

    // Last Stand methods
    pub fn trigger_last_stand(&mut self) {
        self.last_stand_used = true;
        self.last_stand_input.clear();

        // Generate a simple morse code sequence (S.O.S. variations)
        let sequences = [
            vec!['.', '.', '.', '-', '-', '-', '.', '.', '.'], // S.O.S.
            vec!['.', '-', '.'],                               // S.O.S. short
            vec!['.', '.', '-', '.', '-', '.'],                // S.O.S. medium
        ];

        // Pick a random sequence (for now, just use the first one)
        self.last_stand_sequence = sequences[0].clone();
    }

    pub fn check_last_stand_input(&mut self, key: char) -> Option<bool> {
        if key == '.' || key == '-' {
            self.last_stand_input.push(key);

            // Check if we have enough input
            if self.last_stand_input.len() == self.last_stand_sequence.len() {
                let success = self.last_stand_input == self.last_stand_sequence;
                return Some(success);
            }
        }
        None // Incomplete input
    }

    #[allow(dead_code)]
    pub fn restore_random_ship(&mut self) -> bool {
        // Find the smallest sunk ship to restore (Destroyer = 2 cells)
        let (length, name) = SHIPS[4]; // Destroyer is the smallest

        // Find all possible empty positions
        let mut empty_positions = Vec::new();
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                if self.own_grid[y][x] == CellState::Empty {
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
                if self.can_place_ship(x, y, length, horizontal) {
                    self.place_ship(x, y, length, horizontal);
                    self.messages
                        .push(format!("{} restored by Last Stand!", name));
                    return true;
                }
            }
        }

        false // Couldn't place ship anywhere
    }

    // Card system methods
    pub fn draw_card(&mut self) -> Option<PowerUp> {
        if self.current_hand.len() >= self.max_hand_size {
            return None; // Hand is full
        }

        use rand::Rng;
        let mut rng = rand::rng();
        let card_types = [
            PowerUp::Shield,
            PowerUp::Radar,
            PowerUp::Repair,
            PowerUp::MissileStrike,
        ];

        let card = card_types[rng.random_range(0..card_types.len())].clone();
        self.current_hand.push(card.clone());
        Some(card)
    }

    pub fn use_card(&mut self, card_index: usize) -> Option<PowerUp> {
        if card_index < self.current_hand.len() {
            Some(self.current_hand.remove(card_index))
        } else {
            None
        }
    }

    pub fn can_use_card(&self, card_index: usize) -> bool {
        card_index < self.current_hand.len()
    }

    #[allow(dead_code)]
    pub fn get_card_at(&self, index: usize) -> Option<&PowerUp> {
        self.current_hand.get(index)
    }

    pub fn activate_shield(&mut self) {
        self.shield_active = true;
        self.messages
            .push("🛡️ Shield activated! 50% damage reduction for 1 turn!".to_string());
    }

    pub fn deactivate_shield(&mut self) {
        if self.shield_active {
            self.shield_active = false;
            self.messages.push("🛡️ Shield expired!".to_string());
        }
    }

    #[allow(dead_code)]
    pub fn reveal_radar_positions(&mut self, enemy_grid: &[Vec<CellState>]) -> Vec<(usize, usize)> {
        let mut revealed = Vec::new();
        use rand::Rng;
        let mut rng = rand::rng();

        // Find all ship positions on enemy grid
        let mut ship_positions = Vec::new();
        for (y, row) in enemy_grid.iter().enumerate() {
            for (x, cell) in row.iter().enumerate() {
                if *cell == CellState::Ship {
                    ship_positions.push((x, y));
                }
            }
        }

        // Reveal up to 2 random ship positions
        let reveal_count = ship_positions.len().min(2);
        for _ in 0..reveal_count {
            if !ship_positions.is_empty() {
                let random_idx = rng.random_range(0..ship_positions.len());
                let pos = ship_positions.remove(random_idx);
                revealed.push(pos);
            }
        }

        self.radar_reveals = revealed.clone();
        self.messages
            .push("📡 Radar scan complete! Enemy positions revealed!".to_string());
        revealed
    }

    pub fn clear_radar_reveals(&mut self) {
        if !self.radar_reveals.is_empty() {
            self.radar_reveals.clear();
            self.messages.push("📡 Radar scan expired!".to_string());
        }
    }

    pub fn repair_ship(&mut self) -> bool {
        // Find a damaged ship to repair
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                if self.own_grid[y][x] == CellState::Hit {
                    // Check if this hit is part of a ship that's not fully sunk
                    if !GameState::is_ship_sunk_at(&self.own_grid, x, y) {
                        // Repair this hit (convert back to ship)
                        self.own_grid[y][x] = CellState::Ship;
                        self.messages.push("🔧 Ship repaired!".to_string());
                        return true;
                    }
                }
            }
        }
        false // No damaged ships to repair
    }

    #[allow(dead_code)]
    pub fn missile_strike(&mut self, enemy_grid: &mut [Vec<CellState>]) -> Vec<(usize, usize)> {
        let mut hits = Vec::new();
        use rand::Rng;
        let mut rng = rand::rng();

        // Find all empty and ship positions
        let mut targets = Vec::new();
        for (y, row) in enemy_grid.iter().enumerate().take(GRID_SIZE) {
            for (x, cell) in row.iter().enumerate().take(GRID_SIZE) {
                if *cell == CellState::Empty || *cell == CellState::Ship {
                    targets.push((x, y));
                }
            }
        }

        // Strike 2 random positions
        for _ in 0..2 {
            if !targets.is_empty() {
                let random_idx = rng.random_range(0..targets.len());
                let (x, y) = targets.remove(random_idx);
                let was_ship = enemy_grid[y][x] == CellState::Ship;
                enemy_grid[y][x] = if was_ship {
                    CellState::Hit
                } else {
                    CellState::Miss
                };
                hits.push((x, y));
            }
        }

        self.messages
            .push("🚀 Missile strike launched!".to_string());
        hits
    }
}
