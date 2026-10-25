//! User configuration, `~/.config/boxvibe/config.toml`.
//!
//! The tables are the Settings groups: `[boxvibe]`, `[account]`, `[playback]`
//! and `[look]` (see [`LAYOUT`]). The 0.1–0.3 tables `[pandora]`, `[player]`
//! and `[ui]` still load (the new place wins when a key is in both) and are
//! emptied into the new tables the next time anything is saved.
//!
//! Nothing in this file can stop the app from starting. Every field has a
//! default, so a missing or partial file is fine. Unknown keys and sections
//! (a typo, a leftover from testing, a setting from a newer boxvibe or from
//! the old JSON-API boxvibe) and values of the wrong type are skipped with a
//! [`ConfigWarning`] shown in the status bar. A file that is not valid TOML
//! is moved aside to `config.toml.bad-<UTC time>` and the app starts with the
//! defaults ([`Config::load`]); one that can't be read is left alone.
//!
//! Settings changed from the app are written back with [`set_values`], which
//! keeps the user's comments and formatting. The file holds the Pandora
//! password, so it is always written with mode 600.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub pandora: PandoraConfig,
    pub player: PlayerConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct PandoraConfig {
    /// The account's email, saved from Settings.
    pub username: String,
    /// Saved from Settings once a login works, in plain text (the file is mode 600).
    pub password: String,
    /// `high` (192 kbps MP3 where the plan has it, else 128) | `standard`
    /// (128 kbps MP3). Old `low` / `medium` values read as `standard`.
    pub audio_quality: String,
    /// The User-Agent sent to pandora.com; empty is boxvibe's default (a
    /// desktop Firefox). Only for when Pandora's bot check starts refusing it.
    pub user_agent: String,
    /// The station playing when boxvibe closed (`stationId`), tuned on start.
    pub last_station: String,
}

impl Default for PandoraConfig {
    fn default() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            audio_quality: "high".into(),
            user_agent: String::new(),
            last_station: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct PlayerConfig {
    /// 0–100.
    pub volume: u8,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self { volume: 70 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct UiConfig {
    /// `omarchy` follows the current Omarchy theme, or one of the built-in
    /// themes (changed in Settings). Not saved yet: `omarchy` on an Omarchy
    /// system, else `tokyo-night`.
    pub theme: String,
    pub icons: IconSet,
    /// Song changes as a desktop notification (`notify-send`). boxvibe 0.1
    /// called it `notify`, which still reads.
    #[serde(alias = "notify")]
    pub notifications: bool,
    /// Ask GitHub, each time the app opens, whether there is a newer version.
    pub check_updates: bool,
    /// Show the boxvibe boot screen for a moment when the app opens.
    pub splash: bool,
    /// Start the station played last as soon as boxvibe opens (and has
    /// logged in). boxvibe 0.1's `[player] autoplay` reads as this.
    pub autoplay_last_station: bool,
    /// How every pane's border is drawn.
    pub border_style: BorderStyle,
    /// Columns of the left menu ([`MENU_WIDTH`]; never more than half the screen).
    pub menu_width: u16,
    /// How Now Playing shows the cover: a real picture, or `off`. (0.3's
    /// `blocks` reads as `picture`.)
    pub album_art: AlbumArt,
    /// Makes the cover blocky on purpose (for fun): `hd` is full resolution.
    /// 0.3 betas called it `art_pixelation` (off … max), still read.
    #[serde(alias = "art_pixelation")]
    pub retro_mode: RetroMode,
    /// Recolours the cover with the current theme's colours (`off`: as is).
    pub art_tint: ArtTint,
    /// Stations starred with `f`, in the order starred: the menu's FAVORITES.
    pub favorites: Vec<Favorite>,
}

/// A favorite station: its id to play it, its name to show before the
/// stations load. Written as `{ id = "…", name = "…" }`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Favorite {
    pub id: String,
    pub name: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: crate::theme::default_name().into(),
            icons: IconSet::Nerd,
            notifications: true,
            check_updates: true,
            splash: true,
            autoplay_last_station: true,
            border_style: BorderStyle::Plain,
            menu_width: MENU_WIDTH.default,
            album_art: AlbumArt::Picture,
            retro_mode: RetroMode::Hd,
            art_tint: ArtTint::Off,
            favorites: Vec::new(),
        }
    }
}

/// A setting in columns, changed in steps from Settings (tubevibe's `Size`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub key: &'static str,
    pub min: u16,
    pub max: u16,
    pub step: u16,
    pub default: u16,
}

impl Size {
    /// One step up; past the largest, back to the smallest.
    pub fn next(self, value: u16) -> u16 {
        let up = value.saturating_add(self.step);
        if value >= self.max || up > self.max { self.min } else { up.max(self.min) }
    }

    /// One step narrower (`-1`) or wider (`1`), stopping at the ends.
    pub fn step_by(self, value: u16, dir: i8) -> u16 {
        let v = if dir < 0 { value.saturating_sub(self.step) } else { value.saturating_add(self.step) };
        v.clamp(self.min, self.max)
    }

    pub fn contains(self, value: u16) -> bool {
        (self.min..=self.max).contains(&value)
    }
}

/// Columns; 30 is what boxvibe 0.1 gave a 120-column terminal.
pub const MENU_WIDTH: Size = Size { key: "menu_width", min: 20, max: 40, step: 2, default: 30 };

/// Rows; 17 is what Now Playing always had on a tall terminal.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BorderStyle {
    /// `┌─┐`, the look boxvibe always had.
    #[default]
    Plain,
    /// `╭─╮`
    Rounded,
    /// `┏━┓`
    Thick,
    /// `╔═╗`
    Double,
    /// No lines; the panes keep their room and titles.
    None,
}

impl BorderStyle {
    pub const ALL: [Self; 5] = [Self::Plain, Self::Rounded, Self::Thick, Self::Double, Self::None];

    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|b| *b == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    /// Value as written in `config.toml`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Rounded => "rounded",
            Self::Thick => "thick",
            Self::Double => "double",
            Self::None => "none",
        }
    }

    /// As Settings shows it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Plain => "Plain",
            Self::Rounded => "Rounded",
            Self::Thick => "Thick",
            Self::Double => "Double",
            Self::None => "None",
        }
    }
}

/// Settings → LOOK → Album art.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AlbumArt {
    /// The best the terminal can draw: sixel, kitty or iTerm2, else half
    /// blocks. 0.3's `blocks` is this too (the retro modes took its place).
    #[default]
    #[serde(alias = "blocks")]
    Picture,
    /// No cover: the space says so.
    Off,
}

impl AlbumArt {
    pub const ALL: [Self; 2] = [Self::Picture, Self::Off];

    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|a| *a == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Picture => "picture",
            Self::Off => "off",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Picture => "Picture",
            Self::Off => "Off",
        }
    }
}

/// Settings → LOOK → Tint art with theme colors: the cover recoloured with
/// the current theme's colour scheme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtTint {
    #[default]
    Off,
    /// Brightness mapped from the theme's background to its yellow.
    Duotone,
    /// Every pixel snapped to the nearest theme colour.
    Palette,
}

impl ArtTint {
    pub const ALL: [Self; 3] = [Self::Off, Self::Duotone, Self::Palette];

    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|a| *a == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Duotone => "duotone",
            Self::Palette => "palette",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Duotone => "Duotone",
            Self::Palette => "Palette",
        }
    }
}

/// Settings → LOOK → Retro mode: the cover shrunk to a small grid and blown
/// up again (nearest neighbour), so it looks like pixel art. The old
/// `art_pixelation` values (off, extra_low, low, medium, high, max) still read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum RetroMode {
    #[default]
    #[serde(rename = "hd", alias = "off")]
    Hd,
    #[serde(rename = "64bit")]
    Bit64,
    #[serde(rename = "32bit", alias = "extra_low", alias = "extra-low", alias = "extralow")]
    Bit32,
    #[serde(rename = "16bit", alias = "low")]
    Bit16,
    #[serde(rename = "8bit", alias = "medium")]
    Bit8,
    #[serde(rename = "4bit", alias = "high")]
    Bit4,
    #[serde(rename = "2bit", alias = "max")]
    Bit2,
}

impl RetroMode {
    pub const ALL: [Self; 7] = [Self::Hd, Self::Bit64, Self::Bit32, Self::Bit16, Self::Bit8, Self::Bit4, Self::Bit2];

    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|a| *a == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    /// Pixels across the shrunk cover (`None`: full resolution).
    pub fn grid(self) -> Option<u32> {
        match self {
            Self::Hd => None,
            Self::Bit64 => Some(256),
            Self::Bit32 => Some(128),
            Self::Bit16 => Some(64),
            Self::Bit8 => Some(32),
            Self::Bit4 => Some(16),
            Self::Bit2 => Some(8),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hd => "hd",
            Self::Bit64 => "64bit",
            Self::Bit32 => "32bit",
            Self::Bit16 => "16bit",
            Self::Bit8 => "8bit",
            Self::Bit4 => "4bit",
            Self::Bit2 => "2bit",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Hd => "HD",
            Self::Bit64 => "64-bit",
            Self::Bit32 => "32-bit",
            Self::Bit16 => "16-bit",
            Self::Bit8 => "8-bit",
            Self::Bit4 => "4-bit",
            Self::Bit2 => "2-bit",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IconSet {
    /// Colour emoji.
    Emoji,
    /// Needs a Nerd Font (Omarchy ships one).
    #[default]
    Nerd,
    /// Plain Unicode, works with any font.
    Unicode,
}

impl IconSet {
    pub fn next(self) -> Self {
        match self {
            Self::Nerd => Self::Emoji,
            Self::Emoji => Self::Unicode,
            Self::Unicode => Self::Nerd,
        }
    }

    /// Value as written in `config.toml`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Emoji => "emoji",
            Self::Nerd => "nerd",
            Self::Unicode => "unicode",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "emoji" => Some(Self::Emoji),
            "nerd" => Some(Self::Nerd),
            "unicode" => Some(Self::Unicode),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot update {path}: {reason}")]
    Write { path: PathBuf, reason: String },
}

/// Something in `config.toml` that was skipped instead of stopping the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigWarning {
    /// `section.key` (or a whole section) this version does not know.
    Unknown(String),
    /// `section.key` with a value of the wrong type: its default is used.
    BadValue { key: String, error: String },
    /// `pandora.key` of the old mobile JSON API (partner keys and such): unused now.
    Old(String),
    /// The file is not valid TOML: moved to `backup` (if it could be), defaults used.
    Broken { error: String, backup: Option<PathBuf> },
    /// The file exists but can't be read: defaults used, the file left as it is.
    Unreadable(String),
}

/// `[pandora]` keys of boxvibe's first, mobile JSON-API (pianobar-style)
/// version and of testing it; skipped without suggesting a newer boxvibe.
const OLD_PANDORA_KEYS: &[&str] = &[
    "partner",
    "partner_user",
    "partner_username",
    "partner_password",
    "partner_id",
    "device",
    "device_model",
    "decrypt_key",
    "decrypt_password",
    "encrypt_key",
    "encrypt_password",
    "rpc_host",
    "rpc_url",
    "tls",
    "sync_time",
    "station_token",
    "auth_token",
    "user_id",
    "proxy",
    "control_proxy",
];

impl ConfigWarning {
    /// One line for the status bar: the first warning, and how many more.
    pub fn summary(warnings: &[Self]) -> Option<String> {
        let first = warnings.first()?;
        let text = match first {
            Self::Unknown(key) => format!("⚠ Ignored unknown setting {key} in config.toml"),
            Self::BadValue { key, .. } => format!("⚠ Bad value for {key} in config.toml: using the default"),
            Self::Old(key) => format!("⚠ Ignored {key} in config.toml: left over from the old Pandora API"),
            Self::Broken { backup: Some(backup), .. } => format!(
                "⚠ config.toml wasn't valid TOML: moved to {} and started with the defaults",
                backup.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            ),
            Self::Broken { error, backup: None } => {
                format!("⚠ config.toml isn't valid TOML ({error}): using the defaults")
            }
            Self::Unreadable(error) => format!("⚠ Can't read config.toml ({error}): using the defaults"),
        };
        Some(match warnings.len() - 1 {
            0 => text,
            more => format!("{text} (+{more} more)"),
        })
    }
}

/// Keys an earlier boxvibe wrote that no longer do anything; a config with
/// them still loads without a warning. `ui.now_playing_size` went when Up Next
/// and History became fixed at 10 songs (Now Playing takes the rest).
const RETIRED_KEYS: &[&str] = &["ui.now_playing_size"];

/// One `[section]`: each key is tried on its own, so an unknown or broken
/// one is skipped (and reported) while the rest still apply.
fn section<T: serde::de::DeserializeOwned + Default>(
    name: &str,
    value: toml::Value,
    warnings: &mut Vec<ConfigWarning>,
) -> T {
    let toml::Value::Table(table) = value else {
        warnings.push(ConfigWarning::BadValue { key: name.into(), error: "not a [section]".into() });
        return T::default();
    };
    let mut good = toml::Table::new();
    for (key, value) in table {
        let path = format!("{name}.{key}");
        let one = toml::Table::from_iter([(key.clone(), value.clone())]);
        let mut unknown = false;
        let tried: Result<T, toml::de::Error> = serde_ignored::deserialize(toml::Value::Table(one), |_| unknown = true);
        match tried {
            // A favorite with a key boxvibe doesn't know still counts.
            Ok(_) if unknown && path != "ui.favorites" => warnings.push(ConfigWarning::Unknown(path)),
            Ok(_) => {
                good.insert(key, value);
            }
            Err(error) => {
                warnings.push(ConfigWarning::BadValue { key: path, error: error.message().trim().to_owned() })
            }
        }
    }
    // Each key worked on its own; together they still should, but never fail.
    T::deserialize(toml::Value::Table(good)).unwrap_or_else(|e| {
        warnings.push(ConfigWarning::BadValue { key: name.into(), error: e.message().trim().to_owned() });
        T::default()
    })
}

/// Where each setting lives: (table, key, the table 0.1–0.3 kept it in).
/// The tables are Settings' groups: BOXVIBE, ACCOUNT, PLAYBACK and LOOK.
/// The old tables still load, and are cleared out when anything is saved.
pub const LAYOUT: &[(&str, &str, &str)] = &[
    ("boxvibe", "check_updates", "ui"),
    ("boxvibe", "splash", "ui"),
    ("account", "username", "pandora"),
    ("account", "password", "pandora"),
    ("account", "user_agent", "pandora"),
    ("playback", "audio_quality", "pandora"),
    ("playback", "volume", "player"),
    ("playback", "autoplay_last_station", "ui"),
    ("playback", "last_station", "pandora"),
    ("playback", "notifications", "ui"),
    ("playback", "favorites", "ui"),
    ("look", "theme", "ui"),
    ("look", "icons", "ui"),
    ("look", "border_style", "ui"),
    ("look", "menu_width", "ui"),
    ("look", "album_art", "ui"),
    ("look", "retro_mode", "ui"),
    ("look", "art_tint", "ui"),
];

/// Names a setting had before: (old table, old key, table, key). Read when
/// the setting is not there under its own name, and removed when it is saved.
const OLD_NAMES: &[(&str, &str, &str, &str)] = &[
    ("player", "autoplay", "playback", "autoplay_last_station"),
    ("ui", "notify", "playback", "notifications"),
    ("ui", "art_pixelation", "look", "retro_mode"),
];

/// The tables of the old layout.
const OLD_TABLES: [&str; 3] = ["pandora", "player", "ui"];

/// `ui.icons` → `look.icons`: how a setting is named in the file now.
fn new_path(old: &str) -> String {
    let Some((table, key)) = old.split_once('.') else { return old.to_owned() };
    match LAYOUT.iter().find(|(_, k, t)| *t == table && *k == key) {
        Some((new, ..)) => format!("{new}.{key}"),
        None => old.to_owned(),
    }
}

/// Reads the file's tables, in either layout, as the three tables the
/// structs below are built from. A key in its new table beats the same key in
/// an old one, which beats an old name of it. Keys nobody knows are reported.
fn gather(table: toml::Table, warnings: &mut Vec<ConfigWarning>) -> toml::Table {
    let mut internal: toml::Table =
        OLD_TABLES.iter().map(|t| ((*t).to_owned(), toml::Value::Table(toml::Table::new()))).collect();
    let mut rank: std::collections::HashMap<(&str, String), u8> = std::collections::HashMap::new();
    let mut put = |internal: &mut toml::Table, old_table: &'static str, key: String, value: toml::Value, level: u8| {
        if rank.get(&(old_table, key.clone())).is_some_and(|have| *have >= level) {
            return;
        }
        if let Some(toml::Value::Table(t)) = internal.get_mut(old_table) {
            t.insert(key.clone(), value);
        }
        rank.insert((old_table, key), level);
    };
    for (name, value) in table {
        let is_new = LAYOUT.iter().any(|(t, ..)| *t == name);
        if !is_new && !OLD_TABLES.contains(&name.as_str()) {
            warnings.push(ConfigWarning::Unknown(name));
            continue;
        }
        let toml::Value::Table(keys) = value else {
            warnings.push(ConfigWarning::BadValue { key: name, error: "not a [section]".into() });
            continue;
        };
        for (key, value) in keys {
            let path = format!("{name}.{key}");
            if is_new {
                match LAYOUT.iter().find(|(t, k, _)| *t == name && *k == key) {
                    Some((_, _, old)) => put(&mut internal, old, key, value, 3),
                    None => warnings.push(ConfigWarning::Unknown(path)),
                }
            } else if LAYOUT.iter().any(|(_, k, t)| *t == name && *k == key) {
                let old = OLD_TABLES.iter().find(|t| **t == name).copied().unwrap_or_default();
                put(&mut internal, old, key, value, 2);
            } else if let Some((_, _, new, new_key)) = OLD_NAMES.iter().find(|(t, k, ..)| *t == name && *k == key) {
                let old = LAYOUT.iter().find(|(t, k, _)| t == new && k == new_key).map_or("ui", |(.., old)| *old);
                put(&mut internal, old, (*new_key).to_owned(), value, 1);
            } else if name == "pandora" && OLD_PANDORA_KEYS.contains(&key.as_str()) {
                warnings.push(ConfigWarning::Old(path));
            } else if !RETIRED_KEYS.contains(&path.as_str()) {
                warnings.push(ConfigWarning::Unknown(path));
            }
        }
    }
    internal
}

/// `20261021T011500Z` for a moment, in UTC (no time zone database needed).
fn utc_stamp(at: std::time::SystemTime) -> String {
    let secs = at.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z", rest / 3600, rest / 60 % 60, rest % 60)
}

impl Config {
    /// Reads a config and lists what was skipped; only invalid TOML fails.
    pub fn parse(text: &str) -> Result<(Self, Vec<ConfigWarning>), toml::de::Error> {
        let table: toml::Table = toml::from_str(text)?;
        let mut warnings = Vec::new();
        let mut gathered = gather(table, &mut warnings);
        let mut c = Self::default();
        // Each setting is checked on its own; a bad one is named as the file names it now.
        let mut checked = Vec::new();
        for (name, value) in std::mem::take(&mut gathered) {
            match name.as_str() {
                "pandora" => c.pandora = section(&name, value, &mut checked),
                "player" => c.player = section(&name, value, &mut checked),
                "ui" => c.ui = section(&name, value, &mut checked),
                _ => {}
            }
        }
        warnings.extend(checked.into_iter().map(|w| match w {
            ConfigWarning::BadValue { key, error } => ConfigWarning::BadValue { key: new_path(&key), error },
            other => other,
        }));
        c.player.volume = c.player.volume.min(100);
        // Sizes out of range: said, and back to the default.
        {
            let (size, value) = (MENU_WIDTH, &mut c.ui.menu_width);
            if !size.contains(*value) {
                warnings.push(ConfigWarning::BadValue {
                    key: format!("look.{}", size.key),
                    error: format!("{} is not between {} and {}", value, size.min, size.max),
                });
                *value = size.default;
            }
        }
        c.ui.favorites.retain(|f| !f.id.is_empty());
        Ok((c, warnings))
    }

    /// Reads `path` without touching it: a missing file is the defaults; a
    /// broken or unreadable one is `Err` with the warning to show.
    pub fn read(path: &Path) -> Result<(Self, Vec<ConfigWarning>), ConfigWarning> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Self::default(), Vec::new())),
            Err(e) => return Err(ConfigWarning::Unreadable(e.kind().to_string())),
        };
        let Ok(text) = String::from_utf8(bytes) else {
            return Err(ConfigWarning::Broken { error: "not UTF-8 text".into(), backup: None });
        };
        Self::parse(&text).map_err(|e| ConfigWarning::Broken { error: e.message().trim().to_owned(), backup: None })
    }

    /// Loads `path` for the app's start; never fails. A file that isn't valid
    /// TOML is moved to `config.toml.bad-<UTC time>` (so settings saved from
    /// now on go to a fresh file) and the defaults are used; either way the
    /// warnings say what happened.
    pub fn load(path: &Path) -> (Self, Vec<ConfigWarning>) {
        match Self::read(path) {
            Ok(loaded) => loaded,
            Err(ConfigWarning::Broken { error, .. }) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let backup = path.with_file_name(format!("{name}.bad-{}", utc_stamp(std::time::SystemTime::now())));
                let backup = std::fs::rename(path, &backup).is_ok().then_some(backup);
                (Self::default(), vec![ConfigWarning::Broken { error, backup }])
            }
            Err(other) => (Self::default(), vec![other]),
        }
    }

    /// `~/.config/boxvibe/config.toml` on Linux.
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "boxvibe").map(|dirs| dirs.config_dir().join("config.toml"))
    }

    pub fn has_login(&self) -> bool {
        !self.pandora.username.trim().is_empty() && !self.pandora.password.is_empty()
    }
}

/// Writes `text` to `path` with mode 600 (owner only), creating the folder.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Write next to it and rename, so a crash never leaves half a config.
    let tmp = path.with_extension("toml.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&tmp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    #[cfg(unix)]
    std::fs::set_permissions(&tmp, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    std::fs::rename(&tmp, path)
}

/// Moves every setting still in the 0.1–0.3 tables (`[pandora]`, `[player]`,
/// `[ui]`) to its table in [`LAYOUT`], keeping its comments, then drops the old
/// names and the old tables that are empty now. A key already in its new place
/// wins, and keys boxvibe doesn't know stay where they are.
fn migrate(doc: &mut toml_edit::DocumentMut) {
    let moves = LAYOUT
        .iter()
        .map(|(new, key, old)| (*old, *key, *new, *key))
        .chain(OLD_NAMES.iter().copied())
        .collect::<Vec<_>>();
    for (old, old_key, new, key) in moves {
        // Only ordinary [tables] are touched; anything odd is left alone.
        if !doc.get(new).is_none_or(toml_edit::Item::is_table) {
            continue;
        }
        let Some(taken) =
            doc.get_mut(old).and_then(toml_edit::Item::as_table_mut).and_then(|t| t.remove_entry(old_key))
        else {
            continue;
        };
        let (name, item) = taken;
        let Some(target) = doc.entry(new).or_insert_with(toml_edit::table).as_table_mut() else { continue };
        if !target.contains_key(key) {
            if key == old_key {
                target.insert_formatted(&name, item);
            } else {
                target.insert(key, item);
            }
        }
    }
    // 0.3's `album_art = "blocks"` is `picture` now.
    if let Some(art) = doc.get_mut("look").and_then(toml_edit::Item::as_table_mut).and_then(|t| t.get_mut("album_art"))
        && art.as_str() == Some("blocks")
        && let Some(old) = art.as_value()
    {
        let mut value = toml_edit::Value::from("picture");
        *value.decor_mut() = old.decor().clone();
        *art = toml_edit::Item::Value(value);
    }
    // An old table with nothing left goes; a comment above it (often the
    // file's first line) is kept, above the table that is first now.
    let mut comments = String::new();
    for old in OLD_TABLES {
        let Some(table) = doc.get(old).and_then(toml_edit::Item::as_table).filter(|t| t.is_empty()) else { continue };
        let prefix = table.decor().prefix().and_then(toml_edit::RawString::as_str).unwrap_or_default();
        comments.push_str(prefix.trim_start_matches('\n'));
        doc.remove(old);
    }
    if !comments.trim().is_empty()
        && let Some((_, first)) = doc.iter_mut().find(|(_, item)| item.is_table())
        && let Some(first) = first.as_table_mut()
    {
        let own = first.decor().prefix().and_then(toml_edit::RawString::as_str).unwrap_or_default().to_owned();
        first.decor_mut().set_prefix(format!("{comments}{own}"));
    }
}

/// Sets `[table] key = value` for each entry in the file at `path` (the
/// tables of [`LAYOUT`]), creating file and table when missing and leaving
/// everything else, comments included, untouched. A file still in the old
/// layout is moved to the new one first.
pub fn set_values(path: &Path, values: &[(&str, &str, toml_edit::Value)]) -> Result<(), ConfigError> {
    let error = |reason: String| ConfigError::Write { path: path.into(), reason };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(error(e.to_string())),
    };
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| error(e.to_string()))?;
    migrate(&mut doc);
    for (table, key, value) in values {
        debug_assert!(LAYOUT.iter().any(|(t, k, _)| t == table && k == key), "{table}.{key} is not in LAYOUT");
        let section = doc
            .entry(table)
            .or_insert_with(toml_edit::table)
            .as_table_mut()
            .ok_or_else(|| error(format!("`{table}` is not a table")))?;
        let mut value = value.clone();
        // The comment after the old value stays.
        if let Some(old) = section.get(key).and_then(toml_edit::Item::as_value) {
            *value.decor_mut() = old.decor().clone();
        }
        section[*key] = toml_edit::Item::Value(value);
    }
    write_private(path, &doc.to_string()).map_err(|e| error(e.to_string()))
}

#[cfg(test)]
pub(crate) fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("boxvibe-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_empty_file() {
        let (c, w) = Config::parse("").unwrap();
        assert_eq!(c, Config::default());
        assert!(w.is_empty());
        assert_eq!(c.ui.theme, crate::theme::default_name(), "Omarchy on Omarchy, Tokyo Night elsewhere");
        assert_eq!(c.ui.icons, IconSet::Nerd, "a Nerd Font is the default");
        assert_eq!(c.ui.album_art, AlbumArt::Picture);
        assert_eq!(c.player.volume, 70);
        assert_eq!(c.pandora.audio_quality, "high");
        assert!(!c.has_login());
    }

    #[test]
    fn a_newer_config_still_opens() {
        let text =
            "version = 2\n[ui]\nicons = \"nerd\"\nfuture_glow = true\n[future]\nlevel = 3\n[player]\nvolume = 40\n";
        let (c, w) = Config::parse(text).unwrap();
        assert_eq!((c.ui.icons, c.player.volume), (IconSet::Nerd, 40));
        assert_eq!(
            w,
            [
                ConfigWarning::Unknown("version".into()),
                ConfigWarning::Unknown("ui.future_glow".into()),
                ConfigWarning::Unknown("future".into()),
            ]
        );
        assert!(ConfigWarning::summary(&w).unwrap().ends_with("(+2 more)"));
    }

    #[test]
    fn wrong_types_fall_back_to_their_default() {
        let (c, w) = Config::parse("[player]\nvolume = \"loud\"\nautoplay = false\n[ui]\nicons = \"fancy\"\n").unwrap();
        assert_eq!((c.player.volume, c.ui.autoplay_last_station, c.ui.icons), (70, false, IconSet::Nerd));
        assert_eq!(w.len(), 2);
        let (c, _) = Config::parse("ui = 5\n").unwrap();
        assert_eq!(c, Config::default());
    }

    #[test]
    fn saved_settings_keep_comments_and_are_private() {
        let path = temp_dir("save").join("sub/config.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# mine\n[ui]\ntheme = \"gruvbox\" # cosy\nfuture = 1\n").unwrap();
        set_values(
            &path,
            &[
                ("account", "username", "me@example.com".into()),
                ("account", "password", "hunter2".into()),
                ("look", "icons", "unicode".into()),
            ],
        )
        .unwrap();
        set_values(&path, &[("playback", "volume", 55_i64.into())]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# mine") && text.contains("# cosy") && text.contains("future = 1"), "{text}");
        let (c, _) = Config::load(&path);
        assert!(c.has_login());
        assert_eq!((c.ui.theme.as_str(), c.ui.icons, c.player.volume), ("gruvbox", IconSet::Unicode, 55));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn a_v1_config_still_loads() {
        // boxvibe 0.1's HE-AAC qualities read as standard; the new key is known.
        let text = "[pandora]\nusername = \"me@example.com\"\naudio_quality = \"low\"\nuser_agent = \"\"\n";
        let (c, w) = Config::parse(text).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(crate::pandora::Quality::parse(&c.pandora.audio_quality), crate::pandora::Quality::Standard);
        assert_eq!(c.pandora.username, "me@example.com");
    }

    #[test]
    fn notifications_on_by_default_and_the_old_name_reads() {
        assert!(Config::default().ui.notifications);
        let (c, w) = Config::parse("[ui]\nnotify = false\n").unwrap();
        assert!(!c.ui.notifications && w.is_empty(), "{w:?}");
        let (c, w) = Config::parse("[ui]\nnotify = false\nnotifications = true\ntheme = \"gruvbox\"\n").unwrap();
        assert!(c.ui.notifications && w.is_empty(), "{w:?}");
        assert_eq!(c.ui.theme, "gruvbox", "the rest still applies");
        let path = temp_dir("notify").join("config.toml");
        std::fs::write(&path, "[ui]\nnotify = true # old\n").unwrap();
        set_values(&path, &[("playback", "notifications", false.into())]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("notifications = false # old") && !text.contains("notify"), "{text}");
    }

    #[test]
    fn retired_now_playing_size_is_ignored_quietly() {
        let (c, w) = Config::parse("[ui]\nnow_playing_size = 11\nmenu_width = 24\n").unwrap();
        assert_eq!((c.ui.menu_width, w.len()), (24, 0));
        let (_, w) = Config::parse("[ui]\nnow_playing_size = \"big\"\n").unwrap();
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn favorites_read_and_tolerate_junk() {
        let text =
            "[ui]\nfavorites = [{ id = \"1\", name = \"Jazz\" }, { id = \"2\", extra = 3 }, { name = \"no id\" }]\n";
        let (c, _) = Config::parse(text).unwrap();
        let ids: Vec<&str> = c.ui.favorites.iter().map(|f| f.id.as_str()).collect();
        assert_eq!((ids, c.ui.favorites[0].name.as_str()), (vec!["1", "2"], "Jazz"));
        let (c, w) = Config::parse("[ui]\nfavorites = \"jazz\"\nmenu_width = 24\n").unwrap();
        assert_eq!((c.ui.favorites.len(), c.ui.menu_width, w.len()), (0, 24, 1));
    }

    #[test]
    fn sizes_step_and_stop_at_the_ends() {
        assert_eq!((MENU_WIDTH.step_by(30, 1), MENU_WIDTH.step_by(30, -1)), (32, 28));
        assert_eq!((MENU_WIDTH.step_by(40, 1), MENU_WIDTH.step_by(20, -1)), (40, 20));
    }

    #[test]
    fn art_tint_parses_tolerantly() {
        let (c, w) = Config::parse("").unwrap();
        assert_eq!((c.ui.art_tint, w.len()), (ArtTint::Off, 0));
        let (c, w) = Config::parse("[ui]\nart_tint = \"duotone\"\n").unwrap();
        assert_eq!((c.ui.art_tint, w.len()), (ArtTint::Duotone, 0));
        let (c, w) = Config::parse("[ui]\nart_tint = \"sepia\"\nalbum_art = \"blocks\"\n").unwrap();
        assert_eq!((c.ui.art_tint, c.ui.album_art, w.len()), (ArtTint::Off, AlbumArt::Picture, 1));
        assert!(matches!(&w[0], ConfigWarning::BadValue { key, .. } if key == "look.art_tint"), "{w:?}");
        let (c, _) = Config::parse("[ui]\nart_tint = 3\n").unwrap();
        assert_eq!(c.ui.art_tint, ArtTint::Off);
        assert!(ArtTint::ALL.iter().all(|a| {
            Config::parse(&format!("[ui]\nart_tint = \"{}\"\n", a.as_str())).unwrap().0.ui.art_tint == *a
        }));
        assert_eq!(ArtTint::Palette.next(), ArtTint::Off);
    }

    #[test]
    fn album_art_and_retro_mode() {
        let (c, w) = Config::parse("").unwrap();
        assert_eq!((c.ui.album_art, c.ui.retro_mode, w.len()), (AlbumArt::Picture, RetroMode::Hd, 0));
        let (c, w) = Config::parse("[ui]\nalbum_art = \"blocks\"\nretro_mode = \"4bit\"\n").unwrap();
        assert_eq!((c.ui.album_art, c.ui.retro_mode, w.len()), (AlbumArt::Picture, RetroMode::Bit4, 0));
        let (c, w) = Config::parse("[look]\nalbum_art = \"blocks\"\n").unwrap();
        assert_eq!((c.ui.album_art, w.len()), (AlbumArt::Picture, 0), "0.3's blocks is Picture now");
        assert_eq!(AlbumArt::ALL.map(AlbumArt::as_str), ["picture", "off"]);
        assert_eq!(AlbumArt::Off.next(), AlbumArt::Picture);
        let (c, w) = Config::parse("[ui]\nalbum_art = \"hologram\"\nretro_mode = 7\n").unwrap();
        assert_eq!((c.ui.album_art, c.ui.retro_mode, w.len()), (AlbumArt::Picture, RetroMode::Hd, 2));
        assert!(RetroMode::ALL.iter().all(|a| {
            Config::parse(&format!("[ui]\nretro_mode = \"{}\"\n", a.as_str())).unwrap().0.ui.retro_mode == *a
        }));
        assert_eq!(RetroMode::Bit2.next(), RetroMode::Hd);
        assert_eq!(RetroMode::Hd.next(), RetroMode::Bit64, "the mildest step first");
        assert_eq!(RetroMode::Bit64.next(), RetroMode::Bit32);
        assert_eq!(
            RetroMode::ALL.map(RetroMode::grid),
            [None, Some(256), Some(128), Some(64), Some(32), Some(16), Some(8)]
        );
        assert_eq!(
            RetroMode::ALL.map(RetroMode::label),
            ["HD", "64-bit", "32-bit", "16-bit", "8-bit", "4-bit", "2-bit"]
        );
        let (c, w) = Config::parse("[ui]\nretro_mode = \"64bit\"\n").unwrap();
        assert_eq!((c.ui.retro_mode, w.len()), (RetroMode::Bit64, 0));
    }

    #[test]
    fn the_old_art_pixelation_key_and_values_still_read() {
        for (old, new) in [
            ("off", RetroMode::Hd),
            ("extra_low", RetroMode::Bit32),
            ("extra-low", RetroMode::Bit32),
            ("low", RetroMode::Bit16),
            ("medium", RetroMode::Bit8),
            ("high", RetroMode::Bit4),
            ("max", RetroMode::Bit2),
        ] {
            let (c, w) = Config::parse(&format!("[ui]\nart_pixelation = \"{old}\"\n")).unwrap();
            assert_eq!((c.ui.retro_mode, w.len()), (new, 0), "{old}");
        }
        let both = "[ui]\nart_pixelation = \"max\"\nretro_mode = \"16bit\"\n";
        let (c, w) = Config::parse(both).unwrap();
        assert_eq!((c.ui.retro_mode, w.len()), (RetroMode::Bit16, 0), "the new key wins");
    }

    #[test]
    fn border_style_and_menu_width() {
        let (c, w) = Config::parse("[ui]\nborder_style = \"rounded\"\nmenu_width = 24\n").unwrap();
        assert_eq!((c.ui.border_style, c.ui.menu_width, w.len()), (BorderStyle::Rounded, 24, 0));
        let (c, w) = Config::parse("[ui]\nborder_style = \"wavy\"\nmenu_width = 5\n").unwrap();
        assert_eq!((c.ui.border_style, c.ui.menu_width, w.len()), (BorderStyle::Plain, 30, 2));
        let (c, w) = Config::parse("[ui]\nmenu_width = 70000\n").unwrap();
        assert_eq!((c.ui.menu_width, w.len()), (30, 1));
        assert_eq!((MENU_WIDTH.next(30), MENU_WIDTH.next(40), MENU_WIDTH.next(39)), (32, 20, 20));
        assert_eq!(BorderStyle::None.next(), BorderStyle::Plain);
        assert!(
            BorderStyle::ALL.iter().all(|b| Config::parse(&format!("[ui]\nborder_style = \"{}\"\n", b.as_str()))
                .unwrap()
                .0
                .ui
                .border_style
                == *b)
        );
    }

    #[test]
    fn autoplay_last_station_and_its_old_name() {
        assert!(Config::default().ui.autoplay_last_station, "0.1 started the last station: still on");
        let (c, w) = Config::parse("[ui]\nautoplay_last_station = false\n").unwrap();
        assert!(!c.ui.autoplay_last_station && w.is_empty());
        let (c, w) = Config::parse("[player]\nautoplay = false\nvolume = 40\n").unwrap();
        assert_eq!((c.ui.autoplay_last_station, c.player.volume, w.len()), (false, 40, 0));
        let (c, _) = Config::parse("[player]\nautoplay = false\n[ui]\nautoplay_last_station = true\n").unwrap();
        assert!(c.ui.autoplay_last_station, "the new name wins");
        let (c, w) = Config::parse("[ui]\nautoplay_last_station = \"yes\"\n[player]\nautoplay = 3\n").unwrap();
        assert!(c.ui.autoplay_last_station, "a wrong type: the default");
        assert_eq!(w.len(), 1, "{w:?}");
    }

    #[test]
    fn missing_file_loads_defaults() {
        let (c, w) = Config::load(Path::new("/definitely/not/here/config.toml"));
        assert_eq!(c, Config::default());
        assert!(w.is_empty());
    }

    /// Junk that must never stop boxvibe: (file, what must still apply).
    #[test]
    fn junk_configs_always_start() {
        let junk = [
            // Leftovers from the JSON-API boxvibe and from testing.
            "[pandora]\npartner_user = \"android\"\npartner_password = \"x\"\ndevice = \"android-generic\"\n\
             decrypt_password = \"R=U!LH$O2B#\"\nencrypt_password = \"6#26FRL$ZWD\"\nrpc_host = \"tuner.pandora.com\"\n\
             audio_quality = \"medium\"\nlast_station = \"123:abc\"\nusername = \"me@example.com\"\n",
            // Wrong types everywhere.
            "[pandora]\nusername = 42\npassword = true\naudio_quality = [1, 2]\nuser_agent = { a = 1 }\n\
             [player]\nvolume = 300\nautoplay = \"yes\"\n[ui]\ntheme = 7\nicons = \"fancy\"\nnotifications = \"sure\"\n\
             check_updates = 1.5\nborder_style = 9\nmenu_width = \"wide\"\n",
            // Sections that are values, values that are sections, arrays of tables.
            "pandora = \"me\"\nplayer = [1, 2]\n[[ui]]\ntheme = \"gruvbox\"\n[ui2]\nx = 1\n",
            // Deeply nested and odd things.
            "[ui.extra.deep]\nx = 1\n[player.eq]\nbass = 3\n[test]\ndebug = true\nwhen = 2026-10-20T21:00:00Z\n",
            // Both names of the notifications setting, and an empty one.
            "[ui]\nnotify = 1\nnotifications = false\n[pandora]\n",
        ];
        for text in junk {
            let (c, warnings) = Config::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
            assert!(!warnings.is_empty() || text.contains("notifications = false"), "{text}");
            assert!(c.player.volume <= 100);
            assert!(ConfigWarning::summary(&warnings).is_some() || warnings.is_empty());
        }
        let (c, w) = Config::parse(junk[0]).unwrap();
        assert_eq!(c.pandora.username, "me@example.com");
        assert!(w.iter().all(|w| matches!(w, ConfigWarning::Old(_))), "{w:?}");
        assert!(ConfigWarning::summary(&w).unwrap().contains("old Pandora API"));
        assert_eq!(crate::pandora::Quality::parse(&c.pandora.audio_quality), crate::pandora::Quality::Standard);
        let (c, _) = Config::parse(junk[1]).unwrap();
        assert_eq!((c.player.volume, c.ui.icons, c.ui.notifications), (70, IconSet::Nerd, true));
    }

    #[test]
    fn a_broken_file_is_moved_aside_and_the_defaults_start() {
        let dir = temp_dir("broken");
        let path = dir.join("config.toml");
        let broken = "[pandora\nusername = \"me@example.com\"\npassword = \"x\n";
        std::fs::write(&path, broken).unwrap();
        let (c, w) = Config::load(&path);
        assert_eq!(c, Config::default());
        let [ConfigWarning::Broken { backup: Some(backup), .. }] = w.as_slice() else { panic!("{w:?}") };
        assert!(!path.exists(), "moved, so saving makes a fresh file");
        assert_eq!(std::fs::read_to_string(backup).unwrap(), broken, "kept as it was");
        let name = backup.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("config.toml.bad-20") && name.ends_with('Z'), "{name}");
        assert!(ConfigWarning::summary(&w).unwrap().contains("moved to config.toml.bad-"));
        // Not text at all, too.
        std::fs::write(&path, [0xff, 0xfe, 0x00, 0x9f]).unwrap();
        let (c, w) = Config::load(&path);
        assert_eq!((c, w.len()), (Config::default(), 1));
        // Saving still works afterwards.
        set_values(&path, &[("look", "theme", "gruvbox".into())]).unwrap();
        assert_eq!(Config::load(&path).0.ui.theme, "gruvbox");
    }

    #[test]
    fn an_unreadable_path_starts_with_defaults() {
        let dir = temp_dir("unreadable");
        // A folder where the file should be.
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&path).unwrap();
        let (c, w) = Config::load(&path);
        assert_eq!(c, Config::default());
        assert!(matches!(w.as_slice(), [ConfigWarning::Unreadable(_)]), "{w:?}");
        assert!(path.is_dir(), "left alone");
    }

    const OLD_FILE: &str = "# my settings\n[pandora]\nusername = \"me@example.com\" # login\npassword = \"hunter2\"\n\
        audio_quality = \"standard\"\nlast_station = \"42\"\n\n[player]\nvolume = 55 # quiet\n\n[ui]\n\
        theme = \"gruvbox\"\nicons = \"unicode\"\nborder_style = \"thick\"\nmenu_width = 30\n\
        album_art = \"blocks\"\nretro_mode = \"16bit\"\nart_tint = \"duotone\"\nsplash = false\n\
        check_updates = false\nautoplay_last_station = true\nnotifications = false\n\
        favorites = [{ id = \"1\", name = \"Jazz\" }]\nfuture = 1\n";

    #[test]
    fn the_tables_are_the_settings_groups() {
        let tables: Vec<&str> = LAYOUT.iter().map(|(t, ..)| *t).fold(Vec::new(), |mut all, t| {
            if !all.contains(&t) {
                all.push(t);
            }
            all
        });
        assert_eq!(tables, ["boxvibe", "account", "playback", "look"], "BOXVIBE, ACCOUNT, PLAYBACK, LOOK");
    }

    #[test]
    fn the_old_layout_still_loads_with_no_warnings() {
        let (old, w) = Config::parse(OLD_FILE).unwrap();
        // Only the unknown `future` key is reported.
        assert_eq!(w, [ConfigWarning::Unknown("ui.future".into())]);
        assert_eq!(old.pandora.username, "me@example.com");
        assert_eq!((old.player.volume, old.ui.theme.as_str(), old.ui.icons), (55, "gruvbox", IconSet::Unicode));
        assert_eq!(
            (old.ui.album_art, old.ui.retro_mode, old.ui.art_tint),
            (AlbumArt::Picture, RetroMode::Bit16, ArtTint::Duotone)
        );
        assert!(!old.ui.splash && !old.ui.check_updates && old.ui.autoplay_last_station && !old.ui.notifications);
        assert_eq!((old.ui.favorites.len(), old.pandora.last_station.as_str()), (1, "42"));
        // Written the new way, it is the same configuration.
        let new_text = "[boxvibe]\nsplash = false\ncheck_updates = false\n\n[account]\nusername = \"me@example.com\"\n\
            password = \"hunter2\"\n\n[playback]\naudio_quality = \"standard\"\nlast_station = \"42\"\nvolume = 55\n\
            autoplay_last_station = true\nnotifications = false\nfavorites = [{ id = \"1\", name = \"Jazz\" }]\n\n\
            [look]\ntheme = \"gruvbox\"\nicons = \"unicode\"\nborder_style = \"thick\"\nmenu_width = 30\n\
            album_art = \"picture\"\nretro_mode = \"16bit\"\nart_tint = \"duotone\"\n";
        let (new, w) = Config::parse(new_text).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(new, old);
    }

    #[test]
    fn a_setting_in_its_new_table_beats_the_old_one_and_old_names_read() {
        let text = "[ui]\ntheme = \"gruvbox\"\nnotify = false\nart_pixelation = \"8bit\"\n[look]\ntheme = \"catppuccin\"\n\
                    [pandora]\nvolume = 5\n[playback]\nvolume = 40\nnotifications = true\n[boxvibe]\nnonsense = 1\n";
        let (c, w) = Config::parse(text).unwrap();
        assert_eq!(c.ui.theme, "catppuccin", "[look] wins over [ui]");
        assert_eq!(c.player.volume, 40, "[playback] wins over a stray old volume");
        assert!(c.ui.notifications, "the new name wins over `notify`");
        assert_eq!(c.ui.retro_mode, RetroMode::Bit8, "art_pixelation reads as retro_mode");
        // `pandora.volume` is not a thing it ever had; `boxvibe.nonsense` is unknown.
        assert!(w.contains(&ConfigWarning::Unknown("boxvibe.nonsense".into())), "{w:?}");
        assert!(w.contains(&ConfigWarning::Unknown("pandora.volume".into())), "{w:?}");
        // A bad value is named as the file names it, in either layout.
        let (_, w) = Config::parse("[look]\nicons = \"fancy\"\n").unwrap();
        assert!(matches!(&w[..], [ConfigWarning::BadValue { key, .. }] if key == "look.icons"), "{w:?}");
        let (_, w) = Config::parse("[ui]\nicons = \"fancy\"\n[player]\nvolume = \"loud\"\n").unwrap();
        let keys: Vec<&str> = w
            .iter()
            .filter_map(|w| if let ConfigWarning::BadValue { key, .. } = w { Some(key.as_str()) } else { None })
            .collect();
        assert!(keys.contains(&"look.icons") && keys.contains(&"playback.volume"), "{keys:?}");
    }

    #[test]
    fn saving_moves_an_old_file_to_the_new_layout_and_keeps_the_rest() {
        let path = temp_dir("migrate").join("config.toml");
        std::fs::write(&path, OLD_FILE).unwrap();
        let before = Config::load(&path).0;
        set_values(&path, &[("look", "theme", "catppuccin".into())]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let doc: toml_edit::DocumentMut = text.parse().unwrap();
        // The old tables are gone, except for the one key nobody knows.
        assert!(doc.get("pandora").is_none() && doc.get("player").is_none(), "{text}");
        assert_eq!(doc["ui"].as_table().unwrap().len(), 1, "{text}");
        assert_eq!(doc["ui"]["future"].as_integer(), Some(1));
        // Every setting is in its place, with its comment and its value.
        assert_eq!(doc["account"]["username"].as_str(), Some("me@example.com"));
        assert_eq!(doc["playback"]["volume"].as_integer(), Some(55));
        assert_eq!(doc["look"]["theme"].as_str(), Some("catppuccin"));
        assert_eq!(doc["boxvibe"]["splash"].as_bool(), Some(false));
        assert!(text.contains("# my settings") && text.contains("# login") && text.contains("# quiet"), "{text}");
        assert!(text.contains("album_art = \"picture\"") && !text.contains("blocks"), "{text}");
        // Nothing but the saved theme changed (and blocks, which is picture now).
        let after = Config::load(&path).0;
        assert_eq!(after.ui.theme, "catppuccin");
        let mut want = before;
        want.ui.theme = "catppuccin".into();
        assert_eq!(after, want);
        // Saving again is steady: no duplicates, no new tables.
        set_values(&path, &[("playback", "volume", 60_i64.into())]).unwrap();
        let again: toml_edit::DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        assert_eq!(again["playback"]["volume"].as_integer(), Some(60));
        assert!(again.get("player").is_none());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn saving_removes_old_names_and_keeps_the_new_value() {
        let path = temp_dir("migrate-names").join("config.toml");
        std::fs::write(&path, "[player]\nautoplay = true\n[ui]\nnotify = false\nart_pixelation = \"4bit\"\n[look]\nretro_mode = \"2bit\"\n")
            .unwrap();
        set_values(&path, &[("playback", "autoplay_last_station", false.into())]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("autoplay =") && !text.contains("notify") && !text.contains("art_pixelation"), "{text}");
        assert!(text.contains("autoplay_last_station = false"), "{text}");
        assert!(text.contains("notifications = false"), "the value of `notify` moved: {text}");
        assert!(text.contains("retro_mode = \"2bit\""), "the new key wins: {text}");
        assert!(!text.contains("[ui]") && !text.contains("[player]"), "{text}");
    }

    #[test]
    fn a_new_layout_file_is_saved_in_place_and_a_new_file_gets_the_new_tables() {
        let path = temp_dir("fresh").join("config.toml");
        set_values(&path, &[("look", "theme", "omarchy".into()), ("account", "username", "a@b.c".into())]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[look]") && text.contains("[account]") && !text.contains("[ui]"), "{text}");
        assert!(!text.contains("[pandora]"), "an [account] section, not [pandora]: {text}");
        let (c, w) = Config::load(&path);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!((c.ui.theme.as_str(), c.pandora.username.as_str()), ("omarchy", "a@b.c"));
    }

    #[test]
    fn an_odd_old_table_is_left_alone_when_saving() {
        let path = temp_dir("odd").join("config.toml");
        std::fs::write(&path, "ui = 5\n").unwrap();
        // `ui = 5` is not a table: nothing to move, and the new keys still save.
        set_values(&path, &[("look", "theme", "gruvbox".into())]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("ui = 5") && text.contains("theme = \"gruvbox\""), "{text}");
    }

    #[test]
    fn utc_stamps() {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_792_544_100);
        assert_eq!(utc_stamp(at), "20261021T005500Z");
        assert_eq!(utc_stamp(std::time::UNIX_EPOCH), "19700101T000000Z");
    }
}
