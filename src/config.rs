//! User configuration, `~/.config/boxvibe/config.toml`.
//!
//! The tables are the Settings groups: `[boxvibe]`, `[account]`, `[playback]`,
//! `[look]` and `[browse]` (see [`LAYOUT`]). The 0.1–0.3 tables `[pandora]`, `[player]`
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
//! keeps the user's comments and formatting. The file is always written with
//! mode 600. It never holds the Pandora password: that lives in the desktop
//! keychain ([`crate::keychain`]). A password an older boxvibe left in the file
//! (`[account]` or `[pandora]` `password`) is still read, once, so it can be
//! moved to the keychain; [`forget_plaintext_password`] then deletes it.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub pandora: PandoraConfig,
    pub player: PlayerConfig,
    pub ui: UiConfig,
    pub browse: BrowseConfig,
}

/// A password. It prints as `<hidden>` in `{:?}` (so it can't end up in a log
/// or a panic message by accident), and is never written to the config file.
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// The text itself; only for sending it to Pandora or the keychain.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_empty() { "<empty>" } else { "<hidden>" })
    }
}

/// Known Browse directory sections (Settings labels and `[browse]` keys).
/// Each key pins that section as its own main-menu item (default On).
/// Unknown future keys still work: On pins them; missing means On.
pub const BROWSE_SECTIONS: &[(&str, &str)] = &[
    ("artists_picked_for_you", "Artists Picked For You"),
    ("new_music", "New Music"),
    ("browse_genres", "Browse Genres"),
    ("featured_playlists", "Featured Playlists"),
    ("trendsetters_artists_on_the_rise", "Trendsetters: Artists on the rise"),
    ("decades", "Decades"),
    ("recommended_podcasts", "Recommended Podcasts"),
    ("top_artists", "Top artists"),
    ("moods_and_activities", "Moods and activities"),
    ("top_stations", "Top stations"),
];

/// Snake_case `[browse]` key for a section title (known or future).
pub fn browse_section_key(title: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('_');
            }
            gap = false;
            out.push(c.to_ascii_lowercase());
        } else {
            gap = !out.is_empty();
        }
    }
    out
}

/// `[browse]`: per-section On/Off pins on the main menu (no parent Browse entry).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BrowseConfig {
    /// Per-section toggles; missing keys default to On (on the main menu).
    #[serde(flatten)]
    pub sections: std::collections::BTreeMap<String, bool>,
}

impl BrowseConfig {
    /// Whether a directory section is pinned on the main menu (by title).
    /// Missing keys default to On.
    pub fn pinned_on_menu(&self, title: &str) -> bool {
        self.sections.get(&browse_section_key(title)).copied().unwrap_or(true)
    }

    pub fn pinned_key(&self, key: &str) -> bool {
        self.sections.get(key).copied().unwrap_or(true)
    }

    pub fn set_pinned(&mut self, title: &str, on: bool) {
        self.sections.insert(browse_section_key(title), on);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct PandoraConfig {
    /// The account's email, saved from Settings (`[account] email_or_username`).
    #[serde(rename = "email_or_username")]
    pub username: String,
    /// The password in use this session. Not a setting: it is read from the
    /// keychain (or typed), and only a plaintext one left in the file by
    /// boxvibe 0.4 betas is read from there, to be moved. Never written.
    #[serde(skip_serializing)]
    pub password: Secret,
    /// `high` (192 kbps MP3 where the plan has it, else 128) | `standard`
    /// (128 kbps MP3). Old `low` / `medium` values read as `standard`.
    /// A free plan gets 64 kbps HE-AAC whichever is chosen.
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
            password: Secret::default(),
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
    /// Play Pandora's audio ads and report them like the website does (free
    /// accounts get them); Muted plays them silently. Off: no ad is asked
    /// for, so none plays. Written `true`, `"muted"` or `false`.
    pub good_vibes_only: GoodVibes,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self { volume: 70, good_vibes_only: GoodVibes::On }
    }
}

/// Settings → Good Vibes Only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GoodVibes {
    /// Ads play and are reported, like the website.
    #[default]
    On,
    /// Ads play and are reported exactly as with On, at volume 0.
    Muted,
    /// No ad is asked for.
    Off,
}

impl GoodVibes {
    /// Ads are asked for (and played, maybe silently).
    pub fn plays_ads(self) -> bool {
        self != Self::Off
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Muted => "muted",
            Self::Off => "off",
        }
    }

    /// The order Settings steps through.
    pub fn next(self) -> Self {
        match self {
            Self::On => Self::Muted,
            Self::Muted => Self::Off,
            Self::Off => Self::On,
        }
    }

    /// As written in config.toml: `true`, `"muted"`, `false`.
    pub fn to_value(self) -> toml_edit::Value {
        match self {
            Self::On => true.into(),
            Self::Muted => "muted".into(),
            Self::Off => false.into(),
        }
    }
}

impl Serialize for GoodVibes {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::On => s.serialize_bool(true),
            Self::Muted => s.serialize_str("muted"),
            Self::Off => s.serialize_bool(false),
        }
    }
}

impl<'de> Deserialize<'de> for GoodVibes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Bool(bool),
            Text(String),
        }
        match Raw::deserialize(d)? {
            Raw::Bool(true) => Ok(Self::On),
            Raw::Bool(false) => Ok(Self::Off),
            Raw::Text(t) => match t.trim().to_ascii_lowercase().as_str() {
                "muted" | "mute" => Ok(Self::Muted),
                "on" | "true" => Ok(Self::On),
                "off" | "false" => Ok(Self::Off),
                other => Err(serde::de::Error::custom(format!("expected true, false or \"muted\", not \"{other}\""))),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct UiConfig {
    /// Settings' "Theme Auto Mode": `omarchy` follows the current Omarchy
    /// theme (and Theme Select is greyed out), `off` uses Theme Select. Not
    /// saved yet: `omarchy` on an Omarchy system, else `off`. 0.5 betas and
    /// earlier wrote one `theme` key instead, still read ([`split_old_theme`]).
    pub theme_auto_mode: ThemeAutoMode,
    /// Settings' "Theme Select": a built-in theme or one saved in
    /// `~/.config/boxvibe/themes/`, used while Theme Auto Mode is off.
    pub theme_select: String,
    pub icons: IconSet,
    /// Song changes as a desktop notification (`notify-send`). boxvibe 0.1
    /// called it `notify`, which still reads.
    #[serde(rename = "song_change_notifications", alias = "notify")]
    pub notifications: bool,
    /// Ask GitHub, each time the app opens, whether there is a newer version
    /// (`[boxvibe] look_for_new_versions_on_startup`).
    #[serde(rename = "look_for_new_versions_on_startup")]
    pub check_updates: bool,
    /// Explain in a popup why a genre station can't be renamed or edited, and
    /// what the logo and person icons mean (`[boxvibe] genre_edit_popup`).
    /// Turned off by the popup's "Don't show this again" box.
    pub genre_edit_popup: bool,
    /// Show the boxvibe boot screen for a moment when the app opens
    /// (`[look] boot_screen`; 0.4 betas called it `splash`, in `[boxvibe]`).
    #[serde(rename = "boot_screen")]
    pub splash: bool,
    /// Motion in the interface (today: the boot screen's fade). Off: none
    /// (the boot screen is skipped). `[look] animations`.
    pub animations: bool,
    /// The menu lists FAVORITES (`[look] favorites_in_menu`; `f` still stars).
    pub favorites_in_menu: bool,
    /// The menu has a "Pinned" entry (`[look] pinned_in_menu`; Shift+I still pins).
    pub pinned_in_menu: bool,
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
    #[serde(rename = "tint_art_with_theme_colors")]
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

impl UiConfig {
    /// The theme in use: `omarchy` while Theme Auto Mode follows Omarchy,
    /// else Theme Select's.
    pub fn theme(&self) -> &str {
        match self.theme_auto_mode {
            ThemeAutoMode::Omarchy => "omarchy",
            ThemeAutoMode::Off => &self.theme_select,
        }
    }

    /// Sets the theme as one name (`--theme`, the old `theme` key):
    /// `omarchy` turns Auto Mode on, anything else selects it with Auto Mode off.
    pub fn set_theme(&mut self, name: &str) {
        if name == "omarchy" {
            self.theme_auto_mode = ThemeAutoMode::Omarchy;
        } else {
            self.theme_auto_mode = ThemeAutoMode::Off;
            self.theme_select = name.into();
        }
    }
}

/// Settings' Theme Auto Mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeAutoMode {
    /// Follow the current Omarchy theme.
    Omarchy,
    /// Use Theme Select.
    #[default]
    Off,
}

impl ThemeAutoMode {
    /// Value as written in `config.toml` and shown in Settings.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Omarchy => "omarchy",
            Self::Off => "off",
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme_auto_mode: if crate::theme::default_name() == "omarchy" {
                ThemeAutoMode::Omarchy
            } else {
                ThemeAutoMode::Off
            },
            theme_select: crate::theme::FALLBACK.into(),
            icons: IconSet::Nerd,
            notifications: true,
            check_updates: true,
            genre_edit_popup: true,
            splash: true,
            animations: true,
            favorites_in_menu: true,
            pinned_in_menu: true,
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

/// REMOVE AT 1.0. Keys an earlier boxvibe wrote that no longer do anything; a
/// config with them still loads without a warning, and saving deletes them
/// (and the old table, once it is empty). `ui.now_playing_size` went when Up Next
/// and History became fixed at 10 songs (Now Playing takes the rest).
/// `boxvibe.dependency_check` was a 0.4 beta setting for a startup check that
/// is now always the same list, with no switch.
const RETIRED_KEYS: &[&str] = &["ui.now_playing_size", "boxvibe.dependency_check", "browse.browse"];

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
/// The tables are Settings' groups: ACCOUNT, BOXVIBE, PLAYBACK and LOOK.
/// The old tables still load, and are cleared out when anything is saved.
pub const LAYOUT: &[(&str, &str, &str)] = &[
    ("account", "email_or_username", "pandora"),
    ("account", "user_agent", "pandora"),
    ("boxvibe", "look_for_new_versions_on_startup", "ui"),
    ("boxvibe", "genre_edit_popup", "ui"),
    ("playback", "audio_quality", "pandora"),
    ("playback", "volume", "player"),
    ("playback", "good_vibes_only", "player"),
    ("playback", "autoplay_last_station", "ui"),
    ("playback", "last_station", "pandora"),
    ("playback", "song_change_notifications", "ui"),
    ("playback", "favorites", "ui"),
    ("look", "theme_auto_mode", "ui"),
    ("look", "theme_select", "ui"),
    ("look", "icons", "ui"),
    ("look", "border_style", "ui"),
    ("look", "menu_width", "ui"),
    ("look", "favorites_in_menu", "ui"),
    ("look", "pinned_in_menu", "ui"),
    ("look", "album_art", "ui"),
    ("look", "retro_mode", "ui"),
    ("look", "tint_art_with_theme_colors", "ui"),
    ("look", "animations", "ui"),
    ("look", "boot_screen", "ui"),
    ("browse", "artists_picked_for_you", "browse"),
    ("browse", "new_music", "browse"),
    ("browse", "browse_genres", "browse"),
    ("browse", "featured_playlists", "browse"),
    ("browse", "trendsetters_artists_on_the_rise", "browse"),
    ("browse", "decades", "browse"),
    ("browse", "recommended_podcasts", "browse"),
    ("browse", "top_artists", "browse"),
    ("browse", "moods_and_activities", "browse"),
    ("browse", "top_stations", "browse"),
];

// REMOVE AT 1.0: the old-config migration is `OLD_NAMES`, `OLD_TABLES`, `age`,
// the old-name branches of `gather`, `new_path`, `migrate` and the `alias`
// attributes on the structs above. At 1.0 the file layout is final and all of
// it goes (only the pre-1.0 betas ever wrote an old config).

/// REMOVE AT 1.0. Names a setting had before: (old table, old key, table, key). Read when
/// the setting is not there under its own name, and removed when it is saved.
/// Oldest first: when a file has several, the later entry wins, and a name in
/// the setting's own table or a table of [`LAYOUT`] beats all of these.
const OLD_NAMES: &[(&str, &str, &str, &str)] = &[
    ("player", "autoplay", "playback", "autoplay_last_station"),
    ("ui", "art_pixelation", "look", "retro_mode"),
    // Good Vibes Only was `good_guy` in the 0.4 betas, in [player] and then [playback].
    ("player", "good_guy", "playback", "good_vibes_only"),
    ("playback", "good_guy", "playback", "good_vibes_only"),
    // Each of these is named as its Settings row now.
    ("pandora", "username", "account", "email_or_username"),
    ("account", "username", "account", "email_or_username"),
    ("ui", "check_updates", "boxvibe", "look_for_new_versions_on_startup"),
    ("boxvibe", "check_updates", "boxvibe", "look_for_new_versions_on_startup"),
    ("ui", "notify", "playback", "song_change_notifications"),
    ("ui", "notifications", "playback", "song_change_notifications"),
    ("playback", "notifications", "playback", "song_change_notifications"),
    ("ui", "art_tint", "look", "tint_art_with_theme_colors"),
    ("look", "art_tint", "look", "tint_art_with_theme_colors"),
    ("ui", "splash", "look", "boot_screen"),
    ("boxvibe", "splash", "look", "boot_screen"),
    ("look", "splash", "look", "boot_screen"),
];

/// How recent an old name is (later in [`OLD_NAMES`] is newer): 1 and up,
/// always below [`OLD_TABLE`] and [`OWN_PLACE`].
fn age(table: &str, key: &str) -> Option<u16> {
    OLD_NAMES.iter().position(|(t, k, ..)| *t == table && *k == key).map(|i| i as u16 + 1)
}

/// Ranks of where a value was read from: its own table and key, then its old
/// table under the same key, then the old names by [`age`].
const OWN_PLACE: u16 = 1000;
const OLD_TABLE: u16 = 500;

/// REMOVE AT 1.0. Up to 0.5's betas one `theme` key (in `[look]`, or `[ui]`
/// before that) was both settings: `omarchy`, or the theme's name. It becomes
/// `theme_auto_mode` and `theme_select` in the same table, unless the file
/// has those already (then the old key is just dropped).
fn split_old_theme(table: &mut toml::Table) {
    for name in ["ui", "look"] {
        let Some(toml::Value::Table(t)) = table.get_mut(name) else { continue };
        let Some(old) = t.remove("theme") else { continue };
        let Some(old) = old.as_str() else { continue };
        let (mode, select) = if old == "omarchy" { ("omarchy", None) } else { ("off", Some(old)) };
        if !t.contains_key("theme_auto_mode") {
            t.insert("theme_auto_mode".into(), mode.into());
        }
        if let Some(select) = select
            && !t.contains_key("theme_select")
        {
            t.insert("theme_select".into(), select.into());
        }
    }
}

/// [`split_old_theme`] for a file being saved, keeping its comments.
fn split_old_theme_doc(doc: &mut toml_edit::DocumentMut) {
    for name in ["ui", "look"] {
        let Some(t) = doc.get_mut(name).and_then(toml_edit::Item::as_table_mut) else { continue };
        let Some((key, item)) = t.remove_entry("theme") else { continue };
        let Some(old) = item.as_str().map(str::to_owned) else { continue };
        let decor = key.leaf_decor().clone();
        // The old line's trailing comment follows the value it named.
        let after = item.as_value().map(|v| v.decor().clone()).unwrap_or_default();
        let (mode, select) = if old == "omarchy" { ("omarchy", None) } else { ("off", Some(old)) };
        let mut after = Some(after);
        if !t.contains_key("theme_auto_mode") {
            let mut k = toml_edit::Key::new("theme_auto_mode");
            *k.leaf_decor_mut() = decor;
            let mut v = toml_edit::Value::from(mode);
            if select.is_none() {
                *v.decor_mut() = after.take().unwrap_or_default();
            }
            t.insert_formatted(&k, toml_edit::Item::Value(v));
        }
        if let Some(select) = select
            && !t.contains_key("theme_select")
        {
            let mut v = toml_edit::Value::from(select);
            *v.decor_mut() = after.take().unwrap_or_default();
            t.insert("theme_select", toml_edit::Item::Value(v));
        }
    }
}

/// REMOVE AT 1.0. The tables of the old layout.
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
    internal.insert("browse".into(), toml::Value::Table(toml::Table::new()));
    let mut rank: std::collections::HashMap<(&str, String), u16> = std::collections::HashMap::new();
    let mut put = |internal: &mut toml::Table, old_table: &'static str, key: String, value: toml::Value, level: u16| {
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
            // A password an older boxvibe saved: read once, to move it to the keychain.
            if key == "password" && (name == "account" || name == "pandora") {
                put(&mut internal, "pandora", key, value, if name == "account" { OWN_PLACE } else { OLD_TABLE });
                continue;
            }
            if is_new {
                let old_name = OLD_NAMES.iter().find(|(t, k, ..)| *t == name && *k == key);
                match (LAYOUT.iter().find(|(t, k, _)| *t == name && *k == key), old_name) {
                    (Some((_, _, old)), _) => put(&mut internal, old, key, value, OWN_PLACE),
                    // A setting that moved out of this table: still read from here.
                    (None, Some((_, _, new, new_key))) => {
                        let old =
                            LAYOUT.iter().find(|(t, k, _)| t == new && k == new_key).map_or("ui", |(.., old)| *old);
                        put(&mut internal, old, (*new_key).to_owned(), value, age(&name, &key).unwrap_or(1));
                    }
                    (None, None) if RETIRED_KEYS.contains(&path.as_str()) => {}
                    // Future Browse sections: keep under [browse], default handled in BrowseConfig.
                    (None, None) if name == "browse" => put(&mut internal, "browse", key, value, OWN_PLACE),
                    (None, None) => warnings.push(ConfigWarning::Unknown(path)),
                }
            } else if LAYOUT.iter().any(|(_, k, t)| *t == name && *k == key) {
                let old = OLD_TABLES.iter().find(|t| **t == name).copied().unwrap_or_default();
                put(&mut internal, old, key, value, OLD_TABLE);
            } else if let Some((_, _, new, new_key)) = OLD_NAMES.iter().find(|(t, k, ..)| *t == name && *k == key) {
                let old = LAYOUT.iter().find(|(t, k, _)| t == new && k == new_key).map_or("ui", |(.., old)| *old);
                put(&mut internal, old, (*new_key).to_owned(), value, age(&name, &key).unwrap_or(1));
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
        let mut table: toml::Table = toml::from_str(text)?;
        split_old_theme(&mut table);
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
                "browse" => c.browse = section(&name, value, &mut checked),
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
    // Write next to it and rename, so a crash never leaves half a config. The
    // temporary name carries the pid, so two boxvibes writing at once cannot
    // clobber each other's half-written file.
    let tmp = path.with_extension(format!("toml.{}.tmp", std::process::id()));
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
    // REMOVE AT 1.0 (with `OLD_NAMES`): this whole function.
    split_old_theme_doc(doc);
    let moves = LAYOUT
        .iter()
        .map(|(new, key, old)| (*old, *key, *new, *key))
        // Newest old name first, so it is the one that moves when there are several.
        .chain(OLD_NAMES.iter().rev().copied())
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
            // A renamed key keeps the comment lines above it.
            let mut new_name = toml_edit::Key::new(key);
            *new_name.leaf_decor_mut() = name.leaf_decor().clone();
            target.insert_formatted(&new_name, item);
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
    // Retired keys do nothing any more: they go from wherever they are, so an
    // old table they were the last in can go too. Keys nobody knows stay.
    for path in RETIRED_KEYS {
        if let Some((table, key)) = path.split_once('.')
            && let Some(t) = doc.get_mut(table).and_then(toml_edit::Item::as_table_mut)
        {
            t.remove(key);
        }
    }
    if let Some(t) = doc.get_mut("pandora").and_then(toml_edit::Item::as_table_mut) {
        for key in OLD_PANDORA_KEYS {
            t.remove(key);
        }
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

/// Deletes `password` from `[account]` and `[pandora]` in the file at `path`
/// (what 0.4 betas saved there), keeping everything else. `Ok(true)` when
/// there was one. Call it only once the keychain has the password.
pub fn forget_plaintext_password(path: &Path) -> Result<bool, ConfigError> {
    let error = |reason: String| ConfigError::Write { path: path.into(), reason };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(error(e.to_string())),
    };
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| error(e.to_string()))?;
    let mut found = false;
    for table in ["account", "pandora"] {
        if let Some(t) = doc.get_mut(table).and_then(toml_edit::Item::as_table_mut) {
            found |= t.remove("password").is_some();
        }
    }
    if !found {
        return Ok(false);
    }
    migrate(&mut doc);
    write_private(path, &doc.to_string()).map_err(|e| error(e.to_string()))?;
    Ok(true)
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
        // The password is never written to the file.
        debug_assert!(*key != "password", "the password belongs in the keychain");
        if *key == "password" {
            continue;
        }
        debug_assert!(
            LAYOUT.iter().any(|(t, k, _)| t == table && k == key) || *table == "browse",
            "{table}.{key} is not in LAYOUT"
        );
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

/// A scratch directory for a test, under `target/tmp` rather than `/tmp`.
///
/// `/tmp` is a small tmpfs on some machines, and a suite that writes a
/// directory per test fills it, after which unrelated tests fail with "Disk
/// quota exceeded". `target/` is already ignored and `cargo clean` removes it.
/// `CARGO_MANIFEST_DIR` is the repository root, so this works from any cwd.
#[cfg(test)]
pub(crate) fn temp_dir(name: &str) -> PathBuf {
    let dir = scratch_dir(&format!("boxvibe-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `target/tmp/<name>`, created if needed.
#[cfg(test)]
pub(crate) fn scratch_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("tmp").join(name);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[cfg(test)]
mod tests;
