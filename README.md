# tetriCLI — terminal Tetris in Rust

A polished, terminal-based Guideline-style Tetris clone written in Rust, rendered
with `ratatui` and `crossterm`. All seven tetrominoes, SRS rotation with wall
kicks, 7-bag randomizer, hold, ghost piece, lock delay, scoring with combos,
back-to-back, T-Spin / T-Spin mini / perfect-clear detection, and a full
menu system with persistent, rebindable controls.

## Build & run

```sh
cargo run
```

Requires a stable Rust toolchain. Settings are stored in `config/settings.json`
and are created automatically with sensible defaults on first launch.

## How the game works

The field is the standard 10 columns × 20 visible rows, with 2 hidden spawn rows
above the top (22 rows internally). Pieces spawn at column 3 in the upright
rotation state. The game runs a fixed 60 FPS loop (~16 ms per frame).

A game ends on **block-out** (a new piece spawns into occupied cells) or
**lock-out** (any part of a piece locks above the visible field). The final
score, level, lines and playtime are written to your persistent statistics.

### Tetrominoes & randomizer
All seven pieces (I, O, T, S, Z, J, L) use the Guideline color set:
I=Cyan, O=Yellow, T=Magenta, S=Green, Z=Red, J=Blue, L=LightYellow. Pieces come
from a standard **7-bag** randomizer — each bag is a shuffled permutation of all
seven pieces, so no single piece can drought for too long. The next queue keeps
at least 5 pieces ready; the HUD previews the next **4**.

### Rotation: SRS with wall kicks
Rotations use the canonical Super Rotation System shape tables (4 states:
Spawn / Clockwise / One-Eighty / CounterClockwise) with wall-kick offsets.
`JLSTZ` pieces share one kick table and the `I` piece has its own; the identity
`(0,0)` offset is tried first, then the standard ordered alternatives. The
5th JLSTZ kick is used to promote a T-Spin into a full T-Spin (the TST-style
kick).

### Hold
Swap the active piece with the held one using the Hold key. You can only hold
once per piece — the lock of the new piece re-enables holding. A swapped-in
piece always re-spawns in the Spawn rotation. The hold slot starts empty.

### Ghost piece
A dimmed `▒` preview shows where the active piece would land. It is only drawn
on cells not already covered by the active piece, so it never obscures your
piece. Toggle it via the `show_ghost` setting (on by default).

### Soft drop & hard drop
- **Soft drop** (hold the key): gravity is divided by `soft_drop_factor`
  (default 20), awarding **+1 point per cell** dropped.
- **Hard drop**: instantly slams the piece to the floor and locks it
  immediately, awarding **+2 points per cell** dropped. Hard drops skip the
  post-lock spawn delay.

### Gravity & level progression
You start at the configured **starting level** (1–20). Every 10 lines cleared
raises the level by one. Gravity (ms per cell) ramps up quickly:

| Level | ms |  | Level | ms |  | Level | ms |
|------:|---:|--|------:|---:|--|------:|---:|
| 1 | 1000 | | 8 | 158 | | 15 | 22 |
| 2 | 793  | | 9 | 123 | | 16 | 16 |
| 3 | 617  | | 10 | 94 | | 17 | 12 |
| 4 | 472  | | 11 | 70 | | 18 | 9 |
| 5 | 360  | | 12 | 52 | | 19 | 6 |
| 6 | 270  | | 13 | 39 | | ≥20 | 1 |
| 7 | 207  | | 14 | 29 | | | |

From level 20 onward the field is essentially 20G (fraction-of-a-frame drop).
Gravity is accumulated as a float and consumed cell-by-cell, so high levels step
multiple cells per frame correctly.

### Lock delay & move reset
Once a piece touches down, a **lock delay** of 500 ms (configurable via
`lock_delay_ms`) starts before it locks. Moving or rotating the grounded piece
resets that timer up to **15 times** (`max_lock_resets`) — the "infinity"
cap — after which the piece locks regardless. A brief 16 ms spawn-delay after a
normal lock (skipped on hard drop) keeps the next piece from spawning on the
same frame.

### Scoring
All line-clear base scores are multiplied by the current level (min 1):

| Clear | Base pts |
|---|---|
| Single | 100 |
| Double | 300 |
| Triple | 500 |
| Tetris (4 lines) | 800 |
| T-Spin Single | 800 |
| T-Spin Double | 1200 |
| T-Spin Triple | 1600 |
| T-Spin Mini (1 line) | 200 |
| T-Spin (no lines) | 400 |

- **Back-to-back (B2B)**: chaining "difficult" clears (a Tetris, or any T-Spin
  Single/Double/Triple — *mini does not count*) earns a **×1.5** bonus on the
  next difficult clear. Any non-difficult line clear breaks the chain; a T-Spin
  with no lines keeps B2B alive without scoring lines.
- **Combo**: each consecutive lock that clears at least one line increments the
  combo counter, adding `50 × (combo − 1) × level` bonus on top (only when
  combo > 1). A lock with no lines resets the combo.
- **Perfect Clear**: when the field becomes empty after a clear you receive an
  extra bonus — `800 × lvl` (1 line), `1200 × lvl` (2), `1800 × lvl` (3) or
  `2000 × lvl` (Tetris) — added on top of the line-clear score.

### T-Spin detection
Uses the **3-corner Standard rule** on the T piece's rotation center: at least 3
of the 4 diagonal corners must be filled (walls and floor count as filled; the
area above the field counts as empty), and the last action must have been a
successful rotation. If both "front" corners (the side the T faces) are empty
it's a **T-Spin Mini**, otherwise a full **T-Spin**. A Mini is promoted to full
when the 5th SRS kick was used, and any Mini that clears 2+ lines is promoted to
full as well. The HUD flashes a banner ("T-SPIN", "T-SPIN SINGLE", …,
"PERFECT CLEAR (TETRIS)!") for ~2 seconds after the clear.

### Controls (defaults, fully rebindable)

| Action        | Default      |
|---------------|--------------|
| Move Left     | Left Arrow   |
| Move Right    | Right Arrow  |
| Soft Drop     | Down Arrow   |
| Hard Drop     | Space        |
| Rotate CW     | Up Arrow     |
| Rotate CCW    | Z            |
| Hold Piece    | C            |
| Pause         | Escape       |
| Restart       | R            |

> Movement is one cell per key press (auto-repeat / DAS-ARR is intentionally
> disabled). The `das`/`arr` settings remain in the JSON for backward
> compatibility but are not used by the gameplay loop.

## Features

**Gameplay**
- All 7 tetrominoes with Guideline colors
- 7-bag randomizer with a 4-piece next queue
- SRS rotation with JLSTZ and I-piece wall-kick tables
- Hold (once per piece), ghost piece, soft & hard drop
- Lock delay with a capped move (infinity) reset
- Gravity table from level 1 → 20G; level-up every 10 lines
- Scoring with **combos**, **back-to-back**, and **T-Spin / T-Spin Mini** detection
- **Perfect Clear** detection and bonus scoring
- Block-out / lock-out game-over detection

**UI**
- Splash screen with the tetriCLI logo
- Centered playfield with configurable block size (`grid_scale` 1–4, default 2)
- Right-hand HUD: Hold, Next (4 pieces), Stats (score/level/lines), Info (combo/B2B/time)
- Last-clear banner (Singe / Double / Tetris / T-Spin … / Perfect Clear) shown for ~2s
- Optional FPS counter (toggle via `show_fps`)
- Pause and Game Over overlays rendered over the live field

**Menus & navigation**
- Main menu: Play / Settings / Statistics / Quit
- Settings → Controls (rebind any of the 9 actions, captured live and saved immediately)
- Settings → Gameplay (Starting Level 1–20, Grid Scale 1–4)
- Pause menu: Resume / Restart / Main Menu / Quit
- Game Over menu: Play Again / Main Menu / Quit
- Statistics screen: lifetime games played, total lines, best score, best level, total playtime
- Menu navigation is fixed (↑/↓ or `k`/`j`, Enter to select, Esc to back) and independent of the rebindable gameplay keys

**Persistence**
- `config/settings.json`, created on first launch and deep-merged over defaults so partial/corrupt files never break startup
- Controls and gameplay options are saved the moment you change them
- Lifetime statistics are updated and saved at the end of every game

## License

MIT