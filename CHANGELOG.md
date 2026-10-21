# Changelog

## 0.2.0 (unreleased)

### Restart Now after Update Now (`restart-now`)

- Once **Update Now** on About has put the new version in place, the button becomes **Restart Now**:
  `Enter` quits cleanly (sound stopped) and starts the new binary in place of this one (`exec`, same
  terminal, same arguments), like tubevibe 0.8.8. A build from source is never restarted.
- The button shows ▶ ◀ while it has the keys and "(Tab to Select, Enter to Press)" when it doesn't; the
  key bar says `Enter Restart Now`
- The updater matches tubevibe's current one (`no_news`, `latest_or_unknown`)
- Tests: `restart_now_after_the_update_is_in`, `up_to_date_only_on_a_real_version`

## 0.1.0 (2026-10-20)

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
