//! What boxvibe needs from the system besides itself, checked once when it
//! opens, and the one command that installs whatever is missing.
//!
//! Everything that decides something is a plain function of plain data
//! (`missing`, `is_arch`, `install_line`, `terminal_argv`, ...), tested with
//! made-up font lists and package lists. Only [`probe`] touches the system: it
//! runs `fc-list`, reads `/etc/os-release` and looks in `PATH`. boxvibe never
//! runs `sudo` or a package manager itself: on Arch it opens the user's own
//! terminal with the command in it, and the user types their password there.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::config::{Config, IconSet};

/// Something boxvibe can notice is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dep {
    /// A Nerd Font: the icons (and the logo) are glyphs from it.
    NerdFont,
    /// `notify-send`, for song-change notifications.
    NotifySend,
}

impl Dep {
    pub fn name(self) -> &'static str {
        match self {
            Self::NerdFont => "Nerd Font symbols",
            Self::NotifySend => "notify-send",
        }
    }

    /// Why boxvibe wants it, in a few words.
    pub fn why(self) -> &'static str {
        match self {
            Self::NerdFont => "the icons and the logo are its glyphs; without it they show as empty boxes",
            Self::NotifySend => "song-change notifications (Settings → PLAYBACK → Notifications) are sent with it",
        }
    }

    /// The Arch package that provides it.
    pub fn arch_package(self) -> &'static str {
        match self {
            Self::NerdFont => "ttf-nerd-fonts-symbols",
            Self::NotifySend => "libnotify",
        }
    }

    /// `true` when the package is only in the AUR (none is now; the install
    /// command handles it anyway, with `yay` or `paru`).
    pub fn aur(self) -> bool {
        false
    }

    /// What to look for on a system that isn't Arch.
    pub fn elsewhere(self) -> &'static str {
        match self {
            Self::NerdFont => "\"Symbols Nerd Font\" (nerdfonts.com, or your distribution's nerd-fonts package)",
            Self::NotifySend => "libnotify (Debian and Ubuntu call it libnotify-bin)",
        }
    }
}

/// What the settings make boxvibe use, so what is worth checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wants {
    /// The Nerd icon set is on.
    pub nerd_icons: bool,
    /// Notifications are on (and this isn't the demo, which sends none).
    pub notifications: bool,
}

impl Wants {
    pub fn from_config(config: &Config, demo: bool) -> Self {
        Self { nerd_icons: config.ui.icons == IconSet::Nerd, notifications: config.ui.notifications && !demo }
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
    pub pacman: bool,
    /// `yay` or `paru`, if either is installed.
    pub aur_helper: Option<String>,
    /// The text of `/etc/os-release`.
    pub os_release: Option<String>,
    /// A graphical session is running (so a terminal window can open).
    pub display: bool,
    /// `$TERMINAL`.
    pub terminal_env: Option<String>,
    /// The programs of [`TERMINALS`] that are in `PATH` (and `xdg-terminal-exec`).
    pub terminals: Vec<String>,
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

/// What is missing, in the order it is shown. A font that can't be checked is
/// not missing.
pub fn missing(wants: Wants, facts: &Facts) -> Vec<Dep> {
    let mut out = Vec::new();
    if wants.nerd_icons && nerd_font(facts.families.as_deref(), facts.glyph_fonts.as_deref()) == Presence::Missing {
        out.push(Dep::NerdFont);
    }
    if wants.notifications && !facts.notify_send {
        out.push(Dep::NotifySend);
    }
    out
}

/// Arch and what is built on it (Manjaro, EndeavourOS, CachyOS, Omarchy...),
/// from the text of `/etc/os-release`.
pub fn is_arch(os_release: &str) -> bool {
    os_release.lines().any(|line| {
        let Some((key, value)) = line.split_once('=') else { return false };
        let value = value.trim().trim_matches(['"', '\'']);
        match key.trim() {
            "ID" => matches!(value, "arch" | "archarm" | "omarchy"),
            "ID_LIKE" => value.split_whitespace().any(|v| v == "arch"),
            _ => false,
        }
    })
}

/// The install command for `missing` and what it can't cover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    /// `sudo pacman -S --needed a b`, or `yay -S --needed a b`.
    pub line: String,
    /// AUR-only packages when no AUR helper is installed: install them by hand.
    pub left_out: Vec<Dep>,
}

/// One command that installs every missing package (and only those): pacman,
/// or `yay` / `paru` when something is AUR-only. `None` when there is nothing it can install.
pub fn install_line(missing: &[Dep], aur_helper: Option<&str>) -> Option<Install> {
    let any_aur = missing.iter().any(|d| d.aur());
    let (line, left_out): (String, Vec<Dep>) = match (any_aur, aur_helper) {
        (true, Some(helper)) => (format!("{helper} -S --needed"), Vec::new()),
        _ => ("sudo pacman -S --needed".into(), missing.iter().copied().filter(|d| d.aur()).collect()),
    };
    let packages: Vec<&str> = missing.iter().filter(|d| !left_out.contains(d)).map(|d| d.arch_package()).collect();
    (!packages.is_empty()).then(|| Install { line: format!("{line} {}", packages.join(" ")), left_out })
}

/// What the terminal window runs: the command, then a pause so its output can be read.
pub fn script(line: &str) -> String {
    format!("echo '$ {line}'; {line}; echo; read -r -p 'Press Enter to close this window ' _")
}

/// The terminals tried after `xdg-terminal-exec` and `$TERMINAL`, in order.
pub const TERMINALS: [&str; 8] =
    ["foot", "ghostty", "kitty", "alacritty", "wezterm", "konsole", "gnome-terminal", "xterm"];

/// The terminal to open: `xdg-terminal-exec` (Omarchy has it), then
/// `$TERMINAL`, then [`TERMINALS`] in order. `has` says whether a program is installed.
pub fn pick_terminal(env: Option<&str>, has: impl Fn(&str) -> bool) -> Option<String> {
    if has("xdg-terminal-exec") {
        return Some("xdg-terminal-exec".into());
    }
    // A plain program name only: `$TERMINAL` with arguments in it isn't guessed at.
    if let Some(env) = env.map(str::trim).filter(|e| !e.is_empty() && !e.contains(char::is_whitespace))
        && has(env)
    {
        return Some(env.into());
    }
    TERMINALS.iter().find(|t| has(t)).map(|t| (*t).into())
}

/// The command line that opens `program` running `bash -c script`, with the
/// flag each terminal wants before the command. A program that isn't known
/// gets `-e`, which most take.
pub fn terminal_argv(program: &str, script: &str) -> Vec<String> {
    let name = Path::new(program).file_name().and_then(|n| n.to_str()).unwrap_or(program);
    // What goes between the program and `bash`.
    let before: &[&str] = match name {
        // Takes the command as it is, and adds its own terminal's flag.
        "xdg-terminal-exec" | "foot" | "kitty" => &[],
        "wezterm" => &["start", "--"],
        "gnome-terminal" => &["--wait", "--"],
        "konsole" => &["--nofork", "-e"],
        // ghostty, alacritty, xterm and anything else.
        _ => &["-e"],
    };
    let mut argv = vec![program.to_owned()];
    argv.extend(before.iter().map(|s| (*s).to_owned()));
    argv.extend(["bash".to_owned(), "-c".to_owned(), script.to_owned()]);
    argv
}

/// What the startup screen shows and offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub missing: Vec<Dep>,
    /// The command that installs them, when this is an Arch system.
    pub install: Option<Install>,
    /// `y` opens a terminal with that command: Arch, a graphical session and a
    /// terminal to open.
    pub can_install: bool,
    /// The terminal has been opened and the screen waits for it.
    pub waiting: bool,
    /// Why nothing opened, after a failed try.
    pub note: Option<String>,
}

/// The screen for what is missing, or `None` when nothing is.
pub fn prompt(wants: Wants, facts: &Facts) -> Option<Prompt> {
    let missing = missing(wants, facts);
    if missing.is_empty() {
        return None;
    }
    let arch = facts.os_release.as_deref().is_some_and(is_arch) && facts.pacman;
    let install = arch.then(|| install_line(&missing, facts.aur_helper.as_deref())).flatten();
    let can_install = install.is_some()
        && facts.display
        && pick_terminal(facts.terminal_env.as_deref(), |p| facts.terminals.iter().any(|t| t == p)).is_some();
    Some(Prompt { missing, install, can_install, waiting: false, note: None })
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
/// font is unknown, and nothing is said). The rest is quick, and always answered.
pub fn probe(limit: Duration) -> Facts {
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send((output("fc-list", &[":", "family"]), output("fc-list", &[GLYPH_QUERY, "family"])));
    });
    let (families, glyph_fonts) = receive.recv_timeout(limit).unwrap_or_default();
    let mut terminals: Vec<String> =
        TERMINALS.iter().chain(&["xdg-terminal-exec"]).filter(|t| in_path(t)).map(|t| (*t).to_owned()).collect();
    let terminal_env = std::env::var("TERMINAL").ok().filter(|t| !t.trim().is_empty());
    if let Some(t) = terminal_env.as_deref().filter(|t| in_path(t)) {
        terminals.push(t.to_owned());
    }
    Facts {
        families,
        glyph_fonts,
        notify_send: in_path("notify-send"),
        pacman: in_path("pacman"),
        aur_helper: ["yay", "paru"].iter().find(|h| in_path(h)).map(|h| (*h).to_owned()),
        os_release: std::fs::read_to_string("/etc/os-release").ok(),
        display: ["WAYLAND_DISPLAY", "DISPLAY"].iter().any(|v| std::env::var_os(v).is_some_and(|d| !d.is_empty())),
        terminal_env,
        terminals,
    }
}

/// Opens the terminal window with the install command in it (`bash -c` of
/// [`script`]) and returns it to wait on. `Err` says why not, for the screen.
pub fn launch(prompt: &Prompt, facts: &Facts) -> Result<std::process::Child, String> {
    let install = prompt.install.as_ref().ok_or("nothing to install from here")?;
    let program = pick_terminal(facts.terminal_env.as_deref(), |p| facts.terminals.iter().any(|t| t == p))
        .ok_or("no terminal found to open")?;
    let argv = terminal_argv(&program, &script(&install.line));
    Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("couldn't open {program}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONTS: &str =
        "DejaVu Sans\nNoto Sans,Noto Sans Display\nJetBrainsMono Nerd Font,JetBrainsMono Nerd Font Mono\n";
    const PLAIN: &str = "DejaVu Sans\nNoto Sans,Noto Sans Display\nLiberation Mono\nFont Awesome 6 Free\n";
    const ARCH: &str = "NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\nBUILD_ID=rolling\n";

    fn want() -> Wants {
        Wants { nerd_icons: true, notifications: true }
    }

    fn arch_facts() -> Facts {
        Facts {
            families: Some(PLAIN.into()),
            glyph_fonts: Some(String::new()),
            notify_send: false,
            pacman: true,
            os_release: Some(ARCH.into()),
            display: true,
            terminals: vec!["xdg-terminal-exec".into(), "kitty".into()],
            ..Facts::default()
        }
    }

    #[test]
    fn nerd_font_is_found_by_family_name() {
        assert_eq!(nerd_font(Some(FONTS), None), Presence::Present);
        for family in [
            "Symbols Nerd Font",
            "Symbols Nerd Font Mono",
            "Hack Nerd Font Propo",
            "FiraCode NF",
            "Mono NFM",
            "x,JetBrainsMono NFP",
        ] {
            assert_eq!(nerd_font(Some(family), None), Presence::Present, "{family}");
        }
        assert_eq!(nerd_font(Some(PLAIN), None), Presence::Missing, "FontAwesome and friends are not a Nerd Font");
        assert_eq!(
            nerd_font(Some("Inform Sans\nSans NFL Condensed"), None),
            Presence::Missing,
            "NF has to be a word of its own"
        );
        assert_eq!(nerd_font(Some(""), Some("")), Presence::Missing, "fontconfig with no fonts at all");
    }

    #[test]
    fn nerd_font_is_found_by_its_glyphs_when_the_name_gives_nothing_away() {
        // A renamed font that has the logo and the station glyph.
        assert_eq!(nerd_font(Some(PLAIN), Some("My Patched Mono\n")), Presence::Present);
        assert_eq!(nerd_font(Some(PLAIN), Some("\n")), Presence::Missing);
        // The glyph query alone is no use without the answer to the first one.
        assert_eq!(nerd_font(None, Some("My Patched Mono\n")), Presence::Unknown);
    }

    #[test]
    fn no_fontconfig_means_unknown_and_no_nag() {
        assert_eq!(nerd_font(None, None), Presence::Unknown);
        let facts = Facts { notify_send: true, ..Facts::default() };
        assert!(missing(want(), &facts).is_empty());
        assert!(prompt(want(), &facts).is_none());
    }

    #[test]
    fn only_what_is_missing_is_listed() {
        let mut facts = arch_facts();
        assert_eq!(missing(want(), &facts), [Dep::NerdFont, Dep::NotifySend]);
        facts.families = Some(FONTS.into());
        assert_eq!(missing(want(), &facts), [Dep::NotifySend]);
        facts.notify_send = true;
        assert!(missing(want(), &facts).is_empty());
        assert!(prompt(want(), &facts).is_none(), "nothing missing, no screen");
    }

    #[test]
    fn what_the_settings_dont_use_is_not_checked() {
        let facts = arch_facts();
        assert_eq!(
            missing(Wants { nerd_icons: false, notifications: true }, &facts),
            [Dep::NotifySend],
            "emoji icons need no Nerd Font"
        );
        assert_eq!(
            missing(Wants { nerd_icons: true, notifications: false }, &facts),
            [Dep::NerdFont],
            "no notifications, no notify-send"
        );
        assert!(missing(Wants { nerd_icons: false, notifications: false }, &facts).is_empty());
    }

    #[test]
    fn wants_follow_the_config_and_the_demo_sends_no_notifications() {
        let mut config = Config::default();
        assert_eq!(Wants::from_config(&config, false), Wants { nerd_icons: true, notifications: true });
        assert!(!Wants::from_config(&config, true).notifications);
        config.ui.icons = IconSet::Emoji;
        config.ui.notifications = false;
        assert_eq!(Wants::from_config(&config, false), Wants { nerd_icons: false, notifications: false });
    }

    #[test]
    fn arch_and_its_relatives_are_told_from_os_release() {
        assert!(is_arch(ARCH));
        assert!(is_arch("NAME=\"Manjaro Linux\"\nID=manjaro\nID_LIKE=arch\n"));
        assert!(is_arch("ID=endeavouros\nID_LIKE=\"arch\"\n"));
        assert!(is_arch("ID=cachyos\nID_LIKE=\"arch\"\n"));
        assert!(is_arch("ID=omarchy\n"));
        assert!(!is_arch("ID=debian\n"));
        assert!(!is_arch("ID=ubuntu\nID_LIKE=debian\n"));
        assert!(!is_arch("ID=fedora\nNAME=\"Arch-like Fedora\"\n"), "only ID and ID_LIKE count");
        assert!(!is_arch(""));
    }

    #[test]
    fn one_command_installs_only_what_is_missing() {
        let all = install_line(&[Dep::NerdFont, Dep::NotifySend], None).unwrap();
        assert_eq!(all.line, "sudo pacman -S --needed ttf-nerd-fonts-symbols libnotify");
        let one = install_line(&[Dep::NotifySend], None).unwrap();
        assert_eq!(one.line, "sudo pacman -S --needed libnotify", "the font is not in it");
        assert!(one.left_out.is_empty());
        assert_eq!(install_line(&[], None), None);
    }

    #[test]
    fn the_terminal_window_runs_the_command_then_waits_for_enter() {
        let s = script("sudo pacman -S --needed libnotify");
        assert!(s.contains("; sudo pacman -S --needed libnotify; "), "{s}");
        assert!(s.starts_with("echo '$ sudo pacman"), "the command is shown first: {s}");
        assert!(s.contains("read -r -p 'Press Enter to close"), "{s}");
        assert!(!s.contains("--noconfirm"), "the user confirms in the terminal");
    }

    #[test]
    fn terminals_are_tried_in_order() {
        let installed = |list: &'static [&'static str]| move |p: &str| list.contains(&p);
        assert_eq!(
            pick_terminal(Some("st"), installed(&["xdg-terminal-exec", "st", "foot"])).as_deref(),
            Some("xdg-terminal-exec")
        );
        assert_eq!(pick_terminal(Some("st"), installed(&["st", "foot"])).as_deref(), Some("st"), "$TERMINAL next");
        assert_eq!(
            pick_terminal(Some("st"), installed(&["foot"])).as_deref(),
            Some("foot"),
            "$TERMINAL that isn't installed is skipped"
        );
        assert_eq!(
            pick_terminal(Some("st -e"), installed(&["st -e", "xterm"])).as_deref(),
            Some("xterm"),
            "no guessing at arguments"
        );
        assert_eq!(
            pick_terminal(None, installed(&["xterm", "alacritty", "kitty"])).as_deref(),
            Some("kitty"),
            "the list's order, not alphabetical"
        );
        assert_eq!(pick_terminal(Some(""), installed(&["xterm"])).as_deref(), Some("xterm"));
        for (i, t) in TERMINALS.iter().enumerate() {
            let only = |p: &str| p == *t;
            assert_eq!(pick_terminal(None, only).as_deref(), Some(*t), "{t} at {i}");
        }
        assert_eq!(pick_terminal(None, |_| false), None);
    }

    #[test]
    fn every_terminal_gets_its_own_flags_and_runs_bash() {
        let want = |program: &str, flags: &[&str]| {
            let mut v: Vec<String> = std::iter::once(program).chain(flags.iter().copied()).map(String::from).collect();
            v.extend(["bash", "-c", "SCRIPT"].map(String::from));
            assert_eq!(terminal_argv(program, "SCRIPT"), v, "{program}");
        };
        want("xdg-terminal-exec", &[]);
        want("foot", &[]);
        want("kitty", &[]);
        want("ghostty", &["-e"]);
        want("alacritty", &["-e"]);
        want("wezterm", &["start", "--"]);
        want("konsole", &["--nofork", "-e"]);
        want("gnome-terminal", &["--wait", "--"]);
        want("xterm", &["-e"]);
        want("st", &["-e"]);
        // By its file name, so a full path is the same as the name.
        assert_eq!(terminal_argv("/usr/bin/wezterm", "S")[1..3], ["start", "--"]);
        // Every terminal on the list is covered above.
        assert_eq!(TERMINALS.len(), 8);
    }

    #[test]
    fn the_script_is_one_argument_so_nothing_in_it_is_split_or_expanded_by_us() {
        let argv = terminal_argv("xterm", &script("sudo pacman -S --needed a b"));
        assert_eq!(argv.len(), 5);
        assert!(argv[4].contains("sudo pacman -S --needed a b"));
    }

    #[test]
    fn arch_gets_an_install_offer() {
        let p = prompt(want(), &arch_facts()).unwrap();
        assert_eq!(p.missing, [Dep::NerdFont, Dep::NotifySend]);
        assert!(p.can_install && !p.waiting && p.note.is_none());
        assert_eq!(p.install.unwrap().line, "sudo pacman -S --needed ttf-nerd-fonts-symbols libnotify");
    }

    #[test]
    fn other_systems_only_get_the_list() {
        let mut facts = arch_facts();
        facts.os_release = Some("ID=debian\nID_LIKE=\n".into());
        facts.pacman = false;
        let p = prompt(want(), &facts).unwrap();
        assert_eq!(p.missing, [Dep::NerdFont, Dep::NotifySend], "still told what is missing");
        assert!(p.install.is_none() && !p.can_install, "but nothing is launched");
        // An Arch name without pacman is no Arch either (a container, say).
        let mut facts = arch_facts();
        facts.pacman = false;
        assert!(!prompt(want(), &facts).unwrap().can_install);
        facts.os_release = None;
        assert!(!prompt(want(), &facts).unwrap().can_install, "no os-release, no guessing");
    }

    #[test]
    fn no_graphical_session_or_no_terminal_means_no_launch_but_the_command_is_still_shown() {
        let mut facts = arch_facts();
        facts.display = false;
        let p = prompt(want(), &facts).unwrap();
        assert!(!p.can_install && p.install.is_some(), "on a console: shown, to be typed by hand");
        let mut facts = arch_facts();
        facts.terminals.clear();
        let p = prompt(want(), &facts).unwrap();
        assert!(!p.can_install && p.install.is_some());
        facts.terminal_env = Some("st".into());
        assert!(!prompt(want(), &facts).unwrap().can_install, "$TERMINAL that isn't installed doesn't count");
        facts.terminals.push("st".into());
        assert!(prompt(want(), &facts).unwrap().can_install);
    }

    #[test]
    fn a_missing_package_off_the_official_repos_needs_an_aur_helper() {
        // No package is AUR-only today; the plumbing is checked through the
        // helper name: with none installed it is pacman and sudo, which is what runs.
        let p = prompt(want(), &arch_facts()).unwrap();
        assert!(p.install.unwrap().line.starts_with("sudo pacman"));
        let mut facts = arch_facts();
        facts.aur_helper = Some("yay".into());
        let line = prompt(want(), &facts).unwrap().install.unwrap().line;
        assert!(line.starts_with("sudo pacman -S --needed"), "official packages don't need yay: {line}");
    }

    #[test]
    fn the_probe_gives_up_instead_of_hanging() {
        // A limit of nothing can't wait for fc-list: unknown, and unknown says nothing.
        let start = std::time::Instant::now();
        let _ = probe(Duration::ZERO);
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn what_each_dependency_says_for_itself() {
        for dep in [Dep::NerdFont, Dep::NotifySend] {
            assert!(!dep.name().is_empty() && !dep.why().is_empty() && !dep.elsewhere().is_empty());
            assert!(!dep.arch_package().contains(char::is_whitespace));
        }
        assert_eq!(Dep::NerdFont.arch_package(), "ttf-nerd-fonts-symbols");
        assert_eq!(Dep::NotifySend.arch_package(), "libnotify");
    }

    #[test]
    fn launching_says_why_when_it_cannot() {
        let facts = arch_facts();
        let mut p = prompt(want(), &facts).unwrap();
        let mut nothing = facts.clone();
        nothing.terminals.clear();
        assert_eq!(launch(&p, &nothing).unwrap_err(), "no terminal found to open", "nothing is run");
        p.install = None;
        assert_eq!(launch(&p, &facts).unwrap_err(), "nothing to install from here");
    }
}
