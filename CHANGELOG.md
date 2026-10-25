# Changelog

## 0.4.0 (2026-10-24)

### Keys (breaking: please read)

Shift+letter goes places and works from anywhere; lowercase acts on the pane
that has the keys. Help (`?`) lists every key, and a test checks that no key is
bound twice.

- **`N` skips** to the next song from any pane; **`P`** goes to Now Playing
  (what `N` did). Lowercase `n` no longer skips. `p` and `space` still pause.
- **`U` / `D`** are thumbs up / down on the *playing* song from any pane.
  `+`, `-`, `=`, `u` and `d` (and the old Shift+U/D row rating) are gone.
- **`l` / `b`** like / mark bad the *selected row* in Up Next or History, or
  the current song when Now Playing has the keys. The key bar shows `l/b` only
  where it applies.
- **`C`** album art type, **`R`** Retro level, **`H`** tint: Shift keys that
  cycle and save, so the cover can be tried without opening Settings.
- Lowercase `t` (theme) is gone: the theme is only in Settings. `i` still
  cycles icons. The key bar no longer hints at theme or icons.
- Lowercase `s` is gone (it duplicated Shift+S, Stations).

### Stations

- **`r`** renames and **`x`** deletes the selected station, in Stations and in
  Favorites (there they act on that favorite's station, and the box says so).
  Both ask first, in the same box style as Quit; the delete box names the
  station and says it is removed from your Pandora account (`y` deletes, any
  other key keeps it). `f` still stars / unstars.
  Uses `v1/station/updateStation` and `v1/station/removeStation`, which are
  unofficial like the rest of the API; stations Pandora won't let you rename
  or delete (Shuffle) say so.
- **`v`** opens a details popup for the selected station or song: seeds, thumbs
  up / down counts, genres, created / last played, from Pandora's
  `v1/station/getStationDetails`; fields Pandora doesn't send are left out.

### Look

- **64-bit** Retro step (256 px) between HD and 32-bit; `retro_mode = "64bit"`.
  Old values and the old `art_pixelation` key still read.
- The **boxvibe logo** is always the Nerd Font box icon, whatever the icon set
  (menu frame, About, boot screen).
- A **boot screen** in boxvibe's own style: a box and wordmark that fade in
  using the theme's colours, 2 seconds, any key skips it. Settings →
  BOXVIBE → Boot screen, `[boxvibe] splash`, on by default.
- Up Next and History keep equal blank space at their left and right.
- Now Playing has one cell of padding on all four sides.

### Batch 2

- **Text boxes edit like text boxes**: `←` `→` `Home` `End` move a cursor,
  `Backspace` / `Delete` remove at it, typing inserts at it (rename, email,
  password and the rest). `Esc`, `Enter` and `Ctrl+u` as before.
- **The key bar only hints keys that do something where you are**: `l/b` only
  in Up Next, History and Now Playing; `r`, `x`, `f` only on a station; `v`
  only on a station or a song (it used to be bound everywhere, now it does
  nothing, and is not hinted, on Settings and About).
- **Every Settings description says where it is saved** ("Saved as [look]
  icons."), and a test checks that each one is true. Actions (log in, log out,
  edit the file, menu entry) say nothing, as they save nothing.
- No 🎮 in the status line when Retro mode changes.
- **First-run theme**: Omarchy on an Omarchy system (found by its `omarchy`
  command or its config / state folders), Tokyo Night anywhere else.
- **The Nerd Font icon set is the default** (`i` still cycles Nerd → emoji → Unicode).
- **The boot screen is 2 seconds** (was 1.6), any key still skips it.
- **Album art** is Picture or Off. `blocks` is gone: Retro mode does that job.
  An old `album_art = "blocks"` reads as Picture and is rewritten on the next save.
- **No spectrum visualizer.** With no cover (loading, none for the song, a failed
  download, or Album art Off) the art box says "Loading cover art…" or "No cover
  art" in its middle. The visualizer could no longer be reached, so its code
  (and the audio tap that fed it) is removed; it is in the git history.
- **config.toml uses the Settings names**: `[boxvibe]` (check_updates, splash),
  `[account]` (username, password, user_agent), `[playback]` (audio_quality,
  volume, notifications, autoplay_last_station, last_station, favorites) and
  `[look]` (theme, icons, border_style, menu_width, album_art, retro_mode,
  art_tint). Before, BOXVIBE's settings sat under `[ui]` and ACCOUNT's under
  `[pandora]`. Old files load unchanged (a key in a new table wins over the
  same key in an old one) and are moved to the new layout the next time the app
  saves; comments, other tables and unknown keys are left alone, and the file
  stays mode 600. Warnings name the new table (`look.theme`, not `ui.theme`).
- **`Z` (Shift+z) is "tired of this song"** (it was lowercase `z`, which is now
  unbound): any pane, on the playing song, as before.
- **Keys removed:** `g` / `G` / `Home` / `End` (jump to the first / last row)
  and lowercase `i` (cycle the icon set). `j` `k` and the arrows still move
  (and wrap around the ends); the icon set is Settings → LOOK → Icons. Help
  and the key bar no longer list them, and `i` does nothing.

### Fixes

- A cover that failed to download is tried again (after 2, 6 and 15 seconds),
  and again whenever it is needed (a popup closes, the song changes). A
  cover is also fetched when the song changes under Help and shows when it closes.
- **Package installs**: when boxvibe is installed by a package manager
  (`pacman -Qo` knows the file, or it lives under `/usr/bin` and the like), About
  still says a new version is out but hides the GitHub download button and
  tells you to update through the package manager. Manual installs behave as before.

### Repo

- Dependabot (cargo and GitHub Actions, weekly), a CI workflow (build, test,
  clippy), CodeQL for Rust and a weekly `cargo audit`, with badges in the README.
- `rustls-webpki` 0.103.16 and `webpki-roots` 1.0.11 (RUSTSEC-2026-0381 and -0382).

## 0.3.0 (2026-10-23)

### Now Playing polish (`now-playing-polish`)

- The album art has no frame any more: the cover (or the visualizer) fills its box, centred.
- Song info, the time line, volume and the four buttons sit together as one
  block, centred top to bottom beside the art.
- A sixel, kitty or iTerm2 cover no longer draws over Help or the quit box or
  leaves bits behind: it is held back while a popup is open and sent again
  on a cleared screen when it closes.
- Settings → LOOK → **Tint art with theme colors** (Off / Duotone / Palette,
  `[ui] art_tint`): recolours the cover using the current theme's colour
  scheme, for pictures and blocks, live on theme change.
- Up Next has a RATED column like History's, updated as soon as you press `U` or `D`.
- The status bar shows the logged-in account at its far right, in tubevibe's
  account colour (the theme's blue, aqua in Gruvbox); both sides cut with "…" when narrow.
- Confirm boxes (Shift+`Q`'s "Quit boxvibe?") are symmetrical like tubevibe's:
  a blank line above the question and below the keys; a long question wraps
  and is cut with "…" so the box still fits small screens.
- Shift+`T` opens Settings and Shift+`A` opens About, selecting them in the
  menu as Shift+`N` does for Now Playing; Shift+`S` opens Stations like `s`.
- Shift+`F` jumps to the menu's FAVORITES (the first one), or says there are
  none; `f` still stars / unstars. (`F` used to do the same as `f`.)
- Pixelation is now **Retro mode**: HD (off), 32-bit, 16-bit, 8-bit, 4-bit and
  2-bit (128 / 64 / 32 / 16 / 8 px), saved as `[ui] retro_mode = "hd"` …
  `"2bit"`; the status bar says e.g. "Retro mode: 16-bit 🎮". The old
  `art_pixelation` key and its values (off, extra_low, low, medium, high, max)
  still read; when both are there `retro_mode` wins.
- Now Playing is symmetrical: the time line (and the block with it) ends as
  far in from the pane's right border as the cover starts from its left one,
  worked out each frame from where the cover is drawn.
- The key bar no longer shows `t` Theme and `i` Icons; both keys still work and are in Help.
- The key bar shows `U/D` Rate Row only while Up Next or History has the keys.
- While Help or the quit box is open, everything behind it, the half-block
  cover included, is dimmed toward the theme background (about 40%, greyer);
  the popup stays at full brightness.
- Settings has no SETTING / VALUE header row any more, and its BOXVIBE section
  (updates, edit config.toml, app menu entry) comes first, before ACCOUNT.
- Settings → BOXVIBE → **App menu entry** (Installed / Not installed, read
  from disk): `Enter` adds or removes `boxvibe.desktop` in
  `$XDG_DATA_HOME/applications` (default `~/.local/share/applications`) so app
  launchers list boxvibe, running this binary in a terminal. Only a file
  boxvibe wrote (marked `X-Boxvibe-Generated=true`) is ever deleted. Not a
  config key.
- The half-block cover shown under Help or the quit box now fills exactly the
  rectangle the sixel/kitty/iTerm2 picture uses (it is sized with the
  terminal's real font), so it no longer looks smaller or off-centre and the
  song info beside it never moves when a popup opens.
- Help is as wide as its longest line (wrapping only when the terminal is
  narrower), so no line is cut off, and every description lines up after the
  widest key (`Shift ← →` no longer crowds its text).

## 0.2.0 (2026-10-22)

### Album art (`album-art`)

- The Now Playing box shows the cover, drawn with
  [ratatui-image](https://github.com/benjajaja/ratatui-image): sixel, kitty or
  iTerm2 pictures where the terminal supports them (asked once at startup),
  half blocks otherwise and always inside tmux or Zellij. Redrawn to fit when
  the window is resized.
- Covers (about 500 px) download on their own thread, never holding up the
  screen, and are cached in memory and in `~/.cache/boxvibe/art` (50 MB cap).
- No cover, still loading, or art Off: a spectrum visualizer of the decoded
  audio in green, yellow and orange with peak caps, ~18 fps while playing,
  frozen while paused, with a dim "♪ no cover" caption.
- Settings → LOOK: **Album art** (Picture / Blocks / Off, `[ui] album_art`)
  and **Retro mode** (HD / 32-bit / 16-bit / 8-bit / 4-bit / 2-bit, `[ui] retro_mode`).
  Bad values fall back to the defaults with a warning.
- Demo mode and `--snapshot` draw made-up covers, so nothing needs the network.
- Needs Rust 1.90 (the sixel encoder's dependencies).
- The Settings icons for "Look for new versions" and "Edit config.toml" were one
  row off; fixed.

### Layout and key fixes (`ui-refresh`)

- History keeps the last 10 songs (was 30)
- Up Next and History are each sized to fit exactly 10 songs (border, header, 10 rows), side by side or
  stacked; Now Playing takes the rest of the height. On a short terminal Now Playing keeps 9 rows and the
  song panes shrink and scroll to the highlighted row. Settings → LOOK → Now Playing size is gone; an
  old `[ui] now_playing_size` in config.toml is read and ignored without a warning. Tests:
  `song_panes_fit_ten_rows_and_now_playing_takes_the_rest`, `retired_now_playing_size_is_ignored_quietly`
- `N` (Shift+n) puts the menu on NOW PLAYING and the keys on the Now Playing pane, which lights up as active; `Tab` now goes menu → Now Playing → Up Next → History
- Starting a station any way (Enter in Stations, Enter on a Favorite, autoplay of the last station at startup) lands exactly where `N` does: menu on NOW PLAYING and the keys on the Now Playing pane, lit up (the same function, so they can't drift). Tests: `enter_on_a_station_plays_it_and_shows_now_playing`, `enter_on_a_favorite_plays_it_and_shows_now_playing`, `autoplay_at_startup_lands_like_shift_n`
- `u` / `+` and `d` / `-` always rate the playing song, whichever pane has the keys (before, they rated History's row there)
- New `U` / `D` (Shift+u / Shift+d) rate the highlighted Up Next or History row (the focused pane, else the last one used), with Pandora's addFeedback on that song's track token; the row's rating updates, a thumbs down drops an Up Next song, and an empty pane or a song without a token says so in the status line. Tests: `shift_n_focuses_the_now_playing_pane`, `lowercase_u_d_always_rate_the_playing_song`, `shift_u_d_rate_the_highlighted_row`, `tab_goes_menu_now_playing_up_next_history_and_back`
- Up Next and History text can no longer run into the time or rating: that column is its own table in its own space, song text is cut by worst-case width with a cell to spare, and symbols terminals draw at different widths (one-cell pictographs like ❤ ☀, Nerd Font glyphs, joiners, zero-width marks) are dropped from song text. The same applies to the Stations list and Now Playing info
- Up Next's TIME column and selection bar keep a blank cell before the pane's right border, like History's RATED column
- Quit is now `Q` (Shift+q) or `Ctrl+c`; lowercase `q` no longer quits. `Q` asks first in a "Quit boxvibe?" box like tubevibe's (`y` quits, any other key cancels); `Ctrl+c` quits at once
- The menu's entries are left-justified like tubevibe's: one space from the border, icons in one column
- `↑` / `↓` (`k` / `j`) wrap around: up from the top goes to the bottom and down from the bottom to the
  top, in the menu, Stations, Settings, Up Next and History (`g` / `G` still stop at the ends)
- Up Next and History stack (Up Next on top) when the page is under 90 columns wide
- Up Next and History cut long titles and artists with "…", turn tabs and newlines into spaces and drop
  emoji variation selectors, so text never runs past the pane or hides the rating
- Now Playing: the album art fills the pane's height; the time line and volume moved next to it, under
  the song info, which starts one line lower; the buttons no longer show keys under them
- The Help window's title uses tubevibe's keyboard icon and the menu's styled name (blue "box", bold):
  "⌨  boxvibe Help · Any Key Closes", and any key now closes it (was Esc, `?`, `q` or Enter); test:
  `help_title_uses_tubevibes_keyboard_and_the_logo_name`
- Volume is `Shift+←` / `Shift+→` (was `←` / `→`, `[` / `]`, `<` / `>`)
- Tests: `menu_text_is_left_justified_like_tubevibe`, `up_and_down_wrap_around_the_ends`,
  `up_next_and_history_stack_when_narrow`, `long_song_text_is_cut_and_the_rating_stays_visible`,
  `clean_and_truncate_keep_cells_honest`, `now_playing_art_fills_the_pane_with_info_time_and_volume_beside_it`,
  `shift_arrows_change_the_volume_and_plain_arrows_do_not`

### Stations page and Favorites (`ui-refresh`)

- The menu no longer lists every station: a **Stations** entry (also `s`) opens the station list on the
  right; `Enter` plays the highlighted station and shows Now Playing
- `f` (or `F`) stars the highlighted station (on the Stations page), the favorite under the menu cursor,
  or else the playing station; again unstars it. Starred stations show ⭐ on the page and appear in a
  **FAVORITES** section in the menu, like tubevibe's tabs, where `Enter` plays them. No favorites, no section.
- Saved as `[ui] favorites = [{ id = "…", name = "…" }, …]`; names follow Pandora's when the stations load,
  unknown keys in an entry are fine, and entries without an id are skipped
- boxvibe always starts on Now Playing, idle when nothing autoplays; open Stations yourself from the menu or `s`
- Enter on a station or a favorite plays it and switches to Now Playing, with the menu on NOW PLAYING
- Tests: `f_stars_stations_into_a_favorites_section_that_plays_them`, `favorites_read_and_tolerate_junk`,
  `stations_page_and_favorites_section`

### Now Playing size setting (`ui-refresh`, replaced by fixed 10-song panes above)

- Settings → LOOK → **Now Playing size** sets how many rows the Now Playing pane gets (9–25, step 2,
  default 17); Up Next and History get the rest. Saved as `[ui] now_playing_size`; a missing or bad value
  is the default, with a warning for a bad one. On a short terminal Up Next and History keep some room.
- Tests: `now_playing_size_is_saved`, `now_playing_size_reads_and_falls_back`,
  `now_playing_size_sets_the_pane_height`

### Resize the menu with { and } (`ui-refresh`)

- `{` makes the left menu 2 columns narrower and `}` 2 wider (20–40, never more than half the screen),
  saved to `[ui] menu_width` like Settings → LOOK → **Menu width**; the key bar and help show it
- Tests: `braces_resize_the_menu_and_save_it`, `sizes_step_and_stop_at_the_ends`

### Restart Now after Update Now (`ui-refresh`)

- Once **Update Now** on About has put the new version in place, the button becomes **Restart Now**:
  `Enter` quits cleanly (sound stopped) and starts the new binary in place of this one (`exec`, same
  terminal, same arguments), like tubevibe 0.8.8. A build from source is never restarted.
- The button shows ▶ ◀ while it has the keys and "(Tab to Select, Enter to Press)" when it doesn't; the
  key bar says `Enter Restart Now`
- The updater matches tubevibe's current one (`no_news`, `latest_or_unknown`)
- Tests: `restart_now_after_the_update_is_in`, `up_to_date_only_on_a_real_version`

### Up Next and History side by side (`ui-refresh`)

- The single Songs table is gone: under Now Playing, **Up Next** is on the left and **History** on the
  right, like boxvibe's first layout, each a whyd-style table with column headers (`#`, title, artist,
  time; title, artist, rated) and text cut with "…"
- `Tab` goes stations → Up Next → History → stations (`Shift+Tab` back); the pane with the keys has the
  bright border and the selection bar
- `+` / `-` in History rate that song (no skip); in Up Next they say "Rate it once it plays"; anywhere
  else they rate the playing song. The History selection stays on its song as new ones are added.
- Tests: `tab_goes_menu_up_next_history_and_back`, `rating_a_played_song_in_history`

### Song change notifications toggle (`ui-refresh`)

- Settings → PLAYBACK → **Song change notifications** turns the `notify-send` notification for each new
  song on and off, saved as `[ui] notifications = true | false` (default on)
- boxvibe 0.1's `notify = …` still reads as the same setting; the first time the toggle is saved the old
  key is taken out, and when both are there `notifications` wins
- Tests: `notifications_on_by_default_and_the_old_name_reads`, `the_notifications_toggle_is_saved`

### config.toml can never stop boxvibe from starting (`ui-refresh`)

- A file that isn't valid TOML (or isn't text) is moved to `config.toml.bad-<UTC time>` (e.g.
  `config.toml.bad-20261021T011403Z`), boxvibe starts with the defaults, and the status bar says where
  the file went; settings saved from then on go to a fresh file. A config path that can't be read is
  left alone and the defaults are used.
- Old `[pandora]` keys from the JSON-API boxvibe and from testing (`partner_user`, `partner_password`,
  `device`, `encrypt_password`, `decrypt_password`, `rpc_host`, …) are skipped with "left over from the
  old Pandora API" instead of "made by a newer boxvibe"; old `low` / `medium` qualities still read as
  `standard`
- Unknown keys and sections, wrong types, a section that is a value (or an array of tables), and keys
  that only fail together are all skipped with a warning; everything else in the file still applies
- Edit config.toml: a file broken in the editor keeps the settings in use and isn't moved
- Tests: `junk_configs_always_start`, `a_broken_file_is_moved_aside_and_the_defaults_start`,
  `an_unreadable_path_starts_with_defaults`, `utc_stamps`

### The logo on the menu's frame, no header row (`ui-refresh`)

- The logo sits on the left menu's top border, drawn like tubevibe's (` <icon> box` + `vibe `, bold):
  the box icon and "box" in the theme's blue, "vibe" in the text colour; the version is on the same
  border, right-aligned. About's logo uses the same colours.
- The header row is gone (with the account name on its right), so Now Playing's border is the top row.
  The status message moved to a shaded status bar above the keys, where tubevibe has it. The station
  count is now a `STATIONS (n)` heading inside the menu.
- With emoji icons, 📦 keeps the emoji font's own colours (terminals can't recolour colour emoji); the
  Nerd Font and Unicode boxes are blue
- Tests: `the_logo_and_version_sit_on_the_menus_top_border`, `up_next_and_history_sit_side_by_side`

### Border styles and menu width (`ui-refresh`)

- Settings → LOOK → **Border style** cycles plain / rounded / thick / double / none for every pane,
  saved as `[ui] border_style` (tubevibe's set; `none` keeps the room and titles, and the focused
  pane's title lights up instead)
- **Menu width**: the station menu from 20 to 40 columns in steps of 2, saved as `[ui] menu_width`
  (default 30, what 0.1 gave a 120-column terminal), never more than half the screen; out of range
  values warn and use the default
- tubevibe's other width settings (preview, comments) have no pane to size in boxvibe
- `tools/screenshots.sh` also renders About with Restart Now and the rounded borders
- Tests: `border_style_and_menu_width`, `border_style_and_menu_width_are_saved`,
  `every_border_style_draws_and_the_menu_width_holds`

### NOW PLAYING in the menu, `N` to get there (`ui-refresh`)

- Once a station plays, a **NOW PLAYING** entry heads the menu, with a rule under it inset two columns
  from each side of the frame (tubevibe's `SEPARATOR_INSET`; the rule above Settings now matches).
  `Enter` on it shows the player again from Settings or About, and the song keeps playing. While the
  player is shown, this entry is the one shaded in the menu, the way tubevibe marks the tab shown.
- `N` (Shift+n) jumps there from anywhere (`n` still skips). The key bar shows `N Now Playing` on
  Settings and About, and Help lists it. tubevibe has no single key for this: its playing video is a
  tab in the menu, which this entry copies.
- The menu cursor stays on the station you picked when the entry appears
- Tests: `now_playing_from_the_menu_and_with_shift_n`, `no_now_playing_entry_before_a_station_plays`,
  `now_playing_heads_the_menu_with_an_inset_rule`

### No gap between the menu and the page (`ui-refresh`)

- The menu's frame and the right-hand pane's frame now touch, with no blank column between them, the
  way tubevibe lays out its menu and content (Up Next and History keep their one-column gap)
- Test: `the_menu_and_the_page_touch`

### Autoplay last station on open toggle (`ui-refresh`)

- Settings → PLAYBACK → **Autoplay last station on open**, saved as `[ui] autoplay_last_station =
  true | false`. It is on by default, as before: once boxvibe has logged in, the station played last
  starts by itself. Off, boxvibe waits for you to pick a station.
- boxvibe 0.1's `[player] autoplay` still reads as this setting (the new key wins when both are there),
  and saving the toggle takes the old key out. A wrong type warns and uses the default, like every key.
- Tests: `autoplay_last_station_and_its_old_name`, `the_autoplay_toggle_is_saved`,
  `autoplay_off_waits_for_a_station`

## 0.1.0 (unreleased)

First version.

- Log in from the app with email (or username) and password; saved to `~/.config/boxvibe/config.toml` (mode 600)
- Talks to Pandora's website API (`www.pandora.com/api`): logs in again by itself when the login expires,
  takes the stream over from another device, skips ads and curator messages, sends `trackStarted`
- Audio quality `high` (192 kbps MP3 on Plus / Premium, else 128) or `standard` (128 kbps MP3); HE-AAC is
  gone, and old `low` / `medium` settings read as `standard`
- Uses the website API instead of the old mobile JSON API: no request encryption, no app keys
- Stations, Now Playing (title, artist, album, thumbs, progress, volume), the songs played and up next
- Thumbs up / down (also on songs already played), skip, "tired of this song", pause, volume
- MP3 playback with rodio + symphonia (AAC too, if Pandora ever sends it), no ffmpeg; the next song
  downloads while one plays
- Themes: Omarchy (follows the current theme) and 8 built-in ones; emoji / Nerd Font / Unicode icons
- Settings changed in the app are saved; unknown or broken config keys are skipped with a warning
- `--demo` mode with made-up stations
- whyd-style look: a header line with the version and status, titled panels, a songs table with column
  headers, a selection bar, and key caps at the bottom
- Settings and About below the stations; the account (email, password, log in / out) is in Settings, and
  the first start opens it
- Checks GitHub for a new release at startup (`check_updates`); About offers Update Now, which replaces
  the binary in place, or explains how to update by hand
