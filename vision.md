# Vision
## Purpose
Pandora radio in the terminal: stations, big cover art, thumbs, a song menu and station editor, in a fast keyboard-driven Rust ratatui app.
## In scope
CarPlay-style Now Playing, station create/edit (seeds, thumbs, modes, favorites), the `.` song menu, Omarchy live themes and Theme Select, album art (sixel/kitty/iTerm2 or half-blocks, retro/tint), MPRIS, GitHub self-update, pure-Rust MP3/AAC decode, keychain login, `--demo`, and CI (fmt, tests, clippy, MSRV, audit, CodeQL).
## Out of scope
ffmpeg and mpv; official Pandora affiliation or a supported API; tests that call live Pandora; treating planned AUR/.deb packaging as already shipped.
## Design principles and non-negotiables
Share vibekit theming and style. Capital `Q` is the only quit key; lowercase `q` never quits. Shared keys are `Q`, Esc and `?`. Passwords stay in the system keychain, not `config.toml`. Tests never call live Pandora. The repo is public and auditable. Git is `dev`-only, one `dev`→`main` PR at a time, and no push or merge to `main` without Toby's OK. The unofficial Pandora API may conflict with Pandora's ToS.
## Bots may decide alone
In-scope TUI, playback, theme, and art work already described in the README; documented config migrations; relevant tests and CI; docs that match existing behavior; the documented post-release patch bump on `dev`.
## Captain decision hold (needs Toby)
Behavior changes, new dependencies, and anything security- or data-related, including password/keychain handling, playback logs, unofficial-API use, and Good Vibes Only / ad-skip policy. Minor or major version bumps, cutting a release, and any push or merge to `main`.
