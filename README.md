# ssh-battleship

Terminal-based networked Battleship game written in Rust.

## Features

- Two-player networked gameplay over TCP
- Single-player mode against AI opponent
- Terminal UI using ratatui
- SSH tunnel compatible for remote play

## Requirements

- Rust 1.70+
- Terminal with Unicode support

## Installation

```bash
git clone https://github.com/Cod-e-Codes/ssh-battleship
cd ssh-battleship
cargo build --release
```

## Usage

### Two-Player Game

Start server:
```bash
cargo run --release -- server 8080
```

Connect players (in separate terminals):
```bash
cargo run --release -- client 127.0.0.1:8080
```

### AI Opponent

Start AI server:
```bash
cargo run --release -- server-ai 8080
```

Connect:
```bash
cargo run --release -- client 127.0.0.1:8080
```

### Remote Play via SSH

On server machine:
```bash
cargo run --release -- server 8080
```

On client machine:
```bash
ssh -L 8080:localhost:8080 user@server-host
cargo run --release -- client 127.0.0.1:8080
```

## Controls

- Arrow keys: Move cursor
- R: Rotate ship during placement
- Enter: Place ship / Fire at position
- Q: Quit

## Game Rules

- Standard Battleship rules
- 10x10 grid
- 5 ships: Carrier (5), Battleship (4), Cruiser (3), Submarine (3), Destroyer (2)
- Ships cannot touch (including diagonally)
- Players alternate turns after placement phase
- First to sink all opponent ships wins

## Architecture

```
src/
├── main.rs       - Entry point and CLI
├── types.rs      - Core types and messages
├── game_state.rs - Game logic
├── ui.rs         - Terminal rendering
├── input.rs      - Keyboard handling
├── client.rs     - Client implementation
├── server.rs     - Two-player server
└── server_ai.rs  - AI opponent server
```

## Network Protocol

JSON messages over TCP, newline-delimited. Message types:
- `PlaceShips`: Send board configuration
- `Attack`: Fire at coordinates
- `AttackResult`: Hit/miss/sunk feedback
- `YourTurn` / `OpponentTurn`: Turn management
- `GameOver`: End game state

## License

MIT