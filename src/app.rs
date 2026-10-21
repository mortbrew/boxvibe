//! The app's state and what each key does.
//!
//! Like tubevibe, the left panel is the menu: the stations, then Settings,
//! About and Quit. The right side shows what the menu entry opens: the
//! player (Now Playing and the songs table), Settings or About. There is no
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
    Settings,
    About,
}

/// Which panel the keys move in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Menu,
    Content,
}

/// The entries under the stations in the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Station(usize),
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
    CheckUpdates,
    EditFile,
}

impl Setting {
    /// In screen order, each with its group.
    pub const ALL: [(&'static str, Self); 12] = [
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
            Self::Autoplay => "Play the last station on start",
            Self::Notify => "Song change notifications",
            Self::Theme => "Theme",
            Self::Icons => "Icons",
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
            Self::Autoplay => "After logging in, the station you played last starts by itself.",
            Self::Notify => {
                "A desktop notification (notify-send, mako on Omarchy) for every new song. Saved as [ui] notifications."
            }
            Self::Theme => "Omarchy follows your current Omarchy theme; or pick a built-in one. Also t.",
            Self::Icons => "Emoji, Nerd Font (needs one installed) or plain Unicode. Also i.",
            Self::CheckUpdates => "Asks GitHub when boxvibe opens; About shows the new version and Update Now.",
            Self::EditFile => "Opens config.toml in $EDITOR; boxvibe reads it again when you close it.",
        }
    }
}

/// The two song panes under Now Playing; Tab goes menu → Up Next → History.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    UpNext,
    History,
}

pub struct App {
    pub config: Config,
    pub config_path: Option<PathBuf>,
    pub demo: bool,
    pub session: Session,
    pub view: View,
    pub focus: Focus,
    pub menu_cursor: usize,
    pub settings_cursor: usize,
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
            pane: Pane::UpNext,
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

    pub fn menu_items(&self) -> Vec<MenuItem> {
        (0..self.stations.len())
            .map(MenuItem::Station)
            .chain([MenuItem::Settings, MenuItem::About, MenuItem::Quit])
            .collect()
    }

    pub fn menu_selected(&self) -> MenuItem {
        let items = self.menu_items();
        items[self.menu_cursor.min(items.len() - 1)]
    }

    fn open_menu_item(&mut self) {
        match self.menu_selected() {
            MenuItem::Station(i) => {
                self.view = View::Player;
                self.tune(i);
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

    /// The menu entry of what the right side shows.
    fn point_menu_at_view(&mut self) {
        let items = self.menu_items();
        let want = match self.view {
            View::Settings => Some(MenuItem::Settings),
            View::About => Some(MenuItem::About),
            View::Player => {
                let at = self.stations.iter().position(|s| Some(&s.id) == self.current_station.as_ref());
                at.map(MenuItem::Station)
            }
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
                self.menu_cursor = at.unwrap_or(0);
                if self.stations.is_empty() {
                    self.status = "This account has no stations yet: create one on pandora.com".into();
                } else if self.config.player.autoplay && self.current_station.is_none() {
                    // Shuffle comes first in the list: start on a real station.
                    if at.is_none() && self.stations.len() > 1 && self.stations[0].is_shuffle {
                        self.menu_cursor = 1;
                    }
                    self.tune(self.menu_cursor);
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
        self.current_station = Some(station.id.clone());
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
        };
        if len > 0 {
            *cursor = (*cursor as i32 + by).clamp(0, len as i32 - 1) as usize;
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
            Setting::Autoplay => on(self.config.player.autoplay),
            Setting::Notify => on(self.config.ui.notifications),
            Setting::Theme => theme::label(&self.config.ui.theme),
            Setting::Icons => crate::icons::label(self.config.ui.icons).into(),
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
                self.config.player.autoplay = !self.config.player.autoplay;
                self.save(&[("player", "autoplay", self.config.player.autoplay.into())]);
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
        match Config::load(&path) {
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
            Err(e) => self.status = format!("⚠ {e}"),
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
        if self.prompt.is_some() {
            return self.prompt_key(key);
        }
        if self.help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Enter) {
                self.help = false;
            }
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
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
            KeyCode::Char('z') => self.tired(),
            KeyCode::Char('s') => {
                self.view = View::Player;
                self.focus = Focus::Menu;
                self.point_menu_at_view();
            }
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
            KeyCode::Right | KeyCode::Char('>') | KeyCode::Char(']') => self.change_volume(5),
            KeyCode::Left | KeyCode::Char('<') | KeyCode::Char('[') => self.change_volume(-5),
            KeyCode::Char('t') => self.next_theme(),
            KeyCode::Char('i') => self.next_icons(),
            _ => {}
        }
    }

    /// Tab: menu → Up Next → History → menu on the player (Shift+Tab the
    /// other way); menu ⇄ page elsewhere.
    fn next_focus(&mut self, forward: bool) {
        let order = [(Focus::Menu, Pane::UpNext), (Focus::Content, Pane::UpNext), (Focus::Content, Pane::History)];
        if self.view == View::Player {
            let at = if self.focus == Focus::Menu {
                0
            } else if self.pane == Pane::UpNext {
                1
            } else {
                2
            };
            let to = if forward { (at + 1) % 3 } else { (at + 2) % 3 };
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
            (Focus::Content, View::Player) => {}
        }
    }

    fn move_cursor(&mut self, by: i32) {
        let clamp = |at: usize, len: usize| (at as i32 + by).clamp(0, len as i32 - 1) as usize;
        match (self.focus, self.view) {
            (Focus::Menu, _) => self.menu_cursor = clamp(self.menu_cursor, self.menu_items().len()),
            (Focus::Content, View::Settings) => self.settings_cursor = clamp(self.settings_cursor, Setting::ALL.len()),
            (Focus::Content, View::Player) => self.move_song(by),
            (Focus::Content, View::About) => {}
        }
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

    /// `+`/`-` act on the selected History row while History has the keys,
    /// else on the playing song.
    fn rate(&mut self, positive: bool) {
        if self.in_songs() && self.pane == Pane::UpNext {
            self.status = "Rate it once it plays".into();
            return;
        }
        let from_history = self.in_songs() && self.pane == Pane::History;
        let at = self.history_index();
        let song = if from_history { self.history.get_mut(at) } else { self.now.as_mut() };
        let Some(song) = song else { return };
        if song.station_id.is_empty() || song.track_token.is_empty() {
            return;
        }
        song.rating = if positive { Rating::Up } else { Rating::Down };
        let request = Request::Rate { track_token: song.track_token.clone(), positive };
        self.backend.send(request);
        // Like Pandora: a thumbs down on the playing song skips it.
        if !positive && !from_history {
            self.advance();
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

    /// For screenshots: the password input, already typed in.
    pub fn open_password_prompt(&mut self, typed: &str) {
        self.view = View::Settings;
        self.focus = Focus::Content;
        self.open_prompt(Prompt::Password);
        self.query = typed.into();
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEvent;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn demo() -> App {
        let mut app = App::new(Config::default(), None, &[], true);
        app.demo_warm_up(2, Duration::ZERO);
        app
    }

    #[test]
    fn menu_has_stations_then_settings_about_quit() {
        let app = demo();
        let items = app.menu_items();
        assert_eq!(items.len(), app.stations.len() + 3);
        assert_eq!(&items[items.len() - 3..], [MenuItem::Settings, MenuItem::About, MenuItem::Quit]);
    }

    #[test]
    fn enter_on_settings_and_about_opens_them() {
        let mut app = demo();
        app.menu_cursor = app.stations.len();
        app.on_key(key(KeyCode::Enter));
        assert_eq!((app.view, app.focus), (View::Settings, Focus::Content));
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.menu_selected(), MenuItem::Settings);
        app.on_key(key(KeyCode::Char('j')));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.view, View::About);
        app.on_key(key(KeyCode::Char('s')));
        assert_eq!((app.view, app.focus), (View::Player, Focus::Menu));
        assert!(matches!(app.menu_selected(), MenuItem::Station(_)));
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
    fn tab_goes_menu_up_next_history_and_back() {
        let mut app = demo();
        assert_eq!(app.focus, Focus::Menu);
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
    fn rating_a_played_song_in_history() {
        let mut app = demo();
        app.focus = Focus::Content;
        app.pane = Pane::History;
        app.on_key(key(KeyCode::Char('j')));
        assert_eq!(app.history_index(), 1);
        let token = app.history[1].track_token.clone();
        app.on_key(key(KeyCode::Char('d')));
        assert_eq!((app.history[1].track_token.as_str(), app.history[1].rating), (token.as_str(), Rating::Down));
        assert!(app.now.is_some(), "rating History doesn't skip");
        // Up Next can't be rated yet.
        app.pane = Pane::UpNext;
        app.on_key(key(KeyCode::Char('u')));
        assert_eq!(app.status, "Rate it once it plays");
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
        let (saved, _) = Config::load(&path).unwrap();
        assert!(!saved.ui.notifications);
        assert!(!std::fs::read_to_string(&path).unwrap().contains("notify ="), "the 0.1 name is gone");
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
