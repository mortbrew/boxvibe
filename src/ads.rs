//! Pandora's audio ads, played and reported the way the website does it
//! (Settings → PLAYBACK → Good Vibes Only).
//!
//! At a song's end `getAdList` may answer with a pod of audio ads. Each is
//! played in full; as it plays, the tracking URLs Pandora listed for it are
//! fetched (start, 25 %, 50 %, 75 %, complete) with the website's `[MACROS]`
//! filled in, and `registerAudioProgrammaticImpression` tells Pandora it ran.
//!
//! What this never does: click through, open a browser, show or fetch the ad's
//! picture, or call any host Pandora's answer did not name. Tracking runs on
//! its own short-lived threads with short timeouts, https only.

use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

/// More ads than this in one answer are ignored (a pod is two or three).
const MAX_ADS: usize = 4;
/// An ad that claims to last longer is not believed (and not played).
const MAX_AD_SECONDS: f64 = 180.0;
/// One tracking request gives up after this long.
const TRACKING_TIMEOUT: Duration = Duration::from_secs(5);

/// The progress marks, in order: Pandora's event name and the fraction of the ad.
const MARKS: [(&str, f64); 4] =
    [("AUDIO_FIRST_QUARTILE", 0.25), ("AUDIO_MIDPOINT", 0.5), ("AUDIO_THIRD_QUARTILE", 0.75), ("AUDIO_COMPLETE", 1.0)];

/// One audio ad of a pod.
#[derive(Debug, Clone, PartialEq)]
pub struct Ad {
    /// `adToken`, like `pod:1/2/1`.
    pub token: String,
    /// `adRequestMetaData`: handed back when the ad is registered.
    pub request_meta: String,
    /// `adLength`, seconds.
    pub length: f64,
    /// `audioUrlMap.mediumQuality`.
    pub audio_url: String,
    /// `vastTrackingEvents`: event name and its URLs.
    pub events: Vec<(String, Vec<String>)>,
    /// The `pandora-monocle` verification parameters: the query of the
    /// `other_ad_interaction` report.
    pub monocle: String,
}

impl Ad {
    fn urls(&self, event: &str) -> Vec<&str> {
        self.events
            .iter()
            .filter(|(name, _)| name == event)
            .flat_map(|(_, urls)| urls.iter().map(String::as_str))
            .collect()
    }

    /// Whole seconds the ad lasts, rounded up (what the player's clock needs).
    pub fn seconds(&self) -> u32 {
        self.length.ceil() as u32
    }

    /// `https://<haymaker host>/haymaker/te/other_ad_interaction?<monocle>`:
    /// the host is the one of the ad's own haymaker tracking URLs.
    fn other_interaction(&self) -> Option<String> {
        if self.monocle.is_empty() {
            return None;
        }
        let origin = self.events.iter().flat_map(|(_, urls)| urls).find_map(|u| {
            let (scheme, rest) = u.split_once("://")?;
            let (host, path) = rest.split_once('/')?;
            path.starts_with("haymaker/").then(|| format!("{scheme}://{host}"))
        })?;
        Some(format!("{origin}/haymaker/te/other_ad_interaction?{}", self.monocle))
    }
}

/// Whether `url` may be fetched: https (anything but https only for tests),
/// with a name for a host (never an address or localhost), no login in it.
pub fn allowed(url: &str, https_only: bool) -> bool {
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    let Some((scheme, rest)) = url.split_once("://") else { return false };
    if !https_only {
        return scheme == "http" || scheme == "https";
    }
    if scheme != "https" {
        return false;
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || host.contains('@') || host.starts_with('[') {
        return false;
    }
    let name = host.split(':').next().unwrap_or("").to_ascii_lowercase();
    let numeric = name.chars().all(|c| c.is_ascii_digit() || c == '.');
    !name.is_empty() && !numeric && name != "localhost" && !name.ends_with(".localhost") && name.contains('.')
}

/// The ads in a `getAdList` answer that can be played: audio ads with an
/// audio link and a believable length. An empty list is the usual answer.
pub fn parse_ad_list(v: &Value, https_only: bool) -> Vec<Ad> {
    let text = |ad: &Value, key: &str| ad.get(key).and_then(Value::as_str).unwrap_or("").to_owned();
    let mut out = Vec::new();
    for ad in v.get("ads").and_then(Value::as_array).into_iter().flatten() {
        if ad.get("adType").and_then(Value::as_str).is_some_and(|t| t != "AUDIO_AD") {
            continue;
        }
        let audio_url = ad.pointer("/audioUrlMap/mediumQuality").and_then(Value::as_str).unwrap_or("");
        let length = match ad.get("adLength") {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
            Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
            _ => 0.0,
        };
        if !allowed(audio_url, https_only) || !(length > 0.0 && length <= MAX_AD_SECONDS) {
            continue;
        }
        let events = ad
            .get("vastTrackingEvents")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|e| {
                let name = e.get("eventType")?.as_str()?.to_owned();
                let urls = e.get("urls")?.as_array()?;
                let urls: Vec<String> = urls
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|u| allowed(u, https_only))
                    .map(str::to_owned)
                    .collect();
                Some((name, urls))
            })
            .collect();
        let monocle = ad
            .get("adVerificationData")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|d| d.get("vendor").and_then(Value::as_str) == Some("pandora-monocle"))
            .map(|d| text(d, "parameters"))
            .filter(|p| !p.chars().any(|c| c.is_control() || c.is_whitespace()))
            .unwrap_or_default();
        out.push(Ad {
            token: text(ad, "adToken"),
            request_meta: text(ad, "adRequestMetaData"),
            length,
            audio_url: audio_url.to_owned(),
            events,
            monocle,
        });
        if out.len() == MAX_ADS {
            break;
        }
    }
    out
}

/// What to send now: tracking URLs (macros filled), or the registration.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Track { event: &'static str, urls: Vec<String> },
    Register { token: String, request_meta: String, station_id: String },
}

/// A pod being played: which ad, and which marks were reported for it.
#[derive(Debug)]
pub struct Break {
    ads: Vec<Ad>,
    index: usize,
    station_id: String,
    /// Marks of [`MARKS`] already reported for the current ad.
    sent: usize,
    started: bool,
}

impl Break {
    pub fn new(ads: Vec<Ad>, station_id: &str) -> Self {
        Self { ads, index: 0, station_id: station_id.to_owned(), sent: 0, started: false }
    }

    pub fn ad(&self) -> Option<&Ad> {
        self.ads.get(self.index)
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn count(&self) -> usize {
        self.ads.len()
    }

    /// The ad began to play: its impression, the monocle report, `audio_start`
    /// and the registration, in that order.
    pub fn start(&mut self, now: SystemTime) -> Vec<Action> {
        let Some(ad) = self.ads.get(self.index).filter(|_| !self.started) else { return Vec::new() };
        self.started = true;
        let fill = |urls: Vec<&str>, head: Duration| -> Vec<String> {
            urls.into_iter().map(|u| fill_macros(u, head, &ad.audio_url, now)).collect()
        };
        let mut out = vec![Action::Track { event: "IMPRESSION", urls: fill(ad.urls("IMPRESSION"), Duration::ZERO) }];
        if let Some(url) = ad.other_interaction() {
            out.push(Action::Track { event: "other_ad_interaction", urls: vec![url] });
        }
        out.push(Action::Track { event: "AUDIO_START", urls: fill(ad.urls("AUDIO_START"), Duration::ZERO) });
        out.push(Action::Register {
            token: ad.token.clone(),
            request_meta: ad.request_meta.clone(),
            station_id: self.station_id.clone(),
        });
        out.retain(|a| !matches!(a, Action::Track { urls, .. } if urls.is_empty()));
        out
    }

    /// The ad is `at` into its audio: the quartiles passed since last time
    /// (never the end: that is [`Break::finish`]).
    pub fn progress(&mut self, at: Duration, now: SystemTime) -> Vec<Action> {
        let Some(ad) = self.ads.get(self.index).filter(|_| self.started) else { return Vec::new() };
        let mut out = Vec::new();
        while let Some((event, fraction)) = MARKS.get(self.sent).filter(|(_, f)| *f < 1.0) {
            let due = Duration::from_secs_f64(ad.length * fraction);
            if at < due {
                break;
            }
            self.sent += 1;
            out.push(track(ad, event, at, now));
        }
        out.retain(|a| !matches!(a, Action::Track { urls, .. } if urls.is_empty()));
        out
    }

    /// The ad played to its end: whatever was not reported yet, then
    /// `audio_complete`; the next ad (if any) is the current one afterwards.
    pub fn finish(&mut self, now: SystemTime) -> Vec<Action> {
        let Some(ad) = self.ads.get(self.index).filter(|_| self.started) else { return Vec::new() };
        let mut out = Vec::new();
        while let Some((event, fraction)) = MARKS.get(self.sent) {
            self.sent += 1;
            out.push(track(ad, event, Duration::from_secs_f64(ad.length * fraction), now));
        }
        out.retain(|a| !matches!(a, Action::Track { urls, .. } if urls.is_empty()));
        self.index += 1;
        self.sent = 0;
        self.started = false;
        out
    }

    /// The pod is over (every ad played, or it was given up).
    pub fn done(&self) -> bool {
        self.index >= self.ads.len()
    }
}

fn track(ad: &Ad, event: &'static str, head: Duration, now: SystemTime) -> Action {
    let urls = ad.urls(event).into_iter().map(|u| fill_macros(u, head, &ad.audio_url, now)).collect();
    Action::Track { event, urls }
}

/// Fills the VAST macros the website fills: `[PLAYERSTATE]`, `[ADPLAYHEAD]`,
/// `[ASSETURI]` and `[TIMESTAMP]`.
pub fn fill_macros(url: &str, playhead: Duration, asset: &str, now: SystemTime) -> String {
    url.replace("[PLAYERSTATE]", "foreground")
        .replace("[ADPLAYHEAD]", &clock(playhead))
        .replace("[ASSETURI]", &percent_encode(asset))
        .replace("[TIMESTAMP]", &iso_utc(now))
}

/// `00:00:30.000`.
fn clock(at: Duration) -> String {
    let ms = at.as_millis();
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// `2026-10-25T14:03:05.123Z`.
fn iso_utc(at: SystemTime) -> String {
    let d = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    let (days, rest) = (d.as_secs() / 86_400, d.as_secs() % 86_400);
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
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3600,
        rest / 60 % 60,
        rest % 60,
        d.subsec_millis()
    )
}

/// Everything but letters, digits and `-._~` as `%XX`.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Fetches tracking URLs, each batch on its own thread.
#[derive(Clone)]
pub struct Tracker {
    agent: Option<ureq::Agent>,
    https_only: bool,
    user_agent: String,
}

impl Tracker {
    /// `https_only` is always true outside tests. `enabled` false (demo) sends nothing.
    pub fn new(enabled: bool, https_only: bool, user_agent: &str) -> Self {
        let agent = enabled.then(|| {
            ureq::Agent::config_builder()
                .timeout_global(Some(TRACKING_TIMEOUT))
                .https_only(https_only)
                .max_redirects(3)
                .http_status_as_error(false)
                .build()
                .into()
        });
        Self { agent, https_only, user_agent: user_agent.to_owned() }
    }

    /// GETs `urls` one after the other in the background. The thread's handle
    /// is only for tests to wait on; failures are ignored (it is a report).
    pub fn fire(&self, urls: Vec<String>) -> Option<JoinHandle<()>> {
        let agent = self.agent.clone()?;
        let urls: Vec<String> = urls.into_iter().filter(|u| allowed(u, self.https_only)).collect();
        if urls.is_empty() {
            return None;
        }
        let user_agent = self.user_agent.clone();
        Some(thread::spawn(move || {
            for url in urls {
                let _ = agent
                    .get(&url)
                    .header("User-Agent", &user_agent)
                    .header("Referer", "https://www.pandora.com/")
                    .call();
            }
        }))
    }
}

#[cfg(test)]
mod tests;
