# Changelog

Short and scannable; the git history has the details.

## 0.5.0 (unreleased)

### Added
- **Song menu** (`.`): start a station from the artist or song, Tired, Share, Why This Song?, Similar Artists & Bio, Station Mode, Pin, thumbs, Details, Sleep.
- **Station modes** (`m`): My Station, Crowd Faves, Discovery, Deep Cuts, Newly Released, Artist Only (Premium ones shown locked).
- **Theme Select** browser: fuzzy-search ~900 iTerm2-Color-Schemes plus the built-ins; download, use, delete. **Theme Auto Mode** follows Omarchy.
- **Sleep timer** (`B`): 15 / 30 / 60 / 90 min or end of song, with a 10 s fade.
- **Pinned songs** (`I`) with their own menu page, kept in `pinned.toml`.
- **Shuffle** (`X`) and **Notifications** page (the status bar's history).
- **MPRIS**: media keys, `playerctl` and desktop player widgets.
- **Good Vibes Only** has a **Muted** step (On / Muted / Off).
- Status bar shows the look (theme, art, Retro, tint) in the account colour, and `omarchy: <Theme>` when following Omarchy.
- Settings: **Favorites in menu**, **Pinned in menu**.
- `e` on genre stations opens their thumbs.

### Changed
- Pressing the same thumb again takes it back; `b` in Up Next waits for the song's turn.
- The key bar leaves song shortcuts to `.` (they still work and are in Help).
- Loudness follows Pandora's `fileGain`; windowed-sinc resampling and a bigger buffer (no Bluetooth crackle).
- Now Playing has three columns of side padding.

### Fixed
- Missing covers retry every ~60 s; failures go to `~/.cache/boxvibe/art.log`.
- Theme picker tags and hints are never clipped.

## 0.4.0 (2026-10-24)

### Added
- **Make a station by searching** (`a`), **Genres** (`G`), **station editor** (`e`: Seeds, Thumbs Up, Thumbs Down tabs).
- Rename (`r`), delete (`x`) and details (`v`) for stations; `r` / `e` also from Now Playing.
- **Good Vibes Only**: plays and reports Pandora's audio ads like the website (with risk popups to change it).
- Password in the **system keychain** (Secret Service), asked in a popup with a clear risk notice.
- **System Check** for the Nerd Font and `notify-send`.
- Boot screen, **Animations** setting, 64-bit Retro step, station icons (genre logo / your own).
- Text boxes with a real cursor.
- CI, CodeQL, `cargo audit` and Dependabot.

### Changed
- **Keys (breaking)**: Shift+letter goes places from anywhere, lowercase acts on the pane. `N` skips, `P` Now Playing, `U`/`D` thumbs, `l`/`b` rate a row, `Z` tired, `O` Settings, `C`/`R`/`H` art, Retro, tint.
- Config keys match their Settings rows (`[account]`, `[boxvibe]`, `[playback]`, `[look]`); old names still load and are rewritten on save.
- Settings starts with ACCOUNT; Nerd Font icons are the default; first run follows Omarchy if present.
- Album art is Picture or Off (no Blocks); the spectrum visualizer is gone.
- The key bar only hints keys that work where you are.

### Fixed
- Seed suggestions no longer show "Service method not found".
- Covers retry after 2, 6 and 15 s; package-manager installs don't offer a self-update.
- Help's station icons are centred; station rows line up.

## 0.3.0 (2026-10-23)

### Added
- **Tint art with theme colors** (Duotone / Palette) and **Retro mode** (HD … 2-bit).
- **App menu entry** in Settings (a `.desktop` file).
- RATED column in Up Next; account at the right of the status bar.

### Changed
- Cover has no frame; song info, time and buttons form one centred block.
- Popups dim what's behind them; terminal pictures are held back under popups.
- Help is as wide as its longest line.

## 0.2.0 (2026-10-22)

### Added
- **Album art**: sixel, kitty or iTerm2 pictures, half blocks elsewhere; cached on disk (50 MB).
- **Stations page** and **Favorites** (`f`), **NOW PLAYING** menu entry.
- **Restart Now** after Update Now.
- Settings: border style, menu width (`{` `}`), song change notifications, autoplay last station.

### Changed
- Up Next and History side by side (stacked when narrow), 10 songs each.
- Quit is `Q` (asks first) or `Ctrl+c`; volume is `Shift+←/→`; lists wrap around.
- The logo sits on the menu's frame; the status moved to a bar above the keys.

### Fixed
- A broken `config.toml` never stops boxvibe: it is moved aside and defaults are used.
- Long song text no longer runs into the time or rating columns.

## 0.1.0

First version: log in from the app, play your stations through Pandora's website API, thumbs, skip, tired, volume, themes (Omarchy + 8 built-in), three icon sets, `--demo` mode and self-update from GitHub releases.
