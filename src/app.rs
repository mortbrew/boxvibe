//! The app's state and what each key does.
//!
//! Like tubevibe, the left panel is the menu: NOW PLAYING, My Stations, the
//! FAVORITES section (like tubevibe's tabs), then Settings, About and Quit.
//! The right side shows what the menu entry opens: the player (Now Playing,
//! Up Next and History), the station list, Settings or About. There is no
//! separate login screen: the account lives in Settings, and the first run
//! opens Settings on it.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::ads;
use crate::audio::Audio;
use crate::backend::{Backend, Failed, KeychainJob, KeychainReply, Reply, Request};
use crate::config::{self, AlbumArt, Config, ConfigWarning, GoodVibes, Secret};
use crate::input::TextInput;
use crate::keychain::{self, Keychain, NO_KEYCHAIN};
use crate::pandora::{
    BrowseItem, BrowseSection, Quality, Rating, RecentSource, SearchHit, Song, Station, StationDetails,
    station_ids_match,
};
use crate::theme::{self, Palette};
use crate::update::{Progress, Replace, Update};

mod ad;
mod audio_cache;
mod browse;
mod browse_pages;
mod dialogs;
mod editor;
mod panel;
mod pending_password;
mod playback;
mod recent;
mod recent_state;
mod themes;
mod view_all;
pub use ad::{AdPhase, AdView, End};
pub use editor::{Editor, EditorConfirm, EditorTab, FeedState, Loaded};
pub use panel::{Panel, PanelKind};
pub use themes::ThemePicker;

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
    /// Browse directory (`feed/getDirectory`), or one pinned section.
    Browse,
    /// Songs pinned with Shift+I.
    Pinned,
    /// The account's recently played sources, newest first: MY COLLECTION's
    /// last row, from Pandora's GraphQL API.
    Recent,
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
    /// Opens the station list on the right (UI: My Stations).
    Stations,
    /// A Browse section pinned on the main menu ([`App::browse_menu_pins`] index).
    BrowsePin(usize),
    /// The songs pinned with Shift+I (hidden by `[look] pinned_in_menu`).
    Pinned,
    /// The recently played list. Drawn only once Pandora sent something, so
    /// it hides while empty the way FAVORITES does.
    RecentlyPlayed,
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
    /// One of [`config::BROWSE_SECTIONS`] (`[browse] <key>`).
    BrowseSection(u8),
    EditFile,
    MenuEntry,
}

impl Setting {
    /// In screen order, each with its group.
    pub const ALL: [(&'static str, Self); 35] = [
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
        ("BROWSE", Self::BrowseSection(0)),
        ("BROWSE", Self::BrowseSection(1)),
        ("BROWSE", Self::BrowseSection(2)),
        ("BROWSE", Self::BrowseSection(3)),
        ("BROWSE", Self::BrowseSection(4)),
        ("BROWSE", Self::BrowseSection(5)),
        ("BROWSE", Self::BrowseSection(6)),
        ("BROWSE", Self::BrowseSection(7)),
        ("BROWSE", Self::BrowseSection(8)),
        ("BROWSE", Self::BrowseSection(9)),
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
            Self::BrowseSection(i) => config::BROWSE_SECTIONS[i as usize].1,
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
            Self::BrowseSection(i) => Some(("browse", config::BROWSE_SECTIONS[i as usize].0)),
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
                "high asks for 192 kbps MP3 where your plan has it (Plus, Premium), else 128 kbps MP3; standard asks for 128. A free plan gets 64 kbps HE-AAC whatever you pick, so this is greyed out there and the row says what is really playing. Saved as [playback] audio_quality."
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
            Self::BrowseSection(i) => match i {
                0 => "On: show Artists Picked For You on the main menu. Saved as [browse] artists_picked_for_you.",
                1 => "On: show New Music on the main menu. Saved as [browse] new_music.",
                2 => "On: show Browse Genres on the main menu (G opens it too). Saved as [browse] browse_genres.",
                3 => "On: show Featured Playlists on the main menu. Saved as [browse] featured_playlists.",
                4 => "On: show Trendsetters on the main menu. Saved as [browse] trendsetters_artists_on_the_rise.",
                5 => "On: show Decades on the main menu. Saved as [browse] decades.",
                6 => "On: show Recommended Podcasts on the main menu. Saved as [browse] recommended_podcasts.",
                7 => "On: show Top artists on the main menu. Saved as [browse] top_artists.",
                8 => "On: show Moods and activities on the main menu. Saved as [browse] moods_and_activities.",
                _ => "On: show Top stations on the main menu. Saved as [browse] top_stations.",
            },
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
    // Space pauses from anywhere; lowercase p is unbound (STYLE shared core).
    bind(KeyCode::Char(' '), Cmd::Pause),
    // Lowercase: the pane that has the keys.
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

/// Pandora Browse directory (`feed/getDirectory`): the main teaser or one View-all page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BrowseState {
    #[default]
    Idle,
    Loading,
    Loaded(Vec<BrowseSection>),
    Failed(String),
}

/// The Browse page under a category shelf, so Esc returns to it.
struct BrowsePlace {
    state: BrowseState,
    path: Option<String>,
    cursor: usize,
}

enum BrowsePick {
    Station(String, String),
    Page(String),
    Unsupported(String),
    Header,
}

/// One line on the Browse page: a section title or an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseRow<'a> {
    Header(&'a str),
    Item(&'a BrowseItem),
}

fn same_browse_section(section: &BrowseSection, want: &str) -> bool {
    section.title == want || config::browse_section_key(&section.title) == want
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
                    match (s.audio_format, s.bitrate) {
                        (crate::pandora::AudioFormat::Mp3, 0) => "MP3".into(),
                        (crate::pandora::AudioFormat::Mp3, kbps) => format!("MP3 {kbps} kbps"),
                        (crate::pandora::AudioFormat::Aac, 0) => "AAC".into(),
                        (crate::pandora::AudioFormat::Aac, kbps) => format!("AAC {kbps} kbps"),
                        (crate::pandora::AudioFormat::Unknown, _) => String::new(),
                    },
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
    /// The Omarchy theme followed, as shown ("Tokyo Night"); for the status bar.
    pub omarchy_theme: Option<String>,
    last_theme_poll: Instant,
    pub stations: Vec<Station>,
    pub current_station: Option<String>,
    pub queue: VecDeque<Song>,
    pub now: Option<Song>,
    /// Played songs, newest first.
    pub history: Vec<Song>,
    pub status: String,
    /// The sleep timer (`B`), when one is set.
    pub sleep: Option<Sleep>,
    /// The sleep timer ran out at a song's end: pause what starts next.
    sleep_pause_next: bool,
    /// The boot screen: when it began (`None` once it is over or switched off).
    pub splash: Option<Instant>,
    /// What boxvibe needs from the system and whether it is there (`None`:
    /// still looking, or not looked yet).
    pub system: Option<Vec<crate::deps::Check>>,
    /// Open dialogs (quit / unpin / delete questions, Help, details, System Check): see `app/dialogs.rs`.
    pub dialogs: vibekit::dialog::DialogStack,
    /// The System Check list is showing.
    /// The look at the system that is on its way.
    system_look: Option<std::sync::mpsc::Receiver<crate::deps::Facts>>,
    /// `x` asked "Delete this station?": y does, any other key doesn't.
    /// `x` on the Pinned page asked "Unpin this song?" (the pin's index).
    /// Songs pinned with Shift+I, newest first.
    pub pins: Vec<crate::pinned::Pin>,
    /// Where they are kept (`None`: in the demo, only in memory).
    pub pins_path: Option<PathBuf>,
    pub pinned_cursor: usize,
    /// The account's recently played sources, newest first.
    pub recent: Vec<RecentSource>,
    pub recent_cursor: usize,
    /// The request in flight: its generation, and whether another is waiting
    /// (see [`recent_state`]).
    pub recent_state: recent_state::RecentState,
    /// Rows removed on this device only. Pandora's list is unchanged; refresh
    /// may bring them back unless Pandora adds a remove API.
    recent_hidden: std::collections::HashSet<String>,
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
    /// What the Browse page is showing: the teaser, or one View-all page.
    pub browse: BrowseState,
    pub browse_cursor: usize,
    /// `None`: full teaser (every section). `Some(title)`: one pinned section's page.
    pub browse_filter: Option<String>,
    /// Main `/browse` teaser, the View-all page on screen, and the page Esc
    /// returns to (see [`browse_pages`]).
    pages: browse_pages::BrowsePages,
    /// View-all pages already fetched (see [`view_all`]).
    view_all: view_all::ViewAllCache,
    /// Selected genre station, counted over the stations only (not the headings).
    /// A station is being made: a second Enter waits for it.
    creating: bool,
    /// `v`: the details popup.
    /// `e`: the seeds and thumbs editor.
    pub editor: Option<Editor>,
    /// Good Vibes Only: where the ad break is (see [`ad`]).
    pub ad: AdPhase,
    ad_serial: u32,
    ad_tracker: ads::Tracker,
    /// The song whose audio is downloading, and since when (a download that
    /// never answers must not hold the player forever).
    pub buffering: Option<(String, Instant)>,
    /// Downloaded songs kept for their turn (see [`audio_cache`]).
    audio_cache: audio_cache::AudioCache,
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
    /// The password in memory that the keychain does not have yet, and where
    /// it came from (see [`pending_password`]).
    pending_password: pending_password::PendingPassword,
    /// A newer release (About shows it).
    pub update: Option<Update>,
    /// GitHub confirmed this is the latest release.
    pub up_to_date: bool,
    pub self_update: Progress,
    /// Asks the main loop to open config.toml in an editor.
    pub edit_config: bool,
    /// After a login: whether the plan streams 192 kbps MP3.
    pub hifi: Option<bool>,
    /// What Pandora actually sent for the song now playing: its format and
    /// bitrate. `None` until a song plays. The plan's flag says what it
    /// *should* send; this says what it did, which is not always the same.
    pub stream: Option<(crate::pandora::AudioFormat, u32)>,
    /// Restart Now was pressed: once quit, start the new version.
    pub restart: bool,
    /// Shift+Q asked "Quit boxvibe?": y quits, any other key doesn't (like tubevibe).
    pub quit: bool,
}

const HISTORY_LEN: usize = 10;
/// How many View-all pages are kept. Each is a whole Browse page, and there
/// are only ever a handful of pinned sections, so this is generous; it exists
/// so a long session cannot grow the map without end.
/// How many downloaded songs are kept for the next one. Each is a whole MP3
/// (up to 64 MB by `fetch_audio`'s limit), so this stays small: the queue is
/// prefetched one song ahead, and a song that is skipped is not needed again.
/// The longest station name the rename box takes.
const MAX_STATION_NAME: usize = 80;
/// The longest email or password the prompts take.
const MAX_PROMPT: usize = 256;
/// How long the boot screen stays when no key skips it.
pub const SPLASH: Duration = Duration::from_millis(2000);
/// A song's audio gets this long to download before it is given up on.
const BUFFER_TIMEOUT: Duration = Duration::from_secs(30);

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
            omarchy_theme: None,
            last_theme_poll: Instant::now(),
            stations: Vec::new(),
            current_station: None,
            queue: VecDeque::new(),
            now: None,
            history: Vec::new(),
            status: String::new(),
            sleep: None,
            sleep_pause_next: false,
            splash: None,
            system: None,
            dialogs: vibekit::dialog::DialogStack::new(),
            system_look: None,
            pins_path: if demo { None } else { crate::pinned::default_path() },
            pins: Vec::new(),
            pinned_cursor: 0,
            recent: Vec::new(),
            recent_cursor: 0,
            recent_state: recent_state::RecentState::default(),
            recent_hidden: Default::default(),
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
            browse: BrowseState::Idle,
            browse_cursor: 0,
            browse_filter: None,
            pages: browse_pages::BrowsePages::default(),
            view_all: view_all::ViewAllCache::default(),
            creating: false,
            editor: None,
            ad: AdPhase::Off,
            ad_serial: 0,
            ad_tracker: ads::Tracker::new(!demo, true, &user_agent),
            buffering: None,
            audio_cache: audio_cache::AudioCache::default(),
            requested: HashSet::new(),
            playlist_pending: false,
            audio,
            backend,
            keychain,
            keychain_state: KeychainState::Unknown,
            pending_password: pending_password::PendingPassword::default(),
            update: None,
            up_to_date: false,
            self_update: Progress::Idle,
            edit_config: false,
            hifi: None,
            stream: None,
            restart: false,
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
            self.pending_password.read_from_file();
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
                self.system = Some(checks.clone());
                self.open_system();
            }
        } else if self.system_open() {
            self.status = "System checked".into();
        } else if extras > 0 && !self.status.starts_with('⚠') && self.status != NO_KEYCHAIN {
            // (a config warning already showing matters more)
            let names: Vec<&str> = checks.iter().filter(|c| !c.needed && !c.ok()).map(|c| c.dep.name()).collect();
            self.status = format!("Optional, not installed: {}", names.join(", "));
        }
        self.system = Some(checks);
        self.system_changed();
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
                    self.omarchy_theme = self.omarchy.name.clone();
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

    /// The menu lists FAVORITES: the setting is on, something is starred, and
    /// an account is signed in. A favorite is saved config (an id and a name)
    /// while `stations` is fetched per login, so logged out there is nothing
    /// for a row to play or to say about it.
    pub fn favorites_in_menu(&self) -> bool {
        self.config.ui.favorites_in_menu
            && !self.config.ui.favorites.is_empty()
            && matches!(self.session, Session::LoggedIn | Session::SigningIn)
    }

    /// Signed in but the stations are not in yet, so a favorite row cannot say
    /// whether its station is Pandora's or yours: the section shows a loading
    /// row instead of rows with a blank where that mark goes.
    pub fn favorites_loading(&self) -> bool {
        self.favorites_in_menu() && self.stations.is_empty()
    }

    pub fn menu_items(&self) -> Vec<MenuItem> {
        let top = self.has_now_playing().then_some(MenuItem::NowPlaying);
        let pins = (0..self.browse_menu_pins().len()).map(MenuItem::BrowsePin);
        top.into_iter()
            .chain([MenuItem::Stations])
            // Recently Played sits where Pinned used to, above Pinned. An empty
            // list keeps the row absent. Pinned stays above the Browse sections.
            .chain(self.recent_row().then_some(MenuItem::RecentlyPlayed))
            .chain(self.config.ui.pinned_in_menu.then_some(MenuItem::Pinned))
            .chain(pins)
            .chain(
                (0..if self.favorites_in_menu() { self.config.ui.favorites.len() } else { 0 }).map(MenuItem::Favorite),
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
            MenuItem::BrowsePin(i) => self.show_browse_pin(i),
            MenuItem::Pinned => self.show_page(View::Pinned),
            MenuItem::RecentlyPlayed => self.show_recently_played(),
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
            // Starred, but no account is signed in: the section is not there.
            None if !self.config.ui.favorites.is_empty() => {
                self.status = "Log in to see FAVORITES: they are kept for your account".into()
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

    /// My Stations (menu entry or `S`): the list, with the keys on it.
    fn show_stations(&mut self) {
        self.view = View::Stations;
        self.focus = Focus::Content;
        self.on_add_row = false;
        if let Some(at) = self.current_station.as_ref().and_then(|id| self.stations.iter().position(|s| &s.id == id)) {
            self.stations_cursor = at;
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
            self.status = "a adds a station: open My Stations (Shift+S) first".into();
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
            KeyCode::Esc | KeyCode::Char('.') => self.song_menu = None,
            KeyCode::Char('j') | KeyCode::Down => menu.cursor = step(menu.cursor, len, 1),
            KeyCode::Char('k') | KeyCode::Up => menu.cursor = step(menu.cursor, len, -1),
            KeyCode::Home | KeyCode::Char('g') => menu.cursor = 0,
            KeyCode::End => menu.cursor = len - 1,
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
                    self.audio_cache.forget(&song.track_token);
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
            self.open_details(Details { from_pandora: Some(Err("no station for this song".into())), ..details });
            return;
        }
        let current = self.current_station.as_deref() == Some(details.station_id.as_str());
        self.backend.send(Request::StationDetails { station_id: details.station_id.clone(), current });
        self.open_details(details);
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
            let mut command = std::process::Command::new("xdg-open");
            command
                .arg(&link)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            let _ = crate::proc::detached(command);
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
            View::Browse => match &self.browse_filter {
                Some(filter) => {
                    let pins = self.browse_menu_pins();
                    pins.iter().position(|(k, t)| t == filter || k == filter).map(MenuItem::BrowsePin)
                }
                None => None,
            },
            View::Pinned => Some(MenuItem::Pinned),
            View::Recent => Some(MenuItem::RecentlyPlayed),
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
        self.device_tick();
        if self.now.as_ref().is_some_and(|s| self.audio.finished(s.length)) {
            if let Some(song) = &self.now {
                crate::log::log!(
                    "end: {} — {} after {:?} of {} s ({}), {}",
                    song.title,
                    song.artist,
                    self.audio.position(),
                    song.length,
                    if self.audio.has_device() { "device" } else { "silent clock" },
                    if self.audio.position().as_secs() < u64::from(song.length).saturating_sub(5) {
                        "SHORT: ended early"
                    } else {
                        "played through"
                    }
                );
            }
            if self.sleep == Some(Sleep::EndOfSong) {
                self.sleep = None;
                self.sleep_pause_next = true;
                self.audio.set_fade(0.0);
            }
            self.end_song(End::Played);
        }
        self.ad_tick();
        self.buffer_tick();
        self.mpris_tick();
        self.theme_tick();
        self.want_art();
        if self.follows_omarchy() && self.last_theme_poll.elapsed() > Duration::from_secs(2) {
            self.last_theme_poll = Instant::now();
            if let Some(p) = self.omarchy.poll() {
                self.palette = p;
                self.omarchy_theme = self.omarchy.name.clone();
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

    /// Handles replies until none arrives within `timeout`: the demo and
    /// snapshot screens use it to let the fake backend catch up, and so do the
    /// tests. Not part of the running app's loop, which never blocks on a reply.
    pub(crate) fn settle(&mut self, timeout: Duration) {
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
                    if self.pending_password.needs_saving() {
                        self.pending_password.sent();
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
                    self.pending_password.done();
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
                // One call per session, after the list: a station entry carries
                // no name, so the row joins names from `stations`.
                self.ask_recently_played();
                // Favorites keep the names Pandora uses now, and one whose
                // station Pandora no longer has goes -- but only on a list we
                // can trust: a logged-in account that really has stations. An
                // empty list is a logged-out or brand-new account, and pruning
                // on it would throw away every star.
                let mut renamed = false;
                let trusted = self.session == Session::LoggedIn && !self.stations.is_empty();
                let before = self.config.ui.favorites.len();
                if trusted {
                    self.config.ui.favorites.retain(|f| self.stations.iter().any(|s| s.id == f.id));
                }
                let pruned = before - self.config.ui.favorites.len();
                for f in &mut self.config.ui.favorites {
                    if let Some(s) = self.stations.iter().find(|s| s.id == f.id)
                        && s.name != f.name
                    {
                        f.name = s.name.clone();
                        renamed = true;
                    }
                }
                if renamed || pruned > 0 {
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
                    self.status = "Nothing playing: open My Stations (Shift+S) to pick one".into();
                }
                // Last, so autoplay's "Tuning in to …" does not hide it.
                if pruned > 0 {
                    self.status = match pruned {
                        1 => "A favorite was removed: that station is gone from Pandora".into(),
                        n => format!("{n} favorites were removed: those stations are gone from Pandora"),
                    };
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
                if let Some(d) = self.details_mut().filter(|d| d.station_id == station_id) {
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
            Reply::Browse { path, sections } => self.browse_loaded(path, sections),
            Reply::BrowseFailed { path, message } => {
                let index = path.is_none();
                self.note_browse_failure(path, message.clone());
                if index {
                    self.status = format!("⚠ browse: {message}");
                }
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
                if self.buffering.as_ref().is_some_and(|(t, _)| *t == track_token) {
                    self.buffering = None;
                    if !self.play_now(Some(data)) {
                        // The audio would not start: skip on, as `next_song` does.
                        self.advance();
                    }
                } else if self.queue.iter().any(|s| s.track_token == track_token) {
                    self.keep_audio(track_token, data);
                }
            }
            Reply::Art { url, image } => self.art.arrived(url, image),
            Reply::AudioFailed { track_token, message } => {
                if self.ad_audio_failed(&track_token, &message) {
                    return;
                }
                self.requested.remove(&track_token);
                if self.buffering.as_ref().is_some_and(|(t, _)| *t == track_token) {
                    crate::log::log!("download: failed for {track_token}: {message}");
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
            Reply::Recent { generation, sources } => {
                if !self.accept_recent(generation) {
                    return;
                }
                // A new row is inserted above Pinned. Stay on the menu entry
                // the cursor was already on, and on the same recent source.
                let had = self.recent_row();
                let selected = self.menu_selected();
                let keep = self.recent.get(self.recent_cursor).map(|s| s.id.clone());
                self.recent = self.filter_recent(sources);
                if let Some(id) = keep
                    && let Some(at) = self.recent.iter().position(|s| s.id == id)
                {
                    self.recent_cursor = at;
                } else {
                    self.recent_cursor = self.recent_cursor.min(self.recent.len().saturating_sub(1));
                }
                if !had && self.recent_row() {
                    let items = self.menu_items();
                    if let Some(i) = items.iter().position(|m| *m == selected) {
                        self.menu_cursor = i;
                    }
                }
                self.finish_recent_request();
            }
            Reply::RecentFailed { generation, message } => {
                if !self.accept_recent(generation) {
                    return;
                }
                self.status = format!("⚠ Recently Played: {message}");
                self.finish_recent_request();
            }
            Reply::Failed { what, message } => {
                if self.editor_failed(what, &message) || self.panel_failed(what, &message) {
                    return;
                }
                match what {
                    Failed::RecentlyPlayed => {
                        if self.recent_state.loading() {
                            self.status = format!("⚠ Recently Played: {message}");
                            self.finish_recent_request();
                        }
                        return;
                    }
                    Failed::Shuffle => {
                        self.status = format!("⚠ No Shuffle: {message}");
                        return;
                    }
                    Failed::Ads => {
                        self.ad_ask_failed(&message);
                        return;
                    }
                    Failed::Playlist => self.playlist_pending = false,
                    Failed::Search => {
                        if let Some(search) = self.add_station.as_mut() {
                            search.waiting = false;
                            search.error = Some(message);
                            return;
                        }
                    }
                    Failed::CreateStation => self.creating = false,
                    Failed::Details => {
                        if let Some(d) = self.details_mut().filter(|d| d.from_pandora.is_none()) {
                            d.from_pandora = Some(Err(message.clone()));
                            return;
                        }
                    }
                    _ => {}
                }
                self.status = format!("⚠ {}: {message}", what.what());
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
            // No account name here: `status` is kept for the Notifications page
            // and is the one string that could reach a log, so it stays free of
            // anything personal. Settings shows which account is signing in.
            self.status = "Signing in…".into();
        }
        self.backend.send(Request::Login { username, password });
    }

    /// Opens the password box (Settings, on Log in).
    fn ask_password(&mut self) {
        self.view = View::Settings;
        self.focus = Focus::Content;
        self.open_prompt(Prompt::Password);
        self.settings_cursor = Setting::at(Setting::LogIn);
        // The account is on the Settings row above, so it is not repeated here.
        self.status = "Type your password".into();
    }

    /// The password popup with `typed` already in it (the snapshot screens).
    pub(crate) fn ask_password_for_snapshot(&mut self, typed: &str) {
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
                if std::mem::take(&mut self.pending_password).in_file() {
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
                self.pending_password.done();
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
        self.pending_password.done();
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
        self.stream = None;
        // The next account's plan is not known until it logs in, so Audio
        // quality must not stay greyed out from this one.
        self.hifi = None;
        self.queue.clear();
        self.history.clear();
        self.stations.clear();
        // A dislike held on an Up Next row belongs to the account that held it.
        self.held_dislikes.clear();
        self.forget_recent();
        self.current_station = None;
        self.menu_cursor = 0;
        self.view = View::Settings;
        self.session = Session::LoggedOut;
        self.forget_saved_password(false);
        self.config.pandora.password.clear();
        self.pending_password.done();
        self.status = if self.demo || self.keychain_state == KeychainState::Unavailable {
            "Logged out".into()
        } else {
            "Logged out: the saved password was removed from your keychain".into()
        };
    }

    /// The player's panes have the keys.
    /// A popup is drawn over the screen (Help, the quit box, the station boxes, details).
    pub fn overlay_open(&self) -> bool {
        self.dialogs.is_open()
            || self.genre_notice.is_some()
            || self.confirm_good_vibes.is_some()
            || self.rename.is_some()
            || self.add_station.is_some()
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

    /// The Audio quality row: the choice, and what Pandora is really sending.
    /// The plan's flag says what it should send; a free plan gets 64 kbps
    /// HE-AAC whatever `high` asks for, so the stream is what is shown once
    /// a song has arrived.
    fn quality_label(&self) -> String {
        // A free plan has no choice to show: the row is greyed out, and this
        // says what Pandora sends rather than what was asked for.
        if self.quality_greyed() {
            return match self.stream {
                Some((crate::pandora::AudioFormat::Aac, kbps)) if kbps > 0 => {
                    format!("your plan: AAC {kbps} kbps")
                }
                _ => "your plan: 64 kbps HE-AAC".into(),
            };
        }
        let choice = Quality::parse(&self.config.pandora.audio_quality);
        let asked = match (choice, self.hifi) {
            (Quality::High, Some(true)) => "high (MP3 192 kbps)",
            (Quality::High, None) => "high (MP3 192 kbps if your plan has it)",
            (Quality::Standard, _) => "standard (MP3 128 kbps)",
            // `Some(false)` returned above.
            (Quality::High, Some(false)) => "high",
        };
        match self.stream {
            Some((format, kbps)) if kbps > 0 => {
                let what = match format {
                    crate::pandora::AudioFormat::Mp3 => "MP3",
                    crate::pandora::AudioFormat::Aac => "AAC",
                    crate::pandora::AudioFormat::Unknown => "audio",
                };
                format!("{asked} · playing {what} {kbps} kbps")
            }
            _ => asked.into(),
        }
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
                _ if self.pending_password.needs_saving() => "saving to your keychain…".into(),
                _ => "not saved".into(),
            },
            Setting::LogIn => match &self.session {
                Session::LoggedOut => "not logged in".into(),
                Session::SigningIn => "signing in…".into(),
                Session::LoggedIn => "logged in".into(),
                Session::Failed(e) => e.clone(),
            },
            Setting::LogOut => String::new(),
            Setting::Quality => self.quality_label(),
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
            Setting::BrowseSection(i) => {
                let title = config::BROWSE_SECTIONS[i as usize].1;
                on(self.config.browse.pinned_on_menu(title))
            }
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
            Setting::Quality if self.quality_greyed() => {
                self.status = "Audio quality is Pandora's on a free plan: 64 kbps HE-AAC".into();
            }
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
            Setting::BrowseSection(i) => {
                let (key, title) = config::BROWSE_SECTIONS[i as usize];
                let on = !self.config.browse.pinned_on_menu(title);
                self.config.browse.set_pinned(title, on);
                self.save(&[("browse", key, on.into())]);
                self.status = format!("{title} on main menu: {}", if on { "on" } else { "off" });
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
                    self.pending_password.read_from_file();
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
                        self.pending_password.typed();
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
        // Dialogs (questions, Help, details, System Check) are on top: they get the keys.
        if self.dialog_key(key) {
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
            Cmd::Quit => self.ask_quit(),
            Cmd::Help => self.open_help(),
            Cmd::Pause => self.audio.toggle_pause(),
            Cmd::Skip => self.skip(),
            Cmd::NowPlaying => self.show_now_playing(),
            Cmd::Stations => self.show_stations(),
            Cmd::Genres => self.show_browse_section("Browse Genres"),
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
                if self.browse_back() {
                    return;
                }
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
            _ => format!("{what} works on a station: open My Stations (Shift+S) or select a favorite"),
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
                self.ask_unpin(self.pinned_cursor.min(self.pins.len() - 1));
            }
            return;
        }
        if self.focus == Focus::Content && self.view == View::Recent && !self.recent.is_empty() {
            self.ask_forget_recent(self.recent_cursor.min(self.recent.len().saturating_sub(1)));
            return;
        }
        let Some((station, favorite)) = self.station_in_reach() else {
            return self.no_station_here("x deletes");
        };
        if !station.allow_delete {
            self.blocked(&station, "deleted", format!("Pandora doesn't let you delete “{}”", station.name));
            return;
        }
        self.ask_delete_station(StationRef { id: station.id, name: station.name, favorite });
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
                    "No song to show details of: pick a station, or open My Stations (Shift+S) for a station's".into();
                return;
            };
            return self.show_song_details(song, false);
        };
        if details.station_id.is_empty() {
            self.open_details(Details { from_pandora: Some(Err("no station for this song".into())), ..details });
            return;
        }
        let current = self.current_station.as_deref() == Some(details.station_id.as_str());
        self.backend.send(Request::StationDetails { station_id: details.station_id.clone(), current });
        self.open_details(details);
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
            (Focus::Content, View::Browse) => self.activate_browse_row(),
            (Focus::Content, View::Pinned) => self.open_pin(),
            (Focus::Content, View::Recent) => self.play_recent(),
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
            (Focus::Content, View::Browse) => {
                let rows = self.browse_rows();
                let len = rows.len();
                if len == 0 {
                    return;
                }
                let mut at = step(self.browse_index(), len, by);
                // Prefer landing on an item, not a section header.
                for _ in 0..len {
                    if matches!(rows[at], BrowseRow::Item(_)) {
                        break;
                    }
                    at = step(at, len, if by < 0 { -1 } else { 1 });
                }
                self.browse_cursor = at;
            }
            (Focus::Content, View::About) => {}
            (Focus::Content, View::Notifications) => {
                self.notes_cursor = clamp(self.notes_cursor, self.notes.len().max(1))
            }
            (Focus::Content, View::Pinned) => self.pinned_cursor = clamp(self.pinned_cursor, self.pins.len().max(1)),
            (Focus::Content, View::Recent) => self.recent_cursor = clamp(self.recent_cursor, self.recent.len().max(1)),
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
        self.want_art();
        self.save(&[("look", "album_art", next.as_str().into())]);
        self.status = format!("Album art: {}", next.label());
    }

    /// Settings, and Shift+R: the next retro level.
    fn next_retro_mode(&mut self) {
        let next = self.config.ui.retro_mode.next();
        self.config.ui.retro_mode = next;
        self.save(&[("look", "retro_mode", next.as_str().into())]);
        self.status = format!("Retro mode: {}", next.label());
    }

    /// Settings, and Shift+H: off, duotone or palette.
    fn next_art_tint(&mut self) {
        let next = self.config.ui.art_tint.next();
        self.config.ui.art_tint = next;
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

    /// Audio quality can't be chosen on a free plan: Pandora sends 64 kbps
    /// HE-AAC whatever is asked for, so `high` and `standard` both request
    /// `mp3` and the toggle would change nothing. `None` (not logged in yet)
    /// leaves it usable, since the plan is not known.
    pub fn quality_greyed(&self) -> bool {
        self.hifi == Some(false)
    }

    /// Whether a setting is shown greyed out because it can't be used now.
    pub fn setting_greyed(&self, setting: Setting) -> bool {
        match setting {
            Setting::ThemeSelect => self.theme_select_greyed(),
            Setting::Quality => self.quality_greyed(),
            _ => false,
        }
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

    /// For the snapshot screens and the tests: play a few demo songs so History
    /// and the clock have something to show.
    pub(crate) fn demo_warm_up(&mut self, skips: usize, at: Duration) {
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
mod tests;
