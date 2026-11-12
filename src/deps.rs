//! What boxvibe needs from the system besides itself (a Nerd Font, a Secret
//! Service keychain for the password, and `notify-send` for notifications) and whether it is there. This is only
//! a list: boxvibe installs nothing and runs no package manager. Like
//! tubevibe's System Check, it shows every item with a mark, what it is for
//! and the package name, and says so when something is missing.
//!
//! Everything that decides something is a plain function of plain data
//! ([`checks`], [`nerd_font`]), tested with made-up font lists. Only
//! [`probe`] touches the system: it runs `fc-list` and looks in `PATH`.

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Something boxvibe can notice is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dep {
    /// A Nerd Font: the icons (and the logo) are glyphs from it.
    NerdFont,
    /// `notify-send`, for song-change notifications.
    NotifySend,
    /// A Secret Service keychain (GNOME Keyring, KeePassXC…), for the Pandora password.
    Keychain,
}

impl Dep {
    pub fn name(self) -> &'static str {
        match self {
            Self::NerdFont => "Nerd Font symbols",
            Self::NotifySend => "notify-send",
            Self::Keychain => "Secret Service keychain",
        }
    }

    /// What it is for, in a few words.
    pub fn why(self) -> &'static str {
        match self {
            Self::NerdFont => "the icons and the logo (without it they show as empty boxes)",
            Self::NotifySend => "a notification for every new song",
            Self::Keychain => "keeps your Pandora password (without one you type it each start)",
        }
    }

    /// The package that has it, as the list says it.
    pub fn package(self) -> &'static str {
        match self {
            Self::NerdFont => "ttf-nerd-fonts-symbols (Arch) or any Nerd Font",
            Self::NotifySend => "libnotify (Debian: libnotify-bin)",
            Self::Keychain => "a Secret Service keychain, e.g. gnome-keyring or KeePassXC",
        }
    }
}

/// What the system looks like, as far as the check can tell.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// `fc-list : family`; `None` when fontconfig isn't there or didn't answer.
    pub families: Option<String>,
    /// `fc-list ':charset=f0439 f1b2' family`: the fonts that have the glyphs.
    pub glyph_fonts: Option<String>,
    pub notify_send: bool,
    /// A Secret Service keychain answered; `None`: it didn't in time, so boxvibe doesn't say.
    pub keychain: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Present,
    Missing,
    /// Fontconfig isn't there to ask, so boxvibe doesn't know and says nothing.
    Unknown,
}

/// The glyphs that must be there together: the logo and the Nerd set's station icon.
pub const GLYPH_QUERY: &str = ":charset=f0439 f1b2";

/// Whether a Nerd Font is installed. A family named like one counts
/// ("JetBrainsMono Nerd Font", "Symbols Nerd Font Mono", "FiraCode NFM"), and so
/// does any font that has both glyphs. Only installed fonts are seen: not which
/// font the terminal is set to.
pub fn nerd_font(families: Option<&str>, glyph_fonts: Option<&str>) -> Presence {
    let Some(families) = families else { return Presence::Unknown };
    if families.lines().any(is_nerd_family) || glyph_fonts.is_some_and(|f| f.lines().any(|l| !l.trim().is_empty())) {
        Presence::Present
    } else {
        Presence::Missing
    }
}

/// A family line from fontconfig: names are separated by commas, one per language.
fn is_nerd_family(line: &str) -> bool {
    line.split(',').map(str::trim).any(|name| {
        let lower = name.to_ascii_lowercase();
        lower.contains("nerd font")
            // The short names of the 3.x patched fonts: "FiraCode NF", "Hack NFM", "Mono NFP".
            || ["nf", "nfm", "nfp"].iter().any(|s| lower.rsplit_once(' ').is_some_and(|(_, last)| last == *s))
    })
}

/// One line of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Check {
    pub dep: Dep,
    pub presence: Presence,
    /// boxvibe needs it as it is set up (the Nerd icon set is on); else it is optional.
    pub needed: bool,
}

impl Check {
    /// Not missing (a font that can't be checked is not missing).
    pub fn ok(&self) -> bool {
        self.presence != Presence::Missing
    }
}

/// Every item with whether it is there. The Nerd Font is needed while the
/// Nerd icon set is on; `notify-send` and the keychain are always optional
/// (without a keychain the password is asked for at each start).
pub fn checks(nerd_icons: bool, facts: &Facts) -> Vec<Check> {
    let font = nerd_font(facts.families.as_deref(), facts.glyph_fonts.as_deref());
    let notify = if facts.notify_send { Presence::Present } else { Presence::Missing };
    let keychain = match facts.keychain {
        Some(true) => Presence::Present,
        Some(false) => Presence::Missing,
        None => Presence::Unknown,
    };
    vec![
        Check { dep: Dep::NerdFont, presence: font, needed: nerd_icons },
        Check { dep: Dep::NotifySend, presence: notify, needed: false },
        Check { dep: Dep::Keychain, presence: keychain, needed: false },
    ]
}

/// `true` when `program` is a file in one of the `PATH` directories.
pub fn in_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

/// Runs `program args`, and its output when it exits well.
fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Asks the system. The fonts are asked in another thread and only waited
/// for `limit`: a font check that hangs is the same as no fontconfig (the
/// font is unknown, and nothing is said).
pub fn probe(limit: Duration) -> Facts {
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send((output("fc-list", &[":", "family"]), output("fc-list", &[GLYPH_QUERY, "family"])));
    });
    let (families, glyph_fonts) = receive.recv_timeout(limit).unwrap_or_default();
    // The keychain is asked on its own thread too: a desktop with none can take a moment to say so.
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(crate::keychain::system().available());
    });
    let keychain = receive.recv_timeout(limit).ok();
    Facts { families, glyph_fonts, notify_send: in_path("notify-send"), keychain }
}

#[cfg(test)]
mod tests;
