//! Followed podcasts and the per-station thumbs list (view only).

use crate::backend::{Reply, Request};

use super::App;

impl App {
    pub(super) fn open_followed(&mut self) {
        self.view = super::View::Followed;
        self.focus = super::Focus::Content;
        self.mark_draw();
        if self.followed.is_empty() {
            self.status = "Loading followed podcasts…".into();
        }
        self.backend.send(Request::Followed);
    }

    pub(super) fn open_thumbs(&mut self) {
        self.view = super::View::Thumbs;
        self.focus = super::Focus::Content;
        self.mark_draw();
        if self.thumbs.is_empty() {
            self.status = "Loading thumbs…".into();
        }
        self.backend.send(Request::Thumbs);
    }

    /// `f` on a show page follows or unfollows it.
    pub(super) fn toggle_follow(&mut self) {
        let Some(id) = self.pages.path().and_then(|path| path.strip_prefix("podcast:")).map(str::to_owned) else {
            self.status = "Open a podcast to follow it".into();
            return;
        };
        let on = !self.followed.iter().any(|show| show.id == id);
        self.status = if on { "Following…".into() } else { "Unfollowing…".into() };
        self.backend.send(Request::Follow { pandora_id: id, on });
    }

    pub(super) fn show_followed(&mut self, shows: Vec<crate::pandora::FollowedShow>) {
        self.followed = shows;
        if self.followed_cursor >= self.followed.len() {
            self.followed_cursor = 0;
        }
        if self.view == super::View::Followed {
            self.status = if self.followed.is_empty() {
                "No followed podcasts".into()
            } else {
                format!("{} followed", self.followed.len())
            };
        }
    }

    pub(super) fn follow_set(&mut self, id: String, on: bool, message: Option<String>) {
        if let Some(message) = message {
            self.status = format!("⚠ {message}");
            return;
        }
        if on {
            if !self.followed.iter().any(|show| show.id == id) {
                let name = match &self.browse {
                    super::BrowseState::Loaded(sections) => {
                        sections
                            .first()
                            .map(|section| section.title.clone())
                            .unwrap_or_else(|| "Podcast".into())
                    }
                    _ => "Podcast".into(),
                };
                self.followed.insert(
                    0,
                    crate::pandora::FollowedShow { id: id.clone(), name, publisher: String::new() },
                );
            }
            self.status = "Following".into();
        } else {
            self.followed.retain(|show| show.id != id);
            self.status = "Unfollowed".into();
        }
    }

    pub(super) fn show_badges(&mut self, badges: std::collections::HashMap<String, String>) {
        self.badges = badges;
    }

    pub(super) fn show_thumbs(&mut self, lists: Vec<crate::pandora::ThumbList>) {
        self.thumbs = lists
            .into_iter()
            .map(|list| super::ThumbGroup { id: list.id, name: list.name, total: list.total, tracks: None })
            .collect();
        if self.thumbs_cursor >= self.thumbs.len() {
            self.thumbs_cursor = 0;
        }
        if self.view == super::View::Thumbs {
            self.status = if self.thumbs.is_empty() {
                "No thumbs yet".into()
            } else {
                "Thumbs, grouped by station. Enter shows the songs (playing one needs Premium).".into()
            };
        }
    }

    pub(super) fn show_thumb_tracks(&mut self, playlist_id: String, tracks: Vec<crate::pandora::ThumbTrack>) {
        if let Some(group) = self.thumbs.iter_mut().find(|group| group.id == playlist_id) {
            group.tracks = Some(tracks);
        }
    }

    /// Opening a station row loads its tracks once.
    pub(super) fn load_thumb_row(&mut self) {
        let Some(group) = self.thumbs.get(self.thumbs_cursor) else { return };
        if group.tracks.is_some() {
            return;
        }
        let id = group.id.clone();
        self.backend.send(Request::ThumbTracks { playlist_id: id });
    }

    pub(super) fn open_followed_row(&mut self) {
        let Some(show) = self.followed.get(self.followed_cursor).cloned() else { return };
        self.open_podcast(&show.id, &show.name);
    }

    pub(super) fn episode_heartbeat(&mut self) {
        if !self.on_episode || self.audio.is_paused() || !self.audio.is_active() {
            return;
        }
        let due = self
            .last_progress
            .is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(self.progress_every));
        if !due {
            return;
        }
        let Some(source_id) = self.episode_source.clone() else { return };
        self.last_progress = Some(std::time::Instant::now());
        self.backend.send(Request::EpisodeBeat {
            source_id,
            index: self.episode_index,
            elapsed: self.audio.position().as_secs(),
        });
    }

    /// Resume point for this episode, unless Pandora says it is finished.
    pub(super) fn resume_seconds(&self, episode_id: &str) -> u64 {
        self.episode_progress
            .get(episode_id)
            .and_then(|(elapsed, finished)| (!finished).then_some(*elapsed))
            .unwrap_or(0)
    }
}
