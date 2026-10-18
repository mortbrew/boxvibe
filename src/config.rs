//! User configuration, `~/.config/boxvibe/config.toml` (tubevibe's layout).
//!
//! Every field has a default, so a missing or partial file is fine. Unknown
//! keys and sections (a typo, or a setting from a newer boxvibe) and values of
//! the wrong type are skipped with a [`ConfigWarning`] shown in the status
//! bar. Only a file that is not valid TOML stops the app.
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
    /// Start playing the last station as soon as the login is done.
    pub autoplay: bool,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self { volume: 70, autoplay: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct UiConfig {
    /// `omarchy` follows the current Omarchy theme (Tokyo Night elsewhere);
    /// or one of the built-in themes (`t` cycles them).
    pub theme: String,
    pub icons: IconSet,
    /// Song changes as a desktop notification (`notify-send`).
    pub notify: bool,
    /// Ask GitHub, each time the app opens, whether there is a newer version.
    pub check_updates: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self { theme: "omarchy".into(), icons: IconSet::Emoji, notify: true, check_updates: true }
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
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("invalid config {path}: {source}")]
    Parse { path: PathBuf, source: toml::de::Error },
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
}

impl ConfigWarning {
    /// One line for the status bar: the first warning, and how many more.
    pub fn summary(warnings: &[Self]) -> Option<String> {
        let first = warnings.first()?;
        let text = match first {
            Self::Unknown(key) => format!("⚠ Ignored unknown setting {key} in config.toml (made by a newer boxvibe?)"),
            Self::BadValue { key, .. } => format!("⚠ Bad value for {key} in config.toml: using the default"),
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
            Ok(_) if unknown => warnings.push(ConfigWarning::Unknown(path)),
            Ok(_) => {
                good.insert(key, value);
            }
            Err(error) => {
                warnings.push(ConfigWarning::BadValue { key: path, error: error.message().trim().to_owned() })
            }
        }
    }
    T::deserialize(toml::Value::Table(good)).unwrap_or_default()
}

impl Config {
    /// Reads a config and lists what was skipped; only invalid TOML fails.
    pub fn parse(text: &str) -> Result<(Self, Vec<ConfigWarning>), toml::de::Error> {
        let table: toml::Table = toml::from_str(text)?;
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
        Ok((c, warnings))
    }

    /// Loads `path` and lists the settings that were skipped; a missing file yields the defaults.
    pub fn load(path: &Path) -> Result<(Self, Vec<ConfigWarning>), ConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Self::default(), Vec::new())),
            Err(source) => return Err(ConfigError::Read { path: path.into(), source }),
        };
        Self::parse(&text).map_err(|source| ConfigError::Parse { path: path.into(), source })
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
        assert_eq!((c.player.volume, c.player.autoplay, c.ui.icons), (70, false, IconSet::Emoji));
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
        let (c, _) = Config::load(&path).unwrap();
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
    fn missing_file_loads_defaults() {
        let (c, _) = Config::load(Path::new("/definitely/not/here/config.toml")).unwrap();
        assert_eq!(c, Config::default());
    }
}
