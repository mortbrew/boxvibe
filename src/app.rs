//! The app's state and what each key does.
//!
//! Like tubevibe, the left panel is the menu: NOW PLAYING, Stations, the
//! FAVORITES section (like tubevibe's tabs), then Settings, About and Quit.
//! The right side shows what the menu entry opens: the player (Now Playing,
//! Up Next and History), the station list, Settings or About. There is no
//! separate login screen: the account lives in Settings, and the first run
//! opens Settings on it.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::ads;
use crate::audio::Audio;
use crate::backend::{Backend, KeychainJob, KeychainReply, Reply, Request};
use crate::config::{self, AlbumArt, Config, ConfigWarning, GoodVibes, Secret};
use crate::input::TextInput;
use crate::keychain::{self, Keychain, NO_KEYCHAIN};
use crate::pandora::{GenreCategory, Quality, Rating, SearchHit, Song, Station, StationDetails};
use crate::theme::{self, Palette};
use crate::update::{Progress, Replace, Update};

mod ad;
mod editor;
mod panel;
mod themes;
pub use ad::{AdPhase, AdView, End};
pub use editor::{Editor, EditorTab, FeedState, Loaded};
pub use panel::{Panel, PanelKind};
pub use themes::{ThemePicker, ThemeTab};

/// What boxvibe knows about the Pandora password in the desktop keychain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeychainState {
    /// Not asked yet, or being asked.
    Unknown,
    /// It is in the keychain.
    Saved,
    /// The keychain works and has none.
    NotSaved,
    /// There is no keychain (or it said no): the password lasts the session.
    Unavailable,
}

/// What the right side shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Player,
    /// Every station, to pick one (`Enter`) or star it (`f`).
    Stations,
    /// Pandora's genre stations, to add one (`Enter`).
    Genres,
    /// Songs pinned with Shift+I.
    Pinned,
    Settings,
    /// What the bottom bar said, newest first.
    Notifications,
    About,
}

/// Which panel the keys move in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Menu,
    Content,
}

/// The menu's entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    /// At the top once a station plays: back to the player.
    NowPlaying,
    /// Opens the station list on the right.
    Stations,
    /// Opens Pandora's genre stations on the right.
    Genres,
    /// The songs pinned with Shift+I (hidden by `[look] pinned_in_menu`).
    Pinned,
    /// A starred station, by its index in `[playback] favorites`.
    Favorite(usize),
    Settings,
    /// What the bottom bar said lately.
    Notifications,
    About,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Session {
    LoggedOut,
    SigningIn,
    LoggedIn,
    Failed(String),
}

/// The popup shown when a genre station is renamed, edited or deleted and
/// Pandora says no.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenreNotice {
    pub station: String,
    /// What was tried: "renamed", "edited" or "deleted".
    pub what: &'static str,
    /// The "Don't show this again" box.
    pub dont_show: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    Email,
    Password,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Email,
    Password,
    LogIn,
    LogOut,
    Quality,
    Volume,
    Autoplay,
    Notify,
    GoodVibesOnly,
    ThemeAutoMode,
    ThemeSelect,
    Icons,
    BorderStyle,
    MenuWidth,
    FavoritesInMenu,
    PinnedInMenu,
    AlbumArt,
    RetroMode,
    ArtTint,
    Animations,
    CheckUpdates,
    GenrePopup,
    Splash,
    EditFile,
    MenuEntry,
}

impl Setting {
    /// In screen order, each with its group.
    pub const ALL: [(&'static str, Self); 25] = [
        ("ACCOUNT", Self::Email),
        ("ACCOUNT", Self::Password),
        ("ACCOUNT", Self::LogIn),
        ("ACCOUNT", Self::LogOut),
        ("BOXVIBE", Self::CheckUpdates),
        ("BOXVIBE", Self::GenrePopup),
        ("BOXVIBE", Self::EditFile),
        ("BOXVIBE", Self::MenuEntry),
        ("PLAYBACK", Self::Quality),
        ("PLAYBACK", Self::Volume),
        ("PLAYBACK", Self::Autoplay),
        ("PLAYBACK", Self::Notify),
        ("PLAYBACK", Self::GoodVibesOnly),
        ("LOOK", Self::ThemeAutoMode),
        ("LOOK", Self::ThemeSelect),
        ("LOOK", Self::Icons),
        ("LOOK", Self::BorderStyle),
        ("LOOK", Self::MenuWidth),
        ("LOOK", Self::FavoritesInMenu),
        ("LOOK", Self::PinnedInMenu),
        ("LOOK", Self::AlbumArt),
        ("LOOK", Self::RetroMode),
        ("LOOK", Self::ArtTint),
        ("LOOK", Self::Animations),
        ("LOOK", Self::Splash),
    ];

    /// Where `setting` is in [`Setting::ALL`] (the Settings cursor).
    pub fn at(setting: Self) -> usize {
        Self::ALL.iter().position(|(_, s)| *s == setting).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Email => "Email or username",
            Self::Password => "Saved password",
            Self::LogIn => "Log in",
            Self::LogOut => "Log out",
            Self::Quality => "Audio quality",
            Self::Volume => "Volume",
            Self::Autoplay => "Autoplay last station on open",
            Self::Notify => "Song change notifications",
            Self::GoodVibesOnly => "Good Vibes Only",
            Self::ThemeAutoMode => "Theme Auto Mode",
            Self::ThemeSelect => "Theme Select",
            Self::Icons => "Icons",
            Self::BorderStyle => "Border style",
            Self::MenuWidth => "Menu width",
            Self::FavoritesInMenu => "Favorites in menu",
            Self::PinnedInMenu => "Pinned in menu",
            Self::AlbumArt => "Album art",
            Self::RetroMode => "Retro mode",
            Self::ArtTint => "Tint art with theme colors",
            Self::Animations => "Animations",
            Self::CheckUpdates => "Look for new versions on startup",
            Self::GenrePopup => "Genre station edit popup",
            Self::Splash => "Boot screen",
            Self::EditFile => "Edit config.toml",
            Self::MenuEntry => "App menu entry",
        }
    }

    /// The config.toml `[table]` and key a setting is saved in, or `None` for
    /// one that is an action rather than a saved choice (LogIn, LogOut,
    /// EditFile, MenuEntry). `about` says it, and a test holds it to the truth.
    #[cfg(test)]
    fn saved_as(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::CheckUpdates => Some(("boxvibe", "look_for_new_versions_on_startup")),
            Self::GenrePopup => Some(("boxvibe", "genre_edit_popup")),
            Self::Splash => Some(("look", "boot_screen")),
            Self::Email => Some(("account", "email_or_username")),
            Self::Quality => Some(("playback", "audio_quality")),
            Self::Volume => Some(("playback", "volume")),
            Self::Autoplay => Some(("playback", "autoplay_last_station")),
            Self::Notify => Some(("playback", "song_change_notifications")),
            Self::GoodVibesOnly => Some(("playback", "good_vibes_only")),
            Self::ThemeAutoMode => Some(("look", "theme_auto_mode")),
            Self::ThemeSelect => Some(("look", "theme_select")),
            Self::Icons => Some(("look", "icons")),
            Self::BorderStyle => Some(("look", "border_style")),
            Self::MenuWidth => Some(("look", "menu_width")),
            Self::FavoritesInMenu => Some(("look", "favorites_in_menu")),
            Self::PinnedInMenu => Some(("look", "pinned_in_menu")),
            Self::AlbumArt => Some(("look", "album_art")),
            Self::RetroMode => Some(("look", "retro_mode")),
            Self::ArtTint => Some(("look", "tint_art_with_theme_colors")),
            Self::Animations => Some(("look", "animations")),
            Self::Password | Self::LogIn | Self::LogOut | Self::EditFile | Self::MenuEntry => None,
        }
    }

    /// One line on what the setting does, under the list. Every setting kept
    /// in config.toml ends with where ("Saved as [look] theme.").
    pub fn about(self) -> &'static str {
        match self {
            Self::Email => "What you log in to pandora.com with. Enter types it. Saved as [account] email_or_username.",
            Self::Password => {
                "Kept in your desktop keychain (GNOME Keyring, KeePassXC…), never in config.toml. Enter forgets it: you type it again next start."
            }
            Self::LogIn => {
                "Logs in to Pandora with the email above, and the password from your keychain or typed here."
            }
            Self::LogOut => "Stops the music and removes the saved password from your keychain.",
            Self::Quality => {
                "high is 192 kbps MP3 where your plan has it (Plus, Premium), else 128 kbps; standard is 128. Saved as [playback] audio_quality."
            }
            Self::Volume => "Also ← and → anywhere. Saved as [playback] volume.",
            Self::Autoplay => {
                "When boxvibe opens and logs in, the station you played last starts by itself. Saved as [playback] autoplay_last_station."
            }
            Self::Notify => {
                "A desktop notification (notify-send, mako on Omarchy) for every new song. Saved as [playback] song_change_notifications."
            }
            Self::GoodVibesOnly => {
                "On plays Pandora's audio ads and reports them, like the website does. Muted plays and reports them the same way at volume 0 (your volume comes back after). Off skips ads. Muted and Off could break Pandora's terms: asks first. Written true, \"muted\" or false. Saved as [playback] good_vibes_only."
            }
            Self::ThemeAutoMode => {
                "omarchy follows your current Omarchy theme (Theme Select below is greyed out); off uses Theme Select. Enter switches. Saved as [look] theme_auto_mode."
            }
            Self::Icons => {
                "Nerd Font (needs one installed; Omarchy ships one), emoji or plain Unicode. Also i. Saved as [look] icons."
            }
            Self::BorderStyle => {
                "How every pane's border is drawn: plain, rounded, thick, double or none. Saved as [look] border_style."
            }
            Self::MenuWidth => {
                "The menu's width, 20 to 40 columns (never more than half the screen). Also { and }. Saved as [look] menu_width."
            }
            Self::AlbumArt => {
                "Picture: the real cover (sixel, kitty, iTerm2, or half blocks in other terminals); Off: no cover, and the space says Cover art off. Also Shift+C. Saved as [look] album_art."
            }
            Self::ArtTint => {
                "Recolors the album art using your current theme's color scheme. Duotone: theme background to yellow; Palette: only theme colors. Also Shift+H. Saved as [look] tint_art_with_theme_colors."
            }
            Self::RetroMode => {
                "Makes the cover blocky on purpose, like an old console: 64-bit (256 pixels across), 32-bit (128), 16-bit (64), 8-bit (32), 4-bit (16), 2-bit (8), or HD for the full picture. Also Shift+R. Saved as [look] retro_mode."
            }
            Self::FavoritesInMenu => {
                "On: the menu lists FAVORITES (Shift+F jumps there). Off: they are hidden from the menu, but kept, and f still stars a station. Saved as [look] favorites_in_menu."
            }
            Self::PinnedInMenu => {
                "On: the menu has a Pinned entry with the songs Shift+I pinned. Off: the entry is hidden; Shift+I still pins and the list is kept. Saved as [look] pinned_in_menu."
            }
            Self::Animations => {
                "On: the boot screen fades in. Off: no motion anywhere, so the boot screen is skipped (the song progress and clock still update). Saved as [look] animations."
            }
            Self::CheckUpdates => {
                "Asks GitHub when boxvibe opens; About shows the new version and Update Now. Saved as [boxvibe] look_for_new_versions_on_startup."
            }
            Self::GenrePopup => {
                "On: trying to rename or edit a genre station opens a popup that says why and what the logo and person icons mean (the message in the bottom bar always shows too). Its checkbox turns this Off. Saved as [boxvibe] genre_edit_popup."
            }
            Self::Splash => {
                "A two-second boxvibe logo screen when it opens; any key skips it (Animations off skips it too). Saved as [look] boot_screen."
            }
            Self::ThemeSelect => {
                "With Theme Auto Mode off, Enter opens the theme picker: Search about 900 schemes (iTerm2-Color-Schemes and Omarchy's) as you type, or pick an Installed one. Downloads go to ~/.config/boxvibe/themes/. Saved as [look] theme_select."
            }
            Self::EditFile => "Opens config.toml in $EDITOR; boxvibe reads it again when you close it.",
            Self::MenuEntry => {
                "Enter adds or removes boxvibe.desktop in ~/.local/share/applications, so app launchers (walker on Omarchy) list boxvibe; it runs this boxvibe in a terminal. Not in config.toml."
            }
        }
    }
}

/// The two song panes under Now Playing; Tab goes menu → Up Next → History.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    /// The Now Playing card itself (`P` puts the keys here).
    NowPlaying,
    UpNext,
    History,
}

/// The sleep timer's lengths, in minutes, in the order `B` steps through.
pub const SLEEP_STEPS: [u64; 4] = [15, 30, 60, 90];
/// The sleep timer turns the sound down over this long before it pauses.
pub const SLEEP_FADE: Duration = Duration::from_secs(10);

/// A sleep timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sleep {
    /// This many minutes from this moment.
    Minutes(u64, Instant),
    /// When the playing song ends.
    EndOfSong,
}

/// "14:59", or "1:29:59" from an hour up.
pub fn clock_text(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// What a key can do. [`KEYMAP`] says which key does which; the context
/// (which pane has the keys) is each command's own business.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Quit,
    Help,
    Pause,
    /// Skip to the next song, from anywhere.
    Skip,
    NowPlaying,
    Stations,
    Genres,
    Add,
    Settings,
    About,
    Favorites,
    /// Thumbs up / down on the playing song, from anywhere.
    ThumbUp,
    ThumbDown,
    /// Thumbs up / down on the selected row, or on the song when Now Playing has the keys.
    LikePane,
    BadPane,
    Tired,
    Star,
    Rename,
    Delete,
    Details,
    /// Edit a station's seeds and thumbs history.
    Edit,
    NextFocus,
    PrevFocus,
    ToMenu,
    Down,
    Up,
    Enter,
    VolumeUp,
    VolumeDown,
    MenuNarrower,
    MenuWider,
    ArtType,
    Retro,
    Tint,
    /// The sleep timer: 15, 30, 60, 90 minutes, end of this song, off.
    Sleep,
    /// Pin the selected or playing song.
    Pin,
    /// `.`: the song menu of the selected or playing song.
    SongMenu,
    Mode,
    Shuffle,
}

/// One key and what it does. Shift+letters are the capital letters;
/// `shift` is for keys like ← that only count with Shift held.
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub code: KeyCode,
    pub shift: bool,
    pub cmd: Cmd,
}

const fn bind(code: KeyCode, cmd: Cmd) -> Binding {
    Binding { code, shift: false, cmd }
}

/// Every key boxvibe listens to outside of prompts and popups. Shift+letter
/// is for going places and for things that work from anywhere; lowercase
/// acts on the pane that has the keys. No key may appear twice (a test
/// checks), and Help lists them all (another test checks).
pub const KEYMAP: &[Binding] = &[
    // Shift+letter: global.
    bind(KeyCode::Char('Q'), Cmd::Quit),
    bind(KeyCode::Char('?'), Cmd::Help),
    bind(KeyCode::Char('N'), Cmd::Skip),
    bind(KeyCode::Char('P'), Cmd::NowPlaying),
    bind(KeyCode::Char('S'), Cmd::Stations),
    bind(KeyCode::Char('G'), Cmd::Genres),
    bind(KeyCode::Char('O'), Cmd::Settings),
    bind(KeyCode::Char('A'), Cmd::About),
    bind(KeyCode::Char('F'), Cmd::Favorites),
    bind(KeyCode::Char('U'), Cmd::ThumbUp),
    bind(KeyCode::Char('D'), Cmd::ThumbDown),
    bind(KeyCode::Char('Z'), Cmd::Tired),
    bind(KeyCode::Char('C'), Cmd::ArtType),
    bind(KeyCode::Char('R'), Cmd::Retro),
    bind(KeyCode::Char('H'), Cmd::Tint),
    bind(KeyCode::Char('B'), Cmd::Sleep),
    bind(KeyCode::Char('I'), Cmd::Pin),
    bind(KeyCode::Char('.'), Cmd::SongMenu),
    Binding { code: KeyCode::Right, shift: true, cmd: Cmd::VolumeUp },
    Binding { code: KeyCode::Left, shift: true, cmd: Cmd::VolumeDown },
    bind(KeyCode::Char('{'), Cmd::MenuNarrower),
    bind(KeyCode::Char('}'), Cmd::MenuWider),
    // Lowercase: the pane that has the keys.
    bind(KeyCode::Char('p'), Cmd::Pause),
    bind(KeyCode::Char(' '), Cmd::Pause),
    bind(KeyCode::Char('l'), Cmd::LikePane),
    bind(KeyCode::Char('b'), Cmd::BadPane),
    bind(KeyCode::Char('a'), Cmd::Add),
    bind(KeyCode::Char('f'), Cmd::Star),
    bind(KeyCode::Char('r'), Cmd::Rename),
    bind(KeyCode::Char('x'), Cmd::Delete),
    bind(KeyCode::Char('v'), Cmd::Details),
    bind(KeyCode::Char('e'), Cmd::Edit),
    bind(KeyCode::Char('m'), Cmd::Mode),
    bind(KeyCode::Char('X'), Cmd::Shuffle),
    bind(KeyCode::Char('j'), Cmd::Down),
    bind(KeyCode::Down, Cmd::Down),
    bind(KeyCode::Char('k'), Cmd::Up),
    bind(KeyCode::Up, Cmd::Up),
    bind(KeyCode::Tab, Cmd::NextFocus),
    bind(KeyCode::BackTab, Cmd::PrevFocus),
    bind(KeyCode::Esc, Cmd::ToMenu),
    bind(KeyCode::Enter, Cmd::Enter),
];

/// What a key does, if anything.
pub fn command_for(key: KeyEvent) -> Option<Cmd> {
    let shifted = key.modifiers.contains(KeyModifiers::SHIFT);
    KEYMAP.iter().find(|b| b.code == key.code && (!b.shift || shifted)).map(|b| b.cmd)
}

/// The longest search the add-station box takes.
const MAX_SEARCH: usize = 80;

/// Which part of the add-station box has the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// The text: every key types (Enter searches).
    Typing,
    /// The answers: j/k move, Enter makes the station.
    Results,
}

/// The add-station box (`a`, or the "Add station…" row): a search for
/// artists and songs, and what Pandora answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationSearch {
    pub text: TextInput,
    pub mode: SearchMode,
    pub hits: Vec<SearchHit>,
    pub cursor: usize,
    /// The text that was sent, once Enter searched (its answer is `hits`).
    pub asked: Option<String>,
    /// A search is on its way.
    pub waiting: bool,
    /// Why the last search failed.
    pub error: Option<String>,
    /// Started from a song's menu: the cursor goes to the first hit of this kind.
    pub prefer: Option<crate::pandora::HitKind>,
}

impl StationSearch {
    fn new() -> Self {
        Self {
            text: TextInput::default(),
            mode: SearchMode::Typing,
            hits: Vec::new(),
            cursor: 0,
            asked: None,
            waiting: false,
            error: None,
            prefer: None,
        }
    }

    /// The hit the cursor is on.
    pub fn selected(&self) -> Option<&SearchHit> {
        self.hits.get(self.cursor.min(self.hits.len().saturating_sub(1)))
    }
}

/// Pandora's genre stations, loaded once per session.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GenresState {
    /// Not asked for yet.
    #[default]
    Idle,
    Loading,
    Loaded(Vec<GenreCategory>),
    Failed(String),
}

/// A station to rename or delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationRef {
    pub id: String,
    pub name: String,
    /// It was picked as a favorite in the menu: the box says so.
    pub favorite: bool,
}

/// The rename box: the station and what is typed so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rename {
    pub station: StationRef,
    pub text: TextInput,
}

/// What the details popup (`v`) is about.
#[derive(Debug, Clone, PartialEq)]
pub enum DetailsOf {
    Song(Box<Song>),
    Station(Box<Station>),
}

/// The details popup: what is known now, and what Pandora says about the
/// station (`None` while it is still on its way).
#[derive(Debug, Clone, PartialEq)]
pub struct Details {
    pub of: DetailsOf,
    /// "Why This Song?": the song's details led by why the station plays it.
    pub why: bool,
    /// The name of the song's station (or the station itself).
    pub station_name: String,
    pub station_id: String,
    pub from_pandora: Option<Result<StationDetails, String>>,
}

/// "2026-10-23T21:05:00.000-04:00" → "2026-10-23".
fn date_of(stamp: &str) -> Option<&str> {
    let day = stamp.get(..10)?;
    let b = day.as_bytes();
    (b[4] == b'-' && b[7] == b'-' && day.bytes().filter(u8::is_ascii_digit).count() == 8).then_some(day)
}

impl Details {
    pub fn title(&self) -> String {
        match &self.of {
            DetailsOf::Song(s) => s.title.clone(),
            DetailsOf::Station(s) => s.name.clone(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self.of {
            DetailsOf::Song(_) if self.why => "Why This Song?",
            DetailsOf::Song(_) => "Song",
            DetailsOf::Station(_) => "Station",
        }
    }

    /// Why the station plays `s`, from what boxvibe knows: the station's
    /// seeds and your thumb (Pandora's own reasons aren't in its web API).
    fn why_text(&self, s: &Song) -> String {
        let station = if self.station_name.is_empty() {
            "this station".to_owned()
        } else {
            format!("“{}”", self.station_name)
        };
        let mut why = match &self.from_pandora {
            None => return "loading the station's seeds…".into(),
            Some(Err(_)) => format!("Pandora picked it for {station}; its seeds couldn't be loaded."),
            Some(Ok(d)) => {
                let same = |a: &str| !a.is_empty() && a.trim().eq_ignore_ascii_case(s.artist.trim());
                if let Some(seed) = d.seeds.iter().find(|x| x.kind == "Song" && x.name.eq_ignore_ascii_case(&s.title)) {
                    format!("“{}” is itself a seed of {station}.", seed.name)
                } else if d.seeds.iter().any(|x| x.kind == "Artist" && same(&x.name)) {
                    format!("{} is a seed of {station}, so Pandora plays their songs and music like them.", s.artist)
                } else if d.seeds.is_empty() {
                    format!("Pandora picked it for {station} (a station without seeds you set, like a genre).")
                } else {
                    let names: Vec<&str> = d.seeds.iter().take(4).map(|x| x.name.as_str()).collect();
                    format!("Pandora picked it as music like {station}'s seeds: {}.", names.join(", "))
                }
            }
        };
        match s.rating {
            Rating::Up => why.push_str(" You gave it a thumbs up."),
            Rating::Down => why.push_str(" You gave it a thumbs down."),
            Rating::None => {}
        }
        why
    }

    /// The popup's rows (label, value): only what Pandora actually sent.
    pub fn rows(&self) -> Vec<(String, String)> {
        let mut rows: Vec<(String, String)> = Vec::new();
        let mut add = |label: &str, value: String| {
            if !value.trim().is_empty() {
                rows.push((label.into(), value));
            }
        };
        if let (true, DetailsOf::Song(s)) = (self.why, &self.of) {
            add("Why", self.why_text(s));
        }
        match &self.of {
            DetailsOf::Song(s) => {
                add("Title", s.title.clone());
                add("Artist", s.artist.clone());
                add("Album", s.album.clone());
                if s.length > 0 {
                    add("Length", format!("{}:{:02}", s.length / 60, s.length % 60));
                }
                add("Station", self.station_name.clone());
                add(
                    "Your thumb",
                    match s.rating {
                        Rating::Up => "up",
                        Rating::Down => "down",
                        Rating::None => "none yet",
                    }
                    .into(),
                );
                add(
                    "Audio",
                    match s.audio_format {
                        crate::pandora::AudioFormat::Mp3 => "MP3",
                        crate::pandora::AudioFormat::Aac => "AAC",
                        crate::pandora::AudioFormat::Unknown => "",
                    }
                    .into(),
                );
                if s.gain != 0.0 {
                    add("Loudness fix", format!("{:+.1} dB", s.gain));
                }
                add("Pandora page", s.detail_url.clone());
            }
            DetailsOf::Station(s) => {
                add("Name", s.name.clone());
                let kind = if s.is_shuffle { "Shuffle (all your stations)" } else { "Station" };
                add("Kind", kind.into());
            }
        }
        match &self.from_pandora {
            None => add("Station facts", "loading from Pandora…".into()),
            Some(Err(why)) => add("Station facts", format!("not available: {why}")),
            Some(Ok(d)) => {
                for (i, seed) in d.seeds.iter().enumerate() {
                    add(if i == 0 { "Seeds" } else { "" }, format!("{}: {}", seed.kind, seed.name));
                }
                if !d.genres.is_empty() {
                    add("Genres", d.genres.join(", "));
                }
                match (d.thumbs_up, d.thumbs_down) {
                    (Some(up), Some(down)) => add("Station thumbs", format!("{up} up · {down} down")),
                    (Some(up), None) => add("Station thumbs", format!("{up} up")),
                    (None, Some(down)) => add("Station thumbs", format!("{down} down")),
                    (None, None) => {}
                }
                add("Created", date_of(&d.created).unwrap_or_default().into());
                add("Last played", date_of(&d.last_played).unwrap_or_default().into());
                add("Made by", d.creator.clone());
            }
        }
        rows
    }
}

/// How many bottom-bar messages the Notifications page keeps.
pub const NOTES_KEPT: usize = 200;

/// What the song menu (`.`) offers, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongAction {
    StationFromArtist,
    StationFromSong,
    Tired,
    Share,
    Why,
    Pin,
    ThumbUp,
    ThumbDown,
    Details,
    Artist,
    Mode,
    Sleep,
}

impl SongAction {
    pub const ALL: [SongAction; 12] = [
        Self::StationFromArtist,
        Self::StationFromSong,
        Self::Tired,
        Self::Share,
        Self::Why,
        Self::Pin,
        Self::ThumbUp,
        Self::ThumbDown,
        Self::Details,
        Self::Artist,
        Self::Mode,
        Self::Sleep,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::StationFromArtist => "Start Station from Artist",
            Self::StationFromSong => "Start Station from Song",
            Self::Tired => "Tired of This Song",
            Self::Share => "Share (Copy Link)",
            Self::Why => "Why This Song?",
            Self::Pin => "Pin",
            Self::ThumbUp => "Thumbs Up",
            Self::ThumbDown => "Thumbs Down",
            Self::Details => "Details",
            Self::Artist => "Similar Artists & Bio",
            Self::Mode => "Station Mode",
            Self::Sleep => "Sleep Timer",
        }
    }

    /// The key that does the same outside the menu.
    pub fn key(self) -> &'static str {
        match self {
            Self::Tired => "Z",
            Self::Pin => "I",
            Self::ThumbUp => "l",
            Self::ThumbDown => "b",
            Self::Details => "v",
            Self::Mode => "m",
            Self::Sleep => "B",
            _ => "",
        }
    }
}

/// The open song menu: which song (by track token) and the cursor.
#[derive(Debug, Clone)]
pub struct SongMenu {
    pub song: Song,
    pub cursor: usize,
}

/// The link Share copies: the song's Pandora page, else a Pandora search.
pub fn share_link(song: &Song) -> String {
    if song.detail_url.starts_with("http") {
        return song.detail_url.clone();
    }
    if let Some(path) = song.detail_url.strip_prefix('/') {
        return format!("https://www.pandora.com/{path}");
    }
    let mut q = String::new();
    for b in format!("{} {}", song.title, song.artist).bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => q.push(b as char),
            _ => q.push_str(&format!("%{b:02X}")),
        }
    }
    format!("https://www.pandora.com/search/{q}/all")
}

/// Standard base64 (for OSC 52).
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub struct App {
    /// Media keys and the desktop's player widget (`None` until main starts it).
    pub mpris: Option<crate::mpris::Mpris>,
    /// Schemes saved by Settings → Theme Select, by name.
    pub user_themes: Vec<(String, theme::Palette)>,
    /// Where they are saved (`None` in the demo: kept in memory only).
    pub themes_dir: Option<PathBuf>,
    /// A theme list or download on its way.
    theme_job: Option<std::sync::mpsc::Receiver<crate::theme_store::Msg>>,
    /// Settings → Theme Select's popup.
    pub theme_picker: Option<ThemePicker>,
    pub config: Config,
    pub config_path: Option<PathBuf>,
    pub demo: bool,
    pub session: Session,
    pub view: View,
    pub focus: Focus,
    pub menu_cursor: usize,
    pub settings_cursor: usize,
    /// Selected row of the station list.
    pub stations_cursor: usize,
    /// The cursor is on the "Add station…" row above the list.
    pub on_add_row: bool,
    /// Which song pane has the keys while the player has them.
    pub pane: Pane,
    /// Selected row of Up Next (0 = the next song).
    pub next_cursor: usize,
    /// Selected row of History (0 = the last song played).
    pub history_cursor: usize,
    pub prompt: Option<Prompt>,
    pub query: TextInput,
    pub palette: Palette,
    omarchy: theme::Watcher,
    last_theme_poll: Instant,
    pub stations: Vec<Station>,
    pub current_station: Option<String>,
    pub queue: VecDeque<Song>,
    pub now: Option<Song>,
    /// Played songs, newest first.
    pub history: Vec<Song>,
    pub status: String,
    pub help: bool,
    /// The sleep timer (`B`), when one is set.
    pub sleep: Option<Sleep>,
    /// The sleep timer ran out at a song's end: pause what starts next.
    sleep_pause_next: bool,
    /// The boot screen: when it began (`None` once it is over or switched off).
    pub splash: Option<Instant>,
    /// What boxvibe needs from the system and whether it is there (`None`:
    /// still looking, or not looked yet).
    pub system: Option<Vec<crate::deps::Check>>,
    /// The System Check list is showing.
    pub system_open: bool,
    /// The look at the system that is on its way.
    system_look: Option<std::sync::mpsc::Receiver<crate::deps::Facts>>,
    /// `x` asked "Delete this station?": y does, any other key doesn't.
    pub confirm_delete: Option<StationRef>,
    /// `x` on the Pinned page asked "Unpin this song?" (the pin's index).
    pub confirm_unpin: Option<usize>,
    /// Songs pinned with Shift+I, newest first.
    pub pins: Vec<crate::pinned::Pin>,
    /// Where they are kept (`None`: in the demo, only in memory).
    pub pins_path: Option<PathBuf>,
    pub pinned_cursor: usize,
    /// Up Next songs disliked with b: sent, and skipped, when their turn comes.
    pub held_dislikes: std::collections::HashSet<String>,
    /// `.`: the song menu, and the song it is for.
    pub song_menu: Option<SongMenu>,
    /// The artist page or station modes popup.
    pub panel: Option<Panel>,
    /// Bottom-bar messages, newest first (at most `NOTES_KEPT`).
    pub notes: VecDeque<(Instant, String)>,
    pub notes_cursor: usize,
    last_status: String,
    /// The "genre stations can't be changed" popup, while it is open.
    pub genre_notice: Option<GenreNotice>,
    /// Good Vibes Only was turned off in Settings: the risk popup asks first
    /// (Enter = turn it off anyway, Esc = keep it on).
    /// Settings → Good Vibes Only asks before Muted or Off: which one.
    pub confirm_good_vibes: Option<GoodVibes>,
    /// `r` is typing a new station name.
    pub rename: Option<Rename>,
    /// `a`: the add-station search box.
    pub add_station: Option<StationSearch>,
    /// Pandora's genre stations (the Genres page).
    pub genres: GenresState,
    /// Selected genre station, counted over the stations only (not the headings).
    pub genres_cursor: usize,
    /// A station is being made: a second Enter waits for it.
    creating: bool,
    /// `v`: the details popup.
    pub details: Option<Details>,
    /// `e`: the seeds and thumbs editor.
    pub editor: Option<Editor>,
    /// Good Vibes Only: where the ad break is (see [`ad`]).
    pub ad: AdPhase,
    ad_serial: u32,
    ad_tracker: ads::Tracker,
    pub buffering: Option<String>,
    audio_cache: HashMap<String, Vec<u8>>,
    /// The covers.
    pub art: crate::art::Art,
    requested: HashSet<String>,
    playlist_pending: bool,
    pub audio: Audio,
    backend: Backend,
    /// Where the Pandora password is kept (see [`crate::keychain`]).
    keychain: Arc<dyn Keychain>,
    /// What is known about the saved password.
    pub keychain_state: KeychainState,
    /// The typed password is not in the keychain yet: it is saved once Pandora accepts it.
    password_unsaved: bool,
    /// The password came from config.toml (an older boxvibe wrote it there):
    /// the keychain gets it, and the file loses it once that worked.
    password_from_file: bool,
    /// A newer release (About shows it).
    pub update: Option<Update>,
    /// GitHub confirmed this is the latest release.
    pub up_to_date: bool,
    pub self_update: Progress,
    /// Asks the main loop to open config.toml in an editor.
    pub edit_config: bool,
    /// After a login: whether the plan streams 192 kbps MP3.
    pub hifi: Option<bool>,
    /// Restart Now was pressed: once quit, start the new version.
    pub restart: bool,
    /// Shift+Q asked "Quit boxvibe?": y quits, any other key doesn't (like tubevibe).
    pub confirm_quit: bool,
    pub quit: bool,
}

const HISTORY_LEN: usize = 10;
/// The longest station name the rename box takes.
const MAX_STATION_NAME: usize = 80;
/// The longest email or password the prompts take.
const MAX_PROMPT: usize = 256;
/// How long the boot screen stays when no key skips it.
pub const SPLASH: Duration = Duration::from_millis(2000);

impl App {
    pub fn new(config: Config, config_path: Option<PathBuf>, warnings: &[ConfigWarning], demo: bool) -> Self {
        Self::with_keychain(config, config_path, warnings, demo, keychain::system())
    }

    pub fn with_keychain(
        config: Config,
        config_path: Option<PathBuf>,
        warnings: &[ConfigWarning],
        demo: bool,
        keychain: Arc<dyn Keychain>,
    ) -> Self {
        let quality = Quality::parse(&config.pandora.audio_quality);
        let audio = Audio::open(demo, config.player.volume);
        let backend = Backend::spawn(demo, quality, config.pandora.user_agent.clone());
        let user_agent = match config.pandora.user_agent.trim() {
            "" => crate::pandora::DEFAULT_USER_AGENT,
            agent => agent,
        }
        .to_owned();
        let mut app = Self {
            session: Session::LoggedOut,
            view: View::Player,
            focus: Focus::Menu,
            menu_cursor: 0,
            settings_cursor: 0,
            stations_cursor: 0,
            on_add_row: false,
            pane: Pane::UpNext,
            next_cursor: 0,
            history_cursor: 0,
            prompt: None,
            query: TextInput::default(),
            palette: theme::TOKYO_NIGHT,
            omarchy: theme::Watcher::system(),
            last_theme_poll: Instant::now(),
            stations: Vec::new(),
            current_station: None,
            queue: VecDeque::new(),
            now: None,
            history: Vec::new(),
            status: String::new(),
            help: false,
            sleep: None,
            sleep_pause_next: false,
            splash: None,
            system: None,
            system_open: false,
            system_look: None,
            confirm_delete: None,
            confirm_unpin: None,
            pins_path: if demo { None } else { crate::pinned::default_path() },
            pins: Vec::new(),
            pinned_cursor: 0,
            held_dislikes: Default::default(),
            song_menu: None,
            panel: None,
            notes: VecDeque::new(),
            notes_cursor: 0,
            last_status: String::new(),
            genre_notice: None,
            confirm_good_vibes: None,
            rename: None,
            add_station: None,
            genres: GenresState::Idle,
            genres_cursor: 0,
            creating: false,
            details: None,
            editor: None,
            ad: AdPhase::Off,
            ad_serial: 0,
            ad_tracker: ads::Tracker::new(!demo, true, &user_agent),
            buffering: None,
            audio_cache: HashMap::new(),
            requested: HashSet::new(),
            playlist_pending: false,
            audio,
            backend,
            keychain,
            keychain_state: KeychainState::Unknown,
            password_unsaved: false,
            password_from_file: false,
            update: None,
            up_to_date: false,
            self_update: Progress::Idle,
            edit_config: false,
            hifi: None,
            restart: false,
            confirm_quit: false,
            art: crate::art::Art::default(),
            quit: false,
            config,
            config_path,
            user_themes: if demo {
                Vec::new()
            } else {
                theme::user_dir().map(|d| theme::load_user(&d)).unwrap_or_default()
            },
            themes_dir: if demo { None } else { theme::user_dir() },
            theme_job: None,
            theme_picker: None,
            mpris: None,
            demo,
        };
        app.apply_theme();
        app.status = ConfigWarning::summary(warnings)
            .or_else(|| app.audio.problem.clone())
            .unwrap_or_else(|| if demo { "Demo mode: made-up stations, no sound".into() } else { String::new() });
        if demo {
            app.log_in();
        } else {
            if app.config.ui.check_updates {
                app.backend.check_update();
            }
            app.start_account();
        }
        app
    }

    /// First thing after a start: log in with what is saved, or ask.
    fn start_account(&mut self) {
        if self.config.has_login() {
            // A password an older boxvibe left in config.toml moves to the keychain
            // (once Pandora accepts it; see `Reply::LoggedIn`).
            self.password_from_file = true;
            self.password_unsaved = true;
            self.log_in();
        } else if !self.config.pandora.username.trim().is_empty() {
            // The usual start: the password is in the keychain.
            let user = self.config.pandora.username.trim().to_owned();
            self.backend.keychain(&self.keychain, &user, KeychainJob::Load);
            if self.status.is_empty() {
                self.status = "Getting your password from the keychain…".into();
            }
        } else {
            // First run: Settings, on the account, typing the email.
            self.view = View::Settings;
            self.focus = Focus::Content;
            self.settings_cursor = Setting::at(Setting::Email);
            self.open_prompt(Prompt::Email);
            self.status = "Welcome to boxvibe: log in with your Pandora account".into();
        }
    }

    /// Looks at the system in the background (fonts get 1.5 s at most). The
    /// answer arrives in [`App::tick`]; the app never waits for it.
    pub fn start_system_check(&mut self) {
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(crate::deps::probe(Duration::from_millis(1500)));
        });
        self.system_look = Some(receive);
    }

    /// The answer of a look. Something needed missing: the list opens by
    /// itself (unless you are typing something), with a line in the status
    /// bar. Missing extras only get that line.
    pub fn system_checked(&mut self, facts: &crate::deps::Facts) {
        let checks = crate::deps::checks(self.config.ui.icons == config::IconSet::Nerd, facts);
        let lacking: Vec<&str> = checks.iter().filter(|c| c.needed && !c.ok()).map(|c| c.dep.name()).collect();
        let extras = checks.iter().filter(|c| !c.needed && !c.ok()).count();
        if !lacking.is_empty() {
            self.status = format!("⚠ Missing: {}", lacking.join(", "));
            if !self.overlay_open() && self.prompt.is_none() {
                self.system_open = true;
            }
        } else if self.system_open {
            self.status = "System checked".into();
        } else if extras > 0 && !self.status.starts_with('⚠') && self.status != NO_KEYCHAIN {
            // (a config warning already showing matters more)
            let names: Vec<&str> = checks.iter().filter(|c| !c.needed && !c.ok()).map(|c| c.dep.name()).collect();
            self.status = format!("Optional, not installed: {}", names.join(", "));
        }
        self.system = Some(checks);
    }

    /// Keys on the System Check list: Enter looks again, any other key closes it.
    fn on_system_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Enter {
            self.system = None;
            self.start_system_check();
        } else {
            self.system_open = false;
        }
    }

    /// The boot screen starts (main calls this once, if `[look] boot_screen` and `[look] animations` are on).
    pub fn begin_splash(&mut self) {
        self.splash = (self.config.ui.splash && self.config.ui.animations).then(Instant::now);
    }

    /// How far along the boot screen is, 0.0 to 1.0 (`None`: not showing).
    pub fn splash_progress(&self) -> Option<f32> {
        self.splash.map(|t| (t.elapsed().as_secs_f32() / SPLASH.as_secs_f32()).min(1.0))
    }

    pub fn follows_omarchy(&self) -> bool {
        self.theme_palette().is_none()
    }

    /// The built-in or saved palette of the chosen theme (`None`: follow Omarchy).
    fn theme_palette(&self) -> Option<theme::Palette> {
        let name = self.config.ui.theme();
        theme::built_in(name).or_else(|| self.user_themes.iter().find(|(n, _)| n == name).map(|(_, p)| *p))
    }

    pub(crate) fn apply_theme(&mut self) {
        match self.theme_palette() {
            Some(p) => self.palette = p,
            None => {
                self.omarchy.reset();
                if let Some(p) = self.omarchy.poll() {
                    self.palette = p;
                }
            }
        }
    }

    /// Saves settings to `config.toml` (never in the demo).
    fn save(&mut self, values: &[(&str, &str, toml_edit::Value)]) {
        if self.demo {
            return;
        }
        let Some(path) = &self.config_path else { return };
        if let Err(e) = config::set_values(path, values) {
            self.status = format!("⚠ {e}");
        }
    }

    // --- The menu ---

    /// The menu has its NOW PLAYING entry: a station is playing.
    pub fn has_now_playing(&self) -> bool {
        self.current_station.is_some()
    }

    pub fn menu_items(&self) -> Vec<MenuItem> {
        let top = self.has_now_playing().then_some(MenuItem::NowPlaying);
        top.into_iter()
            .chain([MenuItem::Stations, MenuItem::Genres])
            .chain(self.config.ui.pinned_in_menu.then_some(MenuItem::Pinned))
            .chain(
                (0..if self.config.ui.favorites_in_menu { self.config.ui.favorites.len() } else { 0 })
                    .map(MenuItem::Favorite),
            )
            .chain([MenuItem::Settings, MenuItem::Notifications, MenuItem::About, MenuItem::Quit])
            .collect()
    }

    pub fn menu_selected(&self) -> MenuItem {
        let items = self.menu_items();
        items[self.menu_cursor.min(items.len() - 1)]
    }

    fn open_menu_item(&mut self) {
        match self.menu_selected() {
            MenuItem::NowPlaying => self.show_now_playing(),
            MenuItem::Stations => self.show_stations(),
            MenuItem::Genres => self.show_genres(),
            MenuItem::Pinned => self.show_page(View::Pinned),
            MenuItem::Favorite(i) => {
                let Some(fav) = self.config.ui.favorites.get(i).cloned() else { return };
                match self.stations.iter().position(|s| s.id == fav.id) {
                    Some(at) => {
                        // tune() lands on Now Playing exactly like Shift+P.
                        self.tune(at);
                    }
                    None if self.stations.is_empty() => self.status = "The stations are still loading".into(),
                    None => self.status = format!("{} isn't one of your stations any more (f removes it)", fav.name),
                }
            }
            MenuItem::Settings => self.show_page(View::Settings),
            MenuItem::About => self.show_page(View::About),
            MenuItem::Notifications => self.show_page(View::Notifications),
            MenuItem::Quit => self.quit = true,
        }
    }

    /// NOW PLAYING (menu entry or `P`): the player, with the menu on it.
    fn show_now_playing(&mut self) {
        if !self.has_now_playing() {
            self.status = "Nothing is playing yet: pick a station".into();
            return;
        }
        self.view = View::Player;
        self.point_menu_at_view();
        // The keys go to the Now Playing pane, so it shows as active.
        self.focus = Focus::Content;
        self.pane = Pane::NowPlaying;
        // A cover that failed earlier gets another go now that it is wanted.
        self.art.retry_failed();
    }

    /// `F`: the menu, on the first of FAVORITES.
    fn show_favorites(&mut self) {
        match self.menu_items().iter().position(|m| *m == MenuItem::Favorite(0)) {
            Some(at) => {
                self.menu_cursor = at;
                self.focus = Focus::Menu;
            }
            None if !self.config.ui.favorites_in_menu => {
                self.status = "Favorites are hidden from the menu (Settings: Favorites in menu)".into()
            }
            None => self.status = "No favorites yet: f on a station stars it".into(),
        }
    }

    /// Settings or About (menu entry, `O` or `A`): the page, with the keys
    /// on it and its menu entry selected.
    fn show_page(&mut self, view: View) {
        self.view = view;
        self.point_menu_at_view();
        self.focus = Focus::Content;
    }

    /// Stations (menu entry or `S`): the list, with the keys on it.
    fn show_stations(&mut self) {
        self.view = View::Stations;
        self.focus = Focus::Content;
        self.on_add_row = false;
        if let Some(at) = self.current_station.as_ref().and_then(|id| self.stations.iter().position(|s| &s.id == id)) {
            self.stations_cursor = at;
        }
    }

    /// Genres (menu entry or `G`): Pandora's genre stations, loaded the
    /// first time (and again after a failure).
    fn show_genres(&mut self) {
        self.view = View::Genres;
        self.focus = Focus::Content;
        self.point_menu_at_view();
        if matches!(self.genres, GenresState::Idle | GenresState::Failed(_)) {
            if self.session != Session::LoggedIn {
                self.status = "Log in first (Settings, Shift+O) to browse genres".into();
                return;
            }
            self.genres = GenresState::Loading;
            self.backend.send(Request::Genres);
        }
    }

    /// Every genre station, in the order the page lists them.
    pub fn genre_rows(&self) -> Vec<&crate::pandora::Genre> {
        match &self.genres {
            GenresState::Loaded(categories) => categories.iter().flat_map(|c| c.genres.iter()).collect(),
            _ => Vec::new(),
        }
    }

    pub fn genres_index(&self) -> usize {
        self.genres_cursor.min(self.genre_rows().len().saturating_sub(1))
    }

    /// Enter on a genre: Pandora makes the station and it plays; on a
    /// failed or empty page, Enter asks again.
    fn add_genre_station(&mut self) {
        let genre = self.genre_rows().get(self.genres_index()).map(|g| (g.id.clone(), g.name.clone()));
        match (genre, &self.genres) {
            (Some((id, name)), _) => self.create_station(&id, &name),
            (None, GenresState::Loading) => {}
            (None, _) => {
                self.genres = GenresState::Idle;
                self.show_genres();
            }
        }
    }

    // --- Adding stations (a, Genres) ---

    /// `a` or the "Add station…" row: opens the search box.
    fn open_add_station(&mut self) {
        if self.session != Session::LoggedIn {
            self.status = "Log in first (Settings, Shift+O) to add a station".into();
            return;
        }
        self.add_station = Some(StationSearch::new());
    }

    /// `a`: in the station list; anywhere else says where.
    fn ask_add_station(&mut self) {
        if self.focus == Focus::Content && self.view == View::Stations {
            self.open_add_station();
        } else {
            self.status = "a adds a station: open Stations (Shift+S) first".into();
        }
    }

    fn search_key(&mut self, key: KeyEvent) {
        let Some(search) = self.add_station.as_mut() else { return };
        let len = search.hits.len();
        match (search.mode, key.code) {
            (_, KeyCode::Esc) => {
                self.add_station = None;
                self.status = "No station added".into();
            }
            (SearchMode::Typing, KeyCode::Enter) => {
                let text = search.text.text().trim().to_owned();
                if text.is_empty() || search.waiting {
                    return;
                }
                if search.asked.as_deref() == Some(text.as_str()) && search.selected().is_some() {
                    // Already searched for this: Enter picks what the cursor is on.
                    return self.make_selected_station();
                }
                search.asked = Some(text.clone());
                search.waiting = true;
                search.hits.clear();
                search.cursor = 0;
                search.error = None;
                self.backend.send(Request::Search { query: text });
            }
            (SearchMode::Typing, KeyCode::Down | KeyCode::Up | KeyCode::Tab) if len > 0 => {
                search.mode = SearchMode::Results;
            }
            (SearchMode::Typing, _) => {
                search.text.key(key, MAX_SEARCH);
            }
            (SearchMode::Results, KeyCode::Enter) => self.make_selected_station(),
            (SearchMode::Results, KeyCode::Char('j') | KeyCode::Down) => search.cursor = step(search.cursor, len, 1),
            (SearchMode::Results, KeyCode::Char('k') | KeyCode::Up) => search.cursor = step(search.cursor, len, -1),
            (SearchMode::Results, KeyCode::Char('/') | KeyCode::Tab | KeyCode::BackTab) => {
                search.mode = SearchMode::Typing;
            }
            (SearchMode::Results, _) => {}
        }
    }

    fn make_selected_station(&mut self) {
        let Some(hit) = self.add_station.as_ref().and_then(|s| s.selected().cloned()) else { return };
        self.add_station = None;
        self.create_station(&hit.id, &hit.name);
    }

    /// Asks Pandora for a station seeded by `pandora_id` (it plays when it arrives).
    fn create_station(&mut self, pandora_id: &str, name: &str) {
        if self.creating {
            self.status = "Still making the last station…".into();
            return;
        }
        self.creating = true;
        self.status = format!("Making a station from {name}…");
        self.backend.send(Request::CreateStation { pandora_id: pandora_id.to_owned() });
    }

    /// Pandora made (or already had) a station: it joins the list, and plays.
    /// Shift+X: plays Pandora's Shuffle, which mixes songs from the account's
    /// stations (Pandora makes it if the account has none yet).
    fn play_shuffle(&mut self) {
        if let Some(at) = self.stations.iter().position(|s| s.is_shuffle) {
            if self.current_station.as_deref() == Some(self.stations[at].id.as_str()) {
                self.status = "Already playing Shuffle".into();
                return;
            }
            self.stations_cursor = at;
            self.on_add_row = false;
            self.tune(at);
            return;
        }
        self.status = "Asking Pandora for your Shuffle station…".into();
        self.backend.send(Request::Shuffle);
    }

    fn shuffle_arrived(&mut self, station: Station) {
        let at = match self.stations.iter().position(|s| s.id == station.id) {
            Some(at) => at,
            None => {
                self.stations.insert(0, station);
                0
            }
        };
        self.stations_cursor = at;
        self.on_add_row = false;
        self.tune(at);
    }

    fn station_created(&mut self, station: Station) {
        self.creating = false;
        let at = match self.stations.iter().position(|s| s.id == station.id) {
            Some(at) => at,
            None => {
                // Newest first, after Shuffle (which stays on top).
                let at = usize::from(self.stations.first().is_some_and(|s| s.is_shuffle));
                self.stations.insert(at, station);
                at
            }
        };
        self.on_add_row = false;
        self.stations_cursor = at;
        self.tune(at);
    }

    pub fn stations_index(&self) -> usize {
        self.stations_cursor.min(self.stations.len().saturating_sub(1))
    }

    pub fn is_favorite(&self, id: &str) -> bool {
        self.config.ui.favorites.iter().any(|f| f.id == id)
    }

    /// `f`: stars or unstars the station under the cursor (the list, a
    /// favorite in the menu) or else the one playing; saved as `[playback] favorites`.
    fn toggle_favorite(&mut self) {
        let (id, name) = match (self.focus, self.view, self.menu_selected()) {
            (Focus::Content, View::Stations, _) if self.on_add_row => {
                self.status = "f stars a station: move onto one".into();
                return;
            }
            (Focus::Content, View::Stations, _) => match self.stations.get(self.stations_index()) {
                Some(s) => (s.id.clone(), s.name.clone()),
                None => return,
            },
            (Focus::Menu, _, MenuItem::Favorite(i)) => {
                let f = &self.config.ui.favorites[i];
                (f.id.clone(), f.name.clone())
            }
            _ => match self.current_station.as_ref().and_then(|id| self.stations.iter().find(|s| &s.id == id)) {
                Some(s) => (s.id.clone(), s.name.clone()),
                None => {
                    self.status = "Open Stations to pick one to favorite".into();
                    return;
                }
            },
        };
        let selected = self.menu_selected();
        if let Some(at) = self.config.ui.favorites.iter().position(|f| f.id == id) {
            self.config.ui.favorites.remove(at);
            self.status = format!("Removed {name} from Favorites");
        } else {
            self.config.ui.favorites.push(config::Favorite { id, name: name.clone() });
            self.status = format!("{name} added to Favorites");
        }
        // The menu cursor stays on its entry (or the one that took its place).
        if let Some(i) = self.menu_items().iter().position(|m| *m == selected) {
            self.menu_cursor = i;
        }
        self.menu_cursor = self.menu_cursor.min(self.menu_items().len() - 1);
        self.save_favorites();
    }

    fn save_favorites(&mut self) {
        let mut list = toml_edit::Array::new();
        for f in &self.config.ui.favorites {
            let mut t = toml_edit::InlineTable::new();
            t.insert("id", f.id.as_str().into());
            t.insert("name", f.name.as_str().into());
            list.push(t);
        }
        self.save(&[("playback", "favorites", toml_edit::Value::Array(list))]);
    }

    /// Shift+I: pins the selected song (Up Next, History) or else the playing one.
    /// `.`: the song menu for the selected (or playing) song.
    fn open_song_menu(&mut self) {
        let Some(song) = self.selected_song() else {
            self.status = "No song here: pick a station first".into();
            return;
        };
        if song.title.is_empty() || song.track_token.is_empty() {
            self.status = "Ads have no song menu".into();
            return;
        }
        self.song_menu = Some(SongMenu { song, cursor: 0 });
    }

    fn song_menu_key(&mut self, key: KeyEvent) {
        let Some(menu) = self.song_menu.as_mut() else { return };
        let len = SongAction::ALL.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('.' | 'q') => self.song_menu = None,
            KeyCode::Char('j') | KeyCode::Down => menu.cursor = step(menu.cursor, len, 1),
            KeyCode::Char('k') | KeyCode::Up => menu.cursor = step(menu.cursor, len, -1),
            KeyCode::Home | KeyCode::Char('g') => menu.cursor = 0,
            KeyCode::End | KeyCode::Char('G') => menu.cursor = len - 1,
            KeyCode::Enter => {
                let action = SongAction::ALL[menu.cursor.min(len - 1)];
                let Some(menu) = self.song_menu.take() else { return };
                self.song_action(action, menu.song);
            }
            _ => {}
        }
    }

    fn song_action(&mut self, action: SongAction, song: Song) {
        let playing = self.now.as_ref().is_some_and(|n| n.track_token == song.track_token);
        match action {
            SongAction::StationFromArtist => {
                self.station_from_search(song.artist.clone(), crate::pandora::HitKind::Artist)
            }
            SongAction::StationFromSong => {
                self.station_from_search(format!("{} {}", song.title, song.artist), crate::pandora::HitKind::Song)
            }
            SongAction::Tired if playing => self.tired(),
            SongAction::Tired => {
                self.backend.send(Request::Tired { track_token: song.track_token.clone() });
                if let Some(at) = self.queue.iter().position(|s| s.track_token == song.track_token) {
                    self.queue.remove(at);
                    self.held_dislikes.remove(&song.track_token);
                    self.requested.remove(&song.track_token);
                    self.audio_cache.remove(&song.track_token);
                }
                self.status = format!("Tired of “{}”: Pandora rests it for a month", song.title);
            }
            SongAction::Share => self.share(&song),
            SongAction::Why => self.show_song_details(song, true),
            SongAction::Details => self.show_song_details(song, false),
            SongAction::Pin => self.pin(),
            SongAction::ThumbUp | SongAction::ThumbDown => {
                let positive = action == SongAction::ThumbUp;
                if self.in_songs() { self.rate_in_pane(positive) } else { self.rate(positive) }
            }
            SongAction::Artist => self.open_artist(&song),
            SongAction::Mode => {
                let station = self.stations.iter().find(|s| s.id == song.station_id).map(|s| StationRef {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    favorite: false,
                });
                self.open_modes(station)
            }
            SongAction::Sleep => self.next_sleep(),
        }
    }

    /// "Start Station from …": the add-station search, filled in and sent;
    /// Enter makes the station from the highlighted hit.
    fn station_from_search(&mut self, query: String, prefer: crate::pandora::HitKind) {
        self.open_add_station();
        let Some(search) = self.add_station.as_mut() else { return };
        search.text = TextInput::new(query.trim());
        search.prefer = Some(prefer);
        let query = search.text.text().to_owned();
        search.asked = Some(query.clone());
        search.waiting = true;
        self.backend.send(Request::Search { query });
    }

    /// The song's Pandora page (or a Pandora search for it) on the clipboard:
    /// wl-copy, else xclip, else the terminal's OSC 52.
    fn share(&mut self, song: &Song) {
        let link = share_link(song);
        if self.demo {
            self.status = format!("Demo: would copy {link}");
            return;
        }
        let tools: [(&str, &[&str]); 2] = [("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])];
        for (tool, args) in tools {
            let child = std::process::Command::new(tool)
                .args(args)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            if let Ok(mut child) = child {
                use std::io::Write;
                let wrote = child.stdin.take().is_some_and(|mut i| i.write_all(link.as_bytes()).is_ok());
                if wrote && child.wait().is_ok_and(|s| s.success()) {
                    self.status = format!("Copied: {link}");
                    return;
                }
            }
        }
        use std::io::Write;
        let mut out = std::io::stdout();
        let _ = write!(out, "\x1b]52;c;{}\x07", base64(link.as_bytes()));
        let _ = out.flush();
        self.status = format!("Copied (via the terminal): {link}");
    }

    fn show_song_details(&mut self, song: Song, why: bool) {
        let station_name =
            self.stations.iter().find(|s| s.id == song.station_id).map(|s| s.name.clone()).unwrap_or_default();
        let details = Details {
            station_name,
            station_id: song.station_id.clone(),
            of: DetailsOf::Song(Box::new(song)),
            from_pandora: None,
            why,
        };
        if details.station_id.is_empty() {
            self.details = Some(Details { from_pandora: Some(Err("no station for this song".into())), ..details });
            return;
        }
        let current = self.current_station.as_deref() == Some(details.station_id.as_str());
        self.backend.send(Request::StationDetails { station_id: details.station_id.clone(), current });
        self.details = Some(details);
    }

    /// The song a song key is for: the Up Next or History row when that
    /// pane has the keys, else the playing song.
    fn selected_song(&self) -> Option<Song> {
        match (self.in_songs(), self.pane) {
            (true, Pane::UpNext) => self.queue.get(self.next_index()),
            (true, Pane::History) => self.history.get(self.history_index()),
            _ => None,
        }
        .or(self.now.as_ref())
        .cloned()
    }

    fn pin(&mut self) {
        let song = self.selected_song();
        let Some(song) = song else {
            self.status = "No song to pin: pick a station first".into();
            return;
        };
        if song.title.is_empty() {
            self.status = "Ads can't be pinned".into();
            return;
        }
        if self.pins.iter().any(|p| p.same(&song)) {
            self.status = format!("“{}” is already pinned", song.title);
            return;
        }
        self.pins.insert(0, crate::pinned::Pin::of(&song, &crate::pinned::today()));
        self.pinned_cursor = 0;
        self.save_pins();
        let where_ = if self.config.ui.pinned_in_menu { "the menu's Pinned" } else { "pinned.toml" };
        self.status = format!("Pinned “{}” by {} (see {where_})", song.title, song.artist);
    }

    fn save_pins(&mut self) {
        if let Some(path) = &self.pins_path
            && let Err(e) = crate::pinned::save(path, &self.pins)
        {
            self.status = format!("Couldn't save the pins to {}: {e}", path.display());
        }
    }

    /// Reads the pins from disk (main calls it once; the demo keeps none).
    pub fn load_pins(&mut self) {
        if let Some(path) = &self.pins_path {
            self.pins = crate::pinned::load(path);
        }
    }

    /// Enter on the Pinned page: the song's Pandora page in the browser.
    fn open_pin(&mut self) {
        let Some(pin) = self.pins.get(self.pinned_cursor) else {
            self.status = "Nothing pinned yet: Shift+I pins the selected or playing song".into();
            return;
        };
        if pin.link.is_empty() {
            self.status = format!("Pandora gave no page for “{}”", pin.title);
            return;
        }
        let link = pin.link.clone();
        if !self.demo {
            let _ = std::process::Command::new("xdg-open")
                .arg(&link)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
        self.status = format!("Opening {link}");
    }

    /// Keeps each new bottom-bar message for the Notifications page.
    pub fn note_status(&mut self) {
        if self.status == self.last_status {
            return;
        }
        self.last_status.clone_from(&self.status);
        if self.status.trim().is_empty() || self.notes.front().is_some_and(|(_, s)| *s == self.status) {
            return;
        }
        self.notes.push_front((Instant::now(), self.status.clone()));
        self.notes.truncate(NOTES_KEPT);
        if self.notes_cursor > 0 {
            self.notes_cursor = (self.notes_cursor + 1).min(NOTES_KEPT - 1);
        }
    }

    /// The menu entry of what the right side shows.
    fn point_menu_at_view(&mut self) {
        let items = self.menu_items();
        let want = match self.view {
            View::Settings => Some(MenuItem::Settings),
            View::About => Some(MenuItem::About),
            View::Notifications => Some(MenuItem::Notifications),
            View::Stations => Some(MenuItem::Stations),
            View::Genres => Some(MenuItem::Genres),
            View::Pinned => Some(MenuItem::Pinned),
            View::Player if self.has_now_playing() => Some(MenuItem::NowPlaying),
            View::Player => None,
        };
        if let Some(i) = want.and_then(|w| items.iter().position(|m| *m == w)) {
            self.menu_cursor = i;
        }
    }

    // --- Every frame ---

    pub fn tick(&mut self) {
        if self.splash.is_some_and(|t| t.elapsed() >= SPLASH) {
            self.splash = None;
        }
        self.note_status();
        while let Ok(reply) = self.backend.replies.try_recv() {
            self.on_reply(reply);
        }
        self.note_status();
        if let Some(facts) = self.system_look.as_ref().and_then(|r| r.try_recv().ok()) {
            self.system_look = None;
            self.system_checked(&facts);
        }
        self.sleep_tick();
        if self.now.as_ref().is_some_and(|s| self.audio.finished(s.length)) {
            if self.sleep == Some(Sleep::EndOfSong) {
                self.sleep = None;
                self.sleep_pause_next = true;
                self.audio.set_fade(0.0);
            }
            self.end_song(End::Played);
        }
        self.ad_tick();
        self.mpris_tick();
        self.theme_tick();
        self.want_art();
        if self.follows_omarchy() && self.last_theme_poll.elapsed() > Duration::from_secs(2) {
            self.last_theme_poll = Instant::now();
            if let Some(p) = self.omarchy.poll() {
                self.palette = p;
            }
        }
    }

    /// Media keys in, the playing song out.
    fn mpris_tick(&mut self) {
        let Some(m) = self.mpris.as_ref() else { return };
        let asked: Vec<_> = m.controls.try_iter().collect();
        for c in asked {
            use crate::mpris::Control;
            let paused = self.audio.is_paused();
            match c {
                Control::PlayPause => self.run(Cmd::Pause),
                Control::Play if paused => self.run(Cmd::Pause),
                Control::Pause if !paused => self.run(Cmd::Pause),
                Control::Next => self.run(Cmd::Skip),
                _ => {}
            }
        }
        let snap = match &self.now {
            Some(s) => crate::mpris::Snapshot {
                title: s.title.clone(),
                artist: s.artist.clone(),
                album: s.album.clone(),
                art_url: s.cover_art_url.clone(),
                url: s.detail_url.clone(),
                track: s.track_token.clone(),
                length: s.length,
                playing: Some(!self.audio.is_paused()),
            },
            None => crate::mpris::Snapshot::default(),
        };
        let pos = self.audio.position().as_secs_f64();
        if let Some(m) = self.mpris.as_mut() {
            m.show(snap, pos);
        }
    }

    /// Asks for the covers of this song and the next one.
    fn want_art(&mut self) {
        if self.config.ui.album_art == AlbumArt::Off {
            return;
        }
        let urls = [self.now.as_ref(), self.queue.front()].map(|s| s.map(|s| s.cover_art_url.clone()));
        for url in urls.into_iter().flatten() {
            if self.art.want(&url) {
                self.backend.send(Request::Art { url });
            }
        }
    }

    pub fn settle(&mut self, timeout: Duration) {
        while let Ok(reply) = self.backend.replies.recv_timeout(timeout) {
            self.on_reply(reply);
            self.want_art();
        }
    }

    pub(crate) fn on_reply(&mut self, reply: Reply) {
        if self.editor_reply(&reply) {
            return;
        }
        match reply {
            Reply::LoggedIn { hifi } => {
                self.hifi = Some(hifi);
                let first = self.session != Session::LoggedIn;
                self.session = Session::LoggedIn;
                if !self.demo {
                    let email = self.config.pandora.username.clone();
                    self.save(&[("account", "email_or_username", email.as_str().into())]);
                    // The password goes to the keychain, now that Pandora has taken it.
                    if self.password_unsaved {
                        self.password_unsaved = false;
                        let password = self.config.pandora.password.expose().to_owned();
                        self.backend.keychain(&self.keychain, &email, KeychainJob::Save(password));
                    }
                    // The status bar's right side says who is logged in.
                    self.status = if self.keychain_state == KeychainState::Unavailable {
                        NO_KEYCHAIN.into()
                    } else {
                        String::new()
                    };
                }
                if first && self.view == View::Settings && self.stations.is_empty() {
                    // Straight to the music after logging in from Settings.
                    self.view = View::Player;
                    self.focus = Focus::Menu;
                }
                self.backend.send(Request::Stations);
            }
            Reply::LoginFailed { message, wrong_password } => {
                if wrong_password {
                    // A wrong password is not kept: not in memory, not in the keychain.
                    self.config.pandora.password.clear();
                    self.password_unsaved = false;
                    self.forget_saved_password(false);
                }
                self.session = Session::Failed(message.clone());
                self.status = format!("⚠ Login failed: {message}");
                self.view = View::Settings;
                self.focus = Focus::Content;
                self.settings_cursor = Setting::at(Setting::LogIn);
            }
            Reply::Keychain(reply) => self.on_keychain(reply),
            Reply::Stations(stations) => {
                self.stations = stations;
                self.on_add_row = self.stations.is_empty();
                let last = &self.config.pandora.last_station;
                let at = self.stations.iter().position(|s| &s.id == last);
                self.stations_cursor = at.unwrap_or(0);
                // Favorites keep the names Pandora uses now.
                let mut renamed = false;
                for f in &mut self.config.ui.favorites {
                    if let Some(s) = self.stations.iter().find(|s| s.id == f.id)
                        && s.name != f.name
                    {
                        f.name = s.name.clone();
                        renamed = true;
                    }
                }
                if renamed {
                    self.save_favorites();
                }
                if self.stations.is_empty() {
                    self.status = "This account has no stations yet: create one on pandora.com".into();
                } else if self.config.ui.autoplay_last_station && self.current_station.is_none() {
                    // Shuffle comes first in the list: start on a real station.
                    if at.is_none() && self.stations.len() > 1 && self.stations[0].is_shuffle {
                        self.stations_cursor = 1;
                    }
                    self.tune(self.stations_cursor);
                } else if self.current_station.is_none() && self.status.is_empty() {
                    // Startup always stays on Now Playing (idle); Stations is opened by hand.
                    self.status = "Nothing playing: open Stations (Shift+S) to pick one".into();
                }
            }
            Reply::StationRenamed { station_id, name } => {
                if let Some(s) = self.stations.iter_mut().find(|s| s.id == station_id) {
                    s.name = name.clone();
                }
                let mut starred = false;
                for f in self.config.ui.favorites.iter_mut().filter(|f| f.id == station_id) {
                    f.name = name.clone();
                    starred = true;
                }
                if starred {
                    self.save_favorites();
                }
                self.status = format!("Station renamed to “{name}” on your Pandora account");
            }
            Reply::StationRemoved { station_id } => self.station_removed(&station_id),
            Reply::StationDetails { station_id, details } => {
                if let Some(d) = self.details.as_mut().filter(|d| d.station_id == station_id) {
                    d.from_pandora = Some(Ok(details));
                }
            }
            Reply::SearchResults { query, hits } => {
                if self.editor_search_results(&query, &hits) {
                    return;
                }
                if let Some(search) = self.add_station.as_mut().filter(|s| s.asked.as_deref() == Some(query.as_str())) {
                    search.waiting = false;
                    search.error = None;
                    // Typing on while it searched keeps the keys in the text.
                    let same = search.text.text().trim() == query;
                    search.mode = if hits.is_empty() || !same { SearchMode::Typing } else { SearchMode::Results };
                    search.cursor = search.prefer.and_then(|k| hits.iter().position(|h| h.kind == k)).unwrap_or(0);
                    search.hits = hits;
                }
            }
            Reply::StationCreated(station) => self.station_created(station),
            Reply::Artist(info) => self.artist_loaded(info),
            Reply::Shuffle(station) => self.shuffle_arrived(station),
            Reply::StationModes { station_id, modes, set } => self.modes_loaded(&station_id, modes, set),
            Reply::Genres(categories) => {
                self.genres = GenresState::Loaded(categories);
                self.genres_cursor = 0;
            }
            Reply::Playlist { station_id, songs } => {
                if self.current_station.as_deref() != Some(&station_id) {
                    return;
                }
                self.playlist_pending = false;
                if songs.is_empty() && self.now.is_none() {
                    self.status = "Pandora sent no songs for this station".into();
                }
                self.queue.extend(songs);
                if self.now.is_none() && !self.ad_break() {
                    self.advance();
                } else if self.now.is_some() {
                    self.prefetch();
                }
            }
            Reply::Rated { track_token, positive, feedback_id } => {
                if let Some(song) = self.song_mut(&track_token) {
                    song.feedback_id = feedback_id;
                }
                let ic = crate::icons::icons(self.config.ui.icons);
                let what = if positive { format!("{} Liked", ic.up) } else { format!("{} Disliked", ic.down) };
                let title = self.find_song(&track_token).map(|s| s.title.clone()).unwrap_or_default();
                self.status = format!("{what}: {title}");
            }
            Reply::ThumbCleared { track_token } => {
                let title = self.song_mut(&track_token).map(|s| {
                    s.feedback_id.clear();
                    s.title.clone()
                });
                self.status = format!("Thumb taken back: {}", title.unwrap_or_default());
            }
            Reply::TookOver => {
                self.status = "Pandora was playing on another device with this account: boxvibe took over".into();
            }
            Reply::Tired { track_token } => {
                let title = self.find_song(&track_token).map(|s| s.title.clone()).unwrap_or_default();
                let sleep = crate::icons::icons(self.config.ui.icons).sleep;
                self.status = format!("{sleep} Tired of {title}: Pandora won't play it for a month");
            }
            Reply::AdList { station_id, ads } => self.ad_list_arrived(&station_id, ads),
            Reply::Audio { track_token, data } => {
                let Some(data) = self.ad_audio_arrived(&track_token, data) else { return };
                if self.buffering.as_deref() == Some(&track_token) {
                    self.buffering = None;
                    self.play_now(Some(data));
                } else if self.queue.iter().any(|s| s.track_token == track_token) {
                    self.audio_cache.insert(track_token, data);
                }
            }
            Reply::Art { url, image } => self.art.arrived(url, image),
            Reply::AudioFailed { track_token, message } => {
                if self.ad_audio_failed(&track_token, &message) {
                    return;
                }
                self.requested.remove(&track_token);
                if self.buffering.as_deref() == Some(&track_token) {
                    self.buffering = None;
                    self.status = format!("⚠ {message}: skipping");
                    self.advance();
                }
            }
            // The editor took these (or it was closed meanwhile).
            Reply::Seeds { .. }
            | Reply::SeedSuggestions { .. }
            | Reply::SeedAdded { .. }
            | Reply::SeedRemoved { .. }
            | Reply::Feedback { .. }
            | Reply::FeedbackDeleted { .. } => {}
            Reply::Failed { what, message } => {
                if self.editor_failed(what, &message) || self.panel_failed(what, &message) {
                    return;
                }
                if what == "shuffle" {
                    self.status = format!("⚠ No Shuffle: {message}");
                    return;
                }
                if what == "ads" {
                    self.ad_ask_failed(&message);
                    return;
                }
                if what == "playlist" {
                    self.playlist_pending = false;
                }
                if what == "search"
                    && let Some(search) = self.add_station.as_mut()
                {
                    search.waiting = false;
                    search.error = Some(message);
                    return;
                }
                if what == "create station" {
                    self.creating = false;
                }
                if what == "genres" {
                    self.genres = GenresState::Failed(message.clone());
                }
                if what == "details"
                    && let Some(d) = self.details.as_mut().filter(|d| d.from_pandora.is_none())
                {
                    d.from_pandora = Some(Err(message.clone()));
                    return;
                }
                self.status = format!("⚠ {what}: {message}");
            }
            Reply::UpdateFound(update) => {
                self.status = format!(
                    "{} boxvibe {} available · see About",
                    crate::icons::icons(self.config.ui.icons).update,
                    update.version
                );
                self.update = Some(update);
            }
            Reply::UpToDate => self.up_to_date = self.update.is_none(),
            Reply::UpdateProgress(percent) => {
                if let (Progress::Downloading(_), Some(update)) = (&self.self_update, &self.update) {
                    self.self_update = Progress::Downloading(percent);
                    self.status = format!("⬇ Updating boxvibe to {}… {percent}%", update.version);
                }
            }
            Reply::SelfUpdated(Ok(version)) => {
                self.status = format!("✔ boxvibe {version} installed · Restart Now in About opens it");
                self.self_update = Progress::Done(version);
            }
            Reply::SelfUpdated(Err(error)) => {
                self.status = format!("⚠ Could not update: {error}");
                self.self_update = Progress::Failed(error);
            }
        }
    }

    fn song_mut(&mut self, token: &str) -> Option<&mut Song> {
        self.now.iter_mut().chain(self.history.iter_mut()).chain(self.queue.iter_mut()).find(|s| s.track_token == token)
    }

    fn find_song(&self, token: &str) -> Option<&Song> {
        self.now.iter().chain(self.history.iter()).chain(self.queue.iter()).find(|s| s.track_token == token)
    }

    pub fn station_name(&self) -> Option<&str> {
        let id = self.current_station.as_deref()?;
        self.stations.iter().find(|s| s.id == id).map(|s| s.name.as_str())
    }

    // --- Pandora ---

    fn log_in(&mut self) {
        let (username, password) = if self.demo {
            ("demo".to_owned(), String::new())
        } else {
            (self.config.pandora.username.trim().to_owned(), self.config.pandora.password.expose().to_owned())
        };
        if !self.demo && username.is_empty() {
            self.status = "⚠ Type your Pandora email first".into();
            self.settings_cursor = Setting::at(Setting::Email);
            return;
        }
        if !self.demo && password.is_empty() {
            // Nothing in the keychain (or no keychain): ask, like a login form.
            self.ask_password();
            return;
        }
        self.session = Session::SigningIn;
        if !self.demo {
            self.status = format!("Signing in as {username}…");
        }
        self.backend.send(Request::Login { username, password });
    }

    /// Opens the password box (Settings, on Log in).
    fn ask_password(&mut self) {
        self.view = View::Settings;
        self.focus = Focus::Content;
        self.open_prompt(Prompt::Password);
        self.settings_cursor = Setting::at(Setting::LogIn);
        self.status = format!("Type the password for {}", self.config.pandora.username.trim());
    }

    /// The password popup with `typed` already in it (the snapshot screens).
    pub fn ask_password_for_snapshot(&mut self, typed: &str) {
        self.ask_password();
        for c in typed.chars() {
            self.query.key(
                ratatui::crossterm::event::KeyEvent::new(
                    ratatui::crossterm::event::KeyCode::Char(c),
                    ratatui::crossterm::event::KeyModifiers::NONE,
                ),
                MAX_PROMPT,
            );
        }
    }

    /// What the keychain answered (it always runs on its own thread).
    fn on_keychain(&mut self, reply: KeychainReply) {
        match reply {
            KeychainReply::Loaded(Ok(Some(password))) => {
                if self.config.pandora.password.is_empty() {
                    self.config.pandora.password = Secret::new(password);
                    self.keychain_state = KeychainState::Saved;
                    self.log_in();
                }
            }
            KeychainReply::Loaded(Ok(None)) => {
                self.keychain_state = KeychainState::NotSaved;
                if self.config.pandora.password.is_empty() {
                    self.ask_password();
                }
            }
            KeychainReply::Loaded(Err(_)) => {
                self.keychain_state = KeychainState::Unavailable;
                if self.config.pandora.password.is_empty() {
                    self.ask_password();
                }
                self.status = NO_KEYCHAIN.into();
            }
            KeychainReply::Saved(Ok(())) => {
                self.keychain_state = KeychainState::Saved;
                if std::mem::take(&mut self.password_from_file) {
                    // Only now, with the keychain holding it, does the file let go of it.
                    self.status =
                        match self.config_path.as_deref().filter(|_| !self.demo).map(config::forget_plaintext_password)
                        {
                            Some(Err(e)) => format!("⚠ {e}"),
                            Some(Ok(true)) => "Your password moved from config.toml to the keychain".into(),
                            _ => String::new(),
                        };
                }
            }
            KeychainReply::Saved(Err(_)) => {
                // The password stays in memory for this session, and nowhere on disk.
                // (One an older boxvibe left in config.toml is left there: it isn't lost.)
                self.keychain_state = KeychainState::Unavailable;
                self.password_from_file = false;
                self.status = NO_KEYCHAIN.into();
            }
            KeychainReply::Forgotten { result: Ok(()), .. } => {
                if self.keychain_state == KeychainState::Saved {
                    self.keychain_state = KeychainState::NotSaved;
                }
            }
            KeychainReply::Forgotten { result: Err(_), loud } => {
                self.keychain_state = KeychainState::Unavailable;
                if loud {
                    self.status = format!("{NO_KEYCHAIN}: nothing saved to forget");
                }
            }
        }
    }

    /// Deletes the saved password: from the keychain, and from config.toml if an
    /// older boxvibe left it there. `loud`: the user asked, so say it.
    fn forget_saved_password(&mut self, loud: bool) {
        if self.demo {
            if loud {
                self.status = "The demo has no password".into();
            }
            return;
        }
        let user = self.config.pandora.username.trim().to_owned();
        if !user.is_empty() {
            self.backend.keychain(&self.keychain, &user, KeychainJob::Forget { loud });
        }
        let from_file = self.config_path.as_deref().map(config::forget_plaintext_password);
        self.password_from_file = false;
        if let Some(Err(e)) = from_file {
            self.status = format!("⚠ {e}");
        } else if loud {
            self.status = "Forgot the saved password: you'll be asked for it next start".into();
        }
        if self.keychain_state == KeychainState::Saved {
            self.keychain_state = KeychainState::NotSaved;
        }
    }

    fn log_out(&mut self) {
        self.audio.stop();
        self.ad = AdPhase::Off;
        self.now = None;
        self.queue.clear();
        self.history.clear();
        self.stations.clear();
        self.current_station = None;
        self.menu_cursor = 0;
        self.view = View::Settings;
        self.session = Session::LoggedOut;
        self.forget_saved_password(false);
        self.config.pandora.password.clear();
        self.password_unsaved = false;
        self.status = if self.demo || self.keychain_state == KeychainState::Unavailable {
            "Logged out".into()
        } else {
            "Logged out: the saved password was removed from your keychain".into()
        };
    }

    fn tune(&mut self, index: usize) {
        let Some(station) = self.stations.get(index).cloned() else { return };
        if let Some(song) = self.now.take() {
            self.push_history(song);
        }
        self.audio.stop();
        self.ad = AdPhase::Off;
        self.queue.clear();
        self.audio_cache.clear();
        self.requested.clear();
        self.buffering = None;
        self.next_cursor = 0;
        // The NOW PLAYING entry appears on top: the cursor stays on its entry.
        let first = !self.has_now_playing();
        self.current_station = Some(station.id.clone());
        if first {
            self.menu_cursor += 1;
        }
        self.playlist_pending = true;
        self.backend.send(Request::Playlist { station_id: station.id.clone() });
        if self.config.pandora.last_station != station.id {
            self.config.pandora.last_station = station.id.clone();
            self.save(&[("playback", "last_station", station.id.as_str().into())]);
        }
        self.status = format!("Tuning in to {}", station.name);
        // Every way a station starts lands where Shift+P does: same function, no drift.
        self.show_now_playing();
    }

    fn push_history(&mut self, song: Song) {
        self.history.insert(0, song);
        // The selected History row stays on its song as the list grows.
        if self.history_cursor > 0 {
            self.history_cursor = (self.history_cursor + 1).min(HISTORY_LEN - 1);
        }
        self.history.truncate(HISTORY_LEN);
    }

    fn advance(&mut self) {
        if let Some(song) = self.now.take() {
            self.push_history(song);
        }
        self.audio.stop();
        self.buffering = None;
        self.next_song();
    }

    /// Starts the next song of the queue (or asks for more).
    fn next_song(&mut self) {
        // Dislikes held on Up Next rows count now, at their turn: sent, and skipped.
        while let Some(front) = self.queue.front()
            && self.held_dislikes.contains(&front.track_token)
        {
            let Some(song) = self.queue.pop_front() else { break };
            self.held_dislikes.remove(&song.track_token);
            self.backend.send(Request::Rate { track_token: song.track_token.clone(), positive: false });
            self.status = format!("Disliked and skipped: {}", song.title);
            self.audio_cache.remove(&song.track_token);
            self.push_history(song);
        }
        let Some(song) = self.queue.pop_front() else {
            self.request_more();
            return;
        };
        let token = song.track_token.clone();
        let url = song.audio_url.clone();
        self.notify(&song);
        self.now = Some(song);
        if let Some(data) = self.audio_cache.remove(&token) {
            self.play_now(Some(data));
        } else if self.demo || !self.audio.has_device() {
            self.play_now(None);
        } else {
            self.buffering = Some(token.clone());
            if self.requested.insert(token.clone()) {
                self.backend.send(Request::Audio { track_token: token, url });
            }
        }
        if self.queue.len() < 2 {
            self.request_more();
        }
        self.prefetch();
    }

    fn request_more(&mut self) {
        if self.playlist_pending {
            return;
        }
        if let Some(id) = self.current_station.clone() {
            self.playlist_pending = true;
            self.backend.send(Request::Playlist { station_id: id });
        }
    }

    fn prefetch(&mut self) {
        if self.demo || !self.audio.has_device() {
            return;
        }
        if let Some(next) = self.queue.front()
            && self.requested.insert(next.track_token.clone())
        {
            self.backend.send(Request::Audio { track_token: next.track_token.clone(), url: next.audio_url.clone() });
        }
    }

    fn play_now(&mut self, data: Option<Vec<u8>>) {
        let data = data.filter(|d| !d.is_empty());
        match self.audio.start(data, self.now.as_ref().map_or(0.0, |s| s.gain), false) {
            Ok(()) => {
                // Pandora counts plays (and skips) from this report, like the website.
                if let Some(song) = &self.now {
                    self.backend.send(Request::Started { track_token: song.track_token.clone() });
                }
            }
            Err(e) => {
                self.status = format!("⚠ {e}: skipping");
                self.advance();
            }
        }
    }

    fn notify(&self, song: &Song) {
        if self.demo || !self.config.ui.notifications {
            return;
        }
        let _ = std::process::Command::new("notify-send")
            .args(["-a", "boxvibe", "-t", "4000", &song.title, &format!("{} — {}", song.artist, song.album)])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    // --- Up Next and History ---

    fn move_song(&mut self, by: i32) {
        let (cursor, len) = match self.pane {
            Pane::UpNext => (&mut self.next_cursor, self.queue.len()),
            Pane::History => (&mut self.history_cursor, self.history.len()),
            Pane::NowPlaying => return,
        };
        if len > 0 {
            *cursor = step(*cursor, len, by);
        }
    }

    /// The selected rows, kept inside their lists as songs come and go.
    pub fn next_index(&self) -> usize {
        self.next_cursor.min(self.queue.len().saturating_sub(1))
    }

    pub fn history_index(&self) -> usize {
        self.history_cursor.min(self.history.len().saturating_sub(1))
    }

    /// The player's panes have the keys.
    /// A popup is drawn over the screen (Help, the quit box, the station boxes, details).
    pub fn overlay_open(&self) -> bool {
        self.help
            || self.system_open
            || self.confirm_quit
            || self.confirm_delete.is_some()
            || self.confirm_unpin.is_some()
            || self.genre_notice.is_some()
            || self.confirm_good_vibes.is_some()
            || self.rename.is_some()
            || self.add_station.is_some()
            || self.details.is_some()
            || self.song_menu.is_some()
            || self.panel.is_some()
            || self.theme_picker.is_some()
            || self.editor.is_some()
            || self.prompt == Some(Prompt::Password)
    }

    pub fn in_songs(&self) -> bool {
        self.focus == Focus::Content && self.view == View::Player
    }

    // --- Settings ---

    pub fn setting(&self) -> Setting {
        Setting::ALL[self.settings_cursor.min(Setting::ALL.len() - 1)].1
    }

    /// The value shown next to a setting.
    pub fn setting_value(&self, setting: Setting) -> String {
        let on = |b: bool| if b { "on" } else { "off" }.to_owned();
        match setting {
            Setting::Email if self.demo => "demo@example.com".into(),
            Setting::Email if self.config.pandora.username.is_empty() => "not set".into(),
            Setting::Email => self.config.pandora.username.clone(),
            Setting::Password if self.demo => "not used in the demo".into(),
            Setting::Password => match self.keychain_state {
                KeychainState::Saved => "•••••••• in your keychain".into(),
                KeychainState::Unavailable => "no keychain: asked each start".into(),
                _ if self.password_unsaved => "saving to your keychain…".into(),
                _ => "not saved".into(),
            },
            Setting::LogIn => match &self.session {
                Session::LoggedOut => "not logged in".into(),
                Session::SigningIn => "signing in…".into(),
                Session::LoggedIn => "logged in".into(),
                Session::Failed(e) => e.clone(),
            },
            Setting::LogOut => String::new(),
            Setting::Quality => match (Quality::parse(&self.config.pandora.audio_quality), self.hifi) {
                (Quality::High, Some(true)) => "high (MP3 192 kbps)".into(),
                (Quality::High, Some(false)) => "high (your plan: MP3 128 kbps)".into(),
                (Quality::High, None) => "high (MP3 192 kbps if your plan has it)".into(),
                (Quality::Standard, _) => "standard (MP3 128 kbps)".into(),
            },
            Setting::Volume => format!("{}%", self.config.player.volume),
            Setting::Autoplay => on(self.config.ui.autoplay_last_station),
            Setting::Notify => on(self.config.ui.notifications),
            Setting::GoodVibesOnly => self.config.player.good_vibes_only.label().into(),
            Setting::ThemeAutoMode => self.config.ui.theme_auto_mode.as_str().into(),
            Setting::ThemeSelect => format!(
                "{} · {} saved ~ Browse for more…",
                theme::label(&self.config.ui.theme_select),
                self.user_themes.len()
            ),
            Setting::Icons => crate::icons::label(self.config.ui.icons).into(),
            Setting::BorderStyle => self.config.ui.border_style.label().into(),
            Setting::MenuWidth => format!("{} columns", self.config.ui.menu_width),
            Setting::AlbumArt => match self.config.ui.album_art {
                AlbumArt::Picture => format!("Picture ({})", self.art.protocol_name()),
                other => other.label().into(),
            },
            Setting::RetroMode => self.config.ui.retro_mode.label().into(),
            Setting::ArtTint => self.config.ui.art_tint.label().into(),
            Setting::Animations => on(self.config.ui.animations),
            Setting::FavoritesInMenu => on(self.config.ui.favorites_in_menu),
            Setting::PinnedInMenu => on(self.config.ui.pinned_in_menu),
            Setting::CheckUpdates => on(self.config.ui.check_updates),
            Setting::GenrePopup => on(self.config.ui.genre_edit_popup),
            Setting::Splash => on(self.config.ui.splash),
            Setting::EditFile => self
                .config_path
                .as_ref()
                .map(|p| p.display().to_string().replace(&std::env::var("HOME").unwrap_or_default(), "~"))
                .unwrap_or_default(),
            Setting::MenuEntry => match crate::desktop::applications_dir() {
                Some(dir) if crate::desktop::installed(&dir) => "Installed".into(),
                Some(_) => "Not installed".into(),
                None => "no home folder".into(),
            },
        }
    }

    pub(crate) fn open_prompt(&mut self, prompt: Prompt) {
        self.query = match prompt {
            Prompt::Email => TextInput::new(&self.config.pandora.username),
            Prompt::Password => TextInput::default(),
        };
        self.settings_cursor = Setting::at(if prompt == Prompt::Email { Setting::Email } else { Setting::Password });
        self.prompt = Some(prompt);
    }

    fn change_setting(&mut self) {
        match self.setting() {
            Setting::Email => self.open_prompt(Prompt::Email),
            Setting::Password => self.forget_saved_password(true),
            Setting::LogIn => self.log_in(),
            Setting::LogOut => self.log_out(),
            Setting::Quality => {
                let next = Quality::parse(&self.config.pandora.audio_quality).next();
                self.config.pandora.audio_quality = next.as_str().into();
                self.backend.send(Request::Quality(next));
                self.save(&[("playback", "audio_quality", next.as_str().into())]);
                self.status = format!("Audio quality {}: from the next songs Pandora sends", next.as_str());
            }
            Setting::Volume => {
                let v = self.config.player.volume;
                self.change_volume(if v >= 100 { -100 } else { 10 });
            }
            Setting::Autoplay => {
                let on = !self.config.ui.autoplay_last_station;
                self.config.ui.autoplay_last_station = on;
                self.save(&[("playback", "autoplay_last_station", on.into())]);
                self.status = format!("Autoplay last station on open: {}", if on { "on" } else { "off" });
            }
            Setting::Notify => {
                let on = !self.config.ui.notifications;
                self.config.ui.notifications = on;
                self.save(&[("playback", "song_change_notifications", on.into())]);
                self.status = format!("Song change notifications {}", if on { "on" } else { "off" });
            }
            // On → Muted → Off → On. Muted and Off ask first (the risk popup);
            // turning it on needs no question.
            Setting::GoodVibesOnly => match self.config.player.good_vibes_only.next() {
                GoodVibes::On => self.set_good_vibes(GoodVibes::On),
                next => self.confirm_good_vibes = Some(next),
            },
            Setting::ThemeAutoMode => self.toggle_theme_auto_mode(),
            Setting::ThemeSelect if self.theme_select_greyed() => {
                self.status = "Theme Auto Mode follows Omarchy: set it to off to pick a theme".into();
            }
            Setting::ThemeSelect => self.open_themes(),
            Setting::Icons => self.next_icons(),
            Setting::BorderStyle => {
                let next = self.config.ui.border_style.next();
                self.config.ui.border_style = next;
                self.save(&[("look", "border_style", next.as_str().into())]);
                self.status = format!("Border style: {}", next.label());
            }
            Setting::MenuWidth => {
                let next = config::MENU_WIDTH.next(self.config.ui.menu_width);
                self.config.ui.menu_width = next;
                self.save(&[("look", "menu_width", i64::from(next).into())]);
                self.status = format!("Menu width: {next} columns");
            }
            Setting::AlbumArt => self.next_album_art(),
            Setting::ArtTint => self.next_art_tint(),
            Setting::RetroMode => self.next_retro_mode(),
            Setting::CheckUpdates => {
                self.config.ui.check_updates = !self.config.ui.check_updates;
                self.save(&[("boxvibe", "look_for_new_versions_on_startup", self.config.ui.check_updates.into())]);
            }
            Setting::GenrePopup => {
                let on = !self.config.ui.genre_edit_popup;
                self.config.ui.genre_edit_popup = on;
                self.save(&[("boxvibe", "genre_edit_popup", on.into())]);
                self.status = format!("Genre station edit popup: {}", if on { "on" } else { "off" });
            }
            Setting::FavoritesInMenu => {
                let on = !self.config.ui.favorites_in_menu;
                self.config.ui.favorites_in_menu = on;
                self.save(&[("look", "favorites_in_menu", on.into())]);
                self.status = format!("Favorites in menu: {}", if on { "on" } else { "off (kept; f still stars)" });
            }
            Setting::PinnedInMenu => {
                let on = !self.config.ui.pinned_in_menu;
                self.config.ui.pinned_in_menu = on;
                self.save(&[("look", "pinned_in_menu", on.into())]);
                self.status = format!("Pinned in menu: {}", if on { "on" } else { "off (kept; Shift+I still pins)" });
            }
            Setting::Animations => {
                let on = !self.config.ui.animations;
                self.config.ui.animations = on;
                self.save(&[("look", "animations", on.into())]);
                self.status = format!("Animations: {}", if on { "on" } else { "off" });
            }
            Setting::Splash => {
                let on = !self.config.ui.splash;
                self.config.ui.splash = on;
                self.save(&[("look", "boot_screen", on.into())]);
                self.status = format!("Boot screen: {}", if on { "on" } else { "off" });
            }
            Setting::EditFile => {
                if self.demo {
                    self.status = "The demo doesn't touch config.toml".into();
                } else {
                    self.edit_config = true;
                }
            }
            Setting::MenuEntry => self.toggle_menu_entry(),
        }
    }

    /// Adds or removes boxvibe.desktop (a file on disk, not a config key).
    fn toggle_menu_entry(&mut self) {
        use crate::desktop;
        if self.demo {
            self.status = "The demo doesn't add an app menu entry".into();
            return;
        }
        let Some(dir) = desktop::applications_dir() else {
            self.status = "No home folder: can't add an app menu entry".into();
            return;
        };
        let home = std::env::var("HOME").unwrap_or_default();
        let short = |p: &std::path::Path| p.display().to_string().replacen(&home, "~", 1);
        self.status = if desktop::installed(&dir) {
            match desktop::remove(&dir) {
                Ok(p) => format!("Removed {} from the app menu", short(&p)),
                Err(e) => format!("Left {} alone: {e}", short(&desktop::path_in(&dir))),
            }
        } else {
            match desktop::current_exe().and_then(|exe| desktop::install(&dir, &exe)) {
                Ok(p) => format!("Added {}: boxvibe is in the app menu", short(&p)),
                Err(e) => format!("Couldn't add the app menu entry: {e}"),
            }
        };
    }

    /// After the editor closes: read config.toml again.
    pub fn reload_config(&mut self) {
        let Some(path) = self.config_path.clone() else { return };
        // A file broken in the editor keeps the settings in use (and the file as it is).
        match Config::read(&path) {
            Ok((config, warnings)) => {
                // The file has no password (it is in the keychain): keep the one in use,
                // unless the account changed or a new one was typed into the file.
                let mut config = config;
                let account_changed = config.pandora.username != self.config.pandora.username;
                let typed_in_file = !config.pandora.password.is_empty();
                if config.pandora.password.is_empty() && !account_changed {
                    config.pandora.password = self.config.pandora.password.clone();
                }
                let login_changed =
                    account_changed || (typed_in_file && config.pandora.password != self.config.pandora.password);
                if typed_in_file && login_changed {
                    self.password_from_file = true;
                    self.password_unsaved = true;
                }
                if account_changed {
                    self.keychain_state = KeychainState::Unknown;
                }
                self.config = config;
                self.art.forget_shown();
                self.apply_theme();
                self.audio.set_volume(self.config.player.volume);
                self.backend.send(Request::Quality(Quality::parse(&self.config.pandora.audio_quality)));
                self.status = ConfigWarning::summary(&warnings).unwrap_or_else(|| "config.toml read again".into());
                if login_changed && self.config.has_login() {
                    self.log_in();
                } else if login_changed && !self.config.pandora.username.trim().is_empty() {
                    let user = self.config.pandora.username.trim().to_owned();
                    self.backend.keychain(&self.keychain, &user, KeychainJob::Load);
                }
            }
            Err(warning) => {
                let why = ConfigWarning::summary(&[warning]).unwrap_or_default();
                self.status = format!("{why} · kept the settings in use");
            }
        }
    }

    fn prompt_key(&mut self, key: KeyEvent) {
        let Some(prompt) = self.prompt else { return };
        match key.code {
            KeyCode::Esc => {
                self.prompt = None;
                self.query.clear();
            }
            KeyCode::Enter | KeyCode::Tab => {
                let text = self.query.take();
                self.prompt = None;
                match prompt {
                    Prompt::Email => {
                        let email = text.trim().to_owned();
                        if email != self.config.pandora.username {
                            // A saved password belongs to the other account.
                            self.config.pandora.password.clear();
                            self.keychain_state = KeychainState::Unknown;
                        }
                        self.config.pandora.username = email.clone();
                        self.save(&[("account", "email_or_username", email.as_str().into())]);
                        // Then the password, like a login form.
                        if self.config.pandora.password.is_empty() {
                            self.open_prompt(Prompt::Password);
                        }
                    }
                    Prompt::Password if text.is_empty() => {}
                    Prompt::Password => {
                        // Kept in the keychain once Pandora takes it (see LoggedIn).
                        self.config.pandora.password = Secret::new(text);
                        self.password_unsaved = true;
                        self.settings_cursor = Setting::at(Setting::LogIn);
                        self.log_in();
                    }
                }
            }
            _ => {
                self.query.key(key, MAX_PROMPT);
            }
        }
    }

    // --- Keys ---

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.splash.is_some() {
            // Any key skips the boot screen (and is used up doing so).
            self.splash = None;
            return;
        }
        if self.system_open {
            return self.on_system_key(key);
        }
        if self.confirm_quit {
            // y quits; any other key is no, as in tubevibe.
            self.confirm_quit = false;
            self.quit = matches!(key.code, KeyCode::Char('y' | 'Y'));
            return;
        }
        if let Some(at) = self.confirm_unpin.take() {
            // y unpins; any other key is no.
            if matches!(key.code, KeyCode::Char('y' | 'Y')) && at < self.pins.len() {
                let pin = self.pins.remove(at);
                self.pinned_cursor = self.pinned_cursor.min(self.pins.len().saturating_sub(1));
                self.save_pins();
                self.status = format!("Unpinned “{}”", pin.title);
            } else {
                self.status = "Kept the pin".into();
            }
            return;
        }
        if let Some(station) = self.confirm_delete.take() {
            // y deletes; any other key is no, like the quit box.
            if matches!(key.code, KeyCode::Char('y' | 'Y')) {
                self.status = format!("Deleting “{}” from your Pandora account…", station.name);
                self.backend.send(Request::RemoveStation { station_id: station.id });
            } else {
                self.status = "Kept the station".into();
            }
            return;
        }
        if self.genre_notice.is_some() {
            return self.genre_notice_key(key);
        }
        if self.confirm_good_vibes.is_some() {
            return self.good_vibes_off_key(key);
        }
        if self.rename.is_some() {
            return self.rename_key(key);
        }
        if self.add_station.is_some() {
            return self.search_key(key);
        }
        if self.panel.is_some() {
            return self.panel_key(key);
        }
        if self.theme_picker.is_some() {
            return self.theme_picker_key(key);
        }
        if self.editor.is_some() {
            return self.editor_key(key);
        }
        if self.prompt.is_some() {
            return self.prompt_key(key);
        }
        if self.help {
            // The title says "Any Key Closes", so any key does.
            self.help = false;
            return;
        }
        if self.details.is_some() {
            self.details = None;
            return;
        }
        if self.song_menu.is_some() {
            return self.song_menu_key(key);
        }
        if let Some(cmd) = command_for(key) {
            self.run(cmd);
        }
    }

    fn run(&mut self, cmd: Cmd) {
        match cmd {
            // Only Shift+Q quits, so a stray lowercase q can't end playback.
            Cmd::Quit => self.confirm_quit = true,
            Cmd::Help => self.help = true,
            Cmd::Pause => self.audio.toggle_pause(),
            Cmd::Skip => self.skip(),
            Cmd::NowPlaying => self.show_now_playing(),
            Cmd::Stations => self.show_stations(),
            Cmd::Genres => self.show_genres(),
            Cmd::Add => self.ask_add_station(),
            Cmd::Settings => self.show_page(View::Settings),
            Cmd::About => self.show_page(View::About),
            Cmd::Favorites => self.show_favorites(),
            Cmd::ThumbUp => self.rate(true),
            Cmd::ThumbDown => self.rate(false),
            Cmd::LikePane => self.rate_in_pane(true),
            Cmd::BadPane => self.rate_in_pane(false),
            Cmd::Tired => self.tired(),
            Cmd::Star => self.toggle_favorite(),
            Cmd::Rename => self.ask_rename(),
            Cmd::Delete => self.ask_delete(),
            Cmd::Details => self.show_details(),
            Cmd::Edit => self.open_editor(),
            Cmd::NextFocus => self.next_focus(true),
            Cmd::PrevFocus => self.next_focus(false),
            Cmd::ToMenu => {
                self.focus = Focus::Menu;
                self.point_menu_at_view();
            }
            Cmd::Down => self.move_cursor(1),
            Cmd::Up => self.move_cursor(-1),
            Cmd::Enter => self.enter(),
            Cmd::VolumeUp => self.change_volume(5),
            Cmd::VolumeDown => self.change_volume(-5),
            Cmd::MenuNarrower => self.resize_menu(-1),
            Cmd::MenuWider => self.resize_menu(1),
            Cmd::ArtType => self.next_album_art(),
            Cmd::Retro => self.next_retro_mode(),
            Cmd::Tint => self.next_art_tint(),
            Cmd::Sleep => self.next_sleep(),
            Cmd::Pin => self.pin(),
            Cmd::SongMenu => self.open_song_menu(),
            Cmd::Mode => {
                let station =
                    self.station_to_change().map(|(s, fav)| StationRef { id: s.id, name: s.name, favorite: fav });
                self.open_modes(station)
            }
            Cmd::Shuffle => self.play_shuffle(),
        }
    }

    /// `N`: on to the next song, from anywhere.
    fn skip(&mut self) {
        if self.now.is_some() {
            self.status = "Skipped".into();
            self.end_song(End::Skipped);
        } else if matches!(self.ad, AdPhase::Playing(_)) {
            self.status = "Ads can't be skipped: your music is right after".into();
        } else {
            self.status = "Nothing is playing to skip".into();
        }
    }

    // --- Renaming and deleting stations (r and x) ---

    /// The station `r`, `x` and `v` mean: the highlighted one in Stations,
    /// or the station of the favorite the menu is on. `None` anywhere else.
    pub fn station_in_reach(&self) -> Option<(Station, bool)> {
        match (self.focus, self.view, self.menu_selected()) {
            (Focus::Content, View::Stations, _) if self.on_add_row => None,
            (Focus::Content, View::Stations, _) => {
                self.stations.get(self.stations_index()).cloned().map(|s| (s, false))
            }
            (Focus::Menu, _, MenuItem::Favorite(i)) => {
                let fav = self.config.ui.favorites.get(i)?;
                let station = self.stations.iter().find(|s| s.id == fav.id).cloned();
                station.map(|s| (s, true))
            }
            _ => None,
        }
    }

    /// The playing station, where Now Playing has the keys or the menu is on
    /// its NOW PLAYING entry: what `r` and `e` act on there (not `x`).
    pub fn playing_station_in_reach(&self) -> Option<Station> {
        let here = match self.focus {
            Focus::Content => self.view == View::Player && self.pane == Pane::NowPlaying,
            Focus::Menu => self.menu_selected() == MenuItem::NowPlaying,
        };
        let id = self.current_station.as_deref().filter(|_| here)?;
        self.stations.iter().find(|s| s.id == id).cloned()
    }

    /// The station `r` and `e` change: the one in reach, or else the playing one.
    pub fn station_to_change(&self) -> Option<(Station, bool)> {
        self.station_in_reach().or_else(|| self.playing_station_in_reach().map(|s| (s, false)))
    }

    /// The reason `r` / `x` / `v` have nothing to act on here.
    fn no_station_here(&mut self, what: &str) {
        self.status = match (self.focus, self.menu_selected()) {
            (Focus::Menu, MenuItem::Favorite(_)) if self.stations.is_empty() => "The stations are still loading".into(),
            (Focus::Menu, MenuItem::Favorite(_)) => {
                "That favorite isn't one of your stations any more (f removes it)".into()
            }
            _ => format!("{what} works on a station: open Stations (Shift+S) or select a favorite"),
        };
    }

    /// `r`: asks for a new name for the selected station (or the playing one, on Now Playing).
    fn ask_rename(&mut self) {
        let Some((station, favorite)) = self.station_to_change() else {
            return self.no_station_here("r renames");
        };
        if !station.allow_rename {
            self.blocked(&station, "renamed", format!("Pandora doesn't let you rename “{}”", station.name));
            return;
        }
        let text = TextInput::new(&station.name);
        self.rename = Some(Rename { station: StationRef { id: station.id, name: station.name, favorite }, text });
    }

    /// Pandora says no to a change on `station`: the message goes in the
    /// status bar, and for a genre station a popup explains it too (until its
    /// "Don't show this again" box is checked and confirmed).
    pub(super) fn blocked(&mut self, station: &crate::pandora::Station, what: &'static str, message: String) {
        self.status = message;
        if self.config.ui.genre_edit_popup && station.is_locked() && !station.is_shuffle {
            self.genre_notice = Some(GenreNotice { station: station.name.clone(), what, dont_show: false });
        }
    }

    /// Keys in the genre station popup: Space or x ticks the box, Enter says
    /// OK (and saves a ticked box as "off"), Esc closes without saving it.
    fn genre_notice_key(&mut self, key: KeyEvent) {
        let Some(notice) = self.genre_notice.as_mut() else { return };
        match key.code {
            KeyCode::Char(' ' | 'x' | 'X') => notice.dont_show = !notice.dont_show,
            KeyCode::Esc => self.genre_notice = None,
            KeyCode::Enter => {
                let off = notice.dont_show;
                self.genre_notice = None;
                if off {
                    self.config.ui.genre_edit_popup = false;
                    self.save(&[("boxvibe", "genre_edit_popup", false.into())]);
                    // The message stays in the bar; say where the box comes back.
                    self.status =
                        format!("{} · The popup is off (Settings, BOXVIBE: Genre station edit popup)", self.status);
                }
            }
            _ => {}
        }
    }

    /// Keys in the "Mute ads?" / "Turn off Good Vibes Only?" popup: Enter
    /// switches (and saves), Esc keeps what it was. Other keys do nothing.
    fn good_vibes_off_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if let Some(next) = self.confirm_good_vibes.take() {
                    self.set_good_vibes(next);
                }
            }
            KeyCode::Esc => {
                self.confirm_good_vibes = None;
                self.status = format!("Good Vibes Only stays {}", self.config.player.good_vibes_only.label());
            }
            _ => {}
        }
    }

    fn set_good_vibes(&mut self, mode: GoodVibes) {
        self.config.player.good_vibes_only = mode;
        self.save(&[("playback", "good_vibes_only", mode.to_value())]);
        // An ad playing now follows at once.
        if self.ad_break() && self.audio.is_active() {
            self.audio.set_muted(mode == GoodVibes::Muted);
        }
        self.status = match mode {
            GoodVibes::On => "Good Vibes Only on: Pandora's audio ads play",
            GoodVibes::Muted => "Good Vibes Only muted: ads play silently, the volume comes back after",
            GoodVibes::Off => "Good Vibes Only off: no ads",
        }
        .into();
    }

    fn rename_key(&mut self, key: KeyEvent) {
        let Some(rename) = self.rename.as_mut() else { return };
        match key.code {
            KeyCode::Esc => {
                self.rename = None;
                self.status = "Rename cancelled".into();
            }
            KeyCode::Enter => {
                let name = rename.text.text().trim().to_owned();
                let station = rename.station.clone();
                self.rename = None;
                if name.is_empty() || name == station.name {
                    self.status = "Rename cancelled: the name is empty or the same".into();
                    return;
                }
                self.status = format!("Renaming “{}” to “{name}”…", station.name);
                self.backend.send(Request::RenameStation { station_id: station.id, name });
            }
            _ => {
                rename.text.key(key, MAX_STATION_NAME);
            }
        }
    }

    /// `x`: asks before deleting the selected station from the account.
    fn ask_delete(&mut self) {
        if self.focus == Focus::Content && self.view == View::Pinned {
            if self.pins.is_empty() {
                self.status = "Nothing pinned yet: Shift+I pins the selected or playing song".into();
            } else {
                self.confirm_unpin = Some(self.pinned_cursor.min(self.pins.len() - 1));
            }
            return;
        }
        let Some((station, favorite)) = self.station_in_reach() else {
            return self.no_station_here("x deletes");
        };
        if !station.allow_delete {
            self.blocked(&station, "deleted", format!("Pandora doesn't let you delete “{}”", station.name));
            return;
        }
        self.confirm_delete = Some(StationRef { id: station.id, name: station.name, favorite });
    }

    /// Pandora removed a station: it goes from the list, Favorites and,
    /// if it was playing, the player.
    fn station_removed(&mut self, id: &str) {
        let name = self.stations.iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_default();
        let selected = self.menu_selected();
        self.stations.retain(|s| s.id != id);
        if self.config.ui.favorites.iter().any(|f| f.id == id) {
            self.config.ui.favorites.retain(|f| f.id != id);
            self.save_favorites();
        }
        if self.current_station.as_deref() == Some(id) {
            self.audio.stop();
            self.now = None;
            self.queue.clear();
            self.audio_cache.clear();
            self.buffering = None;
            self.playlist_pending = false;
            self.current_station = None;
            if self.view == View::Player {
                self.view = View::Stations;
            }
        }
        self.stations_cursor = self.stations_cursor.min(self.stations.len().saturating_sub(1));
        let items = self.menu_items();
        self.menu_cursor = items.iter().position(|m| *m == selected).unwrap_or(self.menu_cursor).min(items.len() - 1);
        self.status = format!("Deleted “{name}” from your Pandora account");
    }

    // --- Details (v) ---

    /// `v`: what Pandora tells about the selected song or station.
    fn show_details(&mut self) {
        let details = if let Some((station, _)) = self.station_in_reach() {
            Details {
                station_id: station.id.clone(),
                station_name: station.name.clone(),
                of: DetailsOf::Station(Box::new(station)),
                from_pandora: None,
                why: false,
            }
        } else if self.focus == Focus::Menu && matches!(self.menu_selected(), MenuItem::Favorite(_)) {
            return self.no_station_here("v shows details of");
        } else if !self.in_songs() {
            self.status =
                "v shows details of a station (Stations, a favorite) or a song (Now Playing, Up Next, History)".into();
            return;
        } else {
            let song = match self.pane {
                Pane::UpNext => self.queue.get(self.next_index()),
                Pane::History => self.history.get(self.history_index()),
                Pane::NowPlaying => self.now.as_ref(),
            };
            let Some(song) = song.cloned() else {
                self.status =
                    "No song to show details of: pick a station, or open Stations (Shift+S) for a station's".into();
                return;
            };
            return self.show_song_details(song, false);
        };
        if details.station_id.is_empty() {
            self.details = Some(Details { from_pandora: Some(Err("no station for this song".into())), ..details });
            return;
        }
        let current = self.current_station.as_deref() == Some(details.station_id.as_str());
        self.backend.send(Request::StationDetails { station_id: details.station_id.clone(), current });
        self.details = Some(details);
    }

    /// Tab: menu → Up Next → History → menu on the player (Shift+Tab the
    /// other way); menu ⇄ page elsewhere.
    fn next_focus(&mut self, forward: bool) {
        let order = [
            (Focus::Menu, Pane::UpNext),
            (Focus::Content, Pane::NowPlaying),
            (Focus::Content, Pane::UpNext),
            (Focus::Content, Pane::History),
        ];
        if self.view == View::Player {
            let at = match (self.focus, self.pane) {
                (Focus::Menu, _) => 0,
                (_, Pane::NowPlaying) => 1,
                (_, Pane::UpNext) => 2,
                (_, Pane::History) => 3,
            };
            let to = if forward { (at + 1) % 4 } else { (at + 3) % 4 };
            (self.focus, self.pane) = order[to];
            if to == 0 {
                self.pane = Pane::UpNext;
            }
        } else {
            self.focus = if self.focus == Focus::Menu { Focus::Content } else { Focus::Menu };
        }
        if self.focus == Focus::Menu {
            self.point_menu_at_view();
        }
    }

    fn enter(&mut self) {
        match (self.focus, self.view) {
            (Focus::Menu, _) => self.open_menu_item(),
            (Focus::Content, View::Settings) => self.change_setting(),
            (Focus::Content, View::About) => self.press_update(),
            (Focus::Content, View::Stations) if self.on_add_row => self.open_add_station(),
            (Focus::Content, View::Stations) => {
                if self.stations.is_empty() {
                    return;
                }
                self.tune(self.stations_index());
            }
            (Focus::Content, View::Genres) => self.add_genre_station(),
            (Focus::Content, View::Pinned) => self.open_pin(),
            (Focus::Content, View::Notifications) => {}
            (Focus::Content, View::Player) => {}
        }
    }

    fn move_cursor(&mut self, by: i32) {
        let clamp = |at: usize, len: usize| step(at, len, by);
        match (self.focus, self.view) {
            (Focus::Menu, _) => self.menu_cursor = clamp(self.menu_cursor, self.menu_items().len()),
            (Focus::Content, View::Settings) => self.settings_cursor = clamp(self.settings_cursor, Setting::ALL.len()),
            (Focus::Content, View::Player) => self.move_song(by),
            (Focus::Content, View::Stations) => {
                // The "Add station…" row is the list's first row.
                let at = if self.on_add_row { 0 } else { self.stations_index() + 1 };
                let to = clamp(at, self.stations.len() + 1);
                self.on_add_row = to == 0;
                if to > 0 {
                    self.stations_cursor = to - 1;
                }
            }
            (Focus::Content, View::Genres) => {
                self.genres_cursor = clamp(self.genres_index(), self.genre_rows().len().max(1))
            }
            (Focus::Content, View::About) => {}
            (Focus::Content, View::Notifications) => {
                self.notes_cursor = clamp(self.notes_cursor, self.notes.len().max(1))
            }
            (Focus::Content, View::Pinned) => self.pinned_cursor = clamp(self.pinned_cursor, self.pins.len().max(1)),
        }
    }

    /// `{` / `}`: the menu a step narrower or wider, saved as `[look] menu_width`.
    fn resize_menu(&mut self, dir: i8) {
        let next = config::MENU_WIDTH.step_by(self.config.ui.menu_width, dir);
        self.config.ui.menu_width = next;
        self.save(&[("look", "menu_width", i64::from(next).into())]);
        self.status = format!("Menu width: {next} columns ({{ and }} change it)");
    }

    /// Settings, and Shift+C: the picture or no cover.
    fn next_album_art(&mut self) {
        let next = self.config.ui.album_art.next();
        self.config.ui.album_art = next;
        self.art.forget_shown();
        self.want_art();
        self.save(&[("look", "album_art", next.as_str().into())]);
        self.status = format!("Album art: {}", next.label());
    }

    /// Settings, and Shift+R: the next retro level.
    fn next_retro_mode(&mut self) {
        let next = self.config.ui.retro_mode.next();
        self.config.ui.retro_mode = next;
        self.art.forget_shown();
        self.save(&[("look", "retro_mode", next.as_str().into())]);
        self.status = format!("Retro mode: {}", next.label());
    }

    /// Settings, and Shift+H: off, duotone or palette.
    fn next_art_tint(&mut self) {
        let next = self.config.ui.art_tint.next();
        self.config.ui.art_tint = next;
        self.art.forget_shown();
        self.save(&[("look", "tint_art_with_theme_colors", next.as_str().into())]);
        self.status = format!("Tint art with theme colors: {}", next.label());
    }

    /// Theme Auto Mode: omarchy ⇄ off.
    fn toggle_theme_auto_mode(&mut self) {
        use crate::config::ThemeAutoMode;
        let next = match self.config.ui.theme_auto_mode {
            ThemeAutoMode::Omarchy => ThemeAutoMode::Off,
            ThemeAutoMode::Off => ThemeAutoMode::Omarchy,
        };
        self.config.ui.theme_auto_mode = next;
        self.apply_theme();
        let select = self.config.ui.theme_select.clone();
        self.save(&[("look", "theme_auto_mode", next.as_str().into()), ("look", "theme_select", select.into())]);
        self.status = match next {
            ThemeAutoMode::Omarchy => "Theme Auto Mode: omarchy (follows your Omarchy theme)".into(),
            ThemeAutoMode::Off => format!("Theme Auto Mode: off · {}", theme::label(&self.config.ui.theme_select)),
        };
    }

    /// Theme Select can't be used while Auto Mode follows Omarchy.
    pub fn theme_select_greyed(&self) -> bool {
        self.config.ui.theme_auto_mode == crate::config::ThemeAutoMode::Omarchy
    }

    /// Theme Select = `name` (Auto Mode off), applied and saved.
    pub(crate) fn use_theme(&mut self, name: &str) {
        self.config.ui.set_theme(name);
        self.apply_theme();
        self.save(&[
            ("look", "theme_auto_mode", self.config.ui.theme_auto_mode.as_str().into()),
            ("look", "theme_select", name.into()),
        ]);
        self.status = format!("Theme: {}", theme::label(name));
    }

    fn next_icons(&mut self) {
        let next = self.config.ui.icons.next();
        self.config.ui.icons = next;
        self.save(&[("look", "icons", next.as_str().into())]);
        self.status = format!("Icons: {}", crate::icons::label(next));
    }

    /// `B`: off → 15 → 30 → 60 → 90 minutes → end of this song → off.
    fn next_sleep(&mut self) {
        let next = match self.sleep {
            None => Some(Sleep::Minutes(SLEEP_STEPS[0], Instant::now())),
            Some(Sleep::Minutes(m, _)) => match SLEEP_STEPS.iter().position(|s| *s == m) {
                Some(i) if i + 1 < SLEEP_STEPS.len() => Some(Sleep::Minutes(SLEEP_STEPS[i + 1], Instant::now())),
                _ => Some(Sleep::EndOfSong),
            },
            Some(Sleep::EndOfSong) => None,
        };
        self.sleep = next;
        self.sleep_pause_next = false;
        self.audio.set_fade(1.0);
        self.status = match next {
            Some(Sleep::Minutes(m, _)) => {
                format!("Sleep timer: {m} minutes, then a fade out and pause (B: more / off)")
            }
            Some(Sleep::EndOfSong) => "Sleep timer: at the end of this song (B: off)".into(),
            None => "Sleep timer off".into(),
        };
    }

    /// How long until the sleep timer pauses (`None`: no timer, or "end of
    /// song" with no song playing).
    pub fn sleep_left(&self) -> Option<Duration> {
        match self.sleep? {
            Sleep::Minutes(m, from) => Some(Duration::from_secs(m * 60).saturating_sub(from.elapsed())),
            Sleep::EndOfSong => {
                let song = self.now.as_ref()?;
                Some(Duration::from_secs(u64::from(song.length)).saturating_sub(self.audio.position()))
            }
        }
    }

    /// Every frame: fade out over the last [`SLEEP_FADE`], then pause.
    fn sleep_tick(&mut self) {
        if self.sleep_pause_next && self.audio.is_active() {
            if !self.audio.is_paused() {
                self.audio.toggle_pause();
            }
            self.sleep_pause_next = false;
            self.audio.set_fade(1.0);
            self.status = "Sleep timer: paused at the end of the song. p plays on".into();
        }
        let Some(left) = self.sleep_left() else { return };
        self.audio.set_fade(left.as_secs_f32() / SLEEP_FADE.as_secs_f32());
        if left.is_zero() && matches!(self.sleep, Some(Sleep::Minutes(..))) {
            self.sleep = None;
            if !self.audio.is_paused() {
                self.audio.toggle_pause();
            }
            self.audio.set_fade(1.0);
            self.status = "Sleep timer: paused. p plays on".into();
        }
    }

    fn change_volume(&mut self, by: i16) {
        let volume = (i16::from(self.config.player.volume) + by).clamp(0, 100) as u8;
        self.config.player.volume = volume;
        self.audio.set_volume(volume);
        self.save(&[("playback", "volume", i64::from(volume).into())]);
        self.status = format!("Volume {volume}%");
    }

    /// `U` / `D`: thumbs on the playing song, whatever has the keys.
    fn rate(&mut self, positive: bool) {
        let Some(song) = self.now.as_mut() else {
            self.status = if matches!(self.ad, AdPhase::Playing(_)) {
                "Ads can't be rated: thumbs are for your songs".into()
            } else {
                "Nothing is playing to rate".into()
            };
            return;
        };
        if song.station_id.is_empty() || song.track_token.is_empty() {
            self.status = "Pandora can't rate this song (no track token)".into();
            return;
        }
        if song.rating == (if positive { Rating::Up } else { Rating::Down }) {
            let token = song.track_token.clone();
            return self.clear_thumb(&token);
        }
        song.rating = if positive { Rating::Up } else { Rating::Down };
        let request = Request::Rate { track_token: song.track_token.clone(), positive };
        self.backend.send(request);
        // Like Pandora: a thumbs down on the playing song skips it.
        if !positive {
            self.end_song(End::Skipped);
        }
    }

    /// `l` / `b`: thumbs on what the focused pane has selected: a row of Up
    /// Next or History, or the playing song when Now Playing has the keys.
    fn rate_in_pane(&mut self, positive: bool) {
        match (self.in_songs(), self.pane) {
            (true, Pane::NowPlaying) => self.rate(positive),
            (true, pane) => self.rate_row(pane, positive),
            _ => self.status = "l and b rate the selected song: open Up Next, History or Now Playing first".into(),
        }
    }

    /// The highlighted row of Up Next or History. Pandora's addFeedback takes
    /// any track token from the station's playlist, played or not.
    fn rate_row(&mut self, pane: Pane, positive: bool) {
        let (at, name) = match pane {
            Pane::History => (self.history_index(), "History"),
            _ => (self.next_index(), "Up Next"),
        };
        let song = if pane == Pane::History { self.history.get_mut(at) } else { self.queue.get_mut(at) };
        let Some(song) = song else {
            self.status = format!("{name} is empty: no song to rate");
            return;
        };
        if song.station_id.is_empty() || song.track_token.is_empty() {
            self.status = format!("Pandora can't rate “{}” (no track token)", song.title);
            return;
        }
        let token = song.track_token.clone();
        let wanted = if positive { Rating::Up } else { Rating::Down };
        // An Up Next dislike is held here until the song's turn: b again cancels it.
        if self.held_dislikes.contains(&token) {
            self.held_dislikes.remove(&token);
            song.rating = Rating::None;
            if !positive {
                self.status = format!("Dislike cancelled: {} stays in Up Next", song.title);
                return;
            }
        } else if song.rating == wanted {
            return self.clear_thumb(&token);
        }
        if !positive && pane == Pane::UpNext {
            song.rating = Rating::Down;
            self.status = format!("Disliked: {} is skipped at its turn (b again cancels)", song.title);
            self.held_dislikes.insert(token);
            return;
        }
        song.rating = wanted;
        self.backend.send(Request::Rate { track_token: token, positive });
    }

    /// The same thumb pressed again: Pandora takes it back (deleteFeedback).
    fn clear_thumb(&mut self, token: &str) {
        let Some(song) = self.song_mut(token) else { return };
        if song.feedback_id.is_empty() {
            self.status = format!(
                "Pandora didn't say which thumb to take back for “{}” yet (e on its station lists your thumbs)",
                song.title
            );
            return;
        }
        let positive = song.rating == Rating::Up;
        song.rating = Rating::None;
        let request =
            Request::ClearThumb { track_token: token.into(), feedback_id: song.feedback_id.clone(), positive };
        self.status = format!("Taking back the thumb on “{}”…", song.title);
        self.backend.send(request);
    }

    fn tired(&mut self) {
        let Some(song) = &self.now else { return };
        self.backend.send(Request::Tired { track_token: song.track_token.clone() });
        self.end_song(End::Skipped);
    }

    /// About's button can be pressed: a new version is out, this copy can be
    /// replaced, and no update is running (once done it is Restart Now).
    pub fn update_button_ready(&self) -> bool {
        self.update.as_ref().is_some_and(|u| matches!(u.replace, Replace::Ready(_)))
            && matches!(self.self_update, Progress::Idle | Progress::Failed(_) | Progress::Done(_))
    }

    /// The update is in: the button restarts boxvibe into it.
    pub fn restart_ready(&self) -> bool {
        matches!(self.self_update, Progress::Done(_))
    }

    fn press_update(&mut self) {
        match &self.self_update {
            Progress::Downloading(_) => {
                self.status = "⬇ Already updating…".into();
                return;
            }
            Progress::Done(_) => {
                // Restart Now: quit cleanly (sound stopped, settings saved),
                // then main starts the new binary in its place.
                self.restart = true;
                self.quit = true;
                return;
            }
            Progress::Idle | Progress::Failed(_) => {}
        }
        let Some(update) = self.update.clone() else {
            self.status = format!("No new version out · boxvibe {}", env!("CARGO_PKG_VERSION"));
            return;
        };
        match &update.replace {
            Replace::Ready(url) => {
                self.status = format!("⬇ Updating boxvibe to {}…", update.version);
                self.self_update = Progress::Downloading(0);
                self.backend.self_update(url.clone(), update.version.clone());
            }
            other => self.status = other.why_not().unwrap_or_default().into(),
        }
    }

    /// For screenshots: play a few demo songs so History and the clock have something.
    pub fn demo_warm_up(&mut self, skips: usize, at: Duration) {
        self.settle(Duration::from_millis(150));
        for _ in 0..skips {
            self.advance();
            self.settle(Duration::from_millis(80));
        }
        let up = crate::icons::icons(self.config.ui.icons).up;
        if let Some(h) = self.history.get_mut(0) {
            h.rating = Rating::Up;
            self.status = format!("{up} Liked: {}", h.title);
        }
        if let Some(h) = self.history.get_mut(2) {
            h.rating = Rating::Down;
        }
        self.audio.seek_silent(at);
        self.point_menu_at_view();
    }
}

/// One row up or down wraps around the ends (↑ on the first row is the
/// last); bigger jumps (g, G, Home, End) stop at them.
pub fn step(at: usize, len: usize, by: i32) -> usize {
    if len == 0 {
        return 0;
    }
    let at = at.min(len - 1) as i32;
    let len = len as i32;
    if by.abs() == 1 { (at + by).rem_euclid(len) as usize } else { (at + by).clamp(0, len - 1) as usize }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEvent;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn settings_start_with_account_then_boxvibe_playback_look() {
        let mut groups: Vec<&str> = Setting::ALL.iter().map(|(g, _)| *g).collect();
        groups.dedup();
        assert_eq!(groups, ["ACCOUNT", "BOXVIBE", "PLAYBACK", "LOOK"]);
        assert_eq!(Setting::ALL[0].1, Setting::Email, "the account is on top");
        assert!(Setting::at(Setting::Splash) > Setting::at(Setting::ThemeAutoMode), "the boot screen is under LOOK");
        assert_eq!(App::new(Config::default(), None, &[], true).setting(), Setting::Email, "the cursor starts on it");
        let emoji = crate::icons::icons(crate::config::IconSet::Emoji);
        assert_eq!(emoji.setting[Setting::at(Setting::CheckUpdates)], "🆕", "icons move with their settings");
        assert_eq!(emoji.setting[Setting::at(Setting::Email)], "👤");
        let mut app = App::new(Config::default(), None, &[], true);
        app.open_prompt(Prompt::Password);
        assert_eq!(app.setting(), Setting::Password, "the cursor follows a setting, not a row number");
    }

    #[test]
    fn enter_cycles_album_art_and_retro_mode() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.view = View::Settings;
        app.focus = Focus::Content;
        for (setting, first, second) in
            [(Setting::AlbumArt, "Off", "Picture"), (Setting::RetroMode, "64-bit", "32-bit")]
        {
            app.settings_cursor = Setting::ALL.iter().position(|(_, s)| *s == setting).unwrap();
            assert_eq!(Setting::ALL[app.settings_cursor].0, "LOOK");
            app.on_key(key(KeyCode::Enter));
            assert_eq!(app.setting_value(setting), first);
            app.on_key(key(KeyCode::Enter));
            assert!(app.setting_value(setting).starts_with(second), "{}", app.setting_value(setting));
        }
        assert_eq!(app.config.ui.album_art, AlbumArt::Picture);
        assert_eq!(app.config.ui.retro_mode, crate::config::RetroMode::Bit32);
        assert_eq!(app.status, "Retro mode: 32-bit");
        assert!(!app.status.contains('🎮'), "no controller icon: {}", app.status);
    }

    #[test]
    fn enter_on_a_favorite_plays_it_and_shows_now_playing() {
        for playing_first in [false, true] {
            let mut config = Config::default();
            config.ui.autoplay_last_station = false;
            let mut app = App::new(config, None, &[], true);
            app.settle(Duration::from_millis(150));
            if playing_first {
                app.tune(0);
            }
            let fav = app.stations[1].clone();
            app.config.ui.favorites.push(crate::config::Favorite { id: fav.id.clone(), name: fav.name.clone() });
            app.view = View::Settings;
            app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Favorite(0)).unwrap();
            app.focus = Focus::Menu;
            app.on_key(key(KeyCode::Enter));
            assert_eq!(app.current_station.as_deref(), Some(fav.id.as_str()));
            assert_eq!(
                (app.view, app.focus, app.pane, app.menu_selected()),
                (View::Player, Focus::Content, Pane::NowPlaying, MenuItem::NowPlaying)
            );
        }
    }

    #[test]
    fn enter_on_a_station_plays_it_and_shows_now_playing() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.settle(Duration::from_millis(150));
        app.on_key(shift('S'));
        app.on_key(key(KeyCode::Char('j')));
        let want = app.stations[app.stations_index()].id.clone();
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.current_station.as_deref(), Some(want.as_str()));
        assert_eq!(
            (app.view, app.focus, app.pane, app.menu_selected()),
            (View::Player, Focus::Content, Pane::NowPlaying, MenuItem::NowPlaying)
        );
    }

    #[test]
    fn startup_stays_on_an_idle_now_playing() {
        let mut config = Config::default();
        config.ui.autoplay_last_station = false;
        let mut app = App::new(config, None, &[], true);
        app.settle(Duration::from_millis(150));
        assert_eq!(app.view, View::Player);
        assert!(app.now.is_none() && app.current_station.is_none());
        app.on_key(shift('S'));
        assert_eq!(app.view, View::Stations, "Shift+S opens it by hand");
    }

    #[test]
    fn only_shift_q_quits() {
        let mut app = demo();
        app.on_key(key(KeyCode::Char('q')));
        assert!(!app.quit && !app.confirm_quit);
        app.on_key(KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT));
        assert!(app.confirm_quit && !app.quit, "Q asks first");
        app.on_key(key(KeyCode::Char('y')));
        assert!(app.quit);
    }

    #[test]
    fn quit_box_any_other_key_is_no() {
        for no in [KeyCode::Char('n'), KeyCode::Enter, KeyCode::Esc, KeyCode::Char('Q')] {
            let mut app = demo();
            app.on_key(KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT));
            app.on_key(key(no));
            assert!(!app.quit && !app.confirm_quit, "{no:?}");
        }
    }

    #[test]
    fn ctrl_c_quits_at_once_even_in_the_quit_box() {
        let mut app = demo();
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.quit);
        let mut app = demo();
        app.on_key(KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT));
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.quit);
    }

    fn demo() -> App {
        // Ads have their own tests: here a skip goes straight to the next song.
        let mut config = Config::default();
        config.player.good_vibes_only = GoodVibes::Off;
        let mut app = App::new(config, None, &[], true);
        app.demo_warm_up(2, Duration::ZERO);
        app
    }

    /// A demo app that asks for ads (every third answer is a pod of two).
    fn ad_demo() -> App {
        let mut app = App::new(Config::default(), None, &[], true);
        app.settle(Duration::from_millis(300));
        assert!(app.now.is_some() && app.config.player.good_vibes_only == GoodVibes::On);
        app
    }

    #[test]
    fn b_steps_the_sleep_timer_and_it_fades_out_then_pauses() {
        let mut app = ad_demo();
        let mut seen = Vec::new();
        for _ in 0..6 {
            app.on_key(shift('B'));
            seen.push(match app.sleep {
                Some(Sleep::Minutes(m, _)) => m.to_string(),
                Some(Sleep::EndOfSong) => "end".into(),
                None => "off".into(),
            });
        }
        assert_eq!(seen, ["15", "30", "60", "90", "end", "off"]);
        assert!(app.status.contains("off"), "{}", app.status);
        // 5 seconds left: half way down the fade.
        let full = app.audio.level();
        app.sleep = Some(Sleep::Minutes(15, Instant::now() - Duration::from_secs(15 * 60 - 5)));
        app.tick();
        let half = app.audio.level() / full;
        assert!((0.4..=0.55).contains(&half), "{half}");
        assert!(!app.audio.is_paused());
        // Time's up: paused, the volume as it was, the timer gone.
        app.sleep = Some(Sleep::Minutes(15, Instant::now() - Duration::from_secs(15 * 60 + 1)));
        app.tick();
        assert!(app.audio.is_paused() && app.sleep.is_none());
        assert_eq!(app.audio.level(), full);
        app.on_key(key(KeyCode::Char('p')));
        assert!(!app.audio.is_paused(), "p plays on");
        assert_eq!(clock_text(Duration::from_secs(14 * 60 + 59)), "14:59");
        assert_eq!(clock_text(Duration::from_secs(90 * 60)), "1:30:00");
    }

    #[test]
    fn sleep_at_the_end_of_the_song_pauses_the_next_one_at_once() {
        let mut app = App::new(
            Config {
                player: crate::config::PlayerConfig { good_vibes_only: GoodVibes::Off, ..Default::default() },
                ..Default::default()
            },
            None,
            &[],
            true,
        );
        app.settle(Duration::from_millis(300));
        let first = app.now.as_ref().unwrap().track_token.clone();
        let length = app.now.as_ref().unwrap().length;
        app.sleep = Some(Sleep::EndOfSong);
        app.audio.seek_silent(Duration::from_secs(u64::from(length) - 3));
        app.tick();
        assert!(app.audio.level() < 0.5 * level_at(&app), "fading in the last seconds");
        app.audio.seek_silent(Duration::from_secs(u64::from(length) + 1));
        app.tick();
        app.settle(Duration::from_millis(300));
        app.tick();
        assert!(app.now.as_ref().is_some_and(|s| s.track_token != first), "the next song is there");
        assert!(app.audio.is_paused() && app.sleep.is_none(), "{}", app.status);
        assert_eq!(app.audio.level(), level_at(&app));
    }

    fn level_at(app: &App) -> f32 {
        crate::audio::level(app.config.player.volume, app.now.as_ref().map_or(0.0, |s| s.gain))
    }

    #[test]
    fn shift_arrows_change_the_volume_and_plain_arrows_do_not() {
        let mut app = demo();
        app.config.player.volume = 50;
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        assert_eq!(app.config.player.volume, 55);
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
        assert_eq!(app.config.player.volume, 45);
        for code in [KeyCode::Left, KeyCode::Right, KeyCode::Char('['), KeyCode::Char(']')] {
            app.on_key(key(code));
        }
        assert_eq!(app.config.player.volume, 45, "only Shift+arrows touch the volume");
    }

    #[test]
    fn up_and_down_wrap_around_the_ends() {
        assert_eq!((step(0, 5, -1), step(4, 5, 1), step(2, 5, 1)), (4, 0, 3));
        assert_eq!((step(3, 5, -1000), step(1, 5, 1000), step(0, 0, -1)), (0, 4, 0), "g / G stop at the ends");
        let mut app = demo();
        app.focus = Focus::Menu;
        let last = app.menu_items().len() - 1;
        app.menu_cursor = 0;
        app.on_key(key(KeyCode::Up));
        assert_eq!(app.menu_cursor, last, "menu: up from the top is the bottom");
        app.on_key(key(KeyCode::Down));
        assert_eq!(app.menu_cursor, 0, "and down from the bottom is the top");
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.settings_cursor = 0;
        app.on_key(key(KeyCode::Char('k')));
        assert_eq!(app.settings_cursor, Setting::ALL.len() - 1, "settings wrap too");
        app.view = View::Stations;
        app.stations_cursor = app.stations.len() - 1;
        app.on_key(key(KeyCode::Down));
        assert!(app.on_add_row, "stations: the last one is followed by the Add station row");
        app.on_key(key(KeyCode::Down));
        assert_eq!((app.on_add_row, app.stations_index()), (false, 0), "and then the list wraps");
        app.on_key(key(KeyCode::Up));
        assert!(app.on_add_row, "up from the first station is the Add station row");
        app.on_key(key(KeyCode::Up));
        assert_eq!(app.stations_index(), app.stations.len() - 1, "and up from that wraps to the last");
    }

    #[test]
    fn menu_has_stations_then_settings_about_quit() {
        let app = demo();
        let items = app.menu_items();
        assert_eq!(items[0], MenuItem::NowPlaying, "a station plays: NOW PLAYING on top");
        assert_eq!((items[1], items[2], items[3]), (MenuItem::Stations, MenuItem::Genres, MenuItem::Pinned));
        assert_eq!(items.len(), 8, "no favorites yet: no FAVORITES entries");
        assert_eq!(
            &items[items.len() - 4..],
            [MenuItem::Settings, MenuItem::Notifications, MenuItem::About, MenuItem::Quit]
        );
    }

    #[test]
    fn enter_on_settings_and_about_opens_them() {
        let mut app = demo();
        app.focus = Focus::Menu;
        app.menu_cursor = 4;
        app.on_key(key(KeyCode::Enter));
        assert_eq!((app.view, app.focus), (View::Settings, Focus::Content));
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.menu_selected(), MenuItem::Settings);
        app.on_key(key(KeyCode::Char('j')));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.view, View::Notifications);
        app.on_key(key(KeyCode::Esc));
        app.on_key(key(KeyCode::Char('j')));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.view, View::About);
        app.on_key(shift('S'));
        assert_eq!((app.view, app.focus), (View::Stations, Focus::Content));
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.menu_selected(), MenuItem::Stations);
    }

    #[test]
    fn now_playing_from_the_menu_and_with_shift_p() {
        let mut app = demo();
        app.view = View::About;
        app.focus = Focus::Content;
        app.on_key(shift('P'));
        assert_eq!((app.view, app.focus, app.menu_selected()), (View::Player, Focus::Content, MenuItem::NowPlaying));
        assert_eq!(app.pane, Pane::NowPlaying);
        let title = app.now.as_ref().map(|s| s.title.clone());
        app.view = View::Settings;
        app.focus = Focus::Menu;
        app.menu_cursor = 0;
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.view, View::Player);
        assert_eq!(app.now.as_ref().map(|s| s.title.clone()), title, "only shows it: the song plays on");
        // Shift+N skips.
        app.on_key(shift('N'));
        assert_ne!(app.now.as_ref().map(|s| s.title.clone()), title);
    }

    #[test]
    fn no_now_playing_entry_before_a_station_plays() {
        let mut config = Config::default();
        config.ui.autoplay_last_station = false;
        let mut app = App::new(config, None, &[], true);
        app.settle(Duration::from_millis(150));
        assert_eq!(app.view, View::Player, "startup never opens Stations by itself");
        assert!(!app.menu_items().contains(&MenuItem::NowPlaying));
        app.on_key(shift('P'));
        assert!(app.status.starts_with("Nothing is playing"), "{}", app.status);
        assert_eq!(app.view, View::Player, "Shift+P with nothing playing stays put");
        app.on_key(shift('S'));
        assert_eq!((app.view, app.focus), (View::Stations, Focus::Content), "Shift+S opens the list");
        // Enter on the second station: it plays and NOW PLAYING appears, selected.
        app.on_key(key(KeyCode::Char('j')));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.current_station.as_deref(), Some(app.stations[1].id.as_str()));
        assert_eq!(
            (app.view, app.focus, app.pane, app.menu_selected()),
            (View::Player, Focus::Content, Pane::NowPlaying, MenuItem::NowPlaying)
        );
    }

    #[test]
    fn f_stars_stations_into_a_favorites_section_that_plays_them() {
        let path = crate::config::temp_dir("favorites").join("config.toml");
        let mut config = Config::default();
        config.ui.autoplay_last_station = false;
        let mut app = App::new(config, None, &[], true);
        app.settle(Duration::from_millis(150));
        // The demo never saves: point it at a file to see what would be.
        app.demo = false;
        app.config_path = Some(path.clone());
        app.on_key(shift('S'));
        app.on_key(key(KeyCode::Char('j')));
        app.on_key(key(KeyCode::Char('j')));
        app.on_key(key(KeyCode::Char('f')));
        let third = app.stations[2].clone();
        assert!(app.is_favorite(&third.id));
        assert_eq!(
            app.menu_items(),
            [
                MenuItem::Stations,
                MenuItem::Genres,
                MenuItem::Pinned,
                MenuItem::Favorite(0),
                MenuItem::Settings,
                MenuItem::Notifications,
                MenuItem::About,
                MenuItem::Quit
            ]
        );
        let saved = Config::load(&path).0.ui.favorites;
        assert_eq!(
            (saved.len(), saved[0].id.as_str(), saved[0].name.as_str()),
            (1, third.id.as_str(), third.name.as_str())
        );
        // Enter on the favorite in the menu plays it.
        app.on_key(key(KeyCode::Esc));
        app.on_key(key(KeyCode::Char('j')));
        assert_eq!(app.menu_selected(), MenuItem::Genres);
        app.on_key(key(KeyCode::Char('j')));
        assert_eq!(app.menu_selected(), MenuItem::Pinned);
        app.on_key(key(KeyCode::Char('j')));
        assert_eq!(app.menu_selected(), MenuItem::Favorite(0));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.current_station.as_deref(), Some(third.id.as_str()));
        assert_eq!((app.view, app.menu_selected()), (View::Player, MenuItem::NowPlaying), "on to Now Playing");
        app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Favorite(0)).unwrap();
        // f on it in the menu unstars it: the section goes.
        app.on_key(key(KeyCode::Char('f')));
        assert!(
            app.config.ui.favorites.is_empty() && !app.menu_items().iter().any(|m| matches!(m, MenuItem::Favorite(_)))
        );
        assert!(Config::load(&path).0.ui.favorites.is_empty());
        assert!(app.menu_cursor < app.menu_items().len());
    }

    #[test]
    fn first_run_opens_settings_typing_the_email() {
        let dir = crate::config::temp_dir("first-run");
        let path = dir.join("config.toml");
        let mut config = Config::default();
        config.ui.check_updates = false;
        let mut app = App::new(config, Some(path.clone()), &[], false);
        assert_eq!((app.view, app.prompt), (View::Settings, Some(Prompt::Email)));
        for c in "me@example.com".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.prompt, Some(Prompt::Password), "then the password");
        assert_eq!(app.config.pandora.username, "me@example.com");
        assert!(std::fs::read_to_string(&path).unwrap().contains("me@example.com"), "email saved");
        // Esc leaves the password empty: no login is tried.
        app.on_key(key(KeyCode::Esc));
        assert_eq!((app.prompt, &app.session), (None, &Session::LoggedOut));
    }

    #[test]
    fn tab_goes_menu_now_playing_up_next_history_and_back() {
        let mut app = demo();
        assert_eq!((app.focus, app.pane), (Focus::Content, Pane::NowPlaying), "autoplay lands like N");
        app.focus = Focus::Menu;
        app.on_key(key(KeyCode::Tab));
        assert_eq!((app.focus, app.pane), (Focus::Content, Pane::NowPlaying));
        app.on_key(key(KeyCode::Tab));
        assert_eq!((app.focus, app.pane), (Focus::Content, Pane::UpNext));
        app.on_key(key(KeyCode::Tab));
        assert_eq!((app.focus, app.pane), (Focus::Content, Pane::History));
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Menu);
        app.on_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!((app.focus, app.pane), (Focus::Content, Pane::History));
    }

    fn shift(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)
    }

    #[test]
    fn shift_u_d_rate_the_playing_song_from_any_pane() {
        let mut app = demo();
        for pane in [Pane::History, Pane::UpNext] {
            app.focus = Focus::Content;
            app.pane = pane;
            let before = app.history[0].rating;
            let playing = app.now.as_ref().unwrap().track_token.clone();
            app.on_key(shift('U'));
            assert_eq!(app.now.as_ref().unwrap().rating, Rating::Up, "{pane:?}: the playing song");
            assert_eq!(app.history[0].rating, before, "{pane:?}: the History row is untouched");
            assert_eq!(app.now.as_ref().unwrap().track_token, playing);
        }
        // From the menu too, and from another page.
        app.focus = Focus::Menu;
        app.view = View::Settings;
        let playing = app.now.as_ref().unwrap().track_token.clone();
        app.on_key(shift('D'));
        assert!(app.history.iter().any(|s| s.track_token == playing && s.rating == Rating::Down));
        assert_ne!(app.now.as_ref().unwrap().track_token, playing, "thumbs down skips");
    }

    #[test]
    fn the_old_thumb_keys_do_nothing() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::NowPlaying;
        let playing = app.now.as_ref().unwrap().track_token.clone();
        for c in ['+', '-', '=', 'u', 'd', 'n', 's', 't'] {
            let (view, theme) = (app.view, app.config.ui.theme().to_owned());
            app.on_key(key(KeyCode::Char(c)));
            assert_eq!(app.now.as_ref().unwrap().rating, Rating::None, "{c} rates nothing");
            assert_eq!(app.now.as_ref().unwrap().track_token, playing, "{c} skips nothing");
            assert_eq!((app.view, &app.config.ui.theme()), (view, &theme.as_str()), "{c} goes nowhere");
        }
    }

    #[test]
    fn notifications_keep_what_the_bottom_bar_said_newest_first() {
        let mut app = demo();
        for text in ["one", "one", "", "two"] {
            app.status = text.into();
            app.tick();
        }
        let said: Vec<&str> = app.notes.iter().map(|(_, s)| s.as_str()).collect();
        assert_eq!(&said[..2], ["two", "one"], "newest first, repeats once");
        for i in 0..NOTES_KEPT + 5 {
            app.status = format!("n{i}");
            app.note_status();
        }
        assert_eq!(app.notes.len(), NOTES_KEPT);
    }

    #[test]
    fn dot_opens_the_song_menu_and_start_station_fills_the_search() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::NowPlaying;
        app.on_key(key(KeyCode::Char('.')));
        assert!(app.song_menu.is_some());
        assert!(app.overlay_open());
        app.on_key(key(KeyCode::Esc));
        assert!(app.song_menu.is_none());
        let artist = app.now.as_ref().unwrap().artist.clone();
        app.on_key(key(KeyCode::Char('.')));
        app.on_key(key(KeyCode::Enter));
        let search = app.add_station.as_ref().expect("the add-station box opens");
        assert_eq!(search.text.text(), artist.trim());
        assert_eq!(search.prefer, Some(crate::pandora::HitKind::Artist));
    }

    fn menu_pick(app: &mut App, action: SongAction) {
        app.on_key(key(KeyCode::Char('.')));
        let at = SongAction::ALL.iter().position(|a| *a == action).unwrap();
        for _ in 0..at {
            app.on_key(key(KeyCode::Char('j')));
        }
        app.on_key(key(KeyCode::Enter));
    }

    #[test]
    fn the_artist_panel_shows_a_bio_and_starts_a_similar_station() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::NowPlaying;
        let artist = app.now.as_ref().unwrap().artist.clone();
        menu_pick(&mut app, SongAction::Artist);
        let panel = app.panel.as_ref().expect("the artist panel opens");
        assert!(panel.loading && app.overlay_open());
        app.settle(Duration::from_millis(150));
        let panel = app.panel.as_ref().unwrap();
        assert!(!panel.loading);
        assert!(panel.intro.contains(&artist), "{}", panel.intro);
        assert!(!panel.items.is_empty() && panel.items.iter().all(|i| !i.label.eq_ignore_ascii_case(&artist)));
        let similar = panel.items[0].label.clone();
        let stations = app.stations.len();
        app.on_key(key(KeyCode::Enter));
        assert!(app.panel.is_none());
        app.settle(Duration::from_millis(150));
        assert!(app.stations.len() >= stations);
        let playing = app.current_station.clone().unwrap();
        let name = &app.stations.iter().find(|s| s.id == playing).unwrap().name;
        assert!(name.contains(&similar), "{name} from {similar}");
    }

    #[test]
    fn a_failed_artist_page_says_so_in_the_panel() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::NowPlaying;
        menu_pick(&mut app, SongAction::Artist);
        app.on_reply(Reply::Failed { what: "artist", message: "Pandora is down".into() });
        let panel = app.panel.as_ref().unwrap();
        assert!(panel.intro.contains("Pandora is down") && panel.items.is_empty());
        app.on_key(key(KeyCode::Enter));
        assert!(app.panel.is_some(), "Enter on nothing does nothing");
        app.on_key(key(KeyCode::Esc));
        assert!(app.panel.is_none());
    }

    #[test]
    fn m_picks_a_station_mode_and_premium_ones_stay_locked() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::NowPlaying;
        let playing = app.current_station.clone().unwrap();
        app.on_key(key(KeyCode::Char('m')));
        app.settle(Duration::from_millis(150));
        let panel = app.panel.as_ref().expect("m opens the modes");
        assert_eq!(panel.items.len(), 6);
        assert!(panel.items[0].current && panel.cursor == 0);
        // Down to Deep Cuts (the fourth) and switch.
        for _ in 0..3 {
            app.on_key(key(KeyCode::Char('j')));
        }
        app.on_key(key(KeyCode::Enter));
        app.settle(Duration::from_millis(150));
        let panel = app.panel.as_ref().unwrap();
        assert!(panel.items[3].current && !panel.items[0].current);
        assert!(app.status.contains("Deep Cuts"), "{}", app.status);
        assert_eq!(app.current_station.as_deref(), Some(playing.as_str()), "the same station plays on");
        // Artist Only is Premium in the demo: locked, nothing sent.
        app.on_key(key(KeyCode::Char('G')));
        app.on_key(key(KeyCode::Enter));
        assert!(app.status.contains("isn't available"), "{}", app.status);
        app.on_key(key(KeyCode::Esc));
        // Reopened from the song menu, it remembers.
        menu_pick(&mut app, SongAction::Mode);
        app.settle(Duration::from_millis(150));
        assert_eq!(app.panel.as_ref().unwrap().cursor, 3);
    }

    #[test]
    fn no_modes_from_pandora_fails_gracefully() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::NowPlaying;
        app.on_key(key(KeyCode::Char('m')));
        app.on_reply(Reply::Failed { what: "mode", message: "Pandora doesn't offer station modes here".into() });
        let panel = app.panel.as_ref().unwrap();
        assert!(panel.intro.starts_with("No station modes") && !panel.loading);
        assert_eq!(crate::ui::key_caps(&app), [("Esc", "Close")]);
    }

    #[test]
    fn shift_x_plays_shuffle() {
        let mut app = demo();
        let shuffle = app.stations.iter().find(|s| s.is_shuffle).unwrap().id.clone();
        if app.current_station.as_deref() == Some(shuffle.as_str()) {
            let other = app.stations.iter().position(|s| !s.is_shuffle).unwrap();
            app.tune(other);
        }
        app.on_key(shift('X'));
        assert_eq!(app.current_station.as_deref(), Some(shuffle.as_str()));
        app.on_key(shift('X'));
        assert_eq!(app.status, "Already playing Shuffle");
        // An account without one asks Pandora, which makes it.
        app.stations.retain(|s| !s.is_shuffle);
        app.current_station = None;
        app.on_key(shift('X'));
        app.settle(Duration::from_millis(150));
        assert!(app.status.starts_with("⚠ No Shuffle") || app.status.starts_with("Tuning"), "{}", app.status);
        app.on_reply(Reply::Shuffle(Station {
            id: "sh".into(),
            name: "Shuffle".into(),
            is_shuffle: true,
            ..Default::default()
        }));
        assert_eq!((app.current_station.as_deref(), app.stations[0].is_shuffle), (Some("sh"), true));
    }

    #[test]
    fn tired_from_the_menu_drops_an_up_next_song() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::UpNext;
        let first = app.queue[0].track_token.clone();
        app.on_key(key(KeyCode::Char('.')));
        let at = SongAction::ALL.iter().position(|a| *a == SongAction::Tired).unwrap();
        for _ in 0..at {
            app.on_key(key(KeyCode::Char('j')));
        }
        app.on_key(key(KeyCode::Enter));
        assert!(app.queue.iter().all(|s| s.track_token != first));
        assert!(app.status.starts_with("Tired of"), "{}", app.status);
    }

    #[test]
    fn share_links_and_base64() {
        let mut s = Song { title: "A B".into(), artist: "C&D".into(), ..Song::default() };
        assert_eq!(share_link(&s), "https://www.pandora.com/search/A%20B%20C%26D/all");
        s.detail_url = "/artist/x".into();
        assert_eq!(share_link(&s), "https://www.pandora.com/artist/x");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn the_same_thumb_again_takes_it_back() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::NowPlaying;
        app.on_key(key(KeyCode::Char('l')));
        app.settle(Duration::from_millis(150));
        assert_eq!(app.now.as_ref().unwrap().rating, Rating::Up);
        assert!(!app.now.as_ref().unwrap().feedback_id.is_empty(), "Pandora's feedbackId is kept");
        app.on_key(key(KeyCode::Char('l')));
        assert_eq!(app.now.as_ref().unwrap().rating, Rating::None);
        app.settle(Duration::from_millis(150));
        assert!(app.status.starts_with("Thumb taken back"), "{}", app.status);
        // Without a feedbackId it says why and changes nothing.
        app.now.as_mut().unwrap().rating = Rating::Up;
        app.on_key(key(KeyCode::Char('l')));
        assert_eq!(app.now.as_ref().unwrap().rating, Rating::Up);
        assert!(app.status.contains("didn't say which thumb"), "{}", app.status);
    }

    #[test]
    fn an_up_next_dislike_waits_for_its_turn_and_b_again_cancels_it() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::UpNext;
        let first = app.queue[0].track_token.clone();
        app.on_key(key(KeyCode::Char('b')));
        app.on_key(key(KeyCode::Char('b')));
        assert_eq!(app.queue[0].rating, Rating::None, "b again cancels");
        assert!(app.held_dislikes.is_empty());
        app.on_key(key(KeyCode::Char('b')));
        let second = app.queue[1].track_token.clone();
        app.on_key(shift('N'));
        app.settle(Duration::from_millis(150));
        assert_eq!(app.now.as_ref().unwrap().track_token, second, "the held song is skipped at its turn");
        assert!(app.history.iter().any(|s| s.track_token == first && s.rating == Rating::Down));
        assert!(app.held_dislikes.is_empty());
    }

    #[test]
    fn l_and_b_rate_what_the_pane_has() {
        let mut app = demo();
        app.focus = Focus::Content;
        // History: the highlighted row, never the playing song.
        app.pane = Pane::History;
        app.on_key(key(KeyCode::Char('j')));
        let playing = app.now.as_ref().unwrap().track_token.clone();
        app.on_key(key(KeyCode::Char('b')));
        assert_eq!(app.history[1].rating, Rating::Down);
        assert_eq!(app.now.as_ref().unwrap().rating, Rating::None);
        assert_eq!(app.now.as_ref().unwrap().track_token, playing, "rating a row doesn't skip");
        // Up Next: l likes the highlighted song in place; b holds a dislike for its turn.
        app.pane = Pane::UpNext;
        let token = app.queue[0].track_token.clone();
        app.on_key(key(KeyCode::Char('l')));
        assert_eq!((app.queue[0].track_token.as_str(), app.queue[0].rating), (token.as_str(), Rating::Up));
        let n = app.queue.len();
        app.on_key(key(KeyCode::Char('b')));
        assert_eq!((app.queue.len(), app.queue[0].rating), (n, Rating::Down));
        assert!(app.status.contains("b again cancels"), "{}", app.status);
        // Now Playing focused: the current song.
        app.pane = Pane::NowPlaying;
        app.on_key(key(KeyCode::Char('l')));
        assert_eq!(app.now.as_ref().unwrap().rating, Rating::Up);
        // From the menu there is no song pane: it says so and rates nothing.
        app.focus = Focus::Menu;
        let (up, down) = (app.history[0].rating, app.history[1].rating);
        app.on_key(key(KeyCode::Char('b')));
        assert_eq!((app.history[0].rating, app.history[1].rating), (up, down));
        assert!(app.status.contains("open Up Next, History or Now Playing"), "{}", app.status);
        // An empty pane says so and does nothing.
        app.focus = Focus::Content;
        app.pane = Pane::History;
        app.history.clear();
        app.on_key(key(KeyCode::Char('l')));
        assert!(app.status.starts_with("History is empty"), "{}", app.status);
    }

    #[test]
    fn shift_n_skips_and_shift_p_goes_to_now_playing() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::History;
        app.view = View::Settings;
        let playing = app.now.as_ref().unwrap().track_token.clone();
        // Shift+P: where Shift+N used to go.
        app.on_key(shift('P'));
        assert_eq!((app.view, app.focus, app.pane), (View::Player, Focus::Content, Pane::NowPlaying));
        assert_eq!(app.menu_selected(), MenuItem::NowPlaying);
        assert_eq!(app.now.as_ref().unwrap().track_token, playing, "going there skips nothing");
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.pane, Pane::UpNext);
        // Shift+N skips, from any pane and any page.
        app.view = View::About;
        app.focus = Focus::Menu;
        app.on_key(shift('N'));
        assert_ne!(app.now.as_ref().unwrap().track_token, playing);
        assert_eq!((app.view, app.focus), (View::About, Focus::Menu), "skipping doesn't move you");
        // Lowercase n no longer skips.
        let now = app.now.as_ref().unwrap().track_token.clone();
        app.on_key(key(KeyCode::Char('n')));
        assert_eq!(app.now.as_ref().unwrap().track_token, now);
    }

    #[test]
    fn the_notifications_toggle_is_saved() {
        let path = crate::config::temp_dir("notifications").join("config.toml");
        std::fs::write(&path, "[ui]\nnotify = true\n").unwrap();
        let mut config = Config::default();
        config.ui.check_updates = false;
        let mut app = App::new(config, Some(path.clone()), &[], false);
        app.prompt = None;
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.settings_cursor = Setting::ALL.iter().position(|(_, s)| *s == Setting::Notify).unwrap();
        assert_eq!(app.setting_value(Setting::Notify), "on");
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.setting_value(Setting::Notify), "off");
        let (saved, _) = Config::load(&path);
        assert!(!saved.ui.notifications);
        assert!(!std::fs::read_to_string(&path).unwrap().contains("notify ="), "the 0.1 name is gone");
    }

    /// A demo app on the Stations page with the cursor on `pick`'s first match.
    fn on_station(config: Config, path: Option<std::path::PathBuf>, pick: fn(&Station) -> bool) -> App {
        let mut app = App::new(config, path, &[], true);
        app.settle(Duration::from_millis(150));
        app.prompt = None;
        app.on_key(shift('S'));
        app.stations_cursor = app.stations.iter().position(pick).expect("a station like that");
        app
    }

    fn genre(s: &Station) -> bool {
        s.is_locked() && !s.is_shuffle
    }

    #[test]
    fn a_genre_station_that_cant_be_changed_gets_a_popup_and_the_bar_message() {
        {
            let (key_pressed, what, bar) = ('r', "renamed", "doesn't let you rename");
            let mut app = on_station(Config::default(), None, genre);
            assert!(app.config.ui.genre_edit_popup, "on by default");
            app.on_key(key(KeyCode::Char(key_pressed)));
            let notice = app.genre_notice.clone().expect("the popup");
            assert_eq!((notice.what, notice.dont_show), (what, false));
            assert!(app.overlay_open() && app.rename.is_none() && app.editor.is_none());
            assert!(app.status.contains(bar), "the bar says it too: {}", app.status);
            // Other keys do nothing; Esc closes it and the bar keeps the message.
            app.on_key(key(KeyCode::Char('q')));
            assert!(app.genre_notice.is_some() && !app.confirm_quit, "the popup eats other keys");
            app.on_key(key(KeyCode::Esc));
            assert!(app.genre_notice.is_none() && app.status.contains(bar), "{}", app.status);
        }
        // `e` opens its thumbs instead (no popup).
        let mut app = on_station(Config::default(), None, genre);
        app.on_key(key(KeyCode::Char('e')));
        assert!(app.genre_notice.is_none() && app.editor.as_ref().is_some_and(|e| e.thumbs_only));
        // Deleting a genre station Pandora does allow deleting asks as before.
        let mut app = on_station(Config::default(), None, genre);
        app.on_key(key(KeyCode::Char('x')));
        assert!(app.confirm_delete.is_some() && app.genre_notice.is_none());
    }

    #[test]
    fn only_genre_stations_get_the_popup() {
        // Shuffle: the bar only. Your own station: it just opens.
        let mut app = on_station(Config::default(), None, |s| s.is_shuffle);
        app.on_key(key(KeyCode::Char('r')));
        assert!(app.genre_notice.is_none() && app.status.contains("doesn't let you rename"), "{}", app.status);
        let mut app = on_station(Config::default(), None, |s| s.allow_rename);
        app.on_key(key(KeyCode::Char('r')));
        assert!(app.genre_notice.is_none() && app.rename.is_some());
        // A favorite that is a genre station too.
        let mut app = on_station(Config::default(), None, genre);
        let st = app.stations[app.stations_cursor].clone();
        app.config.ui.favorites.push(config::Favorite { id: st.id, name: st.name });
        app.focus = Focus::Menu;
        app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Favorite(0)).unwrap();
        app.on_key(key(KeyCode::Char('r')));
        assert!(app.genre_notice.is_some(), "{}", app.status);
    }

    #[test]
    fn with_the_popup_off_only_the_bar_says_it() {
        let mut config = Config::default();
        config.ui.genre_edit_popup = false;
        {
            let key_pressed = 'r';
            let mut app = on_station(config.clone(), None, genre);
            app.on_key(key(KeyCode::Char(key_pressed)));
            assert!(app.genre_notice.is_none() && !app.overlay_open());
            assert!(app.status.contains("doesn't let you"), "{}", app.status);
        }
    }

    #[test]
    fn the_checkbox_turns_the_popup_off_for_good_only_with_enter() {
        let path = crate::config::temp_dir("genre-popup").join("config.toml");
        let mut config = Config::default();
        config.ui.check_updates = false;
        let mut app = on_station(config, Some(path.clone()), genre);
        app.demo = false;
        // Ticked, then Esc: nothing saved, the popup comes back next time.
        app.on_key(key(KeyCode::Char('r')));
        app.on_key(key(KeyCode::Char(' ')));
        assert!(app.genre_notice.as_ref().unwrap().dont_show);
        app.on_key(key(KeyCode::Char('x')));
        assert!(!app.genre_notice.as_ref().unwrap().dont_show, "x toggles too");
        app.on_key(key(KeyCode::Char(' ')));
        app.on_key(key(KeyCode::Esc));
        assert!(app.config.ui.genre_edit_popup, "Esc keeps the setting");
        assert!(!std::fs::read_to_string(&path).unwrap_or_default().contains("genre_edit_popup"));
        app.on_key(key(KeyCode::Char('r')));
        assert!(app.genre_notice.as_ref().is_some_and(|n| !n.dont_show), "back, with the box empty again");
        // Enter without the tick: just closes.
        app.on_key(key(KeyCode::Enter));
        assert!(app.genre_notice.is_none() && app.config.ui.genre_edit_popup);
        // Ticked, then Enter: saved as off, and the bar still has the message.
        app.on_key(key(KeyCode::Char('r')));
        app.on_key(key(KeyCode::Char(' ')));
        app.on_key(key(KeyCode::Enter));
        assert!(app.genre_notice.is_none() && !app.config.ui.genre_edit_popup);
        assert!(app.status.contains("doesn't let you rename"), "{}", app.status);
        assert_eq!(app.setting_value(Setting::GenrePopup), "off");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[boxvibe]") && text.contains("genre_edit_popup = false"), "{text}");
        assert!(!Config::load(&path).0.ui.genre_edit_popup);
        // And now it stays quiet.
        app.on_key(key(KeyCode::Char('r')));
        assert!(app.genre_notice.is_none());
    }

    #[test]
    fn the_settings_row_toggles_the_popup_and_is_saved() {
        assert!(Config::default().ui.genre_edit_popup);
        let path = crate::config::temp_dir("genre-popup-row").join("config.toml");
        let mut config = Config::default();
        config.ui.check_updates = false;
        let mut app = App::new(config, Some(path.clone()), &[], false);
        app.prompt = None;
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.settings_cursor = Setting::at(Setting::GenrePopup);
        assert_eq!(Setting::GenrePopup.label(), "Genre station edit popup");
        assert_eq!(Setting::ALL[Setting::at(Setting::GenrePopup)].0, "BOXVIBE");
        assert_eq!(app.setting_value(Setting::GenrePopup), "on");
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.setting_value(Setting::GenrePopup), "off");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[boxvibe]") && text.contains("genre_edit_popup = false"), "{text}");
        assert!(!Config::load(&path).0.ui.genre_edit_popup);
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.setting_value(Setting::GenrePopup), "on");
        assert!(Config::load(&path).0.ui.genre_edit_popup);
        let about = Setting::GenrePopup.about();
        assert!(about.contains("genre_edit_popup") && about.contains("Saved"), "{about}");
    }

    #[test]
    fn the_genre_popup_setting_loads_with_a_default_and_tolerates_junk() {
        let (c, w) = Config::parse("").unwrap();
        assert!(c.ui.genre_edit_popup && w.is_empty(), "missing: on");
        let (c, w) = Config::parse("[boxvibe]\ncheck_updates = false\n").unwrap();
        assert!(c.ui.genre_edit_popup && !c.ui.check_updates && w.is_empty(), "an older file");
        let (c, _) = Config::parse("[boxvibe]\ngenre_edit_popup = false\n").unwrap();
        assert!(!c.ui.genre_edit_popup);
        let (c, w) = Config::parse("[boxvibe]\ngenre_edit_popup = \"maybe\"\ncheck_updates = false\n").unwrap();
        assert!(c.ui.genre_edit_popup && !c.ui.check_updates, "a bad value keeps the default, the rest loads");
        assert!(!w.is_empty());
    }

    #[test]
    fn good_vibes_only_steps_on_muted_off_asking_first_and_is_saved() {
        assert_eq!(Config::default().player.good_vibes_only, GoodVibes::On);
        let path = crate::config::temp_dir("good-guy").join("config.toml");
        let mut config = Config::default();
        config.ui.check_updates = false;
        let mut app = App::new(config, Some(path.clone()), &[], false);
        app.prompt = None;
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.settings_cursor = Setting::at(Setting::GoodVibesOnly);
        assert_eq!(Setting::GoodVibesOnly.label(), "Good Vibes Only");
        assert_eq!(app.setting_value(Setting::GoodVibesOnly), "on");
        // Muting asks first; nothing changes or is saved until Enter.
        app.on_key(key(KeyCode::Enter));
        assert!(app.confirm_good_vibes == Some(GoodVibes::Muted) && app.overlay_open());
        assert_eq!(app.setting_value(Setting::GoodVibesOnly), "on");
        assert!(!std::fs::read_to_string(&path).unwrap_or_default().contains("good_vibes_only"));
        app.on_key(key(KeyCode::Enter));
        assert!(app.confirm_good_vibes.is_none());
        assert_eq!(app.setting_value(Setting::GoodVibesOnly), "muted");
        assert!(std::fs::read_to_string(&path).unwrap().contains("good_vibes_only = \"muted\""));
        assert_eq!(Config::load(&path).0.player.good_vibes_only, GoodVibes::Muted);
        // Off asks too.
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.confirm_good_vibes, Some(GoodVibes::Off));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.setting_value(Setting::GoodVibesOnly), "off");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[playback]") && text.contains("good_vibes_only = false"), "{text}");
        assert_eq!(Config::load(&path).0.player.good_vibes_only, GoodVibes::Off);
        assert!(
            Setting::GoodVibesOnly.about().contains("good_vibes_only")
                && Setting::GoodVibesOnly.about().contains("Saved")
                && Setting::GoodVibesOnly.about().contains("Muted")
        );
        // Turning it back on needs no question and is saved at once.
        app.on_key(key(KeyCode::Enter));
        assert!(app.confirm_good_vibes.is_none(), "no popup for turning it on");
        assert_eq!(app.setting_value(Setting::GoodVibesOnly), "on");
        assert!(std::fs::read_to_string(&path).unwrap().contains("good_vibes_only = true"));
        assert_eq!(Config::load(&path).0.player.good_vibes_only, GoodVibes::On);
    }

    #[test]
    fn esc_in_the_good_vibes_popup_keeps_it_on_and_other_keys_do_nothing() {
        let path = crate::config::temp_dir("good-vibes-esc").join("config.toml");
        let mut config = Config::default();
        config.ui.check_updates = false;
        let mut app = App::new(config, Some(path.clone()), &[], false);
        app.prompt = None;
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.settings_cursor = Setting::at(Setting::GoodVibesOnly);
        app.on_key(key(KeyCode::Enter));
        assert!(app.confirm_good_vibes.is_some());
        for code in [KeyCode::Char('y'), KeyCode::Char(' '), KeyCode::Down, KeyCode::Char('q')] {
            app.on_key(key(code));
            assert!(app.confirm_good_vibes.is_some() && app.config.player.good_vibes_only == GoodVibes::On, "{code:?}");
        }
        app.on_key(key(KeyCode::Esc));
        assert!(app.confirm_good_vibes.is_none() && app.config.player.good_vibes_only == GoodVibes::On);
        assert_eq!(app.setting_value(Setting::GoodVibesOnly), "on");
        assert!(!std::fs::read_to_string(&path).unwrap_or_default().contains("good_vibes_only"), "nothing saved");
    }

    /// Skips until the demo's third answer, a pod of two ads.
    fn skip_into_an_ad(app: &mut App) {
        for _ in 0..3 {
            app.on_key(shift('N'));
            app.settle(Duration::from_millis(300));
        }
    }

    #[test]
    fn muted_ads_play_silently_and_the_volume_comes_back_even_when_changed_meanwhile() {
        let mut app = ad_demo();
        app.config.player.good_vibes_only = GoodVibes::Muted;
        let before = app.audio.level();
        assert!(before > 0.0);
        skip_into_an_ad(&mut app);
        assert!(app.ad_view().is_some(), "Muted still plays the ad");
        assert!(app.audio.is_muted() && app.audio.level() == 0.0);
        // A volume change during the ad is what comes back.
        app.change_volume(-20);
        assert!(app.audio.level() == 0.0 && app.config.player.volume == 50);
        app.audio.seek_silent(Duration::from_secs(7));
        app.tick();
        assert!(app.ad_view().is_some() && app.audio.is_muted(), "the second ad is muted too");
        app.audio.seek_silent(Duration::from_secs(5));
        app.tick();
        app.settle(Duration::from_millis(300));
        assert!(app.now.is_some() && !app.audio.is_muted());
        assert!(app.audio.level() > 0.0 && app.audio.level() < before);
    }

    #[test]
    fn an_ad_pod_plays_in_now_playing_without_touching_history_and_the_music_comes_back() {
        let mut app = ad_demo();
        let songs = app.history.len();
        app.on_key(shift('N'));
        app.settle(Duration::from_millis(300));
        assert!(app.now.is_some() && !app.ad_break(), "no ad is due the first time");
        app.on_key(shift('N'));
        app.settle(Duration::from_millis(300));
        app.on_key(shift('N'));
        assert!(app.now.is_none() && app.ad_break(), "asking: the next song waits");
        app.settle(Duration::from_millis(300));
        let view = app.ad_view().expect("an ad is playing");
        assert_eq!((view.number, view.count, view.length), (1, 2, 6));
        assert_eq!(app.history.len(), songs + 3, "the three skipped songs, no ad");
        // No skip, no thumbs; pause works.
        let history = app.history.len();
        app.on_key(shift('N'));
        assert!(app.status.contains("can't be skipped") && app.ad_view().unwrap().number == 1, "{}", app.status);
        app.on_key(shift('U'));
        assert!(app.status.contains("can't be rated"), "{}", app.status);
        app.on_key(shift('D'));
        assert!(app.ad_break() && app.history.len() == history);
        app.on_key(key(KeyCode::Char('p')));
        assert!(app.ad_view().unwrap().paused);
        app.on_key(key(KeyCode::Char('p')));
        assert!(!app.ad_view().unwrap().paused);
        // The first ad ends, then the second, then the music.
        app.audio.seek_silent(Duration::from_secs(7));
        app.tick();
        let view = app.ad_view().expect("the second ad");
        assert_eq!((view.number, view.length), (2, 4));
        app.audio.seek_silent(Duration::from_secs(5));
        app.tick();
        assert!(!app.ad_break() && app.ad_view().is_none());
        app.settle(Duration::from_millis(300));
        assert!(app.now.is_some(), "the next song plays after the ads");
        assert_eq!(app.history.len(), history, "ads are not added to History");
    }

    #[test]
    fn with_good_vibes_only_off_no_ad_is_asked_for_and_the_next_song_starts_at_once() {
        let mut config = Config::default();
        config.player.good_vibes_only = GoodVibes::Off;
        let mut app = App::new(config, None, &[], true);
        app.settle(Duration::from_millis(300));
        let first = app.now.as_ref().unwrap().track_token.clone();
        for _ in 0..4 {
            app.on_key(shift('N'));
            assert!(!app.ad_break(), "no waiting for an answer");
            app.settle(Duration::from_millis(300));
            assert!(app.now.is_some());
        }
        assert_ne!(app.now.as_ref().unwrap().track_token, first);
        app.settle(Duration::from_millis(300));
        assert!(!app.ad_break(), "no pod ever came");
    }

    #[test]
    fn an_ad_question_that_fails_or_never_answers_goes_on_to_the_next_song() {
        let mut app = ad_demo();
        app.end_song(End::Skipped);
        assert!(matches!(app.ad, AdPhase::Asking(_)) && app.now.is_none());
        app.on_reply(Reply::Failed { what: "ads", message: "boom".into() });
        assert!(!app.ad_break() && app.now.is_some(), "{}", app.status);
        assert!(app.status.starts_with("Ads:") && app.status.contains("playing on"), "{}", app.status);
        // No answer at all.
        app.end_song(End::Played);
        app.ad = AdPhase::Asking(Instant::now() - Duration::from_secs(9));
        app.tick();
        assert!(!app.ad_break() && app.now.is_some());
        // The audio of an ad can't be fetched.
        app.demo = false;
        app.ad_list_arrived(&app.current_station.clone().unwrap(), crate::demo::ads(3));
        // (not asking, so ignored)
        assert!(!app.ad_break());
        app.ad = AdPhase::Asking(Instant::now());
        app.ad_list_arrived(&app.current_station.clone().unwrap(), crate::demo::ads(3));
        assert!(app.ad_view().is_some_and(|v| v.loading));
        app.on_reply(Reply::AudioFailed { track_token: format!("ad:{}:0", app.ad_serial), message: "gone".into() });
        assert!(!app.ad_break() && app.status.contains("playing on"), "{}", app.status);
    }

    #[test]
    fn songs_arriving_during_an_ad_wait_and_changing_station_ends_the_ad() {
        let mut app = ad_demo();
        skip_into_an_ad(&mut app);
        assert!(app.ad_view().is_some());
        let station = app.current_station.clone().unwrap();
        app.on_reply(Reply::Playlist { station_id: station, songs: crate::demo::playlist("x", 9) });
        assert!(app.now.is_none() && app.ad_view().is_some(), "the music waits for the ad");
        app.tune(1);
        assert!(!app.ad_break());
    }

    #[test]
    fn border_style_and_menu_width_are_saved() {
        let path = crate::config::temp_dir("borders").join("config.toml");
        let mut config = Config::default();
        config.ui.check_updates = false;
        let mut app = App::new(config, Some(path.clone()), &[], false);
        app.prompt = None;
        app.view = View::Settings;
        app.focus = Focus::Content;
        let at = |s: Setting| Setting::ALL.iter().position(|(_, x)| *x == s).unwrap();
        app.settings_cursor = at(Setting::BorderStyle);
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.setting_value(Setting::BorderStyle), "Rounded");
        app.settings_cursor = at(Setting::MenuWidth);
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.setting_value(Setting::MenuWidth), "32 columns");
        let (saved, w) = Config::load(&path);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!((saved.ui.border_style, saved.ui.menu_width), (crate::config::BorderStyle::Rounded, 32));
    }

    #[test]
    fn braces_resize_the_menu_and_save_it() {
        let path = crate::config::temp_dir("resize").join("config.toml");
        let mut config = Config::default();
        config.ui.check_updates = false;
        let mut app = App::new(config, Some(path.clone()), &[], false);
        app.prompt = None;
        app.on_key(key(KeyCode::Char('}')));
        assert_eq!(app.config.ui.menu_width, 32);
        for _ in 0..20 {
            app.on_key(key(KeyCode::Char('{')));
        }
        assert_eq!(app.config.ui.menu_width, 20, "stops at the narrowest");
        assert_eq!(Config::load(&path).0.ui.menu_width, 20);
    }

    #[test]
    fn the_autoplay_toggle_is_saved() {
        let path = crate::config::temp_dir("autoplay").join("config.toml");
        std::fs::write(&path, "[player]\nautoplay = true\nvolume = 50\n").unwrap();
        let mut config = Config::default();
        config.ui.check_updates = false;
        let mut app = App::new(config, Some(path.clone()), &[], false);
        app.prompt = None;
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.settings_cursor = Setting::ALL.iter().position(|(_, s)| *s == Setting::Autoplay).unwrap();
        assert_eq!(app.setting_value(Setting::Autoplay), "on");
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.setting_value(Setting::Autoplay), "off");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("autoplay_last_station = false") && !text.contains("\nautoplay ="), "{text}");
        assert!(!Config::load(&path).0.ui.autoplay_last_station);
    }

    #[test]
    fn autoplay_at_startup_lands_like_shift_n() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.settle(Duration::from_millis(150));
        assert!(app.current_station.is_some());
        assert_eq!(
            (app.view, app.focus, app.pane, app.menu_selected()),
            (View::Player, Focus::Content, Pane::NowPlaying, MenuItem::NowPlaying)
        );
    }

    #[test]
    fn autoplay_off_waits_for_a_station() {
        let mut config = Config::default();
        config.ui.autoplay_last_station = false;
        let mut app = App::new(config, None, &[], true);
        app.settle(Duration::from_millis(150));
        assert!(!app.stations.is_empty() && app.current_station.is_none() && app.now.is_none());
    }

    #[test]
    fn update_now_says_why_it_cannot() {
        let mut app = demo();
        app.update = Some(Update { version: "9.9.9".into(), how: "cargo build".into(), replace: Replace::FromSource });
        assert!(!app.update_button_ready());
        app.view = View::About;
        app.focus = Focus::Content;
        app.on_key(key(KeyCode::Enter));
        assert!(app.status.starts_with("Built from source"), "{}", app.status);
        assert_eq!(app.self_update, Progress::Idle);
    }

    #[test]
    fn restart_now_after_the_update_is_in() {
        let mut app = demo();
        let ready = Replace::Ready("https://example.com/boxvibe-x86_64-linux.tar.gz".into());
        app.update = Some(Update { version: "9.9.9".into(), how: String::new(), replace: ready });
        app.view = View::About;
        app.focus = Focus::Content;
        app.self_update = Progress::Downloading(40);
        app.on_key(key(KeyCode::Enter));
        assert!(!app.quit, "pressing again while it downloads does nothing");
        app.on_reply(Reply::SelfUpdated(Ok("9.9.9".into())));
        assert!(app.status.contains("9.9.9 installed · Restart Now"), "{}", app.status);
        assert!(app.update_button_ready() && app.restart_ready());
        app.on_key(key(KeyCode::Enter));
        assert!(app.restart && app.quit, "Enter on Restart Now quits so main starts the new one");
    }

    #[test]
    fn shift_keys_open_stations_settings_about_and_favorites() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.on_key(shift('S'));
        assert_eq!((app.view, app.focus), (View::Stations, Focus::Content), "S is Stations");
        app.on_key(shift('O'));
        assert_eq!((app.view, app.focus, app.menu_selected()), (View::Settings, Focus::Content, MenuItem::Settings));
        app.on_key(shift('A'));
        assert_eq!((app.view, app.focus, app.menu_selected()), (View::About, Focus::Content, MenuItem::About));
        app.config.ui.favorites.clear();
        app.on_key(shift('F'));
        assert!(app.status.contains("No favorites"), "{}", app.status);
        assert_eq!(app.focus, Focus::Content);
        app.config.ui.favorites.push(crate::config::Favorite { id: "1".into(), name: "One".into() });
        app.on_key(shift('F'));
        assert_eq!((app.focus, app.menu_selected()), (Focus::Menu, MenuItem::Favorite(0)));
        let theme = app.config.ui.theme().to_owned();
        app.on_key(key(KeyCode::Char('t')));
        assert_eq!(app.config.ui.theme(), theme, "t no longer switches the theme: only Settings does");
    }

    #[test]
    fn shift_o_opens_settings_and_shift_t_is_free() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.on_key(shift('O'));
        assert_eq!((app.view, app.focus, app.menu_selected()), (View::Settings, Focus::Content, MenuItem::Settings));
        app.on_key(key(KeyCode::Esc));
        app.on_key(shift('P'));
        let before = (app.view, app.focus);
        app.on_key(shift('T'));
        assert_eq!((app.view, app.focus), before, "Shift+T is unbound now");
        assert!(!KEYMAP.iter().any(|b| b.code == KeyCode::Char('T')));
    }

    #[test]
    fn shift_i_pins_the_playing_or_selected_song_and_x_unpins_after_asking() {
        let mut app = demo();
        let dir = crate::config::temp_dir("pins-app");
        app.pins_path = Some(dir.join("pinned.toml"));
        let playing = app.now.clone().unwrap();
        app.on_key(shift('I'));
        assert_eq!(app.pins.len(), 1);
        assert_eq!(
            (app.pins[0].title.as_str(), app.pins[0].artist.as_str()),
            (playing.title.as_str(), playing.artist.as_str())
        );
        assert_eq!(app.pins[0].link, playing.detail_url);
        assert_eq!(app.pins[0].pinned.len(), 10);
        app.on_key(shift('I'));
        assert_eq!(app.pins.len(), 1, "already pinned: {}", app.status);
        // The selected Up Next row, when Up Next has the keys.
        app.view = View::Player;
        app.focus = Focus::Content;
        app.pane = Pane::UpNext;
        let next = app.queue[app.next_index()].clone();
        app.on_key(shift('I'));
        assert_eq!(app.pins[0].title, next.title, "newest first");
        let on_disk = crate::pinned::load(app.pins_path.as_ref().unwrap());
        assert_eq!(on_disk, app.pins, "kept on disk, not in the config");
        // The menu's Pinned entry opens the page; x asks, n keeps, y unpins.
        app.focus = Focus::Menu;
        app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Pinned).unwrap();
        app.on_key(key(KeyCode::Enter));
        assert_eq!((app.view, app.focus), (View::Pinned, Focus::Content));
        app.on_key(key(KeyCode::Char('x')));
        assert_eq!(app.confirm_unpin, Some(0));
        app.on_key(key(KeyCode::Char('n')));
        assert_eq!(app.pins.len(), 2);
        app.on_key(key(KeyCode::Char('j')));
        app.on_key(key(KeyCode::Char('x')));
        app.on_key(key(KeyCode::Char('y')));
        assert_eq!(app.pins.len(), 1);
        assert_eq!(app.pins[0].title, next.title);
        assert_eq!(crate::pinned::load(app.pins_path.as_ref().unwrap()).len(), 1);
        app.on_key(key(KeyCode::Enter));
        assert!(app.status.contains("no page"), "the demo songs have no page: {}", app.status);
        app.pins[0].link = "https://www.pandora.com/artist/x".into();
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.status, "Opening https://www.pandora.com/artist/x", "the demo opens nothing");
    }

    #[test]
    fn the_settings_hide_favorites_and_pinned_from_the_menu() {
        let mut app = saving_app("menu-entries");
        app.config.ui.favorites.push(crate::config::Favorite { id: "x".into(), name: "X".into() });
        assert!(app.menu_items().contains(&MenuItem::Pinned) && app.menu_items().contains(&MenuItem::Favorite(0)));
        app.view = View::Settings;
        app.focus = Focus::Content;
        for (setting, key_name) in
            [(Setting::FavoritesInMenu, "favorites_in_menu"), (Setting::PinnedInMenu, "pinned_in_menu")]
        {
            app.settings_cursor = Setting::at(setting);
            assert_eq!(app.setting_value(setting), "on");
            app.on_key(key(KeyCode::Enter));
            assert_eq!(app.setting_value(setting), "off");
            assert!(saved(&app).contains(&format!("{key_name} = false")), "{}", saved(&app));
        }
        let items = app.menu_items();
        assert!(!items.contains(&MenuItem::Pinned) && !items.iter().any(|m| matches!(m, MenuItem::Favorite(_))));
        assert_eq!(app.config.ui.favorites.len(), 1, "hidden, not dropped");
        app.on_key(shift('F'));
        assert!(app.status.contains("hidden"), "{}", app.status);
    }

    fn saving_app(name: &str) -> App {
        let mut app = App::new(Config::default(), None, &[], true);
        app.demo = false;
        app.config_path = Some(crate::config::temp_dir(name).join("config.toml"));
        app
    }

    fn saved(app: &App) -> String {
        std::fs::read_to_string(app.config_path.as_ref().unwrap()).unwrap_or_default()
    }

    // --- The Pandora password lives in the keychain ---

    use crate::keychain::MemoryKeychain;

    const MAIL: &str = "me@example.com";
    const WAIT: Duration = Duration::from_millis(300);

    /// An app as `App::new` makes it for a real account (its backend answers like the
    /// demo, so no network), started from `config.toml` text and a fake keychain.
    fn account_app(name: &str, file: &str, keychain: &Arc<MemoryKeychain>) -> App {
        let path = crate::config::temp_dir(name).join("config.toml");
        std::fs::write(&path, file).unwrap();
        let (config, _) = Config::load(&path);
        let mut app = App::with_keychain(config, Some(path), &[], true, Arc::clone(keychain) as Arc<dyn Keychain>);
        app.settle(WAIT);
        app.demo = false;
        app.session = Session::LoggedOut;
        app.status.clear();
        app.start_account();
        app.settle(WAIT);
        app
    }

    fn enter_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Enter));
        app.settle(WAIT);
    }

    #[test]
    fn a_password_from_an_older_config_moves_to_the_keychain_and_leaves_the_file() {
        let k = MemoryKeychain::new();
        let app = account_app(
            "kc-legacy",
            "# mine\n[account]\nusername = \"me@example.com\" # who\npassword = \"hunter2\"\n[look]\nicons = \"emoji\"\n",
            &k,
        );
        assert_eq!(app.session, Session::LoggedIn, "it logged in with the password from the file");
        assert_eq!(k.saved(MAIL).as_deref(), Some("hunter2"));
        let text = saved(&app);
        assert!(!text.contains("hunter2") && !text.contains("password"), "{text}");
        assert!(text.contains("# mine") && text.contains("# who") && text.contains("emoji"), "{text}");
        assert!(
            text.contains("email_or_username = \"me@example.com\" # who") && !text.contains("\nusername ="),
            "{text}"
        );
        assert!(app.status.contains("moved from config.toml to the keychain"), "{}", app.status);
        assert_eq!(app.keychain_state, KeychainState::Saved);
        assert_eq!(app.setting_value(Setting::Password), "•••••••• in your keychain");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(app.config_path.as_ref().unwrap()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn a_wrong_old_password_is_not_moved_and_leaves_the_file_too() {
        let k = MemoryKeychain::new();
        let mut app =
            account_app("kc-legacy-wrong", "[pandora]\nusername = \"me@example.com\"\npassword = \"old\"\n", &k);
        // (the first login above was the demo's yes; now Pandora says no)
        k.set(MAIL, "old").unwrap();
        app.on_reply(Reply::LoginFailed { message: "Wrong password".into(), wrong_password: true });
        app.settle(WAIT);
        assert_eq!(k.saved(MAIL), None, "a wrong password is not kept");
        assert!(app.config.pandora.password.is_empty());
        assert!(!saved(&app).contains("password"), "{}", saved(&app));
        assert_eq!(app.setting(), Setting::LogIn, "the cursor waits on Log in, which asks again");
    }

    #[test]
    fn with_no_keychain_the_old_password_stays_in_the_file_and_in_memory() {
        let k = MemoryKeychain::missing();
        let file = "[account]\nemail_or_username = \"me@example.com\"\npassword = \"hunter2\"\n";
        let app = account_app("kc-legacy-missing", file, &k);
        assert_eq!(app.session, Session::LoggedIn);
        assert_eq!(app.status, NO_KEYCHAIN);
        assert!(saved(&app).contains("hunter2"), "not lost: the keychain never had it");
        assert_eq!(app.keychain_state, KeychainState::Unavailable);
    }

    #[test]
    fn the_usual_start_logs_in_with_the_keychain_and_the_file_never_has_a_password() {
        let k = MemoryKeychain::new();
        k.set(MAIL, "hunter2").unwrap();
        let file = "[account]\nemail_or_username = \"me@example.com\"\n";
        let app = account_app("kc-usual", file, &k);
        assert_eq!(app.session, Session::LoggedIn);
        assert_eq!(app.config.pandora.password.expose(), "hunter2");
        assert_eq!(saved(&app), file, "nothing was written");
        assert!(app.status.is_empty(), "{}", app.status);
        assert!(!format!("{:?}", app.config).contains("hunter2"));
    }

    #[test]
    fn no_saved_password_asks_for_it_then_keeps_it_in_the_keychain_after_pandora_accepts() {
        let k = MemoryKeychain::new();
        let mut app = account_app("kc-ask", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
        assert_eq!(app.prompt, Some(Prompt::Password), "asked, like a login form");
        assert_eq!(app.keychain_state, KeychainState::NotSaved);
        assert_eq!(app.session, Session::LoggedOut);
        enter_text(&mut app, "hunter2");
        assert_eq!(app.session, Session::LoggedIn);
        assert_eq!(k.saved(MAIL).as_deref(), Some("hunter2"));
        assert!(!saved(&app).contains("password") && !saved(&app).contains("hunter2"), "{}", saved(&app));
        assert!(saved(&app).contains(MAIL));
        assert_eq!(app.setting_value(Setting::Password), "•••••••• in your keychain");
    }

    #[test]
    fn a_password_pandora_never_accepted_is_never_saved() {
        let k = MemoryKeychain::new();
        let mut app = account_app("kc-never", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
        app.on_key(key(KeyCode::Char('x')));
        app.on_key(key(KeyCode::Enter));
        // Typed, but the answer is a no before the demo worker's yes arrives.
        assert!(app.password_unsaved);
        app.on_reply(Reply::LoginFailed { message: "no".into(), wrong_password: true });
        app.settle(WAIT);
        assert_eq!(k.saved(MAIL), None);
        assert!(!app.password_unsaved && app.config.pandora.password.is_empty());
    }

    #[test]
    fn no_keychain_means_a_session_password_a_clear_message_and_nothing_on_disk() {
        let k = MemoryKeychain::missing();
        let file = "[account]\nemail_or_username = \"me@example.com\"\n";
        let mut app = account_app("kc-missing", file, &k);
        assert_eq!(app.prompt, Some(Prompt::Password));
        assert_eq!(app.status, NO_KEYCHAIN, "told at start");
        assert_eq!(app.keychain_state, KeychainState::Unavailable);
        enter_text(&mut app, "hunter2");
        assert_eq!(app.session, Session::LoggedIn, "it works this session");
        assert_eq!(app.config.pandora.password.expose(), "hunter2", "kept in memory");
        assert_eq!(app.status, NO_KEYCHAIN);
        assert_eq!(saved(&app), file, "and never written");
        assert_eq!(app.setting_value(Setting::Password), "no keychain: asked each start");
        // Next start: asked again.
        let again = account_app("kc-missing", file, &k);
        assert!(again.config.pandora.password.is_empty() && again.prompt == Some(Prompt::Password));
    }

    #[test]
    fn forgetting_the_saved_password_empties_the_keychain_and_says_so() {
        let k = MemoryKeychain::new();
        k.set(MAIL, "hunter2").unwrap();
        let mut app = account_app("kc-forget", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.settings_cursor = Setting::at(Setting::Password);
        app.on_key(key(KeyCode::Enter));
        app.settle(WAIT);
        assert_eq!(k.saved(MAIL), None);
        assert_eq!(app.status, "Forgot the saved password: you'll be asked for it next start");
        assert_eq!(app.setting_value(Setting::Password), "not saved");
        assert!(app.prompt.is_none(), "Enter forgets, it doesn't ask");
        // The next start asks.
        let again = account_app("kc-forget", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
        assert_eq!(again.prompt, Some(Prompt::Password));
    }

    #[test]
    fn forgetting_also_removes_a_password_still_in_the_file() {
        let k = MemoryKeychain::missing();
        let mut app = account_app(
            "kc-forget-file",
            "[account]\nemail_or_username = \"me@example.com\"\npassword = \"hunter2\"\n",
            &k,
        );
        app.settings_cursor = Setting::at(Setting::Password);
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.on_key(key(KeyCode::Enter));
        assert!(!saved(&app).contains("hunter2"), "{}", saved(&app));
    }

    #[test]
    fn logging_out_removes_the_password_from_the_keychain() {
        let k = MemoryKeychain::new();
        k.set(MAIL, "hunter2").unwrap();
        let mut app = account_app("kc-logout", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
        assert_eq!(app.session, Session::LoggedIn);
        app.log_out();
        app.settle(WAIT);
        assert_eq!(k.saved(MAIL), None);
        assert!(app.config.pandora.password.is_empty());
        assert_eq!(app.status, "Logged out: the saved password was removed from your keychain");
        // Log in now asks.
        app.log_in();
        assert_eq!(app.prompt, Some(Prompt::Password));
    }

    #[test]
    fn another_email_does_not_borrow_the_old_password() {
        let k = MemoryKeychain::new();
        k.set(MAIL, "hunter2").unwrap();
        let mut app = account_app("kc-switch", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
        app.view = View::Settings;
        app.open_prompt(Prompt::Email);
        enter_text(&mut app, "other@example.com");
        assert!(app.config.pandora.password.is_empty());
        assert_eq!(app.prompt, Some(Prompt::Password), "asks for the other account's password");
    }

    #[test]
    fn editing_config_toml_keeps_the_password_in_use() {
        let k = MemoryKeychain::new();
        k.set(MAIL, "hunter2").unwrap();
        let mut app = account_app("kc-reload", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
        std::fs::write(
            app.config_path.as_ref().unwrap(),
            "[account]\nemail_or_username = \"me@example.com\"\n[look]\nicons = \"emoji\"\n",
        )
        .unwrap();
        app.reload_config();
        app.settle(WAIT);
        assert_eq!(app.config.pandora.password.expose(), "hunter2");
        assert_eq!(app.session, Session::LoggedIn, "no new login for an unrelated edit");
    }

    #[test]
    fn the_demo_never_touches_the_keychain() {
        let k = MemoryKeychain::new();
        let mut app = App::with_keychain(Config::default(), None, &[], true, Arc::clone(&k) as Arc<dyn Keychain>);
        app.settle(WAIT);
        app.settings_cursor = Setting::at(Setting::Password);
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.on_key(key(KeyCode::Enter));
        app.settle(WAIT);
        assert_eq!(app.setting_value(Setting::Password), "not used in the demo");
        assert_eq!(app.status, "The demo has no password");
        assert!(k.saved("demo").is_none());
    }

    #[test]
    fn every_setting_that_is_saved_says_where_and_the_others_do_not() {
        for (_, setting) in Setting::ALL {
            let about = setting.about();
            match setting.saved_as() {
                Some((table, key)) => {
                    assert!(
                        about.contains(&format!("Saved as [{table}] {key}.")),
                        "{setting:?} does not say where it is saved: {about}"
                    );
                    assert!(config::LAYOUT.iter().any(|(t, k, _)| *t == table && *k == key), "{table}.{key}");
                }
                None => assert!(!about.contains("Saved as"), "{setting:?} is not a saved choice: {about}"),
            }
        }
        // The claim matches the tables the Settings groups are named after.
        for (group, setting) in Setting::ALL {
            if let Some((table, _)) = setting.saved_as() {
                assert_eq!(table, group.to_lowercase(), "{setting:?} is in {group}");
            }
        }
    }

    #[test]
    fn what_a_setting_says_about_where_it_is_saved_is_true() {
        for (_, setting) in Setting::ALL {
            // The account and the volume have their own flows (below).
            let Some((table, name)) = setting.saved_as() else { continue };
            // Theme Select opens the picker; its saving is tested with the picker.
            if matches!(setting, Setting::Email | Setting::Password | Setting::Volume | Setting::ThemeSelect) {
                continue;
            }
            let mut app = saving_app(&format!("saved-{setting:?}"));
            app.view = View::Settings;
            app.focus = Focus::Content;
            app.settings_cursor = Setting::at(setting);
            app.on_key(key(KeyCode::Enter));
            if setting == Setting::GoodVibesOnly {
                // Muting asks first; Enter says yes.
                app.on_key(key(KeyCode::Enter));
            }
            let doc: toml_edit::DocumentMut = saved(&app).parse().unwrap();
            assert!(doc.get(table).and_then(|t| t.get(name)).is_some(), "{setting:?} → [{table}] {name}:\n{doc}");
        }
        // Email: when the prompt is accepted.
        let mut app = saving_app("saved-email");
        app.view = View::Settings;
        app.open_prompt(Prompt::Email);
        for c in "me@example.com".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Enter));
        assert!(saved(&app).contains("[account]") && saved(&app).contains("email_or_username = \"me@example.com\""));
        // Volume: Shift+→ anywhere.
        let mut app = saving_app("saved-volume");
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        assert!(saved(&app).contains("[playback]") && saved(&app).contains("volume ="), "{}", saved(&app));
    }

    #[test]
    fn the_boot_screen_is_two_seconds_long() {
        assert_eq!(SPLASH, Duration::from_secs(2));
    }

    #[test]
    fn the_defaults_are_nerd_icons_and_a_theme_that_exists() {
        let c = Config::default();
        assert_eq!(c.ui.icons, crate::config::IconSet::Nerd);
        assert!(theme::names().any(|n| n == c.ui.theme()));
    }

    #[test]
    fn media_keys_pause_play_and_skip_and_the_desktop_sees_the_song() {
        let mut app = demo();
        let (m, tx, _changed) = crate::mpris::Mpris::detached();
        app.mpris = Some(m);
        app.tick();
        let first = app.now.as_ref().unwrap().clone();
        let shown = app.mpris.as_ref().unwrap().shown();
        assert_eq!((shown.title.as_str(), shown.status()), (first.title.as_str(), "Playing"));
        tx.send(crate::mpris::Control::Play).unwrap();
        app.tick();
        assert!(!app.audio.is_paused(), "Play while playing does nothing");
        tx.send(crate::mpris::Control::PlayPause).unwrap();
        app.tick();
        assert!(app.audio.is_paused());
        assert_eq!(app.mpris.as_ref().unwrap().shown().status(), "Paused");
        tx.send(crate::mpris::Control::Pause).unwrap();
        app.tick();
        assert!(app.audio.is_paused(), "Pause while paused does nothing");
        tx.send(crate::mpris::Control::Play).unwrap();
        tx.send(crate::mpris::Control::Next).unwrap();
        app.tick();
        app.settle(Duration::from_millis(150));
        app.tick();
        assert_ne!(app.now.as_ref().unwrap().track_token, first.track_token);
        assert_eq!(app.mpris.as_ref().unwrap().shown().track, app.now.as_ref().unwrap().track_token);
    }

    fn wait_for_themes(app: &mut App) {
        for _ in 0..100 {
            app.tick();
            if app.theme_job.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the theme job never answered");
    }

    fn type_in(app: &mut App, text: &str) {
        for c in text.chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
    }

    fn hit_names(app: &App) -> Vec<String> {
        let p = app.theme_picker.as_ref().unwrap();
        p.hits.iter().map(|i| p.catalog[*i].name.clone()).collect()
    }

    #[test]
    fn theme_select_searches_like_fzf_downloads_installs_and_deletes_without_closing() {
        let mut app = demo();
        let dir = crate::config::temp_dir("theme-picker");
        app.themes_dir = Some(dir.clone());
        app.config.ui.set_theme("kanagawa");
        app.focus = Focus::Content;
        app.view = View::Settings;
        app.settings_cursor = Setting::at(Setting::ThemeSelect);
        assert!(app.setting_value(Setting::ThemeSelect).ends_with("0 saved ~ Browse for more…"));
        app.on_key(key(KeyCode::Enter));
        let p = app.theme_picker.as_ref().unwrap();
        assert!(p.loading && p.tab == ThemeTab::Search);
        wait_for_themes(&mut app);
        // The 5 demo schemes plus the built-in themes (not Omarchy: that's Auto Mode).
        let built_in = crate::theme::names().filter(|n| *n != "omarchy").count();
        assert_eq!(hit_names(&app).len(), 5 + built_in);
        assert!(!hit_names(&app).iter().any(|n| n == "Omarchy"));
        // Typing filters at once, no `/`; j and k are letters here.
        type_in(&mut app, "drc");
        assert_eq!(hit_names(&app)[0], "Dracula", "best match first");
        assert!(!hit_names(&app).contains(&"rose-pine".to_string()));
        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(app.theme_picker.as_ref().unwrap().query, "");
        type_in(&mut app, "jk");
        assert_eq!(app.theme_picker.as_ref().unwrap().query, "jk");
        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        // A built-in theme applies at once: no download, nothing saved.
        type_in(&mut app, "gruvbox");
        assert_eq!(hit_names(&app)[0], crate::theme::label("gruvbox"));
        app.on_key(key(KeyCode::Enter));
        assert!(app.theme_job.is_none(), "no download");
        assert_eq!(app.config.ui.theme(), "gruvbox");
        assert!(app.user_themes.is_empty() && app.theme_picker.is_some());
        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        type_in(&mut app, "dracula");
        // Enter downloads and uses it; the popup stays open.
        app.on_key(key(KeyCode::Enter));
        wait_for_themes(&mut app);
        assert_eq!(app.config.ui.theme(), "dracula");
        assert!(dir.join("dracula.toml").is_file());
        assert!(app.theme_picker.is_some(), "still open");
        assert!(!hit_names(&app).contains(&"Dracula".to_string()), "installed ones leave Search");
        assert!(app.setting_value(Setting::ThemeSelect).contains("1 saved ~ Browse for more…"));
        // A file that isn't a theme is refused and nothing changes.
        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        type_in(&mut app, "broken");
        app.on_key(key(KeyCode::Enter));
        wait_for_themes(&mut app);
        assert!(app.status.contains("Broken"), "{}", app.status);
        assert_eq!(app.config.ui.theme(), "dracula");
        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        type_in(&mut app, "solarized");
        app.on_key(key(KeyCode::Enter));
        wait_for_themes(&mut app);
        assert_eq!(app.config.ui.theme(), "solarized-dark");
        // Installed: only the downloaded ones; Enter uses one and stays open.
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.theme_picker.as_ref().unwrap().tab, ThemeTab::Installed);
        assert_eq!(app.installed_themes(), ["dracula", "solarized-dark"]);
        app.on_key(key(KeyCode::Home));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.config.ui.theme(), "dracula");
        app.on_key(key(KeyCode::Char('j')));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.config.ui.theme(), "solarized-dark");
        assert!(app.theme_picker.is_some());
        // x asks; n keeps it, y deletes and falls back.
        app.on_key(key(KeyCode::Char('k')));
        app.on_key(key(KeyCode::Enter));
        app.on_key(key(KeyCode::Char('x')));
        assert_eq!(app.theme_picker.as_ref().unwrap().confirm_delete.as_deref(), Some("dracula"));
        app.on_key(key(KeyCode::Char('n')));
        assert!(dir.join("dracula.toml").is_file());
        app.on_key(key(KeyCode::Delete));
        app.on_key(key(KeyCode::Char('y')));
        assert!(!dir.join("dracula.toml").exists());
        assert_eq!(app.installed_themes(), ["solarized-dark"]);
        assert_eq!(app.config.ui.theme(), crate::theme::FALLBACK);
        // Deleted, it is back in Search.
        app.on_key(key(KeyCode::Tab));
        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        type_in(&mut app, "dracula");
        assert_eq!(hit_names(&app)[0], "Dracula");
        // Esc closes.
        app.on_key(key(KeyCode::Esc));
        assert!(app.theme_picker.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn theme_auto_mode_greys_theme_select_and_both_save() {
        let mut app = saving_app("theme-auto");
        app.config.ui.set_theme("kanagawa");
        app.focus = Focus::Content;
        app.view = View::Settings;
        app.settings_cursor = Setting::at(Setting::ThemeAutoMode);
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.config.ui.theme_auto_mode, crate::config::ThemeAutoMode::Omarchy);
        assert!(app.theme_select_greyed());
        let text = saved(&app);
        assert!(text.contains("theme_auto_mode = \"omarchy\""), "{text}");
        app.settings_cursor = Setting::at(Setting::ThemeSelect);
        app.on_key(key(KeyCode::Enter));
        assert!(app.theme_picker.is_none(), "greyed: no picker");
        app.settings_cursor = Setting::at(Setting::ThemeAutoMode);
        app.on_key(key(KeyCode::Enter));
        assert!(!app.theme_select_greyed());
        assert_eq!(app.config.ui.theme(), "kanagawa", "back to the selected theme");
        assert!(saved(&app).contains("theme_select = \"kanagawa\""));
    }

    #[test]
    fn rank_is_fuzzy_and_best_first() {
        let demo = crate::theme_store::demo_catalog();
        let names = |q: &str| themes::rank(&demo, q).into_iter().map(|i| demo[i].name.clone()).collect::<Vec<_>>();
        assert_eq!(names("").len(), demo.len());
        assert_eq!(names("sdk"), ["Solarized Dark"]);
        assert!(names("zzzq").is_empty());
    }

    #[test]
    fn no_key_is_bound_twice_and_the_removed_ones_are_gone() {
        for (i, a) in KEYMAP.iter().enumerate() {
            for b in &KEYMAP[i + 1..] {
                assert!(a.code != b.code, "{:?} is bound twice", a.code);
            }
        }
        for gone in ['+', '-', '=', 'u', 'd', 'n', 's', 't'] {
            let bound = command_for(key(KeyCode::Char(gone)));
            assert!(bound.is_none(), "{gone} should be unbound, is {bound:?}");
        }
        assert_eq!(command_for(shift('U')), Some(Cmd::ThumbUp));
        assert_eq!(command_for(shift('N')), Some(Cmd::Skip));
        assert_eq!(command_for(key(KeyCode::Char('l'))), Some(Cmd::LikePane));
    }

    #[test]
    fn shift_c_r_and_h_cycle_the_cover_art_and_save_it() {
        let mut app = saving_app("cover-keys");
        assert_eq!(app.config.ui.album_art, AlbumArt::Picture);
        app.on_key(shift('C'));
        assert_eq!(app.config.ui.album_art, AlbumArt::Off);
        app.on_key(shift('C'));
        assert_eq!(app.config.ui.album_art, AlbumArt::Picture, "Picture and Off, nothing else");
        let retro = app.config.ui.retro_mode;
        app.on_key(shift('R'));
        assert_eq!(app.config.ui.retro_mode, retro.next());
        let tint = app.config.ui.art_tint;
        app.on_key(shift('H'));
        assert_eq!(app.config.ui.art_tint, tint.next());
        let file = saved(&app);
        assert!(
            file.contains("album_art") && file.contains("retro_mode") && file.contains("tint_art_with_theme_colors"),
            "{file}"
        );
        // The retro levels go all the way round, 64-bit included.
        let mut seen = vec![app.config.ui.retro_mode];
        for _ in 0..crate::config::RetroMode::ALL.len() {
            app.on_key(shift('R'));
            seen.push(app.config.ui.retro_mode);
        }
        assert!(seen.contains(&crate::config::RetroMode::Bit64));
        assert_eq!(seen.first(), seen.last());
    }

    #[test]
    fn shift_z_is_tired_of_this_song_and_lowercase_z_is_nothing() {
        assert_eq!(command_for(key(KeyCode::Char('z'))), None);
        assert_eq!(command_for(shift('Z')), Some(Cmd::Tired));
        // From any pane, on the playing song: it goes to History and the next one starts.
        let mut app = App::new(Config::default(), None, &[], true);
        app.begin_splash();
        app.on_key(key(KeyCode::Char(' ')));
        app.settle(std::time::Duration::from_millis(400));
        let Some(playing) = app.now.clone() else { panic!("demo plays a song") };
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.on_key(key(KeyCode::Char('z')));
        assert_eq!(app.now.as_ref().map(|s| &s.track_token), Some(&playing.track_token), "z does nothing");
        app.on_key(shift('Z'));
        assert_ne!(app.now.as_ref().map(|s| &s.track_token), Some(&playing.track_token), "Z moves on");
        assert!(app.history.iter().any(|s| s.track_token == playing.track_token));
    }

    #[test]
    fn i_g_and_home_end_are_not_keys_any_more() {
        let mut app = saving_app("no-icon-key");
        let icons = app.config.ui.icons;
        for code in [KeyCode::Char('i'), KeyCode::Char('g'), KeyCode::Home, KeyCode::End] {
            assert_eq!(command_for(key(code)), None, "{code:?}");
        }
        app.on_key(key(KeyCode::Char('i')));
        assert_eq!(app.config.ui.icons, icons, "i no longer cycles the icons");
        assert!(saved(&app).is_empty());
        // The icon set is still a setting.
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.settings_cursor = Setting::at(Setting::Icons);
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.config.ui.icons, icons.next());
        assert!(saved(&app).contains("icons"));
    }

    #[test]
    fn any_key_skips_the_splash_and_it_is_used_up() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.begin_splash();
        assert!(app.splash.is_some(), "on by default");
        let view = app.view;
        app.on_key(key(KeyCode::Char('i')));
        assert!(app.splash.is_none());
        assert_eq!(app.view, view);
        let mut config = Config::default();
        config.ui.splash = false;
        let mut off = App::new(config, None, &[], true);
        off.begin_splash();
        assert!(off.splash.is_none(), "the toggle turns it off");
    }

    #[test]
    fn r_renames_a_station_and_the_name_is_asked_for_first() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.settle(Duration::from_millis(150));
        app.on_key(shift('S'));
        let id = app.stations[app.stations_index()].id.clone();
        let old = app.stations[app.stations_index()].name.clone();
        app.on_key(key(KeyCode::Char('r')));
        let rename = app.rename.clone().expect("the rename box opens");
        assert_eq!((rename.station.id.as_str(), rename.text.text()), (id.as_str(), old.as_str()));
        for _ in 0..old.chars().count() {
            app.on_key(key(KeyCode::Backspace));
        }
        for c in "Mine".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Enter));
        assert!(app.rename.is_none());
        app.settle(Duration::from_millis(150));
        assert_eq!(app.stations.iter().find(|s| s.id == id).unwrap().name, "Mine");
        // Esc cancels, and nothing is sent.
        app.on_key(key(KeyCode::Char('r')));
        app.on_key(key(KeyCode::Char('X')));
        app.on_key(key(KeyCode::Esc));
        assert!(app.rename.is_none());
        assert_eq!(app.stations.iter().find(|s| s.id == id).unwrap().name, "Mine");
    }

    #[test]
    fn x_asks_then_deletes_only_on_y_and_cleans_favorites() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.settle(Duration::from_millis(150));
        app.on_key(shift('S'));
        let station = app.stations[app.stations_index()].clone();
        app.config.ui.favorites.push(crate::config::Favorite { id: station.id.clone(), name: station.name.clone() });
        let count = app.stations.len();
        app.on_key(key(KeyCode::Char('x')));
        assert_eq!(app.confirm_delete.as_ref().map(|s| s.id.as_str()), Some(station.id.as_str()));
        app.on_key(key(KeyCode::Char('n')));
        assert!(app.confirm_delete.is_none());
        app.settle(Duration::from_millis(100));
        assert_eq!(app.stations.len(), count, "any key but y keeps it");
        app.on_key(key(KeyCode::Char('x')));
        app.on_key(key(KeyCode::Char('y')));
        app.settle(Duration::from_millis(150));
        assert_eq!(app.stations.len(), count - 1);
        assert!(!app.stations.iter().any(|s| s.id == station.id));
        assert!(!app.is_favorite(&station.id), "its favorite goes too");
    }

    #[test]
    fn r_x_and_v_in_favorites_act_on_that_favorites_station_and_f_removes_the_favorite() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.settle(Duration::from_millis(150));
        let station = app.stations.iter().find(|s| s.allow_rename && s.allow_delete).unwrap().clone();
        app.config.ui.favorites.push(crate::config::Favorite { id: station.id.clone(), name: station.name.clone() });
        app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Favorite(0)).unwrap();
        app.focus = Focus::Menu;
        app.on_key(key(KeyCode::Char('r')));
        let rename = app.rename.take().expect("rename box");
        assert!(rename.station.favorite && rename.station.id == station.id);
        app.on_key(key(KeyCode::Char('x')));
        let delete = app.confirm_delete.take().expect("delete box");
        assert!(delete.favorite && delete.id == station.id);
        app.on_key(key(KeyCode::Char('v')));
        assert_eq!(app.details.as_ref().map(|d| d.station_id.as_str()), Some(station.id.as_str()));
        app.on_key(key(KeyCode::Char('q')));
        assert!(app.details.is_none(), "any key closes details");
        app.on_key(key(KeyCode::Char('f')));
        assert!(!app.is_favorite(&station.id), "f still removes the favorite");
        assert!(app.stations.iter().any(|s| s.id == station.id), "and leaves the station alone");
    }

    #[test]
    fn a_station_pandora_will_not_delete_or_rename_is_left_alone() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.settle(Duration::from_millis(150));
        app.on_key(shift('S'));
        let i = app.stations.iter().position(|s| !s.allow_delete).expect("the demo has a shuffle station");
        app.stations_cursor = i;
        app.on_key(key(KeyCode::Char('x')));
        assert!(app.confirm_delete.is_none());
        assert!(app.status.contains("doesn't let you delete"), "{}", app.status);
        app.on_key(key(KeyCode::Char('r')));
        assert!(app.rename.is_none());
    }

    #[test]
    fn v_shows_real_station_details_for_a_station_and_for_the_playing_song() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.settle(Duration::from_millis(150));
        app.tune(0);
        app.settle(Duration::from_millis(150));
        app.on_key(key(KeyCode::Char('v')));
        app.settle(Duration::from_millis(150));
        let d = app.details.clone().expect("details open");
        assert!(matches!(d.of, DetailsOf::Song(_)));
        assert!(matches!(d.from_pandora, Some(Ok(ref s)) if !s.seeds.is_empty()), "{d:?}");
        app.on_key(key(KeyCode::Esc));
        assert!(app.details.is_none());
        app.on_key(shift('S'));
        app.on_key(key(KeyCode::Char('v')));
        app.settle(Duration::from_millis(150));
        assert!(matches!(app.details.as_ref().unwrap().of, DetailsOf::Station(_)));
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
    }

    fn open_stations_page() -> App {
        let mut app = demo();
        app.settle(Duration::from_millis(300));
        app.on_key(shift('S'));
        app
    }

    #[test]
    fn a_searches_then_enter_makes_the_station_and_plays_it() {
        let mut app = open_stations_page();
        let before = app.stations.len();
        app.on_key(key(KeyCode::Char('a')));
        assert!(app.add_station.is_some(), "a opens the search box");
        // Letters go into the box, not to j/k/q/a keys.
        type_text(&mut app, "mid");
        assert_eq!(app.add_station.as_ref().unwrap().text.text(), "mid");
        app.on_key(key(KeyCode::Enter));
        assert!(app.add_station.as_ref().unwrap().waiting);
        app.settle(Duration::from_millis(300));
        let search = app.add_station.as_ref().unwrap();
        assert_eq!(search.hits.len(), 1, "{:?}", search.hits);
        assert_eq!(search.hits[0].name, "Midnight City");
        assert_eq!(search.mode, SearchMode::Results, "the answer is ready to pick from");
        app.on_key(key(KeyCode::Enter));
        assert!(app.add_station.is_none());
        app.settle(Duration::from_millis(400));
        assert_eq!(app.stations.len(), before + 1);
        let at = usize::from(app.stations[0].is_shuffle);
        assert_eq!(app.stations[at].name, "Midnight City Radio", "new stations go after Shuffle");
        assert_eq!(app.current_station.as_deref(), Some(app.stations[at].id.as_str()), "and it plays");
        assert_eq!(app.view, View::Player);
        // Making it again does not add a second copy.
        let again = app.stations[at].clone();
        app.station_created(again);
        assert_eq!(app.stations.len(), before + 1);
    }

    #[test]
    fn the_results_move_with_j_k_and_arrows_and_esc_cancels() {
        let mut app = open_stations_page();
        app.on_key(key(KeyCode::Char('a')));
        type_text(&mut app, "m");
        app.on_key(key(KeyCode::Enter));
        app.settle(Duration::from_millis(300));
        let n = app.add_station.as_ref().unwrap().hits.len();
        assert!(n >= 3, "artists and songs both match 'm': {n}");
        use crate::pandora::HitKind;
        let hits = &app.add_station.as_ref().unwrap().hits;
        assert!(
            hits.iter().any(|h| h.kind == HitKind::Artist) && hits.iter().any(|h| h.kind == HitKind::Song),
            "both artists and songs are listed"
        );
        let cursor = |app: &App| app.add_station.as_ref().unwrap().cursor;
        app.on_key(key(KeyCode::Char('j')));
        assert_eq!(cursor(&app), 1);
        app.on_key(key(KeyCode::Down));
        assert_eq!(cursor(&app), 2);
        app.on_key(key(KeyCode::Char('k')));
        app.on_key(key(KeyCode::Up));
        assert_eq!(cursor(&app), 0);
        app.on_key(key(KeyCode::Up));
        assert_eq!(cursor(&app), n - 1, "wraps");
        // / goes back to typing; j is then just a letter.
        app.on_key(key(KeyCode::Char('/')));
        app.on_key(key(KeyCode::Char('j')));
        assert_eq!(app.add_station.as_ref().unwrap().text.text(), "mj");
        let before = app.stations.len();
        app.on_key(key(KeyCode::Esc));
        assert!(app.add_station.is_none());
        app.settle(Duration::from_millis(100));
        assert_eq!(app.stations.len(), before, "Esc adds nothing");
    }

    #[test]
    fn the_add_station_row_does_the_same_as_a() {
        let mut app = open_stations_page();
        app.stations_cursor = 0;
        app.on_key(key(KeyCode::Up));
        assert!(app.on_add_row, "the row sits above the first station");
        app.on_key(key(KeyCode::Char('f')));
        assert!(app.config.ui.favorites.is_empty(), "f does nothing to the row");
        app.on_key(key(KeyCode::Enter));
        assert!(app.add_station.is_some());
        assert!(app.current_station.is_some(), "Enter did not play a station");
    }

    #[test]
    fn a_late_answer_to_an_older_search_is_not_shown_and_errors_are() {
        let mut app = open_stations_page();
        app.on_key(key(KeyCode::Char('a')));
        type_text(&mut app, "kids");
        app.on_key(key(KeyCode::Enter));
        // Change the text before the answer comes.
        app.on_key(key(KeyCode::Backspace));
        app.settle(Duration::from_millis(300));
        let search = app.add_station.as_ref().unwrap();
        assert_eq!(search.mode, SearchMode::Typing, "the text moved on: keep typing");
        // Enter searches the new text.
        app.on_key(key(KeyCode::Enter));
        app.settle(Duration::from_millis(300));
        assert!(!app.add_station.as_ref().unwrap().hits.is_empty());
        app.on_reply(Reply::Failed { what: "search", message: "Pandora is down".into() });
        let search = app.add_station.as_ref().unwrap();
        assert_eq!((search.waiting, search.error.as_deref()), (false, Some("Pandora is down")));
    }

    #[test]
    fn a_outside_the_station_list_says_where_and_logged_out_asks_to_log_in() {
        let mut app = open_stations_page();
        app.on_key(shift('A'));
        app.on_key(key(KeyCode::Char('a')));
        assert!(app.add_station.is_none() && app.status.contains("Shift+S"), "{}", app.status);
        let mut out = App::new(Config::default(), None, &[], false);
        out.prompt = None;
        out.view = View::Stations;
        out.focus = Focus::Content;
        out.on_key(key(KeyCode::Char('a')));
        assert!(out.add_station.is_none() && out.status.contains("Log in"), "{}", out.status);
    }

    #[test]
    fn a_second_create_is_ignored_while_one_is_on_its_way() {
        let mut app = open_stations_page();
        app.create_station("AR:demo-m83", "M83");
        app.create_station("AR:demo-mgmt", "MGMT");
        assert!(app.status.contains("Still"), "{}", app.status);
        app.settle(Duration::from_millis(300));
        assert!(!app.creating);
        assert_eq!(app.stations.iter().filter(|s| s.name == "M83 Radio").count(), 1);
        assert!(!app.stations.iter().any(|s| s.name == "MGMT Radio"), "the second was dropped");
        app.on_reply(Reply::Failed { what: "create station", message: "no".into() });
        assert!(!app.creating);
    }

    #[test]
    fn shift_g_opens_genres_loads_them_once_and_enter_adds_and_plays() {
        let mut app = demo();
        app.settle(Duration::from_millis(300));
        app.on_key(shift('G'));
        assert_eq!((app.view, app.focus), (View::Genres, Focus::Content));
        assert_eq!(app.menu_selected(), MenuItem::Genres);
        assert_eq!(app.genres, GenresState::Loading);
        app.settle(Duration::from_millis(300));
        let GenresState::Loaded(categories) = &app.genres else { panic!("{:?}", app.genres) };
        assert_eq!(categories.len(), 3);
        // Moving, wrapping.
        let rows = app.genre_rows().len();
        assert_eq!(rows, 5);
        app.on_key(key(KeyCode::Char('j')));
        app.on_key(key(KeyCode::Down));
        assert_eq!(app.genres_index(), 2);
        app.on_key(key(KeyCode::Char('k')));
        app.on_key(key(KeyCode::Up));
        app.on_key(key(KeyCode::Up));
        assert_eq!(app.genres_index(), rows - 1);
        // Leaving and coming back does not ask again (it is cached).
        app.on_key(shift('S'));
        let loaded = app.genres.clone();
        app.on_key(shift('G'));
        assert_eq!(app.genres, loaded);
        assert!(app.backend.replies.recv_timeout(Duration::from_millis(100)).is_err(), "no second request");
        // Enter on Synthwave.
        app.genres_cursor = 2;
        let before = app.stations.len();
        app.on_key(key(KeyCode::Enter));
        app.settle(Duration::from_millis(400));
        assert_eq!(app.stations.len(), before + 1);
        let at = usize::from(app.stations[0].is_shuffle);
        assert_eq!(app.stations[at].name, "Synthwave");
        assert_eq!(app.current_station.as_deref(), Some(app.stations[at].id.as_str()));
    }

    #[test]
    fn a_genre_list_that_failed_is_asked_for_again_and_logged_out_says_so() {
        let mut app = demo();
        app.settle(Duration::from_millis(300));
        app.on_reply(Reply::Failed { what: "genres", message: "no".into() });
        assert_eq!(app.genres, GenresState::Failed("no".into()));
        app.on_key(key(KeyCode::Enter)); // not on the page yet: this is the menu/other view
        app.on_key(shift('G'));
        assert_eq!(app.genres, GenresState::Loading, "G tries again");
        let mut out = App::new(Config::default(), None, &[], false);
        out.prompt = None;
        out.on_key(shift('G'));
        assert!(out.status.contains("Log in") && out.genres == GenresState::Idle, "{}", out.status);
    }

    #[test]
    fn genres_is_in_the_menu_under_stations_and_a_g_are_bound_once() {
        let app = demo();
        let items = app.menu_items();
        let at = items.iter().position(|m| *m == MenuItem::Stations).unwrap();
        assert_eq!(items[at + 1], MenuItem::Genres);
        assert_eq!(command_for(KeyEvent::from(KeyCode::Char('a'))), Some(Cmd::Add));
        assert_eq!(command_for(shift('G')), Some(Cmd::Genres));
    }

    #[test]
    fn animations_off_skips_the_boot_screen_and_is_saved() {
        let mut app = saving_app("animations");
        assert!(app.config.ui.animations, "on by default");
        app.begin_splash();
        assert!(app.splash.is_some());
        app.splash = None;
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.settings_cursor = Setting::at(Setting::Animations);
        assert_eq!(app.setting_value(Setting::Animations), "on");
        app.on_key(key(KeyCode::Enter));
        assert!(!app.config.ui.animations);
        assert_eq!(app.setting_value(Setting::Animations), "off");
        assert!(saved(&app).contains("[look]") && saved(&app).contains("animations = false"), "{}", saved(&app));
        app.begin_splash();
        assert!(app.splash.is_none() && app.splash_progress().is_none(), "no boot screen when off");
        // It loads back.
        let (config, warnings) = Config::load(app.config_path.as_ref().unwrap());
        assert!(!config.ui.animations && warnings.is_empty(), "{warnings:?}");
        // And the other settings are unaffected: the clock and progress still count.
        assert!(config.ui.splash);
        app.on_key(key(KeyCode::Enter));
        assert!(app.config.ui.animations);
        app.begin_splash();
        assert!(app.splash.is_some(), "on again: the boot screen is back");
    }

    // --- System Check ---

    fn facts(font: bool, notify: bool) -> crate::deps::Facts {
        crate::deps::Facts {
            families: Some(if font { "JetBrainsMono Nerd Font\n" } else { "DejaVu Sans\n" }.into()),
            glyph_fonts: Some(String::new()),
            notify_send: notify,
            keychain: Some(true),
        }
    }

    #[test]
    fn the_list_opens_by_itself_only_when_something_needed_is_missing() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.system_checked(&facts(true, false));
        assert!(!app.system_open, "the font is there: only a line about the extra");
        assert_eq!(app.status, "Optional, not installed: notify-send");
        app.system_checked(&facts(false, true));
        assert!(app.system_open, "no Nerd Font: the list opens");
        assert!(app.status.contains("Missing: Nerd Font symbols"), "{}", app.status);
    }

    #[test]
    fn nothing_missing_says_nothing() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.status = String::new();
        app.system_checked(&facts(true, true));
        assert!(!app.system_open && app.status.is_empty(), "{}", app.status);
    }

    #[test]
    fn emoji_icons_need_no_nerd_font_so_nothing_opens() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.config.ui.icons = crate::config::IconSet::Emoji;
        app.system_checked(&facts(false, true));
        assert!(!app.system_open);
        assert!(app.status.contains("Optional"), "the font is listed but optional: {}", app.status);
    }

    #[test]
    fn it_never_opens_over_what_you_are_typing() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.open_prompt(Prompt::Email);
        app.system_checked(&facts(false, true));
        assert!(!app.system_open, "the email box keeps the keys");
        assert!(app.status.contains("Missing: Nerd Font symbols"), "{}", app.status);
        app.prompt = None;
        app.help = true;
        app.system_checked(&facts(false, true));
        assert!(!app.system_open, "nor over Help");
    }

    #[test]
    fn enter_looks_again_and_any_other_key_closes_the_list() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.system_checked(&facts(false, false));
        assert!(app.system_open && app.overlay_open());
        app.on_key(key(KeyCode::Enter));
        assert!(app.system_open && app.system.is_none(), "looking again");
        app.on_key(key(KeyCode::Char('q')));
        assert!(!app.system_open && !app.quit, "q only closes it");
    }

    #[test]
    fn the_answer_arrives_through_tick() {
        let mut app = App::new(Config::default(), None, &[], true);
        let (send, receive) = std::sync::mpsc::channel();
        app.system_look = Some(receive);
        app.tick();
        assert!(app.system.is_none(), "not yet");
        send.send(facts(false, true)).unwrap();
        app.tick();
        assert!(app.system.is_some() && app.system_open);
    }

    #[test]
    fn the_boot_screen_toggle_saves_under_look_and_drops_the_old_place() {
        let mut app = saving_app("splash-look");
        std::fs::write(app.config_path.as_ref().unwrap(), "[boxvibe]\nsplash = true\ncheck_updates = false\n").unwrap();
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.settings_cursor = Setting::at(Setting::Splash);
        assert_eq!(Setting::ALL[app.settings_cursor].0, "LOOK");
        app.on_key(key(KeyCode::Enter));
        assert!(!app.config.ui.splash);
        let text = saved(&app);
        assert!(text.contains("[look]") && text.contains("boot_screen = false"), "{text}");
        assert!(!text.contains("splash"), "the old name is gone: {text}");
        assert!(
            text.contains("look_for_new_versions_on_startup = false") && !text.contains("check_updates"),
            "the rest stays, renamed: {text}"
        );
        assert!(Setting::Splash.about().contains("Saved as [look] boot_screen."));
    }
}
