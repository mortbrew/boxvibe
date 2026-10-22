# Changelog

## 0.2.0 (unreleased)

### Layout and key fixes (`ui-refresh`)

- `N` (Shift+n) puts the menu on NOW PLAYING and the keys on the Now Playing pane, which lights up as active; `Tab` now goes menu → Now Playing → Up Next → History
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

### Now Playing size setting (`ui-refresh`)

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
