# Repository Guidelines

## Code size

- Source files may contain up to 1,000 lines.
- Keep functions within Clippy's default `too_many_lines` threshold.
- Do not bypass size lints with `allow` attributes or custom threshold increases; split code into focused modules or helpers when needed.

## Cargo project layout

- Follow the [Cargo project layout](https://doc.rust-lang.org/cargo/guide/project-layout.html): keep each package manifest at its package root and Rust source under `src`.
- Use `src/lib.rs` and `src/main.rs` for default library and executable targets.
- Put additional executables in `src/bin`; multi-file targets use `src/bin/<target>/main.rs` with their supporting modules beside it.
- Name new executable, example, benchmark, and integration-test targets in kebab-case. Preserve established target names only when compatibility requires it.
- Name Rust modules and their files in snake_case.
