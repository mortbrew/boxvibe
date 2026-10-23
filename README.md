# boxvibe

Pandora in the terminal, written in Rust with [ratatui](https://ratatui.rs).
A sibling of [tubevibe](https://github.com/mortbrew/tubevibe): the same look,
Omarchy colour sync, icon sets and a `config.toml` that the app keeps up to
date. Titled panels, your stations (then Settings and About) on the left with
the logo and version on its frame, a large CarPlay-style **Now Playing** card
with the four big buttons (thumbs down, play/pause, skip, thumbs up), **Up
Next** and **History** side by side under it, a status bar, and the keys that
work right now drawn as key caps at the bottom.

> Early (0.3.0): it plays your stations, but search and station
> editing are still to come. Feedback welcome.

![Now Playing (Gruvbox)](screenshots/gruvbox-main.png)

![Settings](screenshots/gruvbox-settings.png)

![About, with a new version out](screenshots/gruvbox-about.png)

![About, after Update Now: Restart Now](screenshots/gruvbox-about-restart.png)

![Rounded borders (border_style = "rounded")](screenshots/gruvbox-rounded.png)

## What it does

- **Log in inside the app**: the first time, boxvibe opens **Settings** at the
  account section and asks for your Pandora email (or username), then the password, and
  saves them to `~/.config/boxvibe/config.toml`, a file only you can read
  (mode 600). From then on it logs in by itself. Change the account or log
  out in Settings any time
- **Your stations** (Shuffle too) on the left; `Enter` plays one, and the
  last one you played starts again next time
- **Now Playing**: title, artist, album, your thumb, a progress bar with the
  time, and the volume
- **Thumbs up / down** (`+` / `-`). Like in Pandora, a thumbs down skips the
  song. **Tired of this song** (`z`) keeps it off all your stations for a month
- **Up Next** and **History**: the few songs Pandora sent up next on the
  left, the last 10 you heard on the right (stacked on a narrow window). Each pane is
  sized for 10 songs and Now Playing gets the rest of the height; on a short window Now Playing keeps
  9 rows and the song panes shrink and scroll. `Tab` goes stations → Up Next →
  History; in History `+` / `-` rate a song you already heard
- **Settings** (account, audio quality, volume, autoplay, song change
  notifications, theme, icons, border style, menu width, update check, edit `config.toml` in `$EDITOR`) and **About**
  (links, version, and the update button) below the stations
- **Updates itself** from GitHub releases: see [Updating](#updating)
- **Gapless-ish playback**: the next song downloads while this one plays.
  MP3 and AAC (MP4) are decoded in Rust ([symphonia](https://github.com/pdeljanov/Symphonia)
  through [rodio](https://github.com/RustAudio/rodio)), no ffmpeg or mpv needed
- **Themes** (`t`): *Omarchy* (follows your current Omarchy theme, live, and
  is Tokyo Night elsewhere) plus Tokyo Night, Gruvbox, Catppuccin, Catppuccin
  Latte, Rosé Pine Dawn, Nord, Everforest and Kanagawa
- **Icon sets** (`i`): colour emoji, Nerd Font or plain Unicode
- **Song change notifications** with `notify-send` (mako on Omarchy)
- Whatever you change in the app (theme, icons, volume, last station) is
  saved to `config.toml` right away, keeping your comments

## Requirements

- Linux with ALSA (PipeWire's and PulseAudio's ALSA plugins work, as on Omarchy)
- A Pandora account (free or paid). Pandora only works in the US
- Optional: a [Nerd Font](https://www.nerdfonts.com) for `icons = "nerd"`
  (Omarchy ships one), `notify-send` (`libnotify`) for notifications

## Build (Arch / Omarchy)

```sh
sudo pacman -S --needed rust alsa-lib pkgconf     # or rustup for Rust 1.90+
git clone https://github.com/mortbrew/boxvibe
cd boxvibe
cargo build --release
./target/release/boxvibe
```

To have it in your `PATH`: `install -Dm755 target/release/boxvibe ~/.local/bin/boxvibe`
(or `cargo install --locked --path .`).

On Debian / Ubuntu the packages are `libasound2-dev pkg-config`; on Fedora
`alsa-lib-devel pkgconf-pkg-config`.

Try it without an account (made-up stations, no network, no sound):

```sh
./target/release/boxvibe --demo
```

## Keys

| Key | Action |
|---|---|
| `space` `p` | play / pause |
| `n` | skip |
| `+` `u` | thumbs up the playing song (whatever pane has the keys) |
| `-` `d` | thumbs down the playing song (skips it) |
| `U` `D` (Shift) | thumbs up / down the highlighted Up Next or History row (the focused pane, else the last one used); a thumbs down drops an Up Next song |
| `z` | tired of this song (off for a month) |
| `Shift+←` `Shift+→` | volume down / up |
| `j` `k` `↑` `↓` | move (wraps around the ends) · `g` `G` first / last |
| `Enter` | play the station · open Stations / Settings / About · change a setting · Update Now |
| `Tab` | menu → Now Playing → Up Next → History on the player (`Shift+Tab` back); menu ⇄ Stations / Settings / About |
| `Esc` | back to the menu |
| `N` (Shift+n) | Now Playing: back to the player with the keys on the Now Playing pane and the menu on NOW PLAYING |
| `s` | Stations: every station on the right, `Enter` plays one |
| `S` (Shift+s) / `A` (Shift+a) | Settings / About, with the menu on that entry |
| `f` `F` | favorite / unfavorite the station (the menu's FAVORITES section) |
| `{` `}` | menu narrower / wider |
| `t` | next theme |
| `i` | next icon set |
| `?` | every key |
| `Q` (Shift+q) / `Ctrl+c` | quit (asks first: `y` = yes, any other key = no) / quit now |

When a setting asks you to type (email, password): `Enter` saves, `Esc`
cancels, `Ctrl+u` clears.

## Configuration

`~/.config/boxvibe/config.toml` (`--config FILE` picks another one). Every key
is optional, and the app writes the ones you change from it. Keys this version
doesn't know (a typo, a leftover from testing, an old `[pandora]` key from the
JSON-API boxvibe, or a setting from a newer boxvibe) and values of the wrong
type are skipped with a warning in the status bar. Nothing in the file can stop
boxvibe from starting: one that is not valid TOML is moved to
`config.toml.bad-<UTC time>` and boxvibe starts with the defaults and says so.

```toml
[pandora]
username = "you@example.com"   # saved from Settings → Email or username
password = "…"                 # saved once a login works (the file is mode 600)
audio_quality = "high"         # high: MP3 192 kbps if your plan has it (Plus, Premium), else 128
                               # standard: MP3 128 kbps (old low / medium values mean standard)
user_agent = ""                # empty: boxvibe's default (a desktop Firefox); see below
last_station = ""              # the station to start on, kept by the app

[player]
volume = 70                    # 0–100

[ui]
theme = "omarchy"              # omarchy | tokyo-night | gruvbox | catppuccin | catppuccin-latte
                               # | rose-pine | nord | everforest | kanagawa
icons = "emoji"                # emoji | nerd | unicode
notifications = true           # a desktop notification when the song changes (Settings toggles it)
check_updates = true           # look for a new release on GitHub at startup
autoplay_last_station = true   # start the station played last when boxvibe opens (0.1: [player] autoplay)
border_style = "plain"         # plain | rounded | thick | double | none
menu_width = 30                # the menu, in columns: 20–40, step 2 (at most half the screen); also { and }
favorites = []                 # starred stations, { id = "…", name = "…" }, set with f
album_art = "picture"          # picture | blocks | off (off: the spectrum visualizer)
art_pixelation = "off"         # off | low | medium | high | max (64 / 32 / 16 / 8 px, blocky on purpose)
art_tint = "off"               # off | duotone | palette: recolour the cover with the theme's colours
```

`--theme NAME` and `--icons SET` change them for one run without saving.
A bad `album_art`, `art_pixelation` or `art_tint` value is ignored with a warning (picture / off / off).

## Album art

The Now Playing box shows the song's cover. boxvibe asks the terminal once at
startup what it can draw:

- **sixel** (foot, WezTerm, Konsole, xterm -ti vt340, Windows Terminal 1.22+),
  **kitty** graphics (kitty, Ghostty) or **iTerm2** images: a real picture.
- Anything else, and always inside **tmux** or **Zellij**: coloured half blocks (▀).

Settings → LOOK → **Album art** picks Picture, Blocks (half blocks even when
the terminal could do better) or Off. **Pixelation** makes the cover blocky
for fun: it is shrunk to 64, 32, 16 or 8 pixels across and blown back up,
nearest-neighbour. **Tint art with theme colors** recolours the cover using
your current theme's colour scheme: Duotone maps its brightness from the
theme's background to its yellow, Palette snaps every pixel to the nearest
theme colour. It follows theme changes right away and works with Pixelation.

The cover has no frame around it. While Help or the quit box is open, a
terminal picture is held back (it would draw over the popup) and sent again,
cleanly, when the popup closes. The status bar shows the logged-in account at
its right.

Covers download in the background and are kept in memory and in
`~/.cache/boxvibe/art` (`$XDG_CACHE_HOME`), at most 50 MB, oldest dropped
first. While a cover loads, when a song has none, or with Album art Off, the
box shows a spectrum visualizer of the music that is playing (frozen while
paused).

## Updating

The same way tubevibe does it. At startup (unless `check_updates = false`)
boxvibe asks GitHub, in the background, for the latest release of
`mortbrew/boxvibe` (`api.github.com/repos/mortbrew/boxvibe/releases/latest`)
and compares its tag with its own version. If it is newer, the header says
so, **About** gets a ⬆ in the menu, and the About page shows it. If GitHub
can't be reached (or there are no releases yet), nothing is shown.

On About, **Update Now** (`Enter`) downloads the release's
`boxvibe-<arch>-linux.tar.gz`, takes the `boxvibe` binary out of it, checks
that it runs and reports the new version, and puts it in place of the running
one ([self-replace](https://crates.io/crates/self-replace)). Then the button
becomes **Restart Now**, which starts the new version in the same terminal with the
same arguments. The button is only offered for a release binary in a folder you
can write to (like `~/.local/bin`); for a build from source, `cargo install`,
a system package, or a platform without a release download, About explains
why and shows the command to update by hand instead.

`BOXVIBE_FAKE_VERSION=0.0.1 boxvibe` pretends to be an older version, to try it.

Releases are built by `.github/workflows/release.yml` when a `v*` tag is pushed
(Linux x86_64 and aarch64).

## How it talks to Pandora

boxvibe uses the same API as the pandora.com website
(`https://www.pandora.com/api/`), written from the community
[API notes](https://6xq.net/pandora-apidoc/rest/) and from how the site
behaves, with parts adapted from
[Music Assistant](https://github.com/music-assistant/server)'s Pandora
provider (Apache-2.0, see [THIRD_PARTY.md](THIRD_PARTY.md)):

1. `HEAD https://www.pandora.com/` for the `csrftoken` cookie, sent back as
   the `X-CsrfToken` header and the cookie on every call
2. `POST /api/v1/auth/login` with your email (or username) and password. The
   answer has the auth token (sent as `X-AuthToken` from then on) and your
   plan's flags: `highQualityStreamingAvailable` unlocks 192 kbps MP3
3. `v1/station/getStations`, `v1/playlist/getFragment` (a few songs at a
   time, as `mp3-hifi` or `mp3`), `v1/station/addFeedback` (thumbs),
   `v1/listener/addTiredSong` and `v1/station/trackStarted` (sent when a song
   starts, like the site does)
4. When the login expires (HTTP 401) boxvibe logs in again with the saved
   password and retries once. When another device is playing on the account
   (429 `STREAM_VIOLATION`) it takes over with `v1/station/playbackResumed`
   and says so in the header line. Ads, artist messages and curator
   messages are skipped

What that means:

- No rewinding or seeking, and Pandora limits skips (about 6 per station per
  hour on free accounts)
- Stations only for now: no on-demand albums or playlists, even with Premium
- One stream per account: playing in boxvibe stops Pandora on your phone
- **Pandora's bot check**: the website's login is guarded by HUMAN
  (PerimeterX). boxvibe sends no captcha or bot-check answers; it logs in the
  way a plain browser request does, and if Pandora refuses it, it says
  "Pandora's bot check blocked the login". That depends on your network
  (home connections usually pass; servers and VPNs often don't). Waiting,
  another network, or logging in once on pandora.com in a browser may help;
  `[pandora] user_agent` lets you change the browser boxvibe says it is
- The API is unofficial: Pandora could change or close it

## Screenshots

`tools/screenshots.sh` renders the screenshots in `screenshots/` from demo
mode (the About one makes up a 0.2.0 release to show the update button): boxvibe draws a frame with ratatui's TestBackend as HTML and headless
Chrome turns it into a PNG.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
BOXVIBE_DECODE=song.mp3:song.m4a cargo test decodes_real_files -- --ignored
cargo test live_site -- --ignored   # www.pandora.com still hands out a csrftoken (no login)
```

## Disclaimer

boxvibe is not made by or affiliated with Pandora. It uses an unofficial API,
which may go against Pandora's Terms of Service; use it at your own risk.
Your password is stored in plain text in `config.toml` (readable only by you).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option. Parts of the Pandora code are
adapted from Music Assistant (Apache-2.0): see [THIRD_PARTY.md](THIRD_PARTY.md).
