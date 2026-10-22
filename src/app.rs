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
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::audio::Audio;
use crate::backend::{Backend, Reply, Request};
use crate::config::{self, Config, ConfigWarning};
use crate::pandora::{Quality, Rating, Song, Station};
use crate::theme::{self, Palette};
use crate::update::{Progress, Replace, Update};

/// What the right side shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Player,
    /// Every station, to pick one (`Enter`) or star it (`f`).
    Stations,
    Settings,
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
    /// A starred station, by its index in `[ui] favorites`.
    Favorite(usize),
    Settings,
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
    Theme,
    Icons,
    BorderStyle,
    MenuWidth,
    NowPlayingSize,
    CheckUpdates,
    EditFile,
}

impl Setting {
    /// In screen order, each with its group.
    pub const ALL: [(&'static str, Self); 15] = [
        ("ACCOUNT", Self::Email),
        ("ACCOUNT", Self::Password),
        ("ACCOUNT", Self::LogIn),
        ("ACCOUNT", Self::LogOut),
        ("PLAYBACK", Self::Quality),
        ("PLAYBACK", Self::Volume),
        ("PLAYBACK", Self::Autoplay),
        ("PLAYBACK", Self::Notify),
        ("LOOK", Self::Theme),
        ("LOOK", Self::Icons),
        ("LOOK", Self::BorderStyle),
        ("LOOK", Self::MenuWidth),
        ("LOOK", Self::NowPlayingSize),
        ("BOXVIBE", Self::CheckUpdates),
        ("BOXVIBE", Self::EditFile),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Email => "Email or username",
            Self::Password => "Password",
            Self::LogIn => "Log in",
            Self::LogOut => "Log out",
            Self::Quality => "Audio quality",
            Self::Volume => "Volume",
            Self::Autoplay => "Autoplay last station on open",
            Self::Notify => "Song change notifications",
            Self::Theme => "Theme",
            Self::Icons => "Icons",
            Self::BorderStyle => "Border style",
            Self::MenuWidth => "Menu width",
            Self::NowPlayingSize => "Now Playing size",
            Self::CheckUpdates => "Look for new versions on startup",
            Self::EditFile => "Edit config.toml",
        }
    }

    /// One line on what the setting does, under the list.
    pub fn about(self) -> &'static str {
        match self {
            Self::Email => "What you log in to pandora.com with. Enter types it.",
            Self::Password => "Saved in config.toml, a file only you can read (mode 600). Enter types it.",
            Self::LogIn => "Logs in to Pandora with the email and password above and loads your stations.",
            Self::LogOut => "Stops the music and removes the saved password from config.toml.",
            Self::Quality => {
                "high is 192 kbps MP3 where your plan has it (Plus, Premium), else 128 kbps; standard is 128."
            }
            Self::Volume => "Also ← and → anywhere.",
            Self::Autoplay => {
                "When boxvibe opens and logs in, the station you played last starts by itself. Saved as [ui] autoplay_last_station."
            }
            Self::Notify => {
                "A desktop notification (notify-send, mako on Omarchy) for every new song. Saved as [ui] notifications."
            }
            Self::Theme => "Omarchy follows your current Omarchy theme; or pick a built-in one. Also t.",
            Self::Icons => "Emoji, Nerd Font (needs one installed) or plain Unicode. Also i.",
            Self::BorderStyle => "How every pane's border is drawn: plain, rounded, thick, double or none.",
            Self::MenuWidth => "The menu's width, 20 to 40 columns (never more than half the screen). Also { and }.",
            Self::NowPlayingSize => {
                "Rows for the Now Playing pane, 9 to 25; Up Next and History get the rest. Saved as [ui] now_playing_size."
            }
            Self::CheckUpdates => "Asks GitHub when boxvibe opens; About shows the new version and Update Now.",
            Self::EditFile => "Opens config.toml in $EDITOR; boxvibe reads it again when you close it.",
        }
    }
}

/// The two song panes under Now Playing; Tab goes menu → Up Next → History.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    /// The Now Playing card itself (`N` puts the keys here).
    NowPlaying,
    UpNext,
    History,
}

pub struct App {
    /// The song pane `U` / `D` rate in when Now Playing or the menu has the keys.
    pub last_song_pane: Pane,
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
    /// Which song pane has the keys while the player has them.
    pub pane: Pane,
    /// Selected row of Up Next (0 = the next song).
    pub next_cursor: usize,
    /// Selected row of History (0 = the last song played).
    pub history_cursor: usize,
    pub prompt: Option<Prompt>,
    pub query: String,
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
    pub buffering: Option<String>,
    audio_cache: HashMap<String, Vec<u8>>,
    requested: HashSet<String>,
    playlist_pending: bool,
    pub audio: Audio,
    backend: Backend,
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

const HISTORY_LEN: usize = 30;

impl App {
    pub fn new(config: Config, config_path: Option<PathBuf>, warnings: &[ConfigWarning], demo: bool) -> Self {
        let quality = Quality::parse(&config.pandora.audio_quality);
        let audio = Audio::open(demo, config.player.volume);
        let backend = Backend::spawn(demo, quality, config.pandora.user_agent.clone());
        let mut app = Self {
            session: Session::LoggedOut,
            view: View::Player,
            focus: Focus::Menu,
            menu_cursor: 0,
            settings_cursor: 0,
            stations_cursor: 0,
            pane: Pane::UpNext,
            last_song_pane: Pane::UpNext,
            next_cursor: 0,
            history_cursor: 0,
            prompt: None,
            query: String::new(),
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
            buffering: None,
            audio_cache: HashMap::new(),
            requested: HashSet::new(),
            playlist_pending: false,
            audio,
            backend,
            update: None,
            up_to_date: false,
            self_update: Progress::Idle,
            edit_config: false,
            hifi: None,
            restart: false,
            confirm_quit: false,
            quit: false,
            config,
            config_path,
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
            if app.config.has_login() {
                app.log_in();
            } else {
                // First run: Settings, on the account, typing the email.
                app.view = View::Settings;
                app.focus = Focus::Content;
                app.settings_cursor = 0;
                app.open_prompt(if app.config.pandora.username.is_empty() { Prompt::Email } else { Prompt::Password });
                app.status = "Welcome to boxvibe: log in with your Pandora account".into();
            }
        }
        app
    }

    pub fn follows_omarchy(&self) -> bool {
        theme::built_in(&self.config.ui.theme).is_none()
    }

    fn apply_theme(&mut self) {
        match theme::built_in(&self.config.ui.theme) {
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
            .chain([MenuItem::Stations])
            .chain((0..self.config.ui.favorites.len()).map(MenuItem::Favorite))
            .chain([MenuItem::Settings, MenuItem::About, MenuItem::Quit])
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
            MenuItem::Favorite(i) => {
                let Some(fav) = self.config.ui.favorites.get(i).cloned() else { return };
                match self.stations.iter().position(|s| s.id == fav.id) {
                    Some(at) => {
                        // Plays it and lands on Now Playing, cursor on NOW PLAYING.
                        self.view = View::Player;
                        self.focus = Focus::Menu;
                        self.tune(at);
                        self.point_menu_at_view();
                    }
                    None if self.stations.is_empty() => self.status = "The stations are still loading".into(),
                    None => self.status = format!("{} isn't one of your stations any more (f removes it)", fav.name),
                }
            }
            MenuItem::Settings => {
                self.view = View::Settings;
                self.focus = Focus::Content;
            }
            MenuItem::About => {
                self.view = View::About;
                self.focus = Focus::Content;
            }
            MenuItem::Quit => self.quit = true,
        }
    }

    /// NOW PLAYING (menu entry or `N`): the player, with the menu on it.
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
    }

    /// Stations (menu entry or `s`): the list, with the keys on it.
    fn show_stations(&mut self) {
        self.view = View::Stations;
        self.focus = Focus::Content;
        if let Some(at) = self.current_station.as_ref().and_then(|id| self.stations.iter().position(|s| &s.id == id)) {
            self.stations_cursor = at;
        }
    }

    pub fn stations_index(&self) -> usize {
        self.stations_cursor.min(self.stations.len().saturating_sub(1))
    }

    pub fn is_favorite(&self, id: &str) -> bool {
        self.config.ui.favorites.iter().any(|f| f.id == id)
    }

    /// `f`: stars or unstars the station under the cursor (the list, a
    /// favorite in the menu) or else the one playing; saved as `[ui] favorites`.
    fn toggle_favorite(&mut self) {
        let (id, name) = match (self.focus, self.view, self.menu_selected()) {
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
        self.save(&[("ui", "favorites", toml_edit::Value::Array(list))]);
    }

    /// The menu entry of what the right side shows.
    fn point_menu_at_view(&mut self) {
        let items = self.menu_items();
        let want = match self.view {
            View::Settings => Some(MenuItem::Settings),
            View::About => Some(MenuItem::About),
            View::Stations => Some(MenuItem::Stations),
            View::Player if self.has_now_playing() => Some(MenuItem::NowPlaying),
            View::Player => None,
        };
        if let Some(i) = want.and_then(|w| items.iter().position(|m| *m == w)) {
            self.menu_cursor = i;
        }
    }

    // --- Every frame ---

    pub fn tick(&mut self) {
        while let Ok(reply) = self.backend.replies.try_recv() {
            self.on_reply(reply);
        }
        if self.now.as_ref().is_some_and(|s| self.audio.finished(s.length)) {
            self.advance();
        }
        if self.follows_omarchy() && self.last_theme_poll.elapsed() > Duration::from_secs(2) {
            self.last_theme_poll = Instant::now();
            if let Some(p) = self.omarchy.poll() {
                self.palette = p;
            }
        }
    }

    pub fn settle(&mut self, timeout: Duration) {
        while let Ok(reply) = self.backend.replies.recv_timeout(timeout) {
            self.on_reply(reply);
        }
    }

    fn on_reply(&mut self, reply: Reply) {
        match reply {
            Reply::LoggedIn { hifi } => {
                self.hifi = Some(hifi);
                let first = self.session != Session::LoggedIn;
                self.session = Session::LoggedIn;
                if !self.demo {
                    let (email, password) =
                        (self.config.pandora.username.clone(), self.config.pandora.password.clone());
                    self.save(&[
                        ("pandora", "username", email.as_str().into()),
                        ("pandora", "password", password.as_str().into()),
                    ]);
                    self.status = format!("Logged in as {email}");
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
                    // A wrong password is not kept, here or in config.toml.
                    self.config.pandora.password.clear();
                    self.save(&[("pandora", "password", "".into())]);
                }
                self.session = Session::Failed(message.clone());
                self.status = format!("⚠ Login failed: {message}");
                self.view = View::Settings;
                self.focus = Focus::Content;
                self.settings_cursor = 1;
            }
            Reply::Stations(stations) => {
                self.stations = stations;
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
                    self.status = "Nothing playing: open Stations (s) to pick one".into();
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
                if self.now.is_none() {
                    self.advance();
                } else {
                    self.prefetch();
                }
            }
            Reply::Rated { track_token, positive } => {
                let ic = crate::icons::icons(self.config.ui.icons);
                let what = if positive { format!("{} Liked", ic.up) } else { format!("{} Disliked", ic.down) };
                let title = self.find_song(&track_token).map(|s| s.title.clone()).unwrap_or_default();
                self.status = format!("{what}: {title}");
            }
            Reply::TookOver => {
                self.status = "Pandora was playing on another device with this account: boxvibe took over".into();
            }
            Reply::Tired { track_token } => {
                let title = self.find_song(&track_token).map(|s| s.title.clone()).unwrap_or_default();
                let sleep = crate::icons::icons(self.config.ui.icons).sleep;
                self.status = format!("{sleep} Tired of {title}: Pandora won't play it for a month");
            }
            Reply::Audio { track_token, data } => {
                if self.buffering.as_deref() == Some(&track_token) {
                    self.buffering = None;
                    self.play_now(Some(data));
                } else if self.queue.iter().any(|s| s.track_token == track_token) {
                    self.audio_cache.insert(track_token, data);
                }
            }
            Reply::AudioFailed { track_token, message } => {
                self.requested.remove(&track_token);
                if self.buffering.as_deref() == Some(&track_token) {
                    self.buffering = None;
                    self.status = format!("⚠ {message}: skipping");
                    self.advance();
                }
            }
            Reply::Failed { what, message } => {
                if what == "playlist" {
                    self.playlist_pending = false;
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
            (self.config.pandora.username.trim().to_owned(), self.config.pandora.password.clone())
        };
        if !self.demo && (username.is_empty() || password.is_empty()) {
            self.status = "⚠ Type your Pandora email and password first".into();
            self.settings_cursor = if username.is_empty() { 0 } else { 1 };
            return;
        }
        self.session = Session::SigningIn;
        if !self.demo {
            self.status = format!("Signing in as {username}…");
        }
        self.backend.send(Request::Login { username, password });
    }

    fn log_out(&mut self) {
        self.audio.stop();
        self.now = None;
        self.queue.clear();
        self.history.clear();
        self.stations.clear();
        self.current_station = None;
        self.menu_cursor = 0;
        self.view = View::Settings;
        self.session = Session::LoggedOut;
        self.config.pandora.password.clear();
        self.save(&[("pandora", "password", "".into())]);
        self.status = "Logged out: the password was removed from config.toml".into();
    }

    fn tune(&mut self, index: usize) {
        let Some(station) = self.stations.get(index).cloned() else { return };
        if let Some(song) = self.now.take() {
            self.push_history(song);
        }
        self.audio.stop();
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
            self.save(&[("pandora", "last_station", station.id.as_str().into())]);
        }
        self.status = format!("Tuning in to {}", station.name);
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
        match self.audio.start(data) {
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
            Setting::Password if self.demo || !self.config.pandora.password.is_empty() => "•••••••• saved".into(),
            Setting::Password => "not set".into(),
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
            Setting::Theme => theme::label(&self.config.ui.theme),
            Setting::Icons => crate::icons::label(self.config.ui.icons).into(),
            Setting::BorderStyle => self.config.ui.border_style.label().into(),
            Setting::MenuWidth => format!("{} columns", self.config.ui.menu_width),
            Setting::NowPlayingSize => format!("{} rows", self.config.ui.now_playing_size),
            Setting::CheckUpdates => on(self.config.ui.check_updates),
            Setting::EditFile => self
                .config_path
                .as_ref()
                .map(|p| p.display().to_string().replace(&std::env::var("HOME").unwrap_or_default(), "~"))
                .unwrap_or_default(),
        }
    }

    fn open_prompt(&mut self, prompt: Prompt) {
        self.query = match prompt {
            Prompt::Email => self.config.pandora.username.clone(),
            Prompt::Password => String::new(),
        };
        self.settings_cursor = if prompt == Prompt::Email { 0 } else { 1 };
        self.prompt = Some(prompt);
    }

    fn change_setting(&mut self) {
        match self.setting() {
            Setting::Email => self.open_prompt(Prompt::Email),
            Setting::Password => self.open_prompt(Prompt::Password),
            Setting::LogIn => self.log_in(),
            Setting::LogOut => self.log_out(),
            Setting::Quality => {
                let next = Quality::parse(&self.config.pandora.audio_quality).next();
                self.config.pandora.audio_quality = next.as_str().into();
                self.backend.send(Request::Quality(next));
                self.save(&[("pandora", "audio_quality", next.as_str().into())]);
                self.status = format!("Audio quality {}: from the next songs Pandora sends", next.as_str());
            }
            Setting::Volume => {
                let v = self.config.player.volume;
                self.change_volume(if v >= 100 { -100 } else { 10 });
            }
            Setting::Autoplay => {
                let on = !self.config.ui.autoplay_last_station;
                self.config.ui.autoplay_last_station = on;
                self.save(&[("ui", "autoplay_last_station", on.into())]);
                // boxvibe 0.1's name for it goes.
                if !self.demo
                    && let Some(path) = &self.config_path
                    && let Err(e) = config::remove_value(path, "player", "autoplay")
                {
                    self.status = format!("⚠ {e}");
                } else {
                    self.status = format!("Autoplay last station on open: {}", if on { "on" } else { "off" });
                }
            }
            Setting::Notify => {
                let on = !self.config.ui.notifications;
                self.config.ui.notifications = on;
                self.save(&[("ui", "notifications", on.into())]);
                if !self.demo
                    && let Some(path) = &self.config_path
                    && let Err(e) = config::remove_value(path, "ui", "notify")
                {
                    self.status = format!("⚠ {e}");
                } else {
                    self.status = format!("Song change notifications {}", if on { "on" } else { "off" });
                }
            }
            Setting::Theme => self.next_theme(),
            Setting::Icons => self.next_icons(),
            Setting::BorderStyle => {
                let next = self.config.ui.border_style.next();
                self.config.ui.border_style = next;
                self.save(&[("ui", "border_style", next.as_str().into())]);
                self.status = format!("Border style: {}", next.label());
            }
            Setting::MenuWidth => {
                let next = config::MENU_WIDTH.next(self.config.ui.menu_width);
                self.config.ui.menu_width = next;
                self.save(&[("ui", "menu_width", i64::from(next).into())]);
                self.status = format!("Menu width: {next} columns");
            }
            Setting::NowPlayingSize => {
                let next = config::NOW_PLAYING_SIZE.next(self.config.ui.now_playing_size);
                self.config.ui.now_playing_size = next;
                self.save(&[("ui", "now_playing_size", i64::from(next).into())]);
                self.status = format!("Now Playing size: {next} rows");
            }
            Setting::CheckUpdates => {
                self.config.ui.check_updates = !self.config.ui.check_updates;
                self.save(&[("ui", "check_updates", self.config.ui.check_updates.into())]);
            }
            Setting::EditFile => {
                if self.demo {
                    self.status = "The demo doesn't touch config.toml".into();
                } else {
                    self.edit_config = true;
                }
            }
        }
    }

    /// After the editor closes: read config.toml again.
    pub fn reload_config(&mut self) {
        let Some(path) = self.config_path.clone() else { return };
        // A file broken in the editor keeps the settings in use (and the file as it is).
        match Config::read(&path) {
            Ok((config, warnings)) => {
                let login_changed = config.pandora.username != self.config.pandora.username
                    || config.pandora.password != self.config.pandora.password;
                self.config = config;
                self.apply_theme();
                self.audio.set_volume(self.config.player.volume);
                self.backend.send(Request::Quality(Quality::parse(&self.config.pandora.audio_quality)));
                self.status = ConfigWarning::summary(&warnings).unwrap_or_else(|| "config.toml read again".into());
                if login_changed && self.config.has_login() {
                    self.log_in();
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
            KeyCode::Backspace => {
                self.query.pop();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => self.query.clear(),
            KeyCode::Char(c) => self.query.push(c),
            KeyCode::Enter | KeyCode::Tab => {
                let text = std::mem::take(&mut self.query);
                self.prompt = None;
                match prompt {
                    Prompt::Email => {
                        let email = text.trim().to_owned();
                        self.config.pandora.username = email.clone();
                        self.save(&[("pandora", "username", email.as_str().into())]);
                        // Then the password, like a login form.
                        if self.config.pandora.password.is_empty() {
                            self.open_prompt(Prompt::Password);
                        }
                    }
                    Prompt::Password if text.is_empty() => {}
                    Prompt::Password => {
                        // Saved once Pandora takes it (see LoggedIn).
                        self.config.pandora.password = text;
                        self.settings_cursor = 2;
                        self.log_in();
                    }
                }
            }
            _ => {}
        }
    }

    // --- Keys ---

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.confirm_quit {
            // y quits; any other key is no, as in tubevibe.
            self.confirm_quit = false;
            self.quit = matches!(key.code, KeyCode::Char('y' | 'Y'));
            return;
        }
        if self.prompt.is_some() {
            return self.prompt_key(key);
        }
        if self.help {
            // The title says "Any Key Closes", so any key does.
            self.help = false;
            return;
        }
        match key.code {
            // Only Shift+Q quits, so a stray lowercase q can't end playback.
            KeyCode::Char('Q') => self.confirm_quit = true,
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char(' ') | KeyCode::Char('p') => self.audio.toggle_pause(),
            KeyCode::Char('n') => {
                if self.now.is_some() {
                    self.status = "Skipped".into();
                    self.advance();
                }
            }
            KeyCode::Char('+') | KeyCode::Char('u') | KeyCode::Char('=') => self.rate(true),
            KeyCode::Char('-') | KeyCode::Char('d') => self.rate(false),
            KeyCode::Char('U') => self.rate_row(true),
            KeyCode::Char('D') => self.rate_row(false),
            KeyCode::Char('z') => self.tired(),
            KeyCode::Char('N') => self.show_now_playing(),
            KeyCode::Char('s') => {
                self.show_stations();
            }
            KeyCode::Char('f') | KeyCode::Char('F') => self.toggle_favorite(),
            KeyCode::Tab => self.next_focus(true),
            KeyCode::BackTab => self.next_focus(false),
            KeyCode::Esc => {
                self.focus = Focus::Menu;
                self.point_menu_at_view();
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_cursor(-1),
            KeyCode::Char('g') | KeyCode::Home => self.move_cursor(-1000),
            KeyCode::Char('G') | KeyCode::End => self.move_cursor(1000),
            KeyCode::Enter => self.enter(),
            // Volume: Shift+← / Shift+→ (plain arrows are left free).
            KeyCode::Right if key.modifiers.contains(KeyModifiers::SHIFT) => self.change_volume(5),
            KeyCode::Left if key.modifiers.contains(KeyModifiers::SHIFT) => self.change_volume(-5),
            KeyCode::Char('{') => self.resize_menu(-1),
            KeyCode::Char('}') => self.resize_menu(1),
            KeyCode::Char('t') => self.next_theme(),
            KeyCode::Char('i') => self.next_icons(),
            _ => {}
        }
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
            if matches!(self.pane, Pane::UpNext | Pane::History) && self.focus == Focus::Content {
                self.last_song_pane = self.pane;
            }
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
            (Focus::Content, View::Stations) => {
                if self.stations.is_empty() {
                    return;
                }
                self.view = View::Player;
                self.focus = Focus::Menu;
                self.tune(self.stations_index());
                self.point_menu_at_view();
            }
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
                self.stations_cursor = clamp(self.stations_index(), self.stations.len().max(1))
            }
            (Focus::Content, View::About) => {}
        }
    }

    /// `{` / `}`: the menu a step narrower or wider, saved as `[ui] menu_width`.
    fn resize_menu(&mut self, dir: i8) {
        let next = config::MENU_WIDTH.step_by(self.config.ui.menu_width, dir);
        self.config.ui.menu_width = next;
        self.save(&[("ui", "menu_width", i64::from(next).into())]);
        self.status = format!("Menu width: {next} columns ({{ and }} change it)");
    }

    fn next_theme(&mut self) {
        let next = theme::next(&self.config.ui.theme);
        self.config.ui.theme = next.into();
        self.apply_theme();
        self.save(&[("ui", "theme", next.into())]);
        self.status = format!("Theme: {}", theme::label(next));
    }

    fn next_icons(&mut self) {
        let next = self.config.ui.icons.next();
        self.config.ui.icons = next;
        self.save(&[("ui", "icons", next.as_str().into())]);
        self.status = format!("Icons: {}", crate::icons::label(next));
    }

    fn change_volume(&mut self, by: i16) {
        let volume = (i16::from(self.config.player.volume) + by).clamp(0, 100) as u8;
        self.config.player.volume = volume;
        self.audio.set_volume(volume);
        self.save(&[("player", "volume", i64::from(volume).into())]);
        self.status = format!("Volume {volume}%");
    }

    /// `u`/`+` and `d`/`-`: always the playing song, whatever has the keys.
    fn rate(&mut self, positive: bool) {
        let Some(song) = self.now.as_mut() else {
            self.status = "Nothing is playing to rate".into();
            return;
        };
        if song.station_id.is_empty() || song.track_token.is_empty() {
            self.status = "Pandora can't rate this song (no track token)".into();
            return;
        }
        song.rating = if positive { Rating::Up } else { Rating::Down };
        let request = Request::Rate { track_token: song.track_token.clone(), positive };
        self.backend.send(request);
        // Like Pandora: a thumbs down on the playing song skips it.
        if !positive {
            self.advance();
        }
    }

    /// `U` / `D`: the highlighted row of Up Next or History (the focused one,
    /// else the last one that had the keys). Pandora's addFeedback takes any
    /// track token from the station's playlist, played or not.
    fn rate_row(&mut self, positive: bool) {
        let pane = if self.in_songs() && self.pane != Pane::NowPlaying { self.pane } else { self.last_song_pane };
        let (at, name) = match pane {
            Pane::History => (self.history_index(), "History"),
            _ => (self.next_index(), "Up Next"),
        };
        let song = if pane == Pane::History { self.history.get_mut(at) } else { self.queue.get_mut(at) };
        let Some(song) = song else {
            self.status = format!("{name} is empty: no song to rate (U / D rate its highlighted row)");
            return;
        };
        if song.station_id.is_empty() || song.track_token.is_empty() {
            self.status = format!("Pandora can't rate “{}” (no track token)", song.title);
            return;
        }
        song.rating = if positive { Rating::Up } else { Rating::Down };
        self.backend.send(Request::Rate { track_token: song.track_token.clone(), positive });
        // A thumbs down on a song still to come drops it, as skipping would.
        if !positive && pane == Pane::UpNext {
            let Some(gone) = self.queue.remove(at) else { return };
            self.requested.remove(&gone.track_token);
            self.audio_cache.remove(&gone.track_token);
            self.status = format!("Disliked and dropped from Up Next: {}", gone.title);
        }
    }

    fn tired(&mut self) {
        let Some(song) = &self.now else { return };
        self.backend.send(Request::Tired { track_token: song.track_token.clone() });
        self.advance();
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
            assert_eq!((app.view, app.focus, app.menu_selected()), (View::Player, Focus::Menu, MenuItem::NowPlaying));
        }
    }

    #[test]
    fn enter_on_a_station_plays_it_and_shows_now_playing() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.settle(Duration::from_millis(150));
        app.on_key(key(KeyCode::Char('s')));
        app.on_key(key(KeyCode::Char('j')));
        let want = app.stations[app.stations_index()].id.clone();
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.current_station.as_deref(), Some(want.as_str()));
        assert_eq!((app.view, app.menu_selected()), (View::Player, MenuItem::NowPlaying));
    }

    #[test]
    fn startup_stays_on_an_idle_now_playing() {
        let mut config = Config::default();
        config.ui.autoplay_last_station = false;
        let mut app = App::new(config, None, &[], true);
        app.settle(Duration::from_millis(150));
        assert_eq!(app.view, View::Player);
        assert!(app.now.is_none() && app.current_station.is_none());
        app.on_key(key(KeyCode::Char('s')));
        assert_eq!(app.view, View::Stations, "s opens it by hand");
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
        let mut app = App::new(Config::default(), None, &[], true);
        app.demo_warm_up(2, Duration::ZERO);
        app
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
        assert_eq!(app.stations_index(), 0, "stations wrap too");
    }

    #[test]
    fn menu_has_stations_then_settings_about_quit() {
        let app = demo();
        let items = app.menu_items();
        assert_eq!(items[0], MenuItem::NowPlaying, "a station plays: NOW PLAYING on top");
        assert_eq!(items[1], MenuItem::Stations);
        assert_eq!(items.len(), 5, "no favorites yet: no FAVORITES entries");
        assert_eq!(&items[items.len() - 3..], [MenuItem::Settings, MenuItem::About, MenuItem::Quit]);
    }

    #[test]
    fn enter_on_settings_and_about_opens_them() {
        let mut app = demo();
        app.menu_cursor = 2;
        app.on_key(key(KeyCode::Enter));
        assert_eq!((app.view, app.focus), (View::Settings, Focus::Content));
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.menu_selected(), MenuItem::Settings);
        app.on_key(key(KeyCode::Char('j')));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.view, View::About);
        app.on_key(key(KeyCode::Char('s')));
        assert_eq!((app.view, app.focus), (View::Stations, Focus::Content));
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.menu_selected(), MenuItem::Stations);
    }

    #[test]
    fn now_playing_from_the_menu_and_with_shift_n() {
        let mut app = demo();
        app.view = View::About;
        app.focus = Focus::Content;
        app.on_key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT));
        assert_eq!((app.view, app.focus, app.menu_selected()), (View::Player, Focus::Content, MenuItem::NowPlaying));
        assert_eq!(app.pane, Pane::NowPlaying);
        let title = app.now.as_ref().map(|s| s.title.clone());
        app.view = View::Settings;
        app.focus = Focus::Menu;
        app.menu_cursor = 0;
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.view, View::Player);
        assert_eq!(app.now.as_ref().map(|s| s.title.clone()), title, "only shows it: the song plays on");
        // Lowercase n still skips.
        app.on_key(key(KeyCode::Char('n')));
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
        app.on_key(key(KeyCode::Char('N')));
        assert!(app.status.starts_with("Nothing is playing"), "{}", app.status);
        assert_eq!(app.view, View::Player, "N with nothing playing stays put");
        app.on_key(key(KeyCode::Char('s')));
        assert_eq!((app.view, app.focus), (View::Stations, Focus::Content), "s opens the list");
        // Enter on the second station: it plays and NOW PLAYING appears, selected.
        app.on_key(key(KeyCode::Char('j')));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.current_station.as_deref(), Some(app.stations[1].id.as_str()));
        assert_eq!((app.view, app.menu_selected()), (View::Player, MenuItem::NowPlaying));
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
        app.on_key(key(KeyCode::Char('s')));
        app.on_key(key(KeyCode::Char('j')));
        app.on_key(key(KeyCode::Char('j')));
        app.on_key(key(KeyCode::Char('f')));
        let third = app.stations[2].clone();
        assert!(app.is_favorite(&third.id));
        assert_eq!(
            app.menu_items(),
            [MenuItem::Stations, MenuItem::Favorite(0), MenuItem::Settings, MenuItem::About, MenuItem::Quit]
        );
        let saved = Config::load(&path).0.ui.favorites;
        assert_eq!(
            (saved.len(), saved[0].id.as_str(), saved[0].name.as_str()),
            (1, third.id.as_str(), third.name.as_str())
        );
        // Enter on the favorite in the menu plays it.
        app.on_key(key(KeyCode::Esc));
        app.on_key(key(KeyCode::Char('j')));
        assert_eq!(app.menu_selected(), MenuItem::Favorite(0));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.current_station.as_deref(), Some(third.id.as_str()));
        assert_eq!((app.view, app.menu_selected()), (View::Player, MenuItem::NowPlaying), "on to Now Playing");
        app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Favorite(0)).unwrap();
        // F on it in the menu unstars it: the section goes.
        app.on_key(KeyEvent::new(KeyCode::Char('F'), KeyModifiers::SHIFT));
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
        assert_eq!(app.focus, Focus::Menu);
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

    #[test]
    fn lowercase_u_d_always_rate_the_playing_song() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::History;
        let before = app.history[0].rating;
        app.on_key(key(KeyCode::Char('u')));
        assert_eq!(app.now.as_ref().unwrap().rating, Rating::Up);
        assert_eq!(app.history[0].rating, before, "History row untouched");
        let playing = app.now.as_ref().unwrap().track_token.clone();
        app.on_key(key(KeyCode::Char('d')));
        assert!(app.history.iter().any(|s| s.track_token == playing && s.rating == Rating::Down));
        assert_ne!(app.now.as_ref().unwrap().track_token, playing, "thumbs down skips");
    }

    #[test]
    fn shift_u_d_rate_the_highlighted_row() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::History;
        app.on_key(key(KeyCode::Char('j')));
        let playing = app.now.as_ref().unwrap().track_token.clone();
        app.on_key(key(KeyCode::Char('D')));
        assert_eq!(app.history[1].rating, Rating::Down);
        assert_eq!(app.now.as_ref().unwrap().track_token, playing, "rating a row doesn't skip");
        // Up Next: U likes the highlighted song in place; D drops it.
        app.pane = Pane::UpNext;
        let token = app.queue[0].track_token.clone();
        app.on_key(key(KeyCode::Char('U')));
        assert_eq!((app.queue[0].track_token.as_str(), app.queue[0].rating), (token.as_str(), Rating::Up));
        let n = app.queue.len();
        app.on_key(key(KeyCode::Char('D')));
        assert_eq!(app.queue.len(), n - 1);
        assert!(app.status.starts_with("Disliked and dropped from Up Next"), "{}", app.status);
        // From the menu, U uses the last song pane Tab reached.
        app.focus = Focus::Menu;
        app.last_song_pane = Pane::History;
        app.history_cursor = 0;
        app.on_key(key(KeyCode::Char('U')));
        assert_eq!(app.history[0].rating, Rating::Up);
        // An empty pane says so and does nothing.
        app.history.clear();
        app.on_key(key(KeyCode::Char('U')));
        assert!(app.status.starts_with("History is empty"), "{}", app.status);
    }

    #[test]
    fn shift_n_focuses_the_now_playing_pane() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::History;
        app.on_key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT));
        assert_eq!((app.view, app.focus, app.pane), (View::Player, Focus::Content, Pane::NowPlaying));
        assert_eq!(app.menu_selected(), MenuItem::NowPlaying);
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.pane, Pane::UpNext);
        assert_eq!(app.last_song_pane, Pane::UpNext);
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
    fn now_playing_size_is_saved() {
        let path = crate::config::temp_dir("np-size").join("config.toml");
        let mut config = Config::default();
        config.ui.check_updates = false;
        let mut app = App::new(config, Some(path.clone()), &[], false);
        app.prompt = None;
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.settings_cursor = Setting::ALL.iter().position(|(_, s)| *s == Setting::NowPlayingSize).unwrap();
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.setting_value(Setting::NowPlayingSize), "19 rows");
        assert_eq!(Config::load(&path).0.ui.now_playing_size, 19);
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
}
