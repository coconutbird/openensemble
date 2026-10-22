# OpenEnsemble

OpenEnsemble is a Rust reimplementation of the Halo Wars engine (Halo Wars Definitive Edition).
The simulation must match vanilla Halo Wars, so that a game can stay in lockstep with the retail
game. The renderer uses `wgpu`. You must own the game: this repository contains no game data.

## Repositories

The project has three repositories. Each repository uses the one before it:

1. [`ensemble-formats`](https://github.com/coconutbird/ensemble-formats): parsers and writers
   for the game file formats.
2. [`ensemble-assets`](https://github.com/coconutbird/ensemble-assets): game data loading
   (`pipeline`) and the game database.
3. `openensemble` (this repository): the simulation, the renderer and the executables.

`openensemble` uses `../ensemble-formats` and `../ensemble-assets` through path dependencies, and
its `[patch]` section points the git dependencies of `ensemble-assets` at `../ensemble-formats`.
`ensemble-assets` uses `ensemble-formats` through git dependencies.

## Build

1. Clone the three repositories into the same parent directory:

   ```sh
   git clone https://github.com/coconutbird/ensemble-formats
   git clone https://github.com/coconutbird/ensemble-assets
   git clone https://github.com/coconutbird/openensemble
   ```

2. Copy `.env.example` to `.env`.
3. In `.env`, set `OPENENSEMBLE_GAME_DIR` to the Halo Wars DE installation directory.
4. Build the workspace:

   ```sh
   cd openensemble
   cargo build --workspace
   ```

`openensemble::game_dir` (`src/lib.rs`) uses `OPENENSEMBLE_GAME_DIR` if the path exists.
Otherwise, it uses the current directory.

The root library (`src/lib.rs`) also holds the code that the executables share:

- `openensemble::session`: loads a scenario, steps its simulation, refreshes the unit visuals and
  makes the `WorldView` that draws it.
- `openensemble::frame_stats`: measures the frame, simulation and render times.

## Crates

| Crate | Purpose |
|---|---|
| `crates/motion` | Skeletal animation sampling and attachment transforms, shared by `sim` and `render`. |
| `crates/sim` | Deterministic lockstep simulation: commands, entities, orders, powers and triggers. |
| `crates/render` | `wgpu` renderer: UGX models, terrain, particles, lights, shadows, terrain effects and the window loop. `render::world_view::WorldView` draws a loaded scenario. |
| `crates/net` | UDP host and client transport. No executable uses it yet. |
| `crates/test-support` | Test helpers that load the installed game. Use only as a dev-dependency. |
| `crates/render-test-support` | Scene helpers for the renderer tests. Use only as a dev-dependency. |

## Executables

| Executable | Purpose |
|---|---|
| `openensemble` (`src/main.rs`, `src/client/`) | Game client. Loads a 1v1 skirmish as the human player, steps the simulation and draws it with `WorldView` under an RTS camera. |
| `headless` (`src/bin/headless.rs`) | Runs the simulation without a renderer. |
| `terrain_viewer` (`src/bin/terrain_viewer/`) | Loads a scenario and shows it with `WorldView`: terrain, foliage, roads, units and effects. Steps the simulation, moves the camera and writes top-down captures. |
| `renderer-asset-audit` (`src/bin/renderer-asset-audit.rs`) | Prints an inventory of the visual features that the installed assets use. |
| `renderer-gpu-smoke` (`src/bin/renderer-gpu-smoke.rs`) | Creates renderer pipelines that the viewer does not always create, without a window. |
| `scenario-trigger-audit` (`crates/sim/src/bin/`) | Prints the trigger DBIDs of installed scenarios. |

## Run

- To play Blood Gulch, run `cargo run --release --bin openensemble`.
- To play a different scenario, add the scenario name, for example
  `cargo run --release --bin openensemble -- tundra`. Add `--seed N` to change the random seed.
- Client controls:
  - `W`, `A`, `S`, `D`, the arrow keys or the mouse at a window edge scroll the camera.
  - `Insert` and `Delete`, or `Alt` and a mouse move, turn the camera. `Home` resets the turn and
    the zoom.
  - `Alt` and the mouse wheel, `Page Down` and `Page Up` zoom the camera in and out.
  - A left click selects a unit or a building (an own building opens its menu). A left drag
    selects own army units in the box. `Shift` and a left click adds or removes a squad;
    `Shift` and a drag adds the box. A double click or `Ctrl` and a left click selects the same
    kind of unit on the screen. `Q` selects the army on the screen, `E` the whole army.
  - A right click moves or attacks with the selected squads (`Shift` queues the order).
  - `Shift` makes the scroll and the zoom faster.
  - `Escape` quits.
- The client camera uses the retail values of `startup\game.cfg` (yaw, pitch, zoom, field of
  view, scroll, turn and zoom speeds). The client shows the frame rate in the top-left corner.
- To open the terrain viewer on Blood Gulch, run `cargo run --release --bin terrain_viewer`.
- To open a different scenario, add the scenario name or an `.xtd` path:
  `cargo run --release --bin terrain_viewer -- <SCENARIO>`.
- To see the viewer options, run `cargo run --bin terrain_viewer -- --help`.
- The viewer information window (`Tab`) shows the frame rate and the CPU time of the simulation
  update and the render encode. The viewer logs a warning when the average frame takes more than
  33 ms, or when a trigger script loops in one update.
- To run the simulation without a window, run `cargo run --bin headless -- --ticks 1000`.

## Tests

- To run the tests that do not need the game, run `cargo test --workspace`.
- To run the tests that load the installed game, set `OPENENSEMBLE_GAME_DIR`, then run
  `cargo test --workspace -- --ignored`. The run takes about 3 minutes.
- The dev and test profiles build all dependencies at `opt-level = 3` without debug information,
  and the `render` and `motion` crates at `opt-level = 1` (`Cargo.toml`). Thus a debug build
  loads a scenario in seconds. The first build of the dependencies takes longer.
- Before a commit, make sure that `cargo clippy --workspace --all-targets -- -D warnings`
  completes without warnings.

Read [AGENTS.md](AGENTS.md) before you change the code.

## Status

- Simulation coverage: [docs/simulation-coverage.md](docs/simulation-coverage.md).
- Trigger effects and conditions: [docs/triggers.md](docs/triggers.md).
- Renderer and application coverage: [docs/render-coverage.md](docs/render-coverage.md).
- Terrain shaders: [docs/hwde-shader-comparison.md](docs/hwde-shader-comparison.md).
- Mechanics:
  - [aircraft-avoidance.md](docs/aircraft-avoidance.md)
  - [capture.md](docs/capture.md)
  - [combat.md](docs/combat.md)
  - [energy-shields.md](docs/energy-shields.md)
  - [entity-management.md](docs/entity-management.md)
  - [garrison-and-teleporters.md](docs/garrison-and-teleporters.md)
  - [gathering.md](docs/gathering.md)
  - [healing.md](docs/healing.md)
  - [infection.md](docs/infection.md)
  - [movement-and-physics.md](docs/movement-and-physics.md)
  - [production.md](docs/production.md)
  - [repair-other.md](docs/repair-other.md)
  - [research.md](docs/research.md)

Known defects are in the
[issue tracker](https://github.com/coconutbird/openensemble/issues).
