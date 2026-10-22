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

use std::collections::HashSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

pub const SITE: &str = "https://www.pandora.com";
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
    Some(Station {
        id,
        name: str_of(v, "name"),
        is_shuffle: v.get("isShuffle").and_then(Value::as_bool).unwrap_or(false),
        is_shared: v.get("isShared").and_then(Value::as_bool).unwrap_or(false),
        art_url: largest_art(v.get("art")),
    })
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
    })
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
    pub fn add_feedback(&mut self, track_token: &str, positive: bool) -> Result<(), Error> {
        self.post("/api/v1/station/addFeedback", &json!({ "trackToken": track_token, "isPositive": positive }))
            .map(|_| ())
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
