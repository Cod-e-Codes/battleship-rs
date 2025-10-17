use crate::types::{CellState, GRID_SIZE, GamePhase};

pub struct GameState {
    pub own_grid: Vec<Vec<CellState>>,
    pub enemy_grid: Vec<Vec<CellState>>,
    pub phase: GamePhase,
    pub cursor: (usize, usize),
    pub placing_ship_idx: usize,
    pub placing_horizontal: bool,
    pub messages: Vec<String>,
    pub winner: Option<bool>,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            own_grid: vec![vec![CellState::Empty; GRID_SIZE]; GRID_SIZE],
            enemy_grid: vec![vec![CellState::Empty; GRID_SIZE]; GRID_SIZE],
            phase: GamePhase::Placing,
            cursor: (0, 0),
            placing_ship_idx: 0,
            placing_horizontal: true,
            messages: vec!["Place your ships! Use arrows, R to rotate, Enter to place".to_string()],
            winner: None,
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
                // Check adjacent cells
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let ny = y as i32 + dy;
                        let nx = (x + i) as i32 + dx;
                        if ny >= 0
                            && ny < GRID_SIZE as i32
                            && nx >= 0
                            && nx < GRID_SIZE as i32
                            && self.own_grid[ny as usize][nx as usize] == CellState::Ship
                        {
                            return false;
                        }
                    }
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
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let ny = (y + i) as i32 + dy;
                        let nx = x as i32 + dx;
                        if ny >= 0
                            && ny < GRID_SIZE as i32
                            && nx >= 0
                            && nx < GRID_SIZE as i32
                            && self.own_grid[ny as usize][nx as usize] == CellState::Ship
                        {
                            return false;
                        }
                    }
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
}
