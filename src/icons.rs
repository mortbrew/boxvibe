//! The three icon sets (`i` cycles them, `[look] icons` remembers), like tubevibe's. The boxvibe logo ignores the choice: it is always the Nerd Font box.

use crate::config::IconSet;

/// The boxvibe logo (the Nerd Font box): the same in every icon set, so the
/// menu's frame, About and the boot screen always look alike.
pub const LOGO: &str = "\u{f1b2}";

pub struct Icons {
    pub station: &'static str,
    /// The Genres page.
    pub genre: &'static str,
    pub shuffle: &'static str,
    pub shared: &'static str,
    /// A favorite station.
    pub favorite: &'static str,
    /// A pinned song (Shift+I) and the Pinned page.
    pub pin: &'static str,
    /// Right after the star of a Pandora genre station (one you can't rename): the boxvibe logo.
    pub genre_station: &'static str,
    /// Right after the star of a station you made yourself: a little person.
    pub own_station: &'static str,
    pub play: &'static str,
    pub pause: &'static str,
    pub skip: &'static str,
    pub up: &'static str,
    pub down: &'static str,
    /// Neither thumb.
    pub unrated: &'static str,
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
    pub setting: [&'static str; 24],
}

const EMOJI: Icons = Icons {
    station: "📻",
    genre: "🎼",
    shuffle: "🔀",
    shared: "👥",
    favorite: "⭐",
    pin: "📌",
    genre_station: LOGO,
    own_station: "👤",
    // One cell each, like tubevibe: emoji play/pause signs differ per terminal.
    play: "▶",
    pause: "‖",
    skip: "⏩",
    up: "👍",
    down: "👎",
    unrated: "·",
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
    setting: [
        "👤", "🔑", "🔓", "🚪", "🆕", "💬", "📝", "📌", "🎧", "🔊", "🎶", "🔔", "📢", "🎨", "😀", "🔲", "📐", "⭐",
        "📌", "💿", "👾", "🌈", "✨", "🚀",
    ],
};

const NERD: Icons = Icons {
    station: "\u{f0439}",
    genre: "\u{f001}",
    shuffle: "\u{f074}",
    shared: "\u{f0c0}",
    favorite: "\u{f005}",
    pin: "\u{f08d}",
    genre_station: LOGO,
    own_station: "\u{f007}",
    play: "\u{f04b}",
    pause: "\u{f04c}",
    skip: "\u{f051}",
    up: "\u{f164}",
    down: "\u{f165}",
    unrated: "\u{f087}",
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
        "\u{f007}", "\u{f084}", "\u{f09c}", "\u{f08b}", "\u{f0aa}", "\u{f075}", "\u{f044}", "\u{f0c9}", "\u{f025}",
        "\u{f028}", "\u{f04b}", "\u{f0f3}", "\u{f0a1}", "\u{f1fc}", "\u{f118}", "\u{f0c8}", "\u{f07e}", "\u{f005}",
        "\u{f08d}", "\u{f03e}", "\u{f00a}", "\u{f53f}", "\u{f0d0}", "\u{f135}",
    ],
};

const UNICODE: Icons = Icons {
    station: "◉",
    genre: "♬",
    shuffle: "⇄",
    shared: "⚇",
    favorite: "★",
    pin: "⚑",
    // The logo is the Nerd Font box in every set; the person is a plain smiley.
    genre_station: LOGO,
    own_station: "☺",
    play: "▶",
    pause: "‖",
    skip: "»",
    up: "▲",
    down: "▼",
    unrated: "·",
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
    setting: [
        "@", "*", "→", "⏻", "↑", "!", "✎", "☰", "♫", "◖", "▶", "♪", "♡", "◐", "☺", "▢", "↔", "★", "⚑", "▦", "▩", "◑",
        "✦", "◈",
    ],
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
