//! The three icon sets (`i` cycles them, `[ui] icons` remembers), like tubevibe's.

use crate::config::IconSet;

pub struct Icons {
    pub logo: &'static str,
    pub station: &'static str,
    pub shuffle: &'static str,
    pub shared: &'static str,
    /// A favorite station.
    pub favorite: &'static str,
    pub play: &'static str,
    pub pause: &'static str,
    pub skip: &'static str,
    pub up: &'static str,
    pub down: &'static str,
    /// Neither thumb.
    pub unrated: &'static str,
    pub title: &'static str,
    pub volume: &'static str,
    pub muted: &'static str,
    pub sleep: &'static str,
    pub warn: &'static str,
    pub loading: &'static str,
    pub settings: &'static str,
    pub about: &'static str,
    pub quit: &'static str,
    pub update: &'static str,
    pub ok: &'static str,
    /// One per [`crate::app::Setting::ALL`] row.
    pub setting: [&'static str; 16],
}

const EMOJI: Icons = Icons {
    logo: "📦",
    station: "📻",
    shuffle: "🔀",
    shared: "👥",
    favorite: "⭐",
    // One cell each, like tubevibe: emoji play/pause signs differ per terminal.
    play: "▶",
    pause: "‖",
    skip: "⏩",
    up: "👍",
    down: "👎",
    unrated: "·",
    title: "🎵",
    volume: "🔊",
    muted: "🔇",
    sleep: "😴",
    warn: "⚠",
    loading: "⏳",
    settings: "🔧",
    about: "💜",
    quit: "🚪",
    update: "⬆",
    ok: "✅",
    setting: ["👤", "🔑", "🔓", "🚪", "🎧", "🔊", "🎶", "🔔", "🎨", "😀", "🔲", "📐", "💿", "👾", "🆕", "📝"],
};

const NERD: Icons = Icons {
    logo: "\u{f1b2}",
    station: "\u{f0439}",
    shuffle: "\u{f074}",
    shared: "\u{f0c0}",
    favorite: "\u{f005}",
    play: "\u{f04b}",
    pause: "\u{f04c}",
    skip: "\u{f051}",
    up: "\u{f164}",
    down: "\u{f165}",
    unrated: "\u{f087}",
    title: "\u{f001}",
    volume: "\u{f028}",
    muted: "\u{f026}",
    sleep: "\u{f186}",
    warn: "\u{f071}",
    loading: "\u{f252}",
    settings: "\u{f013}",
    about: "\u{f004}",
    quit: "\u{f08b}",
    update: "\u{f0aa}",
    ok: "\u{f058}",
    setting: [
        "\u{f007}", "\u{f084}", "\u{f09c}", "\u{f08b}", "\u{f025}", "\u{f028}", "\u{f04b}", "\u{f0f3}", "\u{f1fc}",
        "\u{f118}", "\u{f0c8}", "\u{f07e}", "\u{f03e}", "\u{f00a}", "\u{f0aa}", "\u{f044}",
    ],
};

const UNICODE: Icons = Icons {
    logo: "▣",
    station: "◉",
    shuffle: "⇄",
    shared: "⚇",
    favorite: "★",
    play: "▶",
    pause: "‖",
    skip: "»",
    up: "▲",
    down: "▼",
    unrated: "·",
    title: "♪",
    volume: "◖",
    muted: "✕",
    sleep: "☾",
    warn: "!",
    loading: "…",
    settings: "⚙",
    about: "♥",
    quit: "⏻",
    update: "↑",
    ok: "✔",
    setting: ["@", "*", "→", "⏻", "♫", "◖", "▶", "♪", "◐", "☺", "▢", "↔", "▦", "▩", "↑", "✎"],
};

pub fn icons(set: IconSet) -> &'static Icons {
    match set {
        IconSet::Emoji => &EMOJI,
        IconSet::Nerd => &NERD,
        IconSet::Unicode => &UNICODE,
    }
}

pub fn label(set: IconSet) -> &'static str {
    match set {
        IconSet::Emoji => "emoji",
        IconSet::Nerd => "Nerd Font",
        IconSet::Unicode => "plain Unicode",
    }
}
