# Agent rules

These rules apply to all agents that change this repository. Obey all rules in all changes.

## Language: ASD-STE100 style

Write all new and changed text in a style like ASD-STE100 Simplified Technical English. This
includes doc comments, code comments, Markdown files and commit messages. Follow the writing
rules below. The STE dictionary does not apply: use any clear English word.

- Keep sentences short: about 20 words for an instruction, about 25 words for a description.
- Write one instruction in each sentence.
- Use the imperative form for instructions.
- Use the active voice.
- Prefer simple tenses.
- Give each word one meaning, and use the same term for the same item each time.
- Use the names from the code for code items (for example `World`, `Simulation`).
- Use articles ("the", "a") where they make the text clearer.
- Avoid long noun clusters. Break up clusters of more than three words.
- Write short paragraphs with one topic each.
- Use a vertical list for a sequence of steps or for many items.
- Put a warning or a caution before the instruction it applies to.

Write a commit message as `area: summary`, with a lowercase summary. Use the crate or module
name as the area (for example `sim`, `render`, `net`).

## Lints: Clippy pedantic

The workspace denies all warnings, all Clippy `pedantic` lints, `missing_docs` and
`unsafe_code`. `Cargo.toml` sets this in `[workspace.lints]`.

- Keep `[lints] workspace = true` in the `Cargo.toml` file of each crate.
- Do not lower a lint level in a crate or in the code.
- Do not use `#[allow(...)]`. The workspace denies `clippy::allow_attributes`.
- Use `#[expect(lint, reason = "...")]` only if no change to the code can remove the lint.
- Do not add an item to `clippy.toml` to hide a lint.
- Do not add `unsafe` code.
- Use `zerocopy` to change plain data into bytes, for example for GPU buffers. Do not use
  `bytemuck`.
- Make sure that this command completes without warnings:

```sh
cargo clippy --workspace --all-targets -- -D warnings
```

## File size: 1000 lines

- Keep each source file at a maximum of 1000 lines. This includes integration tests.
- If a change makes a file longer than 1000 lines, split the file into submodules.
- If you change a file that is already longer than 1000 lines, do not make it longer. Split it
  as part of the change.
- Put each submodule in a new file below the directory of its parent module.
- Split by topic. Do not split at a random line.

Clippy also limits each function to 200 lines (`clippy.toml`).

## Layout: Cargo package layout

Follow the Cargo package layout:
<https://doc.rust-lang.org/cargo/guide/project-layout.html>.

- Put the library root in `src/lib.rs`.
- Put the default executable in `src/main.rs`.
- Put other executables in `src/bin/`.
- Put integration tests in `tests/`.
- Put examples in `examples/`.
- Put benchmarks in `benches/`.
- For a test, example, benchmark or executable with many files, make a directory with a `main.rs`
  file and the modules.
- Use snake_case names for modules.
- Put a module in `foo.rs` and its child modules in `foo/`. Do not use `mod.rs` files. The
  workspace denies `clippy::mod_module_files`.
- Put each library crate in `crates/<crate-name>/`. The root package `openensemble` contains the
  executables. Its small library (`src/lib.rs`) contains only the code that the executables
  share, for example the search for the game directory.

`tests/repository-standards.rs` checks the lint inheritance, the file size and the package
layout.

## Layers

This repository uses two sibling repositories through path dependencies. Clone them next to this
repository:

1. `../ensemble-formats`: parsers and writers for the game file formats.
2. `../ensemble-assets`: game data loading (`pipeline`) and the game database.
3. This repository: the game.

The crates of this repository are in four layers:

1. `xcore` and `motion`.
2. `sim`.
3. `net` and `render`.
4. The root package `openensemble`.

Two crates contain shared test code. Use them only as dev-dependencies:

- `test-support` depends on `sim`. It loads the installed game and has shared assertions.
- `render-test-support` depends on `render`. It has shared scene helpers for the renderer
  tests.

- Make a crate depend only on crates in lower layers.
- Do not make `sim` depend on `render`, `net`, `xcore` or a graphics, window or audio crate.
- Put a new format parser or writer in `ensemble-formats`. Put new game data loading in
  `ensemble-assets`.
- If a change needs a change in a sibling repository, change and commit each repository
  separately.

## Simulation rules

`sim` must match vanilla Halo Wars for network compatibility. Obey these rules in all `sim`
code.

- Keep the simulation deterministic. The same commands from the same state must give the same
  state on each machine.
- Do not let wall-clock time, thread timing or hash map iteration order change the simulation
  state. Use `BTreeMap` or sorted data where the order is important.
- Use `Random` (`crates/sim/src/random.rs`) for all random values in the simulation.
- Keep the command wire format byte-for-byte identical to vanilla Halo Wars.
- When a change alters a mechanic that a file in `docs/` describes, update that file.

## Tests

- Add a regression test for each defect. Make sure that the test fails before the fix.
- Do not use wall-clock bounds in tests. Advance the simulation with explicit updates.
- Make tests deterministic. Compare floating-point values with a tolerance.
- If a test needs the installed game, mark it with
  `#[ignore = "requires a local Halo Wars DE installation"]`. Read the game directory from
  `OPENENSEMBLE_GAME_DIR` with `test_support::game_dir` or `test_support::installed_scenario`.
- Run the ignored tests with `cargo test --workspace -- --ignored`.

## Working copies

- Use one git worktree for each branch when agents work in parallel.
- Use absolute paths for all edits in a worktree. Do not edit the primary checkout from a
  worktree task.

## Checks

Make sure that all these checks pass before you report a change as complete:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
cargo test --workspace
```

`prek run --all-files` runs the format and Clippy hooks.
