//! Pandora's website API (`https://www.pandora.com/api/`): the JSON-over-HTTPS
//! API the pandora.com web player itself uses. Login with email (or username)
//! and password, the station list, playlist fragments, thumbs, "tired of this
//! song" and the playback reports the site expects.
//!
//! Sources: the community docs at <https://6xq.net/pandora-apidoc/rest/>
//! (WTFPL), what the site answers (checked by hand), and Music Assistant's
//! Pandora provider. Parts of this file are adapted from Music Assistant
//! (`music_assistant/providers/pandora/{provider,helpers}.py`,
//! <https://github.com/music-assistant/server>, Apache-2.0, Copyright the Music
//! Assistant contributors), translated from Python to Rust and changed: the
//! login and request headers, the `csrftoken` fetch, the playlist fragment
//! request and its audio format choice by account flag, the stream-takeover
//! call, and skipping curator messages. See THIRD_PARTY.md.
//!
//! Renaming, removing and station details (`v1/station/updateStation` with
//! `{stationId, name}`, `v1/station/removeStation` with `{stationId}`,
//! `v1/station/getStationDetails` with `{stationId, isCurrentStation}`) are
//! written from the community docs (which list the endpoints and show the
//! details answer) and from what other web-API clients send (alexpreiss/
//! pandora-app, libepimetheusKotlin, Panedora): no code was copied from them.
//!
//! Search (`v3/sod/search` with `{query, start, count, annotate, types}`; the
//! answer is a `results` list of ids plus an `annotations` map), creating a
//! station (`v1/station/createStation` with `{pandoraId, stationName}`) and the
//! genre stations (`v1/music/genrecategories` with `{}`, then `v1/music/genres`
//! with `{categoryToken}`) are written from the community docs (which show the
//! `createStation` request and answer and the genre station ids) and from the
//! requests and answers libepimetheusKotlin (GPL-3.0) sends and reads. Only
//! those request and answer shapes were taken, as facts; no code was copied.

use std::collections::HashSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

pub const SITE: &str = "https://www.pandora.com";
/// The web player version `getAdList` reports in `deviceProperties` (the
/// website's own number when this was written).
const AD_APP_VERSION: &str = "1.300.0";

/// What `getAdList` is told about the song that just ended.
#[derive(Debug, Clone, PartialEq)]
pub struct AdAsk {
    pub station_id: String,
    pub track_token: String,
    pub seconds_played: u32,
    /// The listener skipped (or thumbed down) rather than the song ending.
    pub user_initiated: bool,
}
/// A current desktop Firefox on Linux. Pandora's bot check answered Chrome-like
/// agents with 403s in Music Assistant's testing; Firefox ones got through.
/// `[pandora] user_agent` in config.toml overrides it.
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:150.0) Gecko/20100101 Firefox/150.0";
/// The account flag that unlocks `mp3-hifi` (192 kbps MP3: Plus and Premium).
pub const HIFI_FLAG: &str = "highQualityStreamingAvailable";

/// A station, as `v1/station/getStations` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Station {
    /// `stationId`, what the playlist call takes.
    pub id: String,
    pub name: String,
    /// Pandora's Shuffle (all stations mixed).
    pub is_shuffle: bool,
    /// Someone else's station shared with me.
    pub is_shared: bool,
    /// The largest station art there is (not drawn yet).
    pub art_url: String,
    /// Pandora lets this station be renamed (`allowRename`; not Shuffle).
    pub allow_rename: bool,
    /// Pandora lets this station be removed (`allowDelete`; not Shuffle).
    pub allow_delete: bool,
    /// Pandora lets seeds be added and removed (`allowAddSeed`; not Shuffle or
    /// a genre station).
    pub allow_edit_seeds: bool,
}

impl Station {
    /// Pandora won't let this station be renamed: its genre stations (and
    /// Shuffle). The same test as the rename error, so the logo icon in the
    /// lists and that message always agree.
    pub fn is_locked(&self) -> bool {
        !self.allow_rename
    }
}

/// One of the seeds a station is built from (an artist, a song or a genre).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seed {
    /// `Artist`, `Song` or `Genre`.
    pub kind: &'static str,
    pub name: String,
    /// `pandoraId` (`AR:…`, `TR:…`): what removing the seed takes. Empty
    /// when the site did not send one (such a seed can't be removed here).
    pub id: String,
    /// `musicId` (`R…`): what the suggestions call takes.
    pub music_id: String,
}

/// One thumb you gave (`v1/station/getStationFeedback`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feedback {
    /// `feedbackId`: what deleting it takes.
    pub id: String,
    pub positive: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// `feedbackDateCreated`, as the site writes it.
    pub date: String,
}

/// One page of the thumbs history.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FeedbackPage {
    pub items: Vec<Feedback>,
    /// `total`: how many thumbs of this kind the station has.
    pub total: usize,
}

/// How many thumbs one page asks for.
pub const FEEDBACK_PAGE: usize = 30;

/// What a search hit is: only artists and songs can seed a station here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    Artist,
    Song,
}

impl HitKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Artist => "Artist",
            Self::Song => "Song",
        }
    }
}

/// One `v3/sod/search` result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// `pandoraId` (`AR:…`, `CO:…`, `TR:…`): what `createStation` takes.
    pub id: String,
    pub kind: HitKind,
    pub name: String,
    /// A song's artist (empty for artists).
    pub by: String,
}

/// A genre station Pandora offers (`v1/music/genres`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Genre {
    /// `pandoraId` (`GE:…`): what `createStation` takes.
    pub id: String,
    pub name: String,
    pub description: String,
}

/// A genre category and its stations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenreCategory {
    pub name: String,
    pub genres: Vec<Genre>,
}

/// What `v1/station/getStationDetails` tells about a station. Everything
/// the site did not send stays empty (or `None`) and is not shown.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StationDetails {
    pub seeds: Vec<Seed>,
    /// `positiveFeedbackCount`: thumbs up given on this station.
    pub thumbs_up: Option<u32>,
    /// `negativeFeedbackCount`: thumbs down given on this station.
    pub thumbs_down: Option<u32>,
    pub genres: Vec<String>,
    /// `dateCreated`, as the site writes it.
    pub created: String,
    /// `lastPlayed`, as the site writes it.
    pub last_played: String,
    /// `creatorWebname`: whose station it is.
    pub creator: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rating {
    #[default]
    None,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioFormat {
    /// `mp3` (128 kbps) or `mp3-hifi` (192 kbps).
    #[default]
    Mp3,
    /// `aacplus`: HE-AAC, only if Pandora refuses MP3.
    Aac,
    Unknown,
}

/// A song from a playlist fragment.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Song {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// `trackToken`: what thumbs, "tired" and `trackStarted` take.
    pub track_token: String,
    pub station_id: String,
    /// Signed CDN link, valid for a limited time.
    pub audio_url: String,
    pub audio_format: AudioFormat,
    /// The album cover Now Playing draws (about 500 px: [`art_for_box`]).
    pub cover_art_url: String,
    pub detail_url: String,
    /// `trackLength`, seconds (0 when missing).
    pub length: u32,
    /// `fileGain`, dB.
    pub gain: f32,
    pub rating: Rating,
    /// The `feedbackId` of this song's thumb, once Pandora has said it
    /// (addFeedback's answer, or the track's own): what clearing it takes.
    pub feedback_id: String,
}

/// `[pandora] audio_quality`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// 192 kbps MP3 when the account allows it, else 128 kbps.
    #[default]
    High,
    /// 128 kbps MP3.
    Standard,
}

impl Quality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Standard => "standard",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::High => Self::Standard,
            Self::Standard => Self::High,
        }
    }

    /// Old configs said `low` / `medium` (HE-AAC): both are `standard` now.
    pub fn parse(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "standard" | "low" | "medium" | "normal" => Self::Standard,
            _ => Self::High,
        }
    }

    /// The `audioFormat` to ask a fragment for.
    pub fn audio_format(self, hifi_allowed: bool) -> &'static str {
        if self == Self::High && hifi_allowed { "mp3-hifi" } else { "mp3" }
    }
}

/// What the login said about the account.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Account {
    pub listener_id: String,
    pub webname: String,
    /// `config.branding`: `Pandora`, `PandoraPlus`, `PandoraPremium`.
    pub branding: String,
    pub flags: Vec<String>,
}

impl Account {
    pub fn hifi(&self) -> bool {
        self.flags.iter().any(|f| f == HIFI_FLAG)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("network: {0}")]
    Http(String),
    #[error("Pandora sent something boxvibe can't read")]
    InvalidResponse,
    #[error("wrong email or password")]
    WrongLogin,
    #[error(
        "Pandora's bot check blocked the login (HTTP {0}). Try again later or from another network; \
         logging in once on pandora.com in a browser may help"
    )]
    BotCheck(u16),
    #[error("Pandora is playing on another device with this account")]
    StreamViolation,
    #[error("Pandora only works in the US")]
    Country,
    #[error("{message}")]
    Api { status: u16, code: Option<i64>, kind: String, message: String },
}

impl Error {
    /// The login token expired: log in again and retry.
    pub fn is_auth_expired(&self) -> bool {
        matches!(self, Self::Api { status: 401, .. } | Self::Api { code: Some(1001), .. })
    }
}

/// Reads a failed answer: HTTP `status` and its body.
pub fn classify(status: u16, body: &str) -> Error {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let kind = str_of(&v, "errorString");
    let code = v.get("errorCode").and_then(Value::as_i64);
    // HUMAN (PerimeterX) answers with its own block page: an app id and a captcha script.
    if v.get("blockScript").is_some() || str_of(&v, "appId").starts_with("PX") || kind.starts_with("s2s_") {
        return Error::BotCheck(status);
    }
    match kind.as_str() {
        "AUTH_INVALID_USERNAME_PASSWORD" => return Error::WrongLogin,
        "STREAM_VIOLATION" => return Error::StreamViolation,
        "COUNTRY_CODE_INVALID" => return Error::Country,
        _ => {}
    }
    if code == Some(1028) {
        return Error::Country;
    }
    let message = match (str_of(&v, "message"), status) {
        (m, 401) if m.is_empty() => "the login expired".into(),
        (m, _) if m.is_empty() => format!("Pandora answered HTTP {status}"),
        (m, _) => format!("{m} (HTTP {status}{})", if kind.is_empty() { String::new() } else { format!(", {kind}") }),
    };
    Error::Api { status, code, kind, message }
}

fn str_of(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// The URL of the biggest image in an `art` / `albumArt` list.
pub fn largest_art(list: Option<&Value>) -> String {
    list.and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .filter(|a| a.get("url").and_then(Value::as_str).is_some_and(|u| !u.is_empty()))
                .max_by_key(|a| a.get("size").and_then(Value::as_u64).unwrap_or(0))
        })
        .map(|a| str_of(a, "url"))
        .unwrap_or_default()
}

/// The cover for Now Playing's art box: the smallest one at least `ART_SIZE`
/// pixels across (sharp enough for sixel, a small download), else the largest.
pub fn art_for_box(list: Option<&Value>) -> String {
    let items: Vec<(u64, String)> = list
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|a| (a.get("size").and_then(Value::as_u64).unwrap_or(0), str_of(a, "url")))
                .filter(|(_, u)| !u.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let big_enough = items.iter().filter(|(s, _)| *s >= ART_SIZE).min_by_key(|(s, _)| *s);
    big_enough.or_else(|| items.iter().max_by_key(|(s, _)| *s)).map(|(_, u)| u.clone()).unwrap_or_default()
}

/// Pixels across the cover boxvibe downloads.
pub const ART_SIZE: u64 = 500;

/// One `getStations` entry.
pub fn parse_station(v: &Value) -> Option<Station> {
    let id = str_of(v, "stationId");
    if id.is_empty() {
        return None;
    }
    let is_shuffle = v.get("isShuffle").and_then(Value::as_bool).unwrap_or(false);
    Some(Station {
        id,
        name: str_of(v, "name"),
        is_shuffle,
        is_shared: v.get("isShared").and_then(Value::as_bool).unwrap_or(false),
        art_url: largest_art(v.get("art")),
        // The site says what it allows; Shuffle never allows either.
        allow_rename: flag(v, "allowRename").unwrap_or(!is_shuffle),
        allow_delete: flag(v, "allowDelete").unwrap_or(!is_shuffle),
        // Not said: the same as renaming (genre stations allow neither).
        allow_edit_seeds: flag(v, "allowAddSeed").unwrap_or(!is_shuffle && flag(v, "allowRename").unwrap_or(true)),
    })
}

fn flag(v: &Value, key: &str) -> Option<bool> {
    v.get(key).and_then(Value::as_bool)
}

fn nested_text(item: &Value, path: &[&str]) -> String {
    let mut at = item;
    for key in path {
        match at.get(key) {
            Some(next) => at = next,
            None => return String::new(),
        }
    }
    at.as_str().unwrap_or_default().trim().to_owned()
}

/// The seeds of a `getStationDetails` or `getSeeds` answer (`seeds`). Each is
/// read by its kind (`artist`, `song` or `genre` object); a seed of any other
/// shape is skipped rather than guessed at.
pub fn parse_seeds(v: &Value) -> Vec<Seed> {
    v.get("seeds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|seed| {
            let artist = nested_text(seed, &["artist", "artistName"]);
            let song = nested_text(seed, &["song", "songName"]);
            let by = nested_text(seed, &["song", "artistSummary"]);
            let genre = nested_text(seed, &["genre", "stationName"]);
            let (kind, name) = if !artist.is_empty() {
                ("Artist", artist)
            } else if !song.is_empty() {
                ("Song", if by.is_empty() { song } else { format!("{song} by {by}") })
            } else if !genre.is_empty() {
                ("Genre", genre)
            } else {
                return None;
            };
            Some(Seed { kind, name, id: str_of(seed, "pandoraId"), music_id: str_of(seed, "musicId") })
        })
        .collect()
}

/// `getSeedSuggestions`: a bare list of artists (`name`, `pandoraId`). Anything
/// without both is skipped.
pub fn parse_seed_suggestions(v: &Value) -> Vec<SearchHit> {
    v.as_array()
        .or_else(|| v.get("suggestions").and_then(Value::as_array))
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let id = str_of(s, "pandoraId");
            let name = str_of(s, "name").trim().to_owned();
            (!id.is_empty() && !name.is_empty()).then_some(SearchHit {
                id,
                kind: HitKind::Artist,
                name,
                by: String::new(),
            })
        })
        .collect()
}

/// `getStationFeedback`: `total` and the `feedback` list. Entries without an
/// id are skipped (they could not be deleted).
pub fn parse_feedback(v: &Value) -> FeedbackPage {
    let items: Vec<Feedback> = v
        .get("feedback")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|f| {
            let id = str_of(f, "feedbackId");
            if id.is_empty() {
                return None;
            }
            Some(Feedback {
                id,
                positive: f.get("isPositive").and_then(Value::as_bool).unwrap_or(true),
                title: str_of(f, "songTitle").trim().to_owned(),
                artist: str_of(f, "artistName").trim().to_owned(),
                album: str_of(f, "albumTitle").trim().to_owned(),
                date: str_of(f, "feedbackDateCreated"),
            })
        })
        .collect();
    let total = v.get("total").and_then(Value::as_u64).map_or(items.len(), |t| t as usize);
    FeedbackPage { items, total }
}

/// `v1/station/getStationDetails`: the seeds, thumbs counts and dates. Seeds
/// are read by their kind (`artist`, `song` or `genre` object); a seed of any
/// other shape is skipped rather than guessed at.
pub fn parse_station_details(v: &Value) -> StationDetails {
    let seeds = parse_seeds(v);
    let count = |key: &str| v.get(key).and_then(Value::as_u64).map(|n| n.min(u64::from(u32::MAX)) as u32);
    StationDetails {
        seeds,
        thumbs_up: count("positiveFeedbackCount"),
        thumbs_down: count("negativeFeedbackCount"),
        genres: v
            .get("genre")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|g| g.as_str().map(str::to_owned))
            .filter(|g| !g.is_empty())
            .collect(),
        created: nested_text(v, &["dateCreated"]),
        last_played: nested_text(v, &["lastPlayed"]),
        creator: nested_text(v, &["creatorWebname"]),
    }
}

/// `v3/sod/search`: the `results` ids in order, each looked up in
/// `annotations`. Artists (`AR`, `CO`) and songs (`TR`) are kept; albums,
/// playlists, stations and anything unknown are skipped.
pub fn parse_search(v: &Value) -> Vec<SearchHit> {
    let notes = v.get("annotations");
    v.get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|id| {
            let id = id.as_str()?;
            let note = notes?.get(id)?;
            let name = str_of(note, "name").trim().to_owned();
            let own = str_of(note, "pandoraId");
            let pandora_id = if own.is_empty() { id.to_owned() } else { own };
            if name.is_empty() {
                return None;
            }
            match pandora_id.get(..2)? {
                "AR" | "CO" => Some(SearchHit { id: pandora_id, kind: HitKind::Artist, name, by: String::new() }),
                "TR" => {
                    let by = str_of(note, "artistName").trim().to_owned();
                    Some(SearchHit { id: pandora_id, kind: HitKind::Song, name, by })
                }
                _ => None,
            }
        })
        .collect()
}

/// `v1/music/genrecategories`: `(name, categoryToken)` pairs.
pub fn parse_genre_categories(v: &Value) -> Vec<(String, String)> {
    v.get("categories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let token = str_of(c, "token");
            let name = str_of(c, "name").trim().to_owned();
            (!token.is_empty() && !name.is_empty()).then_some((name, token))
        })
        .collect()
}

/// `v1/music/genres`: the genre stations of one category.
pub fn parse_genres(v: &Value) -> Vec<Genre> {
    v.get("genres")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|g| {
            let id = str_of(g, "pandoraId");
            let name = str_of(g, "name").trim().to_owned();
            (!id.is_empty() && !name.is_empty()).then(|| Genre {
                id,
                name,
                description: str_of(g, "description").trim().to_owned(),
            })
        })
        .collect()
}

/// One fragment track; `None` for ads, artist and curator messages and
/// anything else that isn't a song with audio.
pub fn parse_track(v: &Value, station_id: &str) -> Option<Song> {
    let kind = str_of(v, "trackType");
    if !kind.is_empty() && !kind.eq_ignore_ascii_case("Track") {
        return None;
    }
    let title = str_of(v, "songTitle");
    let artist = str_of(v, "artistName");
    let audio_url = str_of(v, "audioURL");
    if title.is_empty() || artist.is_empty() || audio_url.is_empty() {
        return None;
    }
    if title.to_lowercase().contains("curator message") {
        return None;
    }
    let encoding = str_of(v, "audioEncoding").to_ascii_lowercase();
    let station = str_of(v, "stationId");
    Some(Song {
        title,
        artist,
        album: str_of(v, "albumTitle"),
        track_token: str_of(v, "trackToken"),
        station_id: if station.is_empty() { station_id.to_owned() } else { station },
        audio_url,
        audio_format: if encoding.starts_with("mp3") {
            AudioFormat::Mp3
        } else if encoding.starts_with("aac") {
            AudioFormat::Aac
        } else {
            AudioFormat::Unknown
        },
        cover_art_url: art_for_box(v.get("albumArt")),
        detail_url: str_of(v, "songDetailURL"),
        length: v.get("trackLength").and_then(Value::as_u64).unwrap_or(0) as u32,
        gain: v
            .get("fileGain")
            .and_then(|g| g.as_str().and_then(|s| s.trim().parse().ok()).or(g.as_f64()))
            .unwrap_or(0.0) as f32,
        rating: match v.get("rating").and_then(Value::as_i64) {
            Some(r) if r > 0 => Rating::Up,
            Some(r) if r < 0 => Rating::Down,
            _ => Rating::None,
        },
        feedback_id: id_of(v, "feedbackId"),
    })
}

/// A string or number field as text (empty when missing).
fn id_of(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// The `csrftoken` value in a `Set-Cookie` header, if it is that cookie.
pub fn csrf_from_cookie(header: &str) -> Option<String> {
    let first = header.split(';').next()?.trim();
    let value = first.strip_prefix("csrftoken=")?;
    (!value.is_empty()).then(|| value.to_owned())
}

/// A made-up token, for when the site sends none: the API only checks that
/// the header and the cookie match.
fn made_up_csrf() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let mixed = (nanos as u64) ^ (u64::from(std::process::id()) << 32) ^ 0x9e37_79b9_7f4a_7c15;
    format!("{:016x}", mixed.wrapping_mul(0xbf58_476d_1ce4_e5b9))
}

/// A web session: the `csrftoken` and, once logged in, the auth token.
pub struct Client {
    agent: ureq::Agent,
    base: String,
    user_agent: String,
    csrf: Option<String>,
    auth: Option<String>,
    pub account: Account,
    pub quality: Quality,
    /// Stations whose first fragment was asked for (`isStationStart`).
    started: HashSet<String>,
    /// Pandora refused `mp3`: ask for `aacplus` from then on.
    aac_only: bool,
}

impl Client {
    pub fn new(user_agent: &str) -> Self {
        Self::with_base(SITE, user_agent)
    }

    /// A client for another address (tests run a fake Pandora on localhost).
    pub fn with_base(base: &str, user_agent: &str) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();
        let user_agent = if user_agent.trim().is_empty() { DEFAULT_USER_AGENT } else { user_agent.trim() };
        Self {
            agent,
            base: base.trim_end_matches('/').to_owned(),
            user_agent: user_agent.to_owned(),
            csrf: None,
            auth: None,
            account: Account::default(),
            quality: Quality::High,
            started: HashSet::new(),
            aac_only: false,
        }
    }

    /// `HEAD /` hands out the `csrftoken` cookie.
    fn fetch_csrf(&mut self) -> String {
        let token = self.site_csrf().unwrap_or_else(made_up_csrf);
        self.csrf = Some(token.clone());
        token
    }

    /// The `csrftoken` the site set, if it set one.
    fn site_csrf(&self) -> Option<String> {
        self.agent
            .head(format!("{}/", self.base))
            .header("User-Agent", &self.user_agent)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header("Accept-Language", "en-US,en;q=0.9")
            .call()
            .ok()
            .and_then(|r| r.headers().get_all("set-cookie").iter().find_map(|h| csrf_from_cookie(h.to_str().ok()?)))
    }

    /// POSTs `body` as JSON to `path` (`/api/v1/...`) with the site's headers.
    fn post(&mut self, path: &str, body: &Value) -> Result<Value, Error> {
        let csrf = match &self.csrf {
            Some(c) => c.clone(),
            None => self.fetch_csrf(),
        };
        let mut request = self
            .agent
            .post(format!("{}{path}", self.base))
            .header("User-Agent", &self.user_agent)
            .header("Content-Type", "application/json;charset=utf-8")
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Origin", SITE)
            .header("Referer", format!("{SITE}/"))
            .header("X-CsrfToken", &csrf)
            .header("Cookie", format!("csrftoken={csrf}"));
        if let Some(auth) = &self.auth {
            request = request.header("X-AuthToken", auth);
        }
        let mut response = request.send(body.to_string()).map_err(|e| Error::Http(e.to_string()))?;
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().map_err(|e| Error::Http(e.to_string()))?;
        if status != 200 {
            return Err(classify(status, &text));
        }
        // Some calls (removing a station) may answer 200 with nothing in the body.
        if text.trim().is_empty() {
            return Ok(json!({}));
        }
        let v: Value = serde_json::from_str(&text).map_err(|_| Error::InvalidResponse)?;
        // Some failures come back as 200 with an error body.
        if v.get("errorCode").is_some() && v.get("errorString").is_some() {
            return Err(classify(status, &text));
        }
        Ok(v)
    }

    /// `v1/auth/login` with email (or username) and password.
    pub fn login(&mut self, username: &str, password: &str) -> Result<(), Error> {
        self.auth = None;
        // A fresh csrftoken for each login, as the site gets one per page load.
        self.fetch_csrf();
        let body = json!({
            "username": username,
            "password": password,
            "keepLoggedIn": true,
            "existingAuthToken": null,
        });
        let v = self.post("/api/v1/auth/login", &body)?;
        let token = str_of(&v, "authToken");
        if token.is_empty() {
            return Err(Error::InvalidResponse);
        }
        let config = v.get("config").filter(|c| c.is_object());
        self.account = Account {
            listener_id: str_of(&v, "listenerId"),
            webname: str_of(&v, "webname"),
            branding: config.map(|c| str_of(c, "branding")).unwrap_or_default(),
            flags: config
                .and_then(|c| c.get("flags"))
                .and_then(Value::as_array)
                .map(|f| f.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                .unwrap_or_default(),
        };
        self.auth = Some(token);
        // Every station starts over with a new login.
        self.started.clear();
        Ok(())
    }

    /// `v1/station/getStations`, every page of it.
    pub fn stations(&mut self) -> Result<Vec<Station>, Error> {
        let mut stations = Vec::new();
        loop {
            let v =
                self.post("/api/v1/station/getStations", &json!({ "pageSize": 250, "startIndex": stations.len() }))?;
            let page: Vec<Station> =
                v.get("stations").and_then(Value::as_array).into_iter().flatten().filter_map(parse_station).collect();
            let total = v.get("totalStations").and_then(Value::as_u64).unwrap_or(0) as usize;
            let empty = page.is_empty();
            stations.extend(page);
            if empty || stations.len() >= total {
                break;
            }
        }
        // Shuffle first, like the site.
        stations.sort_by_key(|s| !s.is_shuffle);
        Ok(stations)
    }

    /// `v1/station/updateStation`: renames a station. Returns the name the
    /// site now has (the reply's `name`, else the one asked for).
    pub fn rename_station(&mut self, station_id: &str, name: &str) -> Result<String, Error> {
        let v = self.post("/api/v1/station/updateStation", &json!({ "stationId": station_id, "name": name }))?;
        let said = str_of(&v, "name");
        Ok(if said.is_empty() { name.to_owned() } else { said })
    }

    /// `v1/station/removeStation`: deletes a station from the account.
    pub fn remove_station(&mut self, station_id: &str) -> Result<(), Error> {
        self.post("/api/v1/station/removeStation", &json!({ "stationId": station_id })).map(|_| ())
    }

    /// `v1/station/getStationDetails`: seeds, thumbs counts and dates.
    pub fn station_details(&mut self, station_id: &str, current: bool) -> Result<StationDetails, Error> {
        let v = self.post(
            "/api/v1/station/getStationDetails",
            &json!({ "stationId": station_id, "isCurrentStation": current }),
        )?;
        Ok(parse_station_details(&v))
    }

    /// `v1/station/getSeeds`: the seeds of a station you made (newest first).
    pub fn seeds(&mut self, station_id: &str) -> Result<Vec<Seed>, Error> {
        let v = self.post("/api/v1/station/getSeeds", &json!({ "stationId": station_id }))?;
        Ok(parse_seeds(&v))
    }

    /// `v1/search/getSeedSuggestions`: artists the site suggests adding,
    /// near `seed_music_id` (a seed's `musicId`). The path is from the
    /// public endpoint list (the old `v1/station/` one made Pandora answer
    /// "Service method not found"); the site's own call was never captured,
    /// so the body is the captured shape and a failure is silently "no
    /// suggestions" (the editor shows nothing for it).
    pub fn seed_suggestions(&mut self, station_id: &str, seed_music_id: &str) -> Result<Vec<SearchHit>, Error> {
        let v = self.post(
            "/api/v1/search/getSeedSuggestions",
            &json!({ "stationId": station_id, "seedMusicId": seed_music_id, "maxResults": 10 }),
        )?;
        Ok(parse_seed_suggestions(&v))
    }

    /// Adds the artist or song `seed_id` (a `pandoraId`) to a station.
    pub fn add_seed(&mut self, station_id: &str, seed_id: &str) -> Result<(), Error> {
        self.seed_change(
            "AddSeedToStationWeb",
            "mutation AddSeedToStationWeb($stationId: String!, $seedId: String!) { addStationSeed(stationId: $stationId, seedId: $seedId) { status } }",
            "addStationSeed",
            station_id,
            seed_id,
        )
    }

    /// Removes the seed `seed_id` (a `pandoraId`) from a station.
    pub fn remove_seed(&mut self, station_id: &str, seed_id: &str) -> Result<(), Error> {
        self.seed_change(
            "DeleteSeedFromStationWeb",
            "mutation DeleteSeedFromStationWeb($stationId: String!, $seedId: String!) { removeStationSeed(stationId: $stationId, seedId: $seedId) { status } }",
            "removeStationSeed",
            station_id,
            seed_id,
        )
    }

    /// One of the site's GraphQL seed mutations. The site sends `variables`
    /// as a JSON string, and the station as `ST:0:<id>`. Two paths were seen
    /// for it, so the second is tried when the first is not found.
    fn seed_change(
        &mut self,
        operation: &str,
        query: &str,
        field: &str,
        station_id: &str,
        seed_id: &str,
    ) -> Result<(), Error> {
        let variables = json!({ "stationId": format!("ST:0:{station_id}"), "seedId": seed_id }).to_string();
        let body = json!({ "operationName": operation, "query": query, "variables": variables });
        let v = match self.post("/api/v1/graphql", &body) {
            Err(Error::Api { status: 404 | 405, .. }) => self.post("/api/v1/graphql/graphql", &body)?,
            other => other?,
        };
        if let Some(message) = v.get("errors").and_then(Value::as_array).and_then(|e| e.first()) {
            let message = str_of(message, "message");
            return Err(Error::Api {
                status: 200,
                code: None,
                kind: "GRAPHQL".into(),
                message: if message.is_empty() { "Pandora refused the change".into() } else { message },
            });
        }
        match v.pointer(&format!("/data/{field}/status")).and_then(Value::as_str) {
            Some(status) if status.eq_ignore_ascii_case("ok") => Ok(()),
            Some(status) => Err(Error::Api {
                status: 200,
                code: None,
                kind: "GRAPHQL".into(),
                message: format!("Pandora answered {status}"),
            }),
            None => Err(Error::InvalidResponse),
        }
    }

    /// `v1/station/getStationFeedback`: a page of the thumbs up (or down)
    /// you gave on a station, newest first, from `start`.
    pub fn station_feedback(&mut self, station_id: &str, positive: bool, start: usize) -> Result<FeedbackPage, Error> {
        let v = self.post(
            "/api/v1/station/getStationFeedback",
            &json!({ "pageSize": FEEDBACK_PAGE, "startIndex": start, "stationId": station_id, "positive": positive }),
        )?;
        Ok(parse_feedback(&v))
    }

    /// `v1/station/deleteFeedback`: takes one thumb back.
    pub fn delete_feedback(&mut self, feedback_id: &str, positive: bool) -> Result<(), Error> {
        self.post("/api/v1/station/deleteFeedback", &json!({ "feedbackId": feedback_id, "isPositive": positive }))?;
        Ok(())
    }

    /// `v3/sod/search`: artists and songs matching `query`.
    pub fn search(&mut self, query: &str) -> Result<Vec<SearchHit>, Error> {
        let v = self.post(
            "/api/v3/sod/search",
            &json!({ "query": query, "start": 0, "count": 20, "annotate": true, "types": ["AR", "CO", "TR"] }),
        )?;
        Ok(parse_search(&v))
    }

    /// `v1/station/createStation`: a new station seeded by `pandora_id` (a
    /// search hit or a genre station). Returns it as the account lists it.
    pub fn create_station(&mut self, pandora_id: &str) -> Result<Station, Error> {
        let v = self.post("/api/v1/station/createStation", &json!({ "pandoraId": pandora_id, "stationName": "" }))?;
        parse_station(&v).ok_or(Error::InvalidResponse)
    }

    /// `v1/music/genrecategories`, then `v1/music/genres` for each category.
    /// A category whose list fails is left out; the first error is returned
    /// only when nothing could be listed.
    pub fn genres(&mut self) -> Result<Vec<GenreCategory>, Error> {
        let v = self.post("/api/v1/music/genrecategories", &json!({}))?;
        let mut out = Vec::new();
        let mut first_error = None;
        for (name, token) in parse_genre_categories(&v) {
            match self.post("/api/v1/music/genres", &json!({ "categoryToken": token })) {
                Ok(v) => {
                    let genres = parse_genres(&v);
                    if !genres.is_empty() {
                        out.push(GenreCategory { name, genres });
                    }
                }
                Err(e) if e.is_auth_expired() => return Err(e),
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        match (out.is_empty(), first_error) {
            (true, Some(e)) => Err(e),
            _ => Ok(out),
        }
    }

    /// `v1/playlist/getFragment`: the next few songs of `station_id`.
    pub fn fragment(&mut self, station_id: &str) -> Result<Vec<Song>, Error> {
        let start = !self.started.contains(station_id);
        let format = if self.aac_only { "aacplus" } else { self.quality.audio_format(self.account.hifi()) };
        let result = self.fragment_as(station_id, start, format);
        let v = match result {
            // An account or station that won't do MP3: HE-AAC rather than nothing.
            Err(Error::Api { status: 400, .. }) if format == "mp3" && !self.aac_only => {
                self.aac_only = true;
                self.fragment_as(station_id, start, "aacplus")?
            }
            other => other?,
        };
        self.started.insert(station_id.to_owned());
        let tracks = v.get("tracks").and_then(Value::as_array).cloned().unwrap_or_default();
        Ok(tracks.iter().filter_map(|t| parse_track(t, station_id)).collect())
    }

    fn fragment_as(&mut self, station_id: &str, start: bool, format: &str) -> Result<Value, Error> {
        let body = json!({
            "stationId": station_id,
            "isStationStart": start,
            "fragmentRequestReason": "Normal",
            "audioFormat": format,
            "startingAtTrackId": null,
            "onDemandArtistMessageArtistUidHex": null,
            "onDemandArtistMessageIdHex": null,
        });
        self.post("/api/v1/playlist/getFragment", &body)
    }

    /// `v1/station/addFeedback`: thumbs up (`positive`) or down.
    /// Returns the thumb's `feedbackId` (empty if Pandora sent none).
    pub fn add_feedback(&mut self, track_token: &str, positive: bool) -> Result<String, Error> {
        self.post("/api/v1/station/addFeedback", &json!({ "trackToken": track_token, "isPositive": positive }))
            .map(|v| id_of(&v, "feedbackId"))
    }

    /// `v1/listener/addTiredSong`: off every station for a month.
    pub fn add_tired(&mut self, track_token: &str) -> Result<(), Error> {
        self.post("/api/v1/listener/addTiredSong", &json!({ "trackToken": track_token })).map(|_| ())
    }

    /// `v1/station/trackStarted`: what the site reports when a song begins
    /// (it counts toward skips and listening history).
    pub fn track_started(&mut self, track_token: &str) -> Result<(), Error> {
        self.post("/api/v1/station/trackStarted", &json!({ "trackToken": track_token })).map(|_| ())
    }

    /// `v1/station/playbackResumed` with `forceActive`: stop the stream on the
    /// account's other device and play here.
    pub fn take_over(&mut self) -> Result<(), Error> {
        self.post("/api/v1/station/playbackResumed", &json!({ "forceActive": true })).map(|_| ())
    }

    /// `v1/ad/getAdList`: what the site asks when a song ends or is skipped.
    /// The answer is `{"ads": [...]}`, usually empty; read it with
    /// [`crate::ads::parse_ad_list`].
    pub fn ad_list(&mut self, ask: &AdAsk) -> Result<Value, Error> {
        self.post(
            "/api/v1/ad/getAdList",
            &json!({
                "videoAdIndex": 1,
                "audioAdIndex": 1,
                "nextTrackStationId": ask.station_id,
                "secondsUntilAd": 0,
                "currentTrackStationId": ask.station_id,
                "currentTrackToken": ask.track_token,
                "currentTrackType": "Track",
                "adsWizzAdId": null,
                "adBlockerEnabled": false,
                "httpReferrer": "",
                "currentTrackSecondsPlayed": ask.seconds_played,
                "userInitiatedTrackEnd": ask.user_initiated,
                "userInitiatedTrackEndActionType": if ask.user_initiated { "GLOBAL_SKIP_TRACK" } else { "" },
                "secondsSinceDisplayAd": 0,
                "skipForReplay": false,
                "userTimedOut": false,
                "currentTrackDeliveryType": null,
                "nextTrackDeliveryType": null,
                "deviceProperties": { "applicationVersion": AD_APP_VERSION },
            }),
        )
    }

    /// `v1/ad/registerAudioProgrammaticImpression`: tells Pandora an audio ad
    /// began to play.
    pub fn register_audio_impression(
        &mut self,
        ad_token: &str,
        request_meta: &str,
        station_id: &str,
    ) -> Result<(), Error> {
        self.post(
            "/api/v1/ad/registerAudioProgrammaticImpression",
            &json!({
                "adRequestMetaData": request_meta,
                "adToken": ad_token,
                "stationId": station_id,
                "httpReferrer": "",
            }),
        )
        .map(|_| ())
    }

    /// Downloads a song's audio (a few MB) to decode it.
    pub fn fetch_audio(&self, url: &str) -> Result<Vec<u8>, Error> {
        self.fetch(url, "audio", 64 * 1024 * 1024)
    }

    /// An album cover (Pandora's art CDN), at most 8 MB.
    pub fn fetch_art(&self, url: &str) -> Result<Vec<u8>, Error> {
        self.fetch(url, "art", 8 * 1024 * 1024)
    }

    fn fetch(&self, url: &str, what: &str, limit: u64) -> Result<Vec<u8>, Error> {
        let mut response = self
            .agent
            .get(url)
            .header("User-Agent", &self.user_agent)
            .header("Referer", format!("{SITE}/"))
            .call()
            .map_err(|e| Error::Http(e.to_string()))?;
        if !response.status().is_success() {
            return Err(Error::Http(format!("{what}: HTTP {}", response.status())));
        }
        response.body_mut().with_config().limit(limit).read_to_vec().map_err(|e| Error::Http(e.to_string()))
    }
}

/// Runs `call` the way every Pandora call should run: when the login expired
/// (401), log in again with the saved password and try once more; when another
/// device is streaming (429 `STREAM_VIOLATION`), take the stream over and try
/// once more. `took_over` says the second happened.
pub fn with_retries<T>(
    client: &mut Client,
    login: Option<&(String, String)>,
    took_over: &mut bool,
    mut call: impl FnMut(&mut Client) -> Result<T, Error>,
) -> Result<T, Error> {
    let mut relogged = false;
    let mut retook = false;
    loop {
        match call(client) {
            Err(e) if e.is_auth_expired() && !relogged => {
                let Some((user, pass)) = login else { return Err(e) };
                relogged = true;
                client.login(user, pass)?;
            }
            Err(Error::StreamViolation) if !retook => {
                retook = true;
                client.take_over()?;
                *took_over = true;
            }
            other => return other,
        }
    }
}

#[cfg(test)]
mod tests;
