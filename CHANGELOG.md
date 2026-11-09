# Changelog

Short and scannable; the git history has the details.

## 0.6.0 (2026-11-09)

### Fixed
- **Free-plan audio sounds as good as it can.** Pandora's free tier serves 64 kbps HE-AAC: a 22.05 kHz AAC-LC core that SBR doubles to 44.1 kHz, with PS for stereo. symphonia decoded only the core, so it came out at half the rate, mono, with no high band — the duller sound in issue #11. AAC now goes to `syom`, which applies SBR and PS; MP3 stays on symphonia.
- The container decides the decoder, not a trial decode: an MP3 behind an ID3v2 tag was reported as `Malformed(AdtsLayer)` rather than `NotAac`, so an error-based fallback sent every MP3 to the AAC decoder and skipped it. Detection looks past the ID3 tag and walks leading MP4 boxes, since `ftyp` is not always first. A container that looks like AAC but cannot be read is still tried as MP3, so a wrong guess cannot cost a song.
- Changing the look with `Shift+R` / `Shift+H` / `Shift+C` swaps the cover in place instead of tearing the protocol down, so it costs one repaint of the art box rather than a full-screen flash. Prepared covers are cached per (url, look).

### Added
- **Playback log** at `~/.cache/boxvibe/boxvibe.log`: each song as it starts (what Pandora sent, what it decoded to, whether it was resampled), each as it ends (how far it got against how long it should have been), and every reason a song was skipped. Capped at 512 KB and rolls over to `.old`, so it can be left on. `BOXVIBE_LOG=off` silences it; `BOXVIBE_LOG=<file>` writes somewhere else.
- Fragments ask for `supportedFormats` as the site does, and the bitrate Pandora reports is kept, so the interface says what is really playing rather than what was asked for.
- The status bar names the stream playing (`MP3 192 kbps`, `AAC 64 kbps`) to the left of the account, and follows each song.
- **Recently Played** in My Collection, read from the account's list.
- Browse playlists and podcasts; remove a local recent entry.

### Changed
- **Audio quality is greyed out on a free plan.** `high` and `standard` both request `mp3` there, so the toggle changed nothing; Enter now explains instead of toggling, and logging out forgets the plan so the next account is not greyed.
- My Stations and Pinned sit under a **MY COLLECTION** heading; pinned Browse sections sit under a **BROWSE (n)** heading.
- Settings shows what is really playing next to Audio quality.

### Internal
- CI runs on every push and pull request to `main` and `dev`: `fmt`, `ci` (release build, tests, clippy), `msrv` (a build on the `rust-version` in `Cargo.toml`), `audit` and `codeql`. A pull request into `main` needs `fmt`, `ci` and `msrv` green.
- The branch and version policy is written down in the README's Development section.
- Tests keep their scratch directories under `target/tmp` rather than `/tmp`, and no longer spawn `notify-send`.

## 0.5.0 (2026-11-06)

### Internal
- Themes, Theme Select (state, keys and drawing), the theme store and self-update now live in the shared `vibekit` crate (path dependency).
- The quit, unpin, delete, station-editor and theme-delete questions, Help, Details and System Check now use vibekit 0.2.0's `Confirm`, `Help` and `DialogStack`.

### Changed
- Keys follow the shared vibe shortcuts: a second `Q` confirms quit; questions answer only on y / n / Enter / Esc; Help closes on `?` or Esc; Details, System Check, panels and the song menu close on Esc only (lowercase `q` no longer closes anything, and no box closes on any key).
- **Space** pauses from anywhere; lowercase `p` is unbound.
- **G** is Genres globally; **End** (not G) jumps to the end of a list or popup.
- Stations menu/page label is **My Stations**; the parent **Browse** menu entry and `[browse] browse` master toggle are removed (section pins default On).
- A Browse section pin opens that section's full View-all page (`feed/getDirectory` with `deviceType` / `path` / `feedType`), not the short teaser. New Music shows every section the page returns (stations, recommended new releases, all new albums) with no row cap. Artists Picked For You has no View-all, so it stays the teaser. A failed View-all falls back to the teaser and says so.

### Added
- **Browse sections** on the main menu via `feed/getDirectory`. Settings → BROWSE toggles each section (default On). No parent Browse menu entry; **G** opens Browse Genres. Enter starts a station when the row can.
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
- README screenshots come from `scripts/screenshots.py`; the old `tools/screenshots.sh` and `screenshots/` are gone.
- Pressing the same thumb again takes it back; `b` in Up Next waits for the song's turn.
- The key bar leaves song shortcuts to `.` (they still work and are in Help).
- Loudness follows Pandora's `fileGain`; windowed-sinc resampling and a bigger buffer (no Bluetooth crackle).
- Now Playing has three columns of side padding.

### Fixed
- Missing covers retry every ~60 s; failures go to `~/.cache/boxvibe/art.log`.
- Theme picker tags and hints are never clipped.
- The Add station and Add a seed hints always fit (were cut to "Esc = cance").

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
