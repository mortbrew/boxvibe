//! Colours. On an Omarchy system boxvibe starts out following the current
//! Omarchy theme (`theme = "omarchy"`) and recolours itself when `omarchy
//! theme set` swaps it; anywhere else it starts on Tokyo Night
//! ([`default_name`]). It also has built-in themes, which Settings → LOOK →
//! Theme cycles and `[look] theme` remembers.
//!
//! Omarchy keeps the theme in use in `~/.local/state/omarchy/current/theme`
//! (Omarchy 4) or `~/.config/omarchy/current/theme` (before). Its
//! `colors.toml` names the palette (`foreground`, `red`, `selection`… in 4;
//! `color0`–`color15` in 3); older themes only carry `alacritty.toml`.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ratatui::style::Color;
use toml::Value;

/// The colours the interface uses, by role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub background: Color,
    pub fg: Color,
    /// Secondary text: hints, labels, the album.
    pub muted: Color,
    /// Background of the selected row and the status bar.
    pub highlight: Color,
    /// Borders of panels without focus.
    pub line: Color,
    /// The focused panel, the progress bar.
    pub accent: Color,
    pub blue: Color,
    pub cyan: Color,
    pub magenta: Color,
    pub green: Color,
    pub orange: Color,
    pub yellow: Color,
    pub red: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[allow(clippy::too_many_arguments)]
const fn palette(
    background: u32,
    fg: u32,
    muted: u32,
    highlight: u32,
    line: u32,
    accent: u32,
    [blue, cyan, magenta, green, orange, yellow, red]: [u32; 7],
) -> Palette {
    Palette {
        background: rgb(background),
        fg: rgb(fg),
        muted: rgb(muted),
        highlight: rgb(highlight),
        line: rgb(line),
        accent: rgb(accent),
        blue: rgb(blue),
        cyan: rgb(cyan),
        magenta: rgb(magenta),
        green: rgb(green),
        orange: rgb(orange),
        yellow: rgb(yellow),
        red: rgb(red),
    }
}

/// tubevibe's built-in colours.
pub const TOKYO_NIGHT: Palette = palette(
    0x1a1b26,
    0xc0caf5,
    0x565f89,
    0x292e42,
    0x3b4261,
    0x7aa2f7,
    [0x7aa2f7, 0x7dcfff, 0xbb9af7, 0x9ece6a, 0xff9e64, 0xe0af68, 0xf7768e],
);

/// The built-in themes, in the order `t` cycles them (after `omarchy`).
/// Colours as in Omarchy's themes of the same name.
pub const BUILT_IN: [(&str, &str, Palette); 8] = [
    ("tokyo-night", "Tokyo Night", TOKYO_NIGHT),
    (
        "gruvbox",
        "Gruvbox",
        palette(
            0x282828,
            0xd4be98,
            0x7c6f64,
            0x504945,
            0x665c54,
            0x7daea3,
            [0x7daea3, 0x89b482, 0xd3869b, 0xa9b665, 0xe1875c, 0xd8a657, 0xea6962],
        ),
    ),
    (
        "catppuccin",
        "Catppuccin",
        palette(
            0x1e1e2e,
            0xcdd6f4,
            0x6c7086,
            0x313244,
            0x45475a,
            0x89b4fa,
            [0x89b4fa, 0x94e2d5, 0xcba6f7, 0xa6e3a1, 0xfab387, 0xf9e2af, 0xf38ba8],
        ),
    ),
    (
        "catppuccin-latte",
        "Catppuccin Latte",
        palette(
            0xeff1f5,
            0x4c4f69,
            0x8c8fa1,
            0xccd0da,
            0xacb0be,
            0x1e66f5,
            [0x1e66f5, 0x179299, 0x8839ef, 0x40a02b, 0xfe640b, 0xdf8e1d, 0xd20f39],
        ),
    ),
    (
        "rose-pine",
        "Rosé Pine Dawn",
        palette(
            0xfaf4ed,
            0x575279,
            0x9893a5,
            0xdfdad9,
            0xcecacd,
            0x56949f,
            [0x56949f, 0xd7827e, 0x907aa9, 0x286983, 0xcf8057, 0xea9d34, 0xb4637a],
        ),
    ),
    (
        "nord",
        "Nord",
        palette(
            0x2e3440,
            0xd8dee9,
            0x7b88a1,
            0x3b4252,
            0x4c566a,
            0x88c0d0,
            [0x81a1c1, 0x88c0d0, 0xb48ead, 0xa3be8c, 0xd08770, 0xebcb8b, 0xbf616a],
        ),
    ),
    (
        "everforest",
        "Everforest",
        palette(
            0x2d353b,
            0xd3c6aa,
            0x859289,
            0x3d484d,
            0x4f585e,
            0xa7c080,
            [0x7fbbb3, 0x83c092, 0xd699b6, 0xa7c080, 0xe69875, 0xdbbc7f, 0xe67e80],
        ),
    ),
    (
        "kanagawa",
        "Kanagawa",
        palette(
            0x1f1f28,
            0xdcd7ba,
            0x727169,
            0x2d4f67,
            0x54546d,
            0x7e9cd8,
            [0x7e9cd8, 0x7fb4ca, 0x957fb8, 0x98bb6c, 0xffa066, 0xe6c384, 0xe82424],
        ),
    ),
];

/// Every theme name `t` cycles, `omarchy` first.
pub fn names() -> impl Iterator<Item = &'static str> {
    std::iter::once("omarchy").chain(BUILT_IN.iter().map(|(n, ..)| *n))
}

/// The theme after `name` (an unknown name starts over).
#[cfg(test)]
pub fn next(name: &str) -> &'static str {
    let all: Vec<_> = names().collect();
    let at = all.iter().position(|n| *n == name);
    all[at.map_or(0, |i| (i + 1) % all.len())]
}

/// The name shown to the user.
pub fn label(name: &str) -> String {
    if name == "omarchy" {
        return "Omarchy (follows the system)".into();
    }
    BUILT_IN.iter().find(|(n, ..)| *n == name).map_or_else(|| name.to_owned(), |(_, l, _)| (*l).to_owned())
}

/// The built-in palette called `name`.
pub fn built_in(name: &str) -> Option<Palette> {
    BUILT_IN.iter().find(|(n, ..)| *n == name).map(|(.., p)| *p)
}

/// Whether this looks like an Omarchy system: one of its folders is in the
/// user's config, data or state directory, or one of its commands is on the PATH.
fn looks_like_omarchy(folders: &[PathBuf], path: Option<&std::ffi::OsStr>) -> bool {
    folders.iter().any(|d| d.is_dir())
        || path.is_some_and(|path| {
            std::env::split_paths(path)
                .any(|dir| ["omarchy", "omarchy-theme-set"].iter().any(|command| dir.join(command).is_file()))
        })
}

/// Is this an Omarchy system?
pub fn is_omarchy() -> bool {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let Some(home) = var("HOME") else { return false };
    let folders = [
        var("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")).join("omarchy"),
        var("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share")).join("omarchy"),
        var("XDG_STATE_HOME").unwrap_or_else(|| home.join(".local/state")).join("omarchy"),
    ];
    looks_like_omarchy(&folders, std::env::var_os("PATH").as_deref())
}

/// The theme used until one is saved: `omarchy` (follow the system) on an
/// Omarchy system, `tokyo-night` anywhere else.
pub fn default_name() -> &'static str {
    if is_omarchy() { "omarchy" } else { FALLBACK }
}

/// Theme Select's choice until one is saved.
pub const FALLBACK: &str = "tokyo-night";

// --- Omarchy theme files (ported from tubevibe's theme.rs) ---

type Rgb = (u8, u8, u8);

/// `#rrggbb`, `0xrrggbb` (alacritty) or `rrggbb`.
fn hex(text: &str) -> Option<Rgb> {
    let digits = text.trim().trim_start_matches('#').trim_start_matches("0x").trim_start_matches("0X");
    if digits.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(digits, 16).ok()?;
    Some(((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

fn color((r, g, b): Rgb) -> Color {
    Color::Rgb(r, g, b)
}

fn luminance((r, g, b): Rgb) -> f64 {
    let channel = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn mix(from: Rgb, to: Rgb, amount: f64) -> Rgb {
    let step = |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * amount).round() as u8;
    (step(from.0, to.0), step(from.1, to.1), step(from.2, to.2))
}

#[derive(Default)]
struct Named {
    background: Option<Rgb>,
    fg: Option<Rgb>,
    muted: Option<Rgb>,
    selection: Option<Rgb>,
    line: Option<Rgb>,
    accent: Option<Rgb>,
    /// black red green yellow blue magenta cyan white.
    normal: [Option<Rgb>; 8],
    bright: [Option<Rgb>; 8],
    orange: Option<Rgb>,
}

impl Named {
    fn palette(self) -> Option<Palette> {
        let background = self.background.or(self.normal[0])?;
        let fg = self.fg.or(self.normal[7])?;
        let pick = |i: usize| self.normal[i].or(self.bright[i]);
        let (red, green, yellow, blue, magenta, cyan) = (pick(1)?, pick(2)?, pick(3)?, pick(4)?, pick(5)?, pick(6)?);
        let readable = |c: &Rgb| contrast(*c, background) >= 2.0;
        let muted = self.muted.filter(readable).unwrap_or_else(|| mix(fg, background, 0.45));
        let near = |c: &Rgb| contrast(*c, background) < 2.0 && contrast(*c, fg) >= 3.0;
        let highlight = self.selection.filter(near).unwrap_or_else(|| mix(background, fg, 0.14));
        let line = self.line.filter(|c| *c != background).unwrap_or_else(|| mix(background, fg, 0.25));
        Some(Palette {
            background: color(background),
            fg: color(fg),
            muted: color(muted),
            highlight: color(highlight),
            line: color(line),
            accent: color(self.accent.unwrap_or(blue)),
            blue: color(blue),
            cyan: color(cyan),
            magenta: color(magenta),
            green: color(green),
            orange: color(self.orange.unwrap_or_else(|| mix(red, yellow, 0.5))),
            yellow: color(yellow),
            red: color(red),
        })
    }
}

const NAMES: [&str; 8] = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];

/// An Omarchy `colors.toml` (Omarchy 4's names or Omarchy 3's `color0`–`color15`).
pub fn parse_colors_toml(text: &str) -> Option<Palette> {
    let table: toml::Table = text.parse().ok()?;
    let get = |key: &str| table.get(key).and_then(Value::as_str).and_then(hex);
    let mut named = Named {
        background: get("background"),
        fg: get("bright_foreground").or(get("foreground")),
        muted: get("dark_foreground"),
        selection: get("selection").or(get("selection_background")),
        line: get("muted"),
        accent: get("accent"),
        orange: get("orange"),
        ..Named::default()
    };
    for (i, name) in NAMES.iter().enumerate() {
        named.normal[i] = get(name).or(get(&format!("color{i}")));
        named.bright[i] = get(&format!("bright_{name}")).or(get(&format!("color{}", i + 8)));
    }
    named.palette()
}

/// An alacritty theme (the only file of Omarchy 2 themes).
pub fn parse_alacritty(text: &str) -> Option<Palette> {
    let table: toml::Table = text.parse().ok()?;
    let colors = table.get("colors")?;
    let get = |group: &str, key: &str| colors.get(group)?.get(key)?.as_str().and_then(hex);
    let mut named = Named {
        background: get("primary", "background"),
        fg: get("primary", "foreground"),
        muted: get("primary", "dim_foreground"),
        selection: get("selection", "background"),
        ..Named::default()
    };
    for (i, name) in NAMES.iter().enumerate() {
        named.normal[i] = get("normal", name);
        named.bright[i] = get("bright", name);
    }
    named.palette()
}

/// A downloaded or saved scheme: alacritty (`[colors.primary]`…) or an Omarchy `colors.toml`.
pub fn parse_any(text: &str) -> Option<Palette> {
    let table: toml::Table = text.parse().ok()?;
    if table.get("colors").is_some_and(Value::is_table) { parse_alacritty(text) } else { parse_colors_toml(text) }
}

/// Where Settings → More themes saves schemes: `~/.config/boxvibe/themes/`.
pub fn user_dir() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("boxvibe/themes"))
}

/// The saved schemes in `dir` (`<name>.toml`), by name; ones that don't
/// parse or would shadow a built-in theme are left out.
pub fn load_user(dir: &Path) -> Vec<(String, Palette)> {
    let Ok(read_dir) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<(String, Palette)> = read_dir
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension()? != "toml" {
                return None;
            }
            let name = path.file_stem()?.to_str()?.to_owned();
            if name == "omarchy" || built_in(&name).is_some() {
                return None;
            }
            Some((name, parse_any(&std::fs::read_to_string(&path).ok()?)?))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

fn theme_file(dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter().find_map(|dir| ["colors.toml", "alacritty.toml"].iter().map(|f| dir.join(f)).find(|f| f.is_file()))
}

fn read(file: &Path) -> Option<Palette> {
    let text = std::fs::read_to_string(file).ok()?;
    if file.file_name().is_some_and(|n| n == "alacritty.toml") {
        parse_alacritty(&text)
    } else {
        parse_colors_toml(&text)
    }
}

/// Follows the current Omarchy theme: [`Watcher::poll`] answers the palette
/// when the theme file changed since the last call.
pub struct Watcher {
    dirs: Vec<PathBuf>,
    last: Option<Option<(PathBuf, Option<SystemTime>, u64)>>,
}

impl Watcher {
    pub fn system() -> Self {
        let home = std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from);
        let state = std::env::var_os("XDG_STATE_HOME").filter(|s| !s.is_empty()).map(PathBuf::from);
        let dirs = match home {
            Some(home) => {
                let state = state.unwrap_or_else(|| home.join(".local/state"));
                vec![state.join("omarchy/current/theme"), home.join(".config/omarchy/current/theme")]
            }
            None => Vec::new(),
        };
        Self { dirs, last: None }
    }

    pub fn reset(&mut self) {
        self.last = None;
    }

    /// The Omarchy palette when it changed (or on the first call); Tokyo
    /// Night when there is no Omarchy theme.
    pub fn poll(&mut self) -> Option<Palette> {
        let file = theme_file(&self.dirs);
        let stamp = file.as_deref().and_then(|f| {
            let real = std::fs::canonicalize(f).ok()?;
            let meta = std::fs::metadata(&real).ok()?;
            Some((real, meta.modified().ok(), meta.len()))
        });
        if self.last.as_ref() == Some(&stamp) {
            return None;
        }
        self.last = Some(stamp);
        Some(file.as_deref().and_then(read).unwrap_or(TOKYO_NIGHT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_cycle_wraps() {
        assert_eq!(next("omarchy"), "tokyo-night");
        assert_eq!(next("kanagawa"), "omarchy");
        assert_eq!(next("nope"), "omarchy");
        assert_eq!(names().count(), 9);
        assert_eq!(label("gruvbox"), "Gruvbox");
    }

    #[test]
    fn an_omarchy_system_is_told_by_its_folders_or_commands() {
        let dir = crate::config::temp_dir("omarchy-detect");
        let nothing: [PathBuf; 2] = [dir.join("config/omarchy"), dir.join("data/omarchy")];
        assert!(!looks_like_omarchy(&nothing, None));
        let empty_path = std::env::join_paths([dir.join("bin")]).unwrap();
        assert!(!looks_like_omarchy(&nothing, Some(&empty_path)));
        // A folder of Omarchy's, in any of the places.
        std::fs::create_dir_all(dir.join("data/omarchy")).unwrap();
        assert!(looks_like_omarchy(&nothing, None));
        std::fs::remove_dir_all(dir.join("data")).unwrap();
        assert!(!looks_like_omarchy(&nothing, None));
        // Or one of its commands.
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin/omarchy-theme-set"), "#!/bin/sh\n").unwrap();
        assert!(looks_like_omarchy(&nothing, Some(&empty_path)));
        std::fs::remove_file(dir.join("bin/omarchy-theme-set")).unwrap();
        std::fs::write(dir.join("bin/omarchy"), "#!/bin/sh\n").unwrap();
        assert!(looks_like_omarchy(&nothing, Some(&empty_path)));
    }

    #[test]
    fn the_default_theme_exists_and_the_names_are_known() {
        assert!(names().any(|n| n == default_name()), "{}", default_name());
        assert!(matches!(default_name(), "omarchy" | "tokyo-night"));
        assert!(built_in("tokyo-night").is_some());
    }

    #[test]
    fn saved_themes_load() {
        let dir = crate::config::temp_dir("user-themes");
        std::fs::create_dir_all(&dir).unwrap();
        let alacritty = "[colors.primary]\nbackground = '#101010'\nforeground = '#eeeeee'\n[colors.normal]\nblack = '#000000'\nred = '#ff0000'\ngreen = '#00ff00'\nyellow = '#ffff00'\nblue = '#0000ff'\nmagenta = '#ff00ff'\ncyan = '#00ffff'\nwhite = '#ffffff'\n";
        std::fs::write(dir.join("zeta.toml"), alacritty).unwrap();
        std::fs::write(dir.join("gruvbox.toml"), alacritty).unwrap();
        std::fs::write(dir.join("broken.toml"), "x = 1").unwrap();
        std::fs::write(dir.join("notes.txt"), alacritty).unwrap();
        let user = load_user(&dir);
        assert_eq!(user.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["zeta"]);
        assert_eq!(user[0].1.background, Color::Rgb(0x10, 0x10, 0x10));
        assert!(load_user(&dir.join("missing")).is_empty());
    }

    #[test]
    fn omarchy_colors_toml() {
        let text = "background = \"#282828\"\nforeground = \"#d4be98\"\nred = \"#ea6962\"\ngreen = \"#a9b665\"\n\
                    yellow = \"#d8a657\"\nblue = \"#7daea3\"\nmagenta = \"#d3869b\"\ncyan = \"#89b482\"\n";
        let p = parse_colors_toml(text).unwrap();
        assert_eq!((p.background, p.fg, p.red), (rgb(0x282828), rgb(0xd4be98), rgb(0xea6962)));
        assert_eq!(parse_colors_toml("background = \"#000000\""), None);
    }

    #[test]
    fn built_in_themes_are_readable() {
        let c = |x: Color| match x {
            Color::Rgb(r, g, b) => (r, g, b),
            _ => panic!(),
        };
        for (name, _, p) in BUILT_IN {
            assert!(contrast(c(p.fg), c(p.background)) > 4.5, "{name} text");
            assert!(contrast(c(p.fg), c(p.highlight)) >= 3.0, "{name} selected row");
        }
    }
}
