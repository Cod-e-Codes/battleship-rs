use serde::{Deserialize, Serialize};

pub const GRID_SIZE: usize = 10;
pub const SHIPS: [(usize, &str); 5] = [
    (5, "Carrier"),
    (4, "Battleship"),
    (3, "Cruiser"),
    (3, "Submarine"),
    (2, "Destroyer"),
];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CellState {
    Empty,
    Ship,
    Hit,
    Miss,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    PlaceShips(Vec<Vec<CellState>>),
    Attack {
        x: usize,
        y: usize,
    },
    AttackResult {
        x: usize,
        y: usize,
        hit: bool,
        sunk: bool,
    },
    YourTurn,
    OpponentTurn,
    GameOver {
        won: bool,
    },
    WaitingForOpponent,
    GameStart,
    PlayAgainRequest,
    PlayAgainResponse {
        wants_to_play: bool,
    },
    PlayAgainTimeout,
    OpponentQuit,
    NewGameStart,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GamePhase {
    Placing,
    WaitingForOpponent,
    YourTurn,
    OpponentTurn,
    GameOver,
    PlayAgainPrompt,
}
