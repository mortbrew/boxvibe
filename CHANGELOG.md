# Changelog

Short and scannable; the git history has the details.

## Unreleased

## 0.6.3 (2026-11-13)

### Fixed
- **Songs with a fractional M4A edit list are no longer skipped.** syom refused them because the edit duration was not a whole number of samples, and the symphonia fallback then failed with an I/O error. The edit list is dropped and the song plays at 44.1 kHz.
- **HE-AAC keeps its gapless trim.** Stripping every ID3 tag also removed the `iTunSMPB` tag on ADTS, so those songs played a little long.
- **Ads syom cannot decode play again.** Some ads stop on an AAC extension syom does not know. The MP4 decoder had been removed from the fallback, so those ads were skipped. It is restored.

### Changed
- **Now Playing stacks the cover over the song** when the window is too narrow for them side by side.
- Settings rows are drawn with vibekit's shared rows.

### Internal
- vibekit 0.3: shared text input, dialogs, private file writes, desktop entry, and process helpers. Pinned to tag `v0.3.0`.
- ADTS HE-AAC is decoded a few frames at a time instead of expanding the whole song to PCM first.
- Audio downloads carry a generation, so a fetch abandoned by a skip cannot start the next song.
- Ad tracking is one worker with a fixed-size queue. Logout wipes credentials. Idle frames are not redrawn.

## 0.6.2 (2026-11-12)

### Fixed
- **A sound device that goes away no longer freezes the player.** A dead stream reports nothing: its queue still holds the song, so it never looks empty, and its position stops advancing because nothing is pulling samples. The progress bar sat frozen for the rest of the session. A position that has not moved for three seconds is now treated as a lost device: the song is skipped and the status line says so.
- **Pausing for more than three seconds no longer skips the song.** The stall check above returns early while paused, so resuming looked like a stream that had died — the position was unchanged and its timestamp was old. Resuming starts the clock again.
- **A stream error no longer prints over the interface.** rodio's default error callback writes to stderr, which the TUI owns: the message landed in the middle of the screen and was wiped by the next frame. The callback is boxvibe's now, and the message goes to the playback log and the status line.
- **A cover that arrives twice no longer evicts one that is still cached.** The in-memory cover cache kept a map and an arrival order as separate fields, so a repeated url took two places in the order and pushed out a cover that was still in use.
- **A held dislike no longer survives into another station.** Tuning away or logging out cleared the queue but not the held dislikes, so a dislike held on one station could auto-skip the same track on the next.
- **"Buffering…" can no longer hang for the rest of the session.** A lost audio answer left it on screen with no song and no error; it now gives up after 30 seconds with a status line and a skip. A panic in the audio worker answers with an error instead of killing the thread and leaving every later song waiting.
- **No more zombie processes.** `notify-send` and `xdg-open` were spawned and never waited on, leaving one zombie per song change and per opened page for as long as boxvibe ran.
- **Favorites wait for the station list.** Logged out, or before the list arrived, every favorite row had a blank where the Pandora / your-own mark goes. The section now needs a signed-in account and shows one loading row until the stations are in. A favorite whose station Pandora no longer has is pruned, but never on an empty list — that is a logged-out or brand-new account, and pruning on it would throw away every star.
- **The account name is kept out of the status line.** Two messages put it there ("Signing in as …", "Type the password for …"). Nothing leaked — no log call takes `status` — but `status` is kept for the Notifications page, so the next log call that did would have written it to disk.
- **The menu and the page are separated by a blank column.** Their borders sat in adjacent cells and read as one line.

### Changed
- **Downloaded songs and View-all pages are capped.** Both were kept in maps that grew for the whole session; each song is a whole MP3 (up to 64 MB) and each View-all page is a whole Browse page. The oldest go first, and the View-all page on screen is never dropped.
- **`pinned::save` writes like `config::write_private`:** mode 600, fsync before the rename, and a pid in the temporary name so two boxvibes writing at once cannot clobber each other's half-written file.
- **Request bodies are capped at 8 MB**, as audio and art already were: a broken or hostile JSON answer was read whole into memory.
- **The key bar is centred** at the bottom of the screen.

### Internal
- **The README screenshots are drawn in Rust.** The screenshot test wrote a JSON cell grid that a Python script rasterized with Pillow; `src/screenshots.rs` now draws the frame itself with `ab_glyph`, so it needs no browser and no Python. The PNGs are written with best compression: 2.65 MB becomes 1.59 MB, pixel for pixel the same.
- **Every test module lives in its own `tests.rs`.** CodeQL kept opening test-only alerts (`cleartext-logging` on an assertion message, `hard-coded-cryptographic-value` on the fake server's fixtures) that had to be dismissed one at a time, because dismissing does not survive a line number moving. Rust has no inline suppression, so a whole-file ignore is the only way to say it once. `app.rs` goes from 7,332 lines to 4,135 and `ui.rs` from 5,279 to 2,920.
- **`Reply::Failed` carries an enum of its 21 call domains** rather than a string, so a new domain cannot be silently swallowed by the catch-all status line.
- **The private state with invariants is now types rather than loose fields:** `AudioCache`, `ViewAllCache`, `RecentState`, `PendingPassword`, `ImageCache`, `BrowsePages` and `InFlight`. Each keeps state that has to agree with itself in one file, so it cannot drift from another. Writing them caught two real bugs: an eviction that left a token behind, and a password cleared from `config.toml` before the keychain had confirmed it.
- **`app.rs` is split** into `app/browse.rs`, `app/recent.rs` and `app/playback.rs`, following the submodule pattern already used by `ad.rs`, `editor.rs`, `panel.rs` and `themes.rs`.
- **The skip loop is iterative.** A run of unplayable songs recursed through `play_now` → `advance` → `next_song` → `play_now`.
- **The palette snap is integer arithmetic:** it ran about 5M `powi` calls per cover on the UI thread.
- **Now Playing draws from a borrow**, not a copy of the song: 15 fields and about eight heap allocations per frame are gone.
- Removed dead code: `Station::art_url`, `largest_art`, the genre API (`Genre`, `GenreCategory`, `parse_genres`, `parse_genre_categories`, `Client::genres`, `demo::genres`) and a duplicate of `deps::in_path`.
- Both logs rotate through one `log::append(path, cap, line)` instead of two copies of the same rename-and-append.

## 0.6.1 (2026-11-09)

### Internal
- `toml` 1.1.8 → 1.2.0 and `toml_edit` 0.25.17 → 0.25.18, which bring
  `toml_writer` 1.2.0 with them. Both are performance releases upstream:
  pre-allocated serialization buffers, faster writing of syntax, strings and
  keys, and reused key allocations. No API changes.

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
