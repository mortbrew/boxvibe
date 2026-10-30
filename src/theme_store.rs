//! Settings → More themes: the colour schemes of iTerm2-Color-Schemes (its
//! alacritty files, about 900) and Omarchy's own themes, listed from GitHub.
//! One you pick is saved in `~/.config/boxvibe/themes/<name>.toml` and joins
//! the Theme list.
//!
//! Network calls run on a thread and answer through a channel; the demo
//! answers from a few made-up schemes and never touches the network.

use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use crate::theme;

const ITERM_LIST: &str = "https://api.github.com/repos/mbadolato/iTerm2-Color-Schemes/contents/alacritty";
const OMARCHY_LIST: &str = "https://api.github.com/repos/omacom/omarchy/contents/themes";
const OMARCHY_RAW: &str = "https://raw.githubusercontent.com/omacom/omarchy/HEAD/themes";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Iterm,
    Omarchy,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Iterm => "iTerm2-Color-Schemes",
            Self::Omarchy => "Omarchy",
        }
    }
}

/// One scheme to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub source: Source,
    /// As the source names it ("Builtin Solarized Dark", "nord").
    pub name: String,
    /// The file (iTerm2) or the theme's folder (Omarchy: `colors.toml`, else `alacritty.toml`).
    pub url: String,
}

impl Entry {
    /// The theme name boxvibe saves it as: the file name and config value.
    pub fn slug(&self) -> String {
        let base = slug(&self.name);
        match self.source {
            Source::Omarchy => format!("omarchy-{base}"),
            Source::Iterm if theme::built_in(&base).is_some() || base == "omarchy" || base.starts_with("omarchy-") => {
                format!("{base}-iterm")
            }
            Source::Iterm => base,
        }
    }
}

/// Lowercase letters and digits, the rest one `-`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

pub enum Msg {
    Catalog(Result<Vec<Entry>, String>),
    /// The slug and the file's text, or why not.
    Downloaded(Entry, Result<String, String>),
}

/// The schemes of both sources, on a thread.
pub fn fetch_catalog(demo: bool) -> Receiver<Msg> {
    spawn(move || Msg::Catalog(if demo { Ok(demo_catalog()) } else { catalog() }))
}

/// One scheme's file, checked to be a theme boxvibe can read.
pub fn fetch_theme(entry: Entry, demo: bool) -> Receiver<Msg> {
    spawn(move || {
        let text = if demo { demo_file(&entry) } else { download(&entry) };
        let text = text.and_then(|t| match theme::parse_any(&t) {
            Some(_) => Ok(t),
            None => Err("it isn't a colour scheme boxvibe can read".into()),
        });
        Msg::Downloaded(entry, text)
    })
}

fn spawn(work: impl FnOnce() -> Msg + Send + 'static) -> Receiver<Msg> {
    let (tx, rx) = mpsc::channel();
    let _ = std::thread::Builder::new().name("themes".into()).spawn(move || {
        let _ = tx.send(work());
    });
    rx
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .user_agent(concat!("boxvibe/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn get(url: &str) -> Result<String, String> {
    match agent().get(url).header("Accept", "application/vnd.github+json").call() {
        Ok(mut reply) => reply.body_mut().read_to_string().map_err(|e| e.to_string()),
        Err(ureq::Error::StatusCode(403 | 429)) => Err("GitHub's rate limit: try again in an hour".into()),
        Err(ureq::Error::StatusCode(404)) => Err("not found on GitHub".into()),
        Err(ureq::Error::StatusCode(s)) => Err(format!("GitHub answered {s}")),
        Err(e) => Err(format!("can't reach GitHub ({e})")),
    }
}

fn catalog() -> Result<Vec<Entry>, String> {
    let iterm = get(ITERM_LIST).map(|t| parse_iterm_list(&t));
    let omarchy = get(OMARCHY_LIST).map(|t| parse_omarchy_list(&t, OMARCHY_RAW));
    match (omarchy, iterm) {
        (Err(a), Err(_)) => Err(a),
        (a, b) => {
            let mut all = a.unwrap_or_default();
            all.extend(b.unwrap_or_default());
            Ok(all)
        }
    }
}

fn download(entry: &Entry) -> Result<String, String> {
    match entry.source {
        Source::Iterm => get(&entry.url),
        Source::Omarchy => get(&format!("{}/colors.toml", entry.url))
            .or_else(|first| get(&format!("{}/alacritty.toml", entry.url)).map_err(|_| first)),
    }
}

fn list(json: &str) -> Vec<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(json).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default()
}

fn field<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v.get(key).and_then(|s| s.as_str()).unwrap_or("")
}

/// GitHub's listing of iTerm2-Color-Schemes' `alacritty/` folder.
pub fn parse_iterm_list(json: &str) -> Vec<Entry> {
    list(json)
        .iter()
        .filter(|v| field(v, "type") == "file" && !field(v, "download_url").is_empty())
        .filter_map(|v| {
            let name = field(v, "name").strip_suffix(".toml")?;
            Some(Entry { source: Source::Iterm, name: name.to_owned(), url: field(v, "download_url").to_owned() })
        })
        .collect()
}

/// GitHub's listing of Omarchy's `themes/` folder (one folder per theme).
pub fn parse_omarchy_list(json: &str, raw: &str) -> Vec<Entry> {
    list(json)
        .iter()
        .filter(|v| field(v, "type") == "dir" && !field(v, "name").is_empty())
        .map(|v| Entry {
            source: Source::Omarchy,
            name: field(v, "name").to_owned(),
            url: format!("{raw}/{}", field(v, "name")),
        })
        .collect()
}

/// Saves a downloaded scheme as `<dir>/<slug>.toml`.
pub fn save(dir: &Path, slug: &str, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(format!("{slug}.toml")), text)
}

/// Deletes the saved scheme `<dir>/<slug>.toml` (a missing file is fine).
pub fn remove(dir: &Path, slug: &str) -> std::io::Result<()> {
    match std::fs::remove_file(dir.join(format!("{slug}.toml"))) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

pub(crate) fn demo_catalog() -> Vec<Entry> {
    [
        (Source::Omarchy, "nord"),
        (Source::Omarchy, "rose-pine"),
        (Source::Iterm, "Dracula"),
        (Source::Iterm, "Solarized Dark"),
        (Source::Iterm, "Broken"),
    ]
    .into_iter()
    .map(|(source, name)| Entry { source, name: name.into(), url: format!("demo:{name}") })
    .collect()
}

fn demo_file(entry: &Entry) -> Result<String, String> {
    if entry.name == "Broken" {
        return Ok("not a theme".into());
    }
    let n = entry.name.len() as u32;
    let c = |base: u32| format!("'#{:06x}'", (base + n * 0x010203) & 0xffffff);
    Ok(format!(
        "[colors.primary]\nbackground = {}\nforeground = {}\n[colors.normal]\nblack = {}\nred = {}\ngreen = {}\nyellow = {}\nblue = {}\nmagenta = {}\ncyan = {}\nwhite = {}\n",
        c(0x101010),
        c(0xe0e0e0),
        c(0x202020),
        c(0xd04040),
        c(0x40c040),
        c(0xd0c040),
        c(0x4060d0),
        c(0xb050c0),
        c(0x40c0c0),
        c(0xd0d0d0)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_listings_become_entries() {
        let iterm = r#"[{"name":"0x96f.toml","type":"file","download_url":"https://raw/x/0x96f.toml"},
            {"name":"README.md","type":"file","download_url":"https://raw/x/README.md"},
            {"name":"Builtin Solarized Dark.toml","type":"file","download_url":"https://raw/x/b.toml"}]"#;
        let e = parse_iterm_list(iterm);
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].name.as_str(), e[0].url.as_str()), ("0x96f", "https://raw/x/0x96f.toml"));
        assert_eq!(e[1].slug(), "builtin-solarized-dark");
        let omarchy = r#"[{"name":"nord","type":"dir"},{"name":"README","type":"file"}]"#;
        let o = parse_omarchy_list(omarchy, "https://raw/o");
        assert_eq!(o.len(), 1);
        assert_eq!((o[0].slug().as_str(), o[0].url.as_str()), ("omarchy-nord", "https://raw/o/nord"));
        assert!(parse_iterm_list(r#"{"message":"API rate limit exceeded"}"#).is_empty());
    }

    #[test]
    fn slugs_never_shadow_built_in_themes() {
        let e = |name: &str| Entry { source: Source::Iterm, name: name.into(), url: String::new() };
        assert_eq!(e("Gruvbox").slug(), "gruvbox-iterm");
        assert_eq!(e("Omarchy").slug(), "omarchy-iterm");
        assert_eq!(e("Tokyo Night (Storm)").slug(), "tokyo-night-storm");
        assert_eq!(slug("  A++ b  "), "a-b");
    }

    #[test]
    fn the_demo_downloads_without_the_network_and_checks_the_file() {
        let all = demo_catalog();
        let get = |name: &str| {
            let entry = all.iter().find(|e| e.name == name).unwrap().clone();
            match fetch_theme(entry, true).recv_timeout(Duration::from_secs(2)).unwrap() {
                Msg::Downloaded(_, text) => text,
                Msg::Catalog(_) => unreachable!(),
            }
        };
        assert!(theme::parse_any(&get("Dracula").unwrap()).is_some());
        assert!(get("Broken").unwrap_err().contains("can read"));
    }

    /// Talks to GitHub: `cargo test themes_from_github -- --ignored`.
    #[test]
    #[ignore]
    fn themes_from_github() {
        let all = catalog().unwrap();
        let iterm = all.iter().filter(|e| e.source == Source::Iterm).count();
        let omarchy: Vec<_> = all.iter().filter(|e| e.source == Source::Omarchy).collect();
        assert!(iterm > 500 && omarchy.len() > 10, "{iterm} {}", omarchy.len());
        for e in [omarchy[0], all.iter().find(|e| e.source == Source::Iterm).unwrap()] {
            let text = download(e).unwrap();
            assert!(theme::parse_any(&text).is_some(), "{}", e.name);
        }
        eprintln!("{iterm} iTerm2 + {} Omarchy", omarchy.len());
    }
}
