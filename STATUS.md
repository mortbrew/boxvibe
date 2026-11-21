# Status

## Done

- CarPlay-style TUI: stations, thumbs, themes, album art, MPRIS, self-update.
- CI on `dev` and `main`: fmt, release build, tests, clippy, MSRV, audit, CodeQL.
- 0.6.5 playback recovery: a dead device is reopened or the silent clock takes over, a stall has to last, media fetches stay on HTTPS, transient HTTP errors are retried, the playback log is written off the UI thread, cover tint runs off the UI thread, a short terminal keeps a row for Up Next and History, and a failed config save stays on screen until one works.
- List polish: `/` filters My Stations, Browse, and Settings; `o` sorts My Stations (name, recent, thumbs) and the choice is saved; the last view, pane, and volume are remembered; help groups keys and moves ones that do nothing here to NOT ON THIS SCREEN; Log out asks first; the song card shows a year and an explicit tag when Pandora sends them. The status bar says when audio is buffering or playing silently. Clipboard, notifications, and a short HTTP retry pause stay off the UI thread. A failed config save is a suffix. A device error reopens the device and skips only when the song has stalled.

## Next

- Nothing queued in-tree. Repo settings live in `docs/setup.md`.

## Decisions

- `dev` is the working branch. `main` is a release marker.
- The `Cargo.toml` version on `dev` stays ahead of the newest tag so the UI shows a source build.
- Tests never call live Pandora.
