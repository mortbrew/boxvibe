# Changelog

## 0.2.0 (unreleased)

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
