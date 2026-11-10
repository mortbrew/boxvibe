# Status

## Done

- CarPlay-style TUI: stations, thumbs, themes, album art, MPRIS, self-update.
- CI on `dev` and `main`: fmt, release build, tests, clippy, MSRV, audit, CodeQL.

## Next

- Nothing queued in-tree. Repo settings live in `docs/setup.md`.

## Decisions

- `dev` is the working branch. `main` is a release marker.
- The `Cargo.toml` version on `dev` stays ahead of the newest tag so the UI shows a source build.
- Tests never call live Pandora.
