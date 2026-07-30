# tetriCLI — terminal Tetris in Rust

A polished, terminal-based Guideline-style Tetris clone written in Rust, rendered
with `ratatui` and `crossterm`. All seven tetrominoes, SRS rotation with wall
kicks, 7-bag randomizer, hold, ghost piece, lock delay, DAS/ARR, scoring with
combos, back-to-back, T-Spin / T-Spin mini / perfect-clear detection, and a full
menu system with persistent, rebindable controls.

## Build & run

```sh
cargo run
```

Requires a stable Rust toolchain. Settings are stored in `config/settings.json`
and are created automatically with sensible defaults on first launch.

## Controls (defaults, fully rebindable)

| Action        | Default      |
|---------------|--------------|
| Move Left     | Left Arrow   |
| Move Right    | Right Arrow  |
| Soft Drop     | Down Arrow   |
| Hard Drop     | Space        |
| Rotate CW     | Up Arrow    |
| Rotate CCW    | Z            |
| Hold Piece    | C            |
| Pause         | Escape       |
| Restart       | R            |

## License

MIT