//! Good Vibes Only: the audio ads Pandora's website plays between songs.
//!
//! When a song ends (or is skipped), and Settings → Good Vibes Only is on,
//! `getAdList` is asked whether an ad is due. Usually the answer is none and
//! the next song starts as always. A pod of ads plays one after the other in
//! the Now Playing card (no skip, no thumbs; pause works), each reported as
//! the website reports it (see [`crate::ads`]). Whatever goes wrong, the next
//! song starts: an ad never holds the music back for long.

use std::time::SystemTime;

use super::*;
use crate::ads::{self, Action, Break};
use crate::config::GoodVibes;
use crate::pandora::AdAsk;

/// Pandora gets this long to say whether an ad is due.
const ASK_TIMEOUT: Duration = Duration::from_secs(8);
/// An ad's audio gets this long to download.
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);
/// An ad that stops before this much of it played is given up, not reported done.
const PLAYED_ENOUGH: f64 = 0.8;

/// How a song ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Played,
    Skipped,
}

/// Where the ad break is.
#[derive(Debug, Default)]
pub enum AdPhase {
    #[default]
    Off,
    /// `getAdList` was asked at this moment; the next song waits for the answer.
    Asking(Instant),
    Playing(Box<AdPlay>),
}

/// An ad pod being played.
#[derive(Debug)]
pub struct AdPlay {
    brk: Break,
    /// Which download the current ad waits for, and since when.
    waiting: Option<(String, Instant)>,
    /// What the audio worker answers to (one per ad, so a late answer is ignored).
    serial: u32,
}

/// What Now Playing shows while an ad plays.
#[derive(Debug, Clone, PartialEq)]
pub struct AdView {
    /// 1-based.
    pub number: usize,
    pub count: usize,
    pub length: u32,
    pub elapsed: Duration,
    pub paused: bool,
    /// The ad's audio is still downloading.
    pub loading: bool,
}

impl App {
    /// An ad (or the question about one) is in the way of the next song.
    pub fn ad_break(&self) -> bool {
        !matches!(self.ad, AdPhase::Off)
    }

    /// What to draw in Now Playing while an ad plays.
    pub fn ad_view(&self) -> Option<AdView> {
        let AdPhase::Playing(play) = &self.ad else { return None };
        let ad = play.brk.ad()?;
        Some(AdView {
            number: play.brk.index() + 1,
            count: play.brk.count(),
            length: ad.seconds(),
            elapsed: if play.waiting.is_some() { Duration::ZERO } else { self.audio.position() },
            paused: self.audio.is_paused(),
            loading: play.waiting.is_some(),
        })
    }

    /// A song is over: ask whether an ad is due, or go straight on.
    pub(super) fn end_song(&mut self, how: End) {
        let ask = self.ad_ask(how);
        let Some(ask) = ask else { return self.advance() };
        if let Some(song) = self.now.take() {
            self.push_history(song);
        }
        self.audio.stop();
        self.buffering = None;
        self.ad = AdPhase::Asking(Instant::now());
        self.backend.send(Request::AdList { ask });
    }

    fn ad_ask(&self, how: End) -> Option<AdAsk> {
        let song = self.now.as_ref()?;
        let can_play = self.demo || self.audio.has_device();
        (self.config.player.good_vibes_only.plays_ads()
            && can_play
            && self.session == Session::LoggedIn
            && !self.ad_break()
            && !song.station_id.is_empty()
            && !song.track_token.is_empty())
        .then(|| AdAsk {
            station_id: song.station_id.clone(),
            track_token: song.track_token.clone(),
            seconds_played: self.audio.position().as_secs() as u32,
            user_initiated: how == End::Skipped,
        })
    }

    /// Back to the music: no ad (more).
    fn ad_over(&mut self) {
        self.ad = AdPhase::Off;
        self.audio.stop();
        self.next_song();
    }

    /// Gives the ad break up (a message says why) and plays the next song.
    fn ad_give_up(&mut self, why: &str) {
        self.status = format!("Ads: {why}: playing on");
        self.ad_over();
    }

    /// The answer to `getAdList`.
    pub(super) fn ad_list_arrived(&mut self, station_id: &str, ads: Vec<ads::Ad>) {
        if !matches!(self.ad, AdPhase::Asking(_)) || self.current_station.as_deref() != Some(station_id) {
            return;
        }
        if ads.is_empty() {
            return self.ad_over();
        }
        self.ad_serial += 1;
        let play = AdPlay { brk: Break::new(ads, station_id), waiting: None, serial: self.ad_serial };
        self.ad = AdPhase::Playing(Box::new(play));
        self.ad_load();
    }

    /// `getAdList` failed.
    pub(super) fn ad_ask_failed(&mut self, message: &str) {
        if matches!(self.ad, AdPhase::Asking(_)) {
            self.ad_give_up(&format!("Pandora didn't answer ({message})"));
        }
    }

    /// Gets the current ad's audio: asked for from Pandora's ad server (the demo
    /// plays silently, as it does for songs).
    fn ad_load(&mut self) {
        let demo = self.demo;
        let AdPhase::Playing(play) = &mut self.ad else { return };
        let Some(ad) = play.brk.ad() else { return self.ad_over() };
        if demo {
            return self.ad_started(None);
        }
        let token = format!("ad:{}:{}", play.serial, play.brk.index());
        let url = ad.audio_url.clone();
        play.waiting = Some((token.clone(), Instant::now()));
        self.backend.send(Request::Audio { track_token: token, url });
    }

    /// The audio worker brought an ad (true: it was the one waited for).
    /// Gives the bytes back when they are not an ad's.
    pub(super) fn ad_audio_arrived(&mut self, token: &str, data: Vec<u8>) -> Option<Vec<u8>> {
        if !self.ad_waits_for(token) {
            return Some(data);
        }
        self.ad_started(Some(data));
        None
    }

    /// The audio worker could not fetch an ad.
    pub(super) fn ad_audio_failed(&mut self, token: &str, message: &str) -> bool {
        if !self.ad_waits_for(token) {
            return false;
        }
        self.ad_give_up(message);
        true
    }

    fn ad_waits_for(&self, token: &str) -> bool {
        matches!(&self.ad, AdPhase::Playing(p) if p.waiting.as_ref().is_some_and(|(t, _)| t == token))
    }

    /// The ad's audio is here: play it, and report that it began.
    fn ad_started(&mut self, data: Option<Vec<u8>>) {
        if let AdPhase::Playing(play) = &mut self.ad {
            play.waiting = None;
        }
        if let Err(e) = self.audio.start(
            data.filter(|d| !d.is_empty()),
            0.0,
            self.config.player.good_vibes_only == GoodVibes::Muted,
        ) {
            return self.ad_give_up(&e);
        }
        let AdPhase::Playing(play) = &mut self.ad else { return };
        let actions = play.brk.start(SystemTime::now());
        self.ad_send(actions);
    }

    /// Every frame: report the quartiles, move on when an ad is over, and give
    /// up on a question or download that takes too long.
    pub(super) fn ad_tick(&mut self) {
        match &self.ad {
            AdPhase::Off => {}
            AdPhase::Asking(since) => {
                if since.elapsed() > ASK_TIMEOUT {
                    self.ad_give_up("Pandora took too long to answer");
                }
            }
            AdPhase::Playing(play) => {
                if let Some((_, since)) = &play.waiting {
                    if since.elapsed() > FETCH_TIMEOUT {
                        self.ad_give_up("the ad took too long to download");
                    }
                    return;
                }
                let Some(length) = play.brk.ad().map(|a| a.seconds()) else { return self.ad_over() };
                let at = self.audio.position();
                if self.audio.finished(length) {
                    self.ad_ended(at, length);
                } else if let AdPhase::Playing(play) = &mut self.ad {
                    let actions = play.brk.progress(at, SystemTime::now());
                    self.ad_send(actions);
                }
            }
        }
    }

    /// The ad's audio ran out at `at` of its `length` seconds.
    fn ad_ended(&mut self, at: Duration, length: u32) {
        if self.audio_is_real() && at.as_secs_f64() < f64::from(length) * PLAYED_ENOUGH {
            return self.ad_give_up("the ad stopped early");
        }
        let AdPhase::Playing(play) = &mut self.ad else { return };
        let actions = play.brk.finish(SystemTime::now());
        let done = play.brk.done();
        self.ad_send(actions);
        if done {
            self.ad_over();
        } else {
            self.audio.stop();
            self.ad_load();
        }
    }

    /// Sound is really playing (not the silent clock of the demo).
    fn audio_is_real(&self) -> bool {
        !self.demo && self.audio.has_device()
    }

    /// Does what the break decided: fetch tracking URLs, register the ad.
    fn ad_send(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Track { urls, .. } => {
                    self.ad_tracker.fire(urls);
                }
                Action::Register { token, request_meta, station_id } => {
                    if !self.demo {
                        self.backend.send(Request::AdImpression { token, request_meta, station_id });
                    }
                }
            }
        }
    }
}
