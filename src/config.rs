//! User configuration, `~/.config/boxvibe/config.toml` (tubevibe's layout).
//!
//! Nothing in this file can stop the app from starting. Every field has a
//! default, so a missing or partial file is fine. Unknown keys and sections
//! (a typo, a leftover from testing, a setting from a newer boxvibe or from
//! the old JSON-API boxvibe) and values of the wrong type are skipped with a
//! [`ConfigWarning`] shown in the status bar. A file that is not valid TOML
//! is moved aside to `config.toml.bad-<UTC time>` and the app starts with the
//! defaults ([`Config::load`]); one that can't be read is left alone.
//!
//! Settings changed from the app are written back with [`set_value`], which
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
    /// `omarchy` follows the current Omarchy theme (Tokyo Night elsewhere);
    /// or one of the built-in themes (`t` cycles them).
    pub theme: String,
    pub icons: IconSet,
    /// Song changes as a desktop notification (`notify-send`). boxvibe 0.1
    /// called it `notify`, which still reads.
    #[serde(alias = "notify")]
    pub notifications: bool,
    /// Ask GitHub, each time the app opens, whether there is a newer version.
    pub check_updates: bool,
    /// Start the station played last as soon as boxvibe opens (and has
    /// logged in). boxvibe 0.1's `[player] autoplay` reads as this.
    pub autoplay_last_station: bool,
    /// How every pane's border is drawn.
    pub border_style: BorderStyle,
    /// Columns of the left menu ([`MENU_WIDTH`]; never more than half the screen).
    pub menu_width: u16,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "omarchy".into(),
            icons: IconSet::Emoji,
            notifications: true,
            check_updates: true,
            autoplay_last_station: true,
            border_style: BorderStyle::Plain,
            menu_width: MENU_WIDTH.default,
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

    pub fn contains(self, value: u16) -> bool {
        (self.min..=self.max).contains(&value)
    }
}

/// Columns; 30 is what boxvibe 0.1 gave a 120-column terminal.
pub const MENU_WIDTH: Size = Size { key: "menu_width", min: 20, max: 40, step: 2, default: 30 };

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IconSet {
    /// Colour emoji.
    #[default]
    Emoji,
    /// Needs a Nerd Font (Omarchy ships one).
    Nerd,
    /// Plain Unicode, works with any font.
    Unicode,
}

impl IconSet {
    pub fn next(self) -> Self {
        match self {
            Self::Emoji => Self::Nerd,
            Self::Nerd => Self::Unicode,
            Self::Unicode => Self::Emoji,
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
            Ok(_) if unknown && name == "pandora" && OLD_PANDORA_KEYS.contains(&key.as_str()) => {
                warnings.push(ConfigWarning::Old(path))
            }
            Ok(_) if unknown => warnings.push(ConfigWarning::Unknown(path)),
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
        let mut table: toml::Table = toml::from_str(text)?;
        // 0.1's `[player] autoplay` is now `[ui] autoplay_last_station` (which wins when both are there).
        let old_autoplay = match table.get_mut("player") {
            Some(toml::Value::Table(player)) => player.remove("autoplay"),
            _ => None,
        };
        if let Some(value) = old_autoplay {
            let ui = table.entry("ui").or_insert_with(|| toml::Value::Table(toml::Table::new()));
            if let toml::Value::Table(ui) = ui {
                ui.entry("autoplay_last_station").or_insert(value);
            }
        }
        // Both the 0.1 name and the new one: the new one wins.
        if let Some(toml::Value::Table(ui)) = table.get_mut("ui")
            && ui.contains_key("notifications")
        {
            ui.remove("notify");
        }
        let mut warnings = Vec::new();
        let mut c = Self::default();
        for (name, value) in table {
            let w = &mut warnings;
            match name.as_str() {
                "pandora" => c.pandora = section(&name, value, w),
                "player" => c.player = section(&name, value, w),
                "ui" => c.ui = section(&name, value, w),
                _ => w.push(ConfigWarning::Unknown(name)),
            }
        }
        c.player.volume = c.player.volume.min(100);
        // Sizes out of range: said, and back to the default.
        if !MENU_WIDTH.contains(c.ui.menu_width) {
            warnings.push(ConfigWarning::BadValue {
                key: format!("ui.{}", MENU_WIDTH.key),
                error: format!("{} is not between {} and {}", c.ui.menu_width, MENU_WIDTH.min, MENU_WIDTH.max),
            });
            c.ui.menu_width = MENU_WIDTH.default;
        }
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

/// Sets `[table] key = value` for each entry in the file at `path`, creating
/// file and table when missing and leaving everything else untouched.
pub fn set_values(path: &Path, values: &[(&str, &str, toml_edit::Value)]) -> Result<(), ConfigError> {
    let error = |reason: String| ConfigError::Write { path: path.into(), reason };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(error(e.to_string())),
    };
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| error(e.to_string()))?;
    for (table, key, value) in values {
        let section = doc
            .entry(table)
            .or_insert_with(toml_edit::table)
            .as_table_mut()
            .ok_or_else(|| error(format!("`{table}` is not a table")))?;
        section[*key] = toml_edit::Item::Value(value.clone());
    }
    write_private(path, &doc.to_string()).map_err(|e| error(e.to_string()))
}

/// Removes `[table] key` from the file at `path`, if it is there (an old
/// name of a setting that was just saved under its new one).
pub fn remove_value(path: &Path, table: &str, key: &str) -> Result<(), ConfigError> {
    let error = |reason: String| ConfigError::Write { path: path.into(), reason };
    let Ok(text) = std::fs::read_to_string(path) else { return Ok(()) };
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| error(e.to_string()))?;
    let removed = doc.get_mut(table).and_then(|t| t.as_table_like_mut()).and_then(|t| t.remove(key)).is_some();
    if removed { write_private(path, &doc.to_string()).map_err(|e| error(e.to_string())) } else { Ok(()) }
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
        assert_eq!(c.ui.theme, "omarchy");
        assert_eq!(c.ui.icons, IconSet::Emoji);
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
        assert_eq!((c.player.volume, c.ui.autoplay_last_station, c.ui.icons), (70, false, IconSet::Emoji));
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
                ("pandora", "username", "me@example.com".into()),
                ("pandora", "password", "hunter2".into()),
                ("ui", "icons", "unicode".into()),
            ],
        )
        .unwrap();
        set_values(&path, &[("player", "volume", 55_i64.into())]).unwrap();
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
        set_values(&path, &[("ui", "notifications", false.into())]).unwrap();
        remove_value(&path, "ui", "notify").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("notifications = false") && !text.contains("notify ="), "{text}");
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
        assert_eq!((c.player.volume, c.ui.icons, c.ui.notifications), (70, IconSet::Emoji, true));
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
        set_values(&path, &[("ui", "theme", "gruvbox".into())]).unwrap();
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

    #[test]
    fn utc_stamps() {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_792_544_100);
        assert_eq!(utc_stamp(at), "20261021T005500Z");
        assert_eq!(utc_stamp(std::time::UNIX_EPOCH), "19700101T000000Z");
    }
}
