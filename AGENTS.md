# boxvibe

Rust + ratatui TUI (edition 2024, `rust-version` 1.90). Work on `dev`.

## Commands

Use `CARGO_TARGET_DIR=$PWD/target` so builds stay in the repo, not `/tmp`.

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
```

CI (`.github/workflows/ci.yml`) also runs `cargo build --release --locked`. Linux needs `libasound2-dev` and `pkg-config`. Tests use demo mode or a fake server, never live Pandora.

## Layout

- `src/main.rs` — binary and module list
- `src/app/`, `src/ui.rs` — screens and input
- `src/pandora/` — Pandora client
- `src/audio.rs`, `src/art.rs`, `src/config.rs` — playback, covers, `config.toml`
- `docs/setup.md` — branch, CI, and repo-settings checklist
- `docs/screenshots/`, `scripts/screenshots.py` — generated images
- `packaging/` — linux package notes

Unit tests sit next to the code (`mod tests`). Scratch dirs go under `target/tmp`.

## Conventions

- Run the binary from `target/`. Do not install it to `PATH`.
- `dev`'s version stays ahead of the latest release on purpose. Do not revert it or hand-edit `Cargo.lock`.
- `rustfmt.toml`: `max_width = 120`. Do not set `TMPDIR` in `.cargo/config.toml`.
- `main` changes only through a pull request, and only to cut a release.
