//! Pandora calls run on worker threads so the interface never waits: one
//! for the website API (login, stations, playlists, thumbs), one that
//! downloads audio. Every API call goes through [`pandora::with_retries`]
//! (log in again on 401, take the stream over on `STREAM_VIOLATION`).
//! `--demo` answers from [`crate::demo`] instead.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::ads::{self, Ad};
use crate::demo;
use crate::keychain::Keychain;
use crate::pandora::{
    self, AdAsk, Client, Error, FeedbackPage, GenreCategory, Quality, SearchHit, Seed, Song, Station, StationDetails,
};
use crate::update::{self, Checked, Update};

pub enum Request {
    Login {
        username: String,
        password: String,
    },
    Stations,
    /// Rename a station on the Pandora account.
    RenameStation {
        station_id: String,
        name: String,
    },
    /// Remove a station from the Pandora account.
    RemoveStation {
        station_id: String,
    },
    /// Seeds and thumbs counts of a station (`current`: it is the one playing).
    StationDetails {
        station_id: String,
        current: bool,
    },
    /// The seeds of a station (the seeds editor).
    Seeds {
        station_id: String,
    },
    /// Artists to suggest as seeds, near `seed_music_id` (a seed's `musicId`).
    SeedSuggestions {
        station_id: String,
        seed_music_id: String,
    },
    /// Add the artist or song `seed_id` (`name` is for the message).
    AddSeed {
        station_id: String,
        seed_id: String,
        name: String,
    },
    RemoveSeed {
        station_id: String,
        seed_id: String,
        name: String,
    },
    /// A page of the thumbs up (or down) you gave on a station, from `start`.
    Feedback {
        station_id: String,
        positive: bool,
        start: usize,
    },
    DeleteFeedback {
        station_id: String,
        feedback_id: String,
        positive: bool,
    },
    /// Search Pandora for artists and songs to start a station from.
    Search {
        query: String,
    },
    /// Make a station from a search hit or a genre station (`pandora_id`).
    CreateStation {
        pandora_id: String,
    },
    /// Pandora's genre categories and their stations.
    Genres,
    Playlist {
        station_id: String,
    },
    Rate {
        track_token: String,
        positive: bool,
    },
    Tired {
        track_token: String,
    },
    /// `getAdList`: a song ended (or was skipped); is an ad due?
    AdList {
        ask: AdAsk,
    },
    /// `registerAudioProgrammaticImpression`: an audio ad began to play.
    AdImpression {
        token: String,
        request_meta: String,
        station_id: String,
    },
    /// The song began playing (Pandora's `trackStarted`).
    Started {
        track_token: String,
    },
    Audio {
        track_token: String,
        url: String,
    },
    Quality(Quality),
    /// An album cover for Now Playing (its own thread: never waits on audio).
    Art {
        url: String,
    },
}

pub enum Reply {
    /// `hifi`: the plan streams 192 kbps MP3.
    LoggedIn {
        hifi: bool,
    },
    /// `wrong_password`: Pandora said the email or password is wrong.
    LoginFailed {
        message: String,
        wrong_password: bool,
    },
    Stations(Vec<Station>),
    /// `name`: what Pandora now calls the station.
    StationRenamed {
        station_id: String,
        name: String,
    },
    StationRemoved {
        station_id: String,
    },
    StationDetails {
        station_id: String,
        details: StationDetails,
    },
    Seeds {
        station_id: String,
        seeds: Vec<Seed>,
    },
    SeedSuggestions {
        station_id: String,
        hits: Vec<SearchHit>,
    },
    SeedAdded {
        station_id: String,
        name: String,
    },
    SeedRemoved {
        station_id: String,
        name: String,
    },
    /// `start`: where the page began (0: the list starts over).
    Feedback {
        station_id: String,
        positive: bool,
        start: usize,
        page: FeedbackPage,
    },
    FeedbackDeleted {
        station_id: String,
        positive: bool,
    },
    /// `query`: what was asked, so a late answer to an older query is ignored.
    SearchResults {
        query: String,
        hits: Vec<SearchHit>,
    },
    /// A station Pandora just made (the account now has it).
    StationCreated(Station),
    Genres(Vec<GenreCategory>),
    Playlist {
        station_id: String,
        songs: Vec<Song>,
    },
    Rated {
        track_token: String,
        positive: bool,
    },
    Tired {
        track_token: String,
    },
    /// The ads due now (usually none), for the song that ended on `station_id`.
    AdList {
        station_id: String,
        ads: Vec<Ad>,
    },
    /// Another device was streaming on the account; boxvibe took over.
    TookOver,
    Audio {
        track_token: String,
        data: Vec<u8>,
    },
    AudioFailed {
        track_token: String,
        message: String,
    },
    Failed {
        what: &'static str,
        message: String,
    },
    /// `None`: no cover (download or decode failed).
    Art {
        url: String,
        image: Option<image::DynamicImage>,
    },
    /// An answer from the keychain (see [`Backend::keychain`]).
    Keychain(KeychainReply),
    UpdateFound(Update),
    UpToDate,
    UpdateProgress(u8),
    SelfUpdated(Result<String, String>),
}

/// What the keychain said. The errors are only "it didn't work": the app tells
/// the user [`crate::keychain::NO_KEYCHAIN`], not D-Bus details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeychainReply {
    /// The saved password (`None`: nothing saved for this account).
    Loaded(Result<Option<String>, String>),
    Saved(Result<(), String>),
    /// `loud`: the user asked (Settings, Log out), so say how it went.
    Forgotten {
        result: Result<(), String>,
        loud: bool,
    },
}

/// One keychain job, run on its own thread (a locked keychain can wait for the
/// user to unlock it, which must never freeze the interface).
pub enum KeychainJob {
    Load,
    Save(String),
    Forget { loud: bool },
}

pub struct Backend {
    api: Sender<Request>,
    audio: Sender<Request>,
    art: Sender<Request>,
    pub replies: Receiver<Reply>,
    tx: Sender<Reply>,
}

impl Backend {
    pub fn send(&self, request: Request) {
        let to = match request {
            Request::Audio { .. } => &self.audio,
            Request::Art { .. } => &self.art,
            _ => &self.api,
        };
        let _ = to.send(request);
    }

    pub fn spawn(demo: bool, quality: Quality, user_agent: String) -> Self {
        let (reply_tx, replies) = mpsc::channel();
        let tx = reply_tx.clone();
        let (api, api_rx) = mpsc::channel();
        let (audio, audio_rx) = mpsc::channel();
        let (art, art_rx) = mpsc::channel();
        let art_tx = reply_tx.clone();
        let art_agent = user_agent.clone();
        thread::spawn(move || art_worker(art_rx, art_tx, demo, &art_agent));
        if demo {
            let demo_tx = reply_tx.clone();
            thread::spawn(move || demo_worker(api_rx, demo_tx));
            thread::spawn(move || {
                for request in audio_rx {
                    if let Request::Audio { track_token, .. } = request {
                        let _ = reply_tx.send(Reply::Audio { track_token, data: Vec::new() });
                    }
                }
            });
        } else {
            let api_tx = reply_tx.clone();
            let agent = user_agent.clone();
            thread::spawn(move || api_worker(api_rx, api_tx, quality, &agent));
            thread::spawn(move || audio_worker(audio_rx, reply_tx, &user_agent));
        }
        Self { api, audio, art, replies, tx }
    }

    /// Runs `job` on the Pandora password of `account` in `keychain`; the
    /// answer arrives as [`Reply::Keychain`].
    pub fn keychain(&self, keychain: &Arc<dyn Keychain>, account: &str, job: KeychainJob) {
        let (keychain, account, tx) = (Arc::clone(keychain), account.to_owned(), self.tx.clone());
        thread::spawn(move || {
            let reply = match job {
                KeychainJob::Load => KeychainReply::Loaded(keychain.get(&account).map_err(|e| e.to_string())),
                KeychainJob::Save(password) => {
                    KeychainReply::Saved(keychain.set(&account, &password).map_err(|e| e.to_string()))
                }
                KeychainJob::Forget { loud } => {
                    KeychainReply::Forgotten { result: keychain.forget(&account).map_err(|e| e.to_string()), loud }
                }
            };
            let _ = tx.send(Reply::Keychain(reply));
        });
    }

    /// Asks GitHub for a newer release, quietly: no network or no news says nothing.
    pub fn check_update(&self) {
        let tx = self.tx.clone();
        thread::spawn(move || {
            let reply = match update::check() {
                Checked::Newer(update) => Reply::UpdateFound(update),
                Checked::Latest => Reply::UpToDate,
                Checked::Unknown => return,
            };
            let _ = tx.send(reply);
        });
    }

    /// About's Update Now: download the release and put it in place of this program.
    pub fn self_update(&self, url: String, version: String) {
        let tx = self.tx.clone();
        thread::spawn(move || {
            let progress = |percent| {
                let _ = tx.send(Reply::UpdateProgress(percent));
            };
            let result = update::install(&url, &version, progress);
            let _ = tx.send(Reply::SelfUpdated(result.map(|_| version)));
        });
    }
}

fn demo_worker(requests: Receiver<Request>, replies: Sender<Reply>) {
    let mut round = 0;
    let mut ads_asked = 0;
    // The made-up account: renames and removals change this list only.
    let mut stations = demo::stations();
    let mut editing = demo::Editing::default();
    for request in requests {
        let reply = match request {
            Request::Login { .. } => Reply::LoggedIn { hifi: true },
            Request::Stations => Reply::Stations(stations.clone()),
            Request::RenameStation { station_id, name } => {
                match stations.iter_mut().find(|s| s.id == station_id && s.allow_rename) {
                    Some(station) => {
                        station.name = name.clone();
                        Reply::StationRenamed { station_id, name }
                    }
                    None => Reply::Failed { what: "rename", message: "no such station".into() },
                }
            }
            Request::RemoveStation { station_id } => {
                match stations.iter().position(|s| s.id == station_id && s.allow_delete) {
                    Some(at) => {
                        stations.remove(at);
                        Reply::StationRemoved { station_id }
                    }
                    None => Reply::Failed { what: "remove", message: "no such station".into() },
                }
            }
            Request::StationDetails { station_id, .. } => {
                Reply::StationDetails { details: editing.details(&station_id), station_id }
            }
            Request::Seeds { station_id } => Reply::Seeds { seeds: editing.seeds(&station_id).clone(), station_id },
            Request::SeedSuggestions { station_id, .. } => {
                Reply::SeedSuggestions { hits: editing.suggestions(&station_id), station_id }
            }
            Request::AddSeed { station_id, seed_id, .. } => match editing.add_seed(&station_id, &seed_id) {
                Ok(name) => Reply::SeedAdded { station_id, name },
                Err(message) => Reply::Failed { what: "add seed", message },
            },
            Request::RemoveSeed { station_id, seed_id, .. } => match editing.remove_seed(&station_id, &seed_id) {
                Ok(name) => Reply::SeedRemoved { station_id, name },
                Err(message) => Reply::Failed { what: "remove seed", message },
            },
            Request::Feedback { station_id, positive, start } => {
                Reply::Feedback { page: editing.feedback(&station_id, positive, start), station_id, positive, start }
            }
            Request::DeleteFeedback { station_id, feedback_id, positive } => {
                match editing.delete_feedback(&station_id, &feedback_id, positive) {
                    Ok(()) => Reply::FeedbackDeleted { station_id, positive },
                    Err(message) => Reply::Failed { what: "delete thumb", message },
                }
            }
            Request::Search { query } => Reply::SearchResults { hits: demo::search(&query), query },
            Request::CreateStation { pandora_id } => match demo::new_station(&pandora_id, stations.len()) {
                Some(station) => {
                    // Like Pandora: an existing one is not made twice; a new one comes after Shuffle.
                    match stations.iter().find(|s| s.name == station.name) {
                        Some(known) => Reply::StationCreated(known.clone()),
                        None => {
                            let at = usize::from(stations.first().is_some_and(|s| s.is_shuffle));
                            stations.insert(at, station.clone());
                            Reply::StationCreated(station)
                        }
                    }
                }
                None => Reply::Failed { what: "create station", message: "no such artist, song or genre".into() },
            },
            Request::Genres => Reply::Genres(demo::genres()),
            Request::Playlist { station_id } => {
                round += 1;
                Reply::Playlist { songs: demo::playlist(&station_id, round - 1), station_id }
            }
            Request::Rate { track_token, positive } => Reply::Rated { track_token, positive },
            Request::Tired { track_token } => Reply::Tired { track_token },
            Request::AdList { ask } => {
                ads_asked += 1;
                Reply::AdList { ads: demo::ads(ads_asked), station_id: ask.station_id }
            }
            Request::Audio { .. }
            | Request::Art { .. }
            | Request::Quality(_)
            | Request::Started { .. }
            | Request::AdImpression { .. } => continue,
        };
        if replies.send(reply).is_err() {
            break;
        }
    }
}

fn api_worker(requests: Receiver<Request>, replies: Sender<Reply>, quality: Quality, user_agent: &str) {
    let mut client = Client::new(user_agent);
    client.quality = quality;
    let mut login: Option<(String, String)> = None;
    for request in requests {
        let mut took_over = false;
        let reply = match request {
            Request::Login { username, password } => match client.login(&username, &password) {
                Ok(()) => {
                    login = Some((username, password));
                    Reply::LoggedIn { hifi: client.account.hifi() }
                }
                Err(e) => Reply::LoginFailed { wrong_password: e == Error::WrongLogin, message: e.to_string() },
            },
            Request::Stations => {
                run(&mut client, &login, &mut took_over, "stations", |c| c.stations().map(Reply::Stations))
            }
            Request::RenameStation { station_id, name } => run(&mut client, &login, &mut took_over, "rename", |c| {
                c.rename_station(&station_id, &name)
                    .map(|name| Reply::StationRenamed { station_id: station_id.clone(), name })
            }),
            Request::RemoveStation { station_id } => run(&mut client, &login, &mut took_over, "remove", |c| {
                c.remove_station(&station_id).map(|()| Reply::StationRemoved { station_id: station_id.clone() })
            }),
            Request::StationDetails { station_id, current } => {
                run(&mut client, &login, &mut took_over, "details", |c| {
                    c.station_details(&station_id, current)
                        .map(|details| Reply::StationDetails { station_id: station_id.clone(), details })
                })
            }
            Request::Seeds { station_id } => run(&mut client, &login, &mut took_over, "seeds", |c| {
                c.seeds(&station_id).map(|seeds| Reply::Seeds { station_id: station_id.clone(), seeds })
            }),
            Request::SeedSuggestions { station_id, seed_music_id } => {
                run(&mut client, &login, &mut took_over, "suggestions", |c| {
                    c.seed_suggestions(&station_id, &seed_music_id)
                        .map(|hits| Reply::SeedSuggestions { station_id: station_id.clone(), hits })
                })
            }
            Request::AddSeed { station_id, seed_id, name } => {
                run(&mut client, &login, &mut took_over, "add seed", |c| {
                    c.add_seed(&station_id, &seed_id)
                        .map(|()| Reply::SeedAdded { station_id: station_id.clone(), name: name.clone() })
                })
            }
            Request::RemoveSeed { station_id, seed_id, name } => {
                run(&mut client, &login, &mut took_over, "remove seed", |c| {
                    c.remove_seed(&station_id, &seed_id)
                        .map(|()| Reply::SeedRemoved { station_id: station_id.clone(), name: name.clone() })
                })
            }
            Request::Feedback { station_id, positive, start } => {
                run(&mut client, &login, &mut took_over, "thumbs history", |c| {
                    c.station_feedback(&station_id, positive, start).map(|page| Reply::Feedback {
                        station_id: station_id.clone(),
                        positive,
                        start,
                        page,
                    })
                })
            }
            Request::DeleteFeedback { station_id, feedback_id, positive } => {
                run(&mut client, &login, &mut took_over, "delete thumb", |c| {
                    c.delete_feedback(&feedback_id, positive)
                        .map(|()| Reply::FeedbackDeleted { station_id: station_id.clone(), positive })
                })
            }
            Request::Search { query } => run(&mut client, &login, &mut took_over, "search", |c| {
                c.search(&query).map(|hits| Reply::SearchResults { query: query.clone(), hits })
            }),
            Request::CreateStation { pandora_id } => run(&mut client, &login, &mut took_over, "create station", |c| {
                c.create_station(&pandora_id).map(Reply::StationCreated)
            }),
            Request::Genres => run(&mut client, &login, &mut took_over, "genres", |c| c.genres().map(Reply::Genres)),
            Request::Playlist { station_id } => run(&mut client, &login, &mut took_over, "playlist", |c| {
                c.fragment(&station_id).map(|songs| Reply::Playlist { station_id: station_id.clone(), songs })
            }),
            Request::Rate { track_token, positive } => run(&mut client, &login, &mut took_over, "thumbs", |c| {
                c.add_feedback(&track_token, positive)
                    .map(|()| Reply::Rated { track_token: track_token.clone(), positive })
            }),
            Request::Tired { track_token } => run(&mut client, &login, &mut took_over, "tired", |c| {
                c.add_tired(&track_token).map(|()| Reply::Tired { track_token: track_token.clone() })
            }),
            Request::AdList { ask } => run(&mut client, &login, &mut took_over, "ads", |c| {
                c.ad_list(&ask)
                    .map(|v| Reply::AdList { station_id: ask.station_id.clone(), ads: ads::parse_ad_list(&v, true) })
            }),
            Request::AdImpression { token, request_meta, station_id } => {
                // A report only: Pandora's answer is empty and a failure is not worth a message.
                let _ = pandora::with_retries(&mut client, login.as_ref(), &mut took_over, |c| {
                    c.register_audio_impression(&token, &request_meta, &station_id)
                });
                if !took_over {
                    continue;
                }
                Reply::TookOver
            }
            Request::Started { track_token } => {
                // A report only: a failure here is not worth a message.
                let _ = pandora::with_retries(&mut client, login.as_ref(), &mut took_over, |c| {
                    c.track_started(&track_token)
                });
                if !took_over {
                    continue;
                }
                Reply::TookOver
            }
            Request::Quality(quality) => {
                client.quality = quality;
                continue;
            }
            Request::Audio { .. } | Request::Art { .. } => continue,
        };
        if took_over && !matches!(reply, Reply::TookOver) && replies.send(Reply::TookOver).is_err() {
            break;
        }
        if replies.send(reply).is_err() {
            break;
        }
    }
}

/// One API call with the retries; its failure as a [`Reply::Failed`].
fn run(
    client: &mut Client,
    login: &Option<(String, String)>,
    took_over: &mut bool,
    what: &'static str,
    call: impl FnMut(&mut Client) -> Result<Reply, Error>,
) -> Reply {
    pandora::with_retries(client, login.as_ref(), took_over, call).unwrap_or_else(|e| failed(what, e))
}

fn failed(what: &'static str, error: Error) -> Reply {
    Reply::Failed { what, message: error.to_string() }
}

fn audio_worker(requests: Receiver<Request>, replies: Sender<Reply>, user_agent: &str) {
    let client = Client::new(user_agent);
    for request in requests {
        let Request::Audio { track_token, url } = request else { continue };
        let reply = if url.is_empty() {
            Reply::AudioFailed { track_token, message: "Pandora gave no audio URL for this song".into() }
        } else {
            match client.fetch_audio(&url) {
                Ok(data) => Reply::Audio { track_token, data },
                Err(e) => Reply::AudioFailed { track_token, message: e.to_string() },
            }
        };
        if replies.send(reply).is_err() {
            break;
        }
    }
}

/// Covers: the disk cache first, else Pandora's CDN (kept on disk, capped);
/// `demo` makes one up so nothing touches the network.
fn art_worker(requests: Receiver<Request>, replies: Sender<Reply>, demo: bool, user_agent: &str) {
    let client = Client::new(user_agent);
    let dir = if demo { None } else { crate::art::cache_dir() };
    let log = if demo { None } else { crate::art::log_path() };
    for request in requests {
        let Request::Art { url } = request else { continue };
        let image = if demo {
            Some(crate::art::demo_cover(&url))
        } else {
            let file = dir.as_ref().map(|d| crate::art::cache_path(d, &url));
            let cached = file.as_ref().and_then(|f| std::fs::read(f).ok()).and_then(|b| crate::art::decode(&b));
            let fetched = || -> Result<image::DynamicImage, String> {
                let bytes = client.fetch_art(&url).map_err(|e| format!("download failed: {e}"))?;
                let image = crate::art::decode(&bytes)
                    .ok_or_else(|| format!("not a picture boxvibe can read ({} bytes)", bytes.len()))?;
                if let (Some(d), Some(f)) = (&dir, &file)
                    && std::fs::create_dir_all(d).is_ok()
                    && std::fs::write(f, &bytes).is_ok()
                {
                    crate::art::trim_cache(d, crate::art::DISK_CAP);
                }
                Ok(image)
            };
            match cached.map(Ok).unwrap_or_else(fetched) {
                Ok(image) => Some(image),
                Err(why) => {
                    if let Some(log) = &log {
                        crate::art::log_failure(log, &url, &why);
                    }
                    None
                }
            }
        };
        if replies.send(Reply::Art { url, image }).is_err() {
            break;
        }
    }
}
