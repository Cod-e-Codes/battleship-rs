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
    LastStandTrigger,
    LastStandResult {
        success: bool,
    },
    CardDrawn {
        card: PowerUp,
    },
    CardUsed {
        card: PowerUp,
        target_x: Option<usize>,
        target_y: Option<usize>,
    },
    CardEffect {
        effect_type: String,
        data: String, // JSON data for effect-specific information
    },
    GridUpdate {
        grid: Vec<Vec<CellState>>,
    },
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
    LastStand,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SidePanelMode {
    Statistics,
    Deck,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PowerUp {
    Shield,        // Reduce 50% damage to all ships for 1 turn
    Radar,         // Reveal 2 random enemy ship positions for 1 turn
    Repair,        // Restore 1 HP to a ship
    MissileStrike, // Hit 2 random tiles on opponent grid
}

impl PowerUp {
    pub fn name(&self) -> &'static str {
        match self {
            PowerUp::Shield => "Shield",
            PowerUp::Radar => "Radar",
            PowerUp::Repair => "Repair",
            PowerUp::MissileStrike => "Missile Strike",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            PowerUp::Shield => "Reduce 50% damage for 1 turn",
            PowerUp::Radar => "Reveal 2 enemy positions",
            PowerUp::Repair => "Restore 1 HP to a ship",
            PowerUp::MissileStrike => "Hit 2 random enemy tiles",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            PowerUp::Shield => "🛡️",
            PowerUp::Radar => "📡",
            PowerUp::Repair => "🔧",
            PowerUp::MissileStrike => "🚀",
        }
    }
}
