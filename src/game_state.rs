use crate::types::{CellState, GRID_SIZE, GamePhase, SHIPS};
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
    pub show_side_panel: bool,
    pub ship_status: Vec<ShipStatus>,
    pub total_shots: usize,
    pub total_hits: usize,
    pub turn_count: usize,
    pub turn_start_time: Option<Instant>,
    pub turn_times: Vec<f64>, // Store last 10 turn times
    // Play again functionality
    pub play_again_response: Option<bool>,
    pub waiting_for_play_again: bool,
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
            show_side_panel: false,
            ship_status,
            total_shots: 0,
            total_hits: 0,
            turn_count: 0,
            turn_start_time: None,
            turn_times: Vec::new(),
            // Play again functionality
            play_again_response: None,
            waiting_for_play_again: false,
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
    }
}
