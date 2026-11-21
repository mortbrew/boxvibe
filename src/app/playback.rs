//! Playback: tuning a station, the queue, the song playing, and the audio
//! behind it. Split out of `app.rs`; these are still `App` methods.

use super::*;

impl App {
    /// Invalidate queued / late audio downloads (station change, timeout, …).
    pub(super) fn bump_audio_generation(&mut self) {
        let generation = self.audio_fetch.bump();
        self.backend.set_audio_epoch(generation);
    }

    /// Ask the audio worker for `url`, stamped with the current generation.
    pub(super) fn send_audio(&mut self, track_token: String, url: String) {
        let generation = self.audio_fetch.generation;
        self.backend.send(Request::Audio { track_token, url, generation });
    }

    /// Ask Pandora for the podcast `source_id` (`PE:` episode or `PC:` program) and play it.
    pub(super) fn play_episode(&mut self, source_id: &str, name: &str) {
        if self.episode_wait {
            return;
        }
        self.episode_wait = true;
        self.status = format!("Getting “{name}”…");
        self.backend.send(Request::Episode { source_id: source_id.to_owned(), name: name.to_owned() });
    }

    /// The episode arrived: play it on its own, and do not pull the station queue along.
    pub(super) fn reply_episode(&mut self, played: crate::pandora::EpisodePlayback) {
        self.episode_wait = false;
        self.on_episode = true;
        self.episode_source = Some(played.source_id);
        self.episode_index = played.index;
        if let Some(song) = self.now.take() {
            self.push_history(song);
        }
        self.audio.stop();
        self.ad = AdPhase::Off;
        self.queue.clear();
        self.audio_fetch.cache.clear();
        self.bump_audio_generation();
        self.audio_fetch.buffering = None;
        self.held_dislikes.clear();
        let title = played.song.title.clone();
        let show = played.song.artist.clone();
        self.queue.push_back(played.song);
        self.next_song();
        self.show_now_playing();
        self.status = format!("Playing {title} — {show}");
    }

    pub(super) fn tune(&mut self, index: usize) {
        let Some(station) = self.stations.get(index).cloned() else { return };
        self.on_episode = false;
        self.episode_source = None;
        self.episode_index = 0;
        self.episode_wait = false;
        if let Some(song) = self.now.take() {
            self.push_history(song);
        }
        self.audio.stop();
        self.ad = AdPhase::Off;
        self.queue.clear();
        self.audio_fetch.cache.clear();
        self.bump_audio_generation();
        // Held dislikes are for songs of the station being left.
        self.held_dislikes.clear();
        self.audio_fetch.buffering = None;
        self.next_cursor = 0;
        // The NOW PLAYING entry appears on top: the cursor stays on its entry.
        let first = !self.has_now_playing();
        self.current_station = Some(station.id.clone());
        if first {
            self.refresh_menu_cache();
            self.menu_cursor += 1;
        }
        self.in_flight.begin_playlist();
        self.backend.send(Request::Playlist { station_id: station.id.clone() });
        if self.config.pandora.last_station != station.id {
            self.config.pandora.last_station = station.id.clone();
            self.save(&[("playback", "last_station", station.id.as_str().into())]);
        }
        self.status = format!("Tuning in to {}", station.name);
        // Every way a station starts lands where Shift+P does: same function, no drift.
        self.show_now_playing();
        self.refresh_recently_played();
    }

    pub(super) fn push_history(&mut self, song: Song) {
        self.history.insert(0, song);
        // The selected History row stays on its song as the list grows.
        if self.history_cursor > 0 {
            self.history_cursor = (self.history_cursor + 1).min(HISTORY_LEN - 1);
        }
        self.history.truncate(HISTORY_LEN);
    }

    pub(super) fn advance(&mut self) {
        // Abandoning a download in flight: invalidate its generation so a
        // queued fetch for this song does not block the next one. A normal
        // skip while already playing leaves the prefetch generation alone.
        if self.audio_fetch.buffering.is_some() {
            self.bump_audio_generation();
        }
        if let Some(song) = self.now.take() {
            self.push_history(song);
        }
        self.audio.stop();
        self.audio_fetch.buffering = None;
        self.next_song();
    }

    /// Starts the next song of the queue (or asks for more).
    ///
    /// A song whose audio will not start is skipped here, in this loop, rather
    /// than by calling back into [`App::advance`]: a run of bad songs is then a
    /// loop over the queue instead of a recursion through three functions.
    pub(super) fn next_song(&mut self) {
        // Dislikes held on Up Next rows count now, at their turn: sent, and skipped.
        while let Some(front) = self.queue.front()
            && self.held_dislikes.contains(&front.track_token)
        {
            let Some(song) = self.queue.pop_front() else { break };
            self.held_dislikes.remove(&song.track_token);
            self.backend.send(Request::Rate { track_token: song.track_token.clone(), positive: false });
            self.status = format!("Disliked and skipped: {}", song.title);
            self.audio_fetch.cache.forget(&song.track_token);
            self.push_history(song);
        }
        loop {
            if self.skips_left == 0 {
                // A run of songs that will not decode waits for the next tick.
                self.resume_skips = true;
                return;
            }
            let Some(song) = self.queue.pop_front() else {
                self.request_more();
                self.resume_skips = false;
                return;
            };
            let token = song.track_token.clone();
            let url = song.audio_url.clone();
            self.notify(&song);
            // What Pandora really sent for this song, which is not always what was
            // asked for: a free plan gets 64 kbps HE-AAC whatever `high` says.
            self.stream = Some((song.audio_format, song.bitrate));
            self.now = Some(song);
            let data = if let Some(data) = self.audio_fetch.cache.take(&token) {
                Some(data)
            } else if self.demo || !self.audio.has_device() {
                None
            } else {
                // The download answers later and calls `play_now` itself.
                self.audio_fetch.buffering = Some((token.clone(), Instant::now()));
                if self.audio_fetch.requested.insert(token.clone()) {
                    self.send_audio(token, url);
                }
                self.resume_skips = false;
                break;
            };
            if self.play_now(data) {
                self.resume_skips = false;
                break;
            }
            // The audio would not start: this song is done, so try the next one.
            self.skips_left = self.skips_left.saturating_sub(1);
            if let Some(song) = self.now.take() {
                self.push_history(song);
            }
            self.audio_fetch.buffering = None;
            self.audio.stop();
        }
        if self.queue.len() < 2 {
            self.request_more();
        }
        self.prefetch();
    }

    pub(super) fn request_more(&mut self) {
        if self.on_episode {
            if self.now.is_none() && self.queue.is_empty() {
                self.status = "That episode is finished".into();
            }
            return;
        }
        if let Some(id) = self.current_station.clone()
            && self.in_flight.begin_playlist()
        {
            self.backend.send(Request::Playlist { station_id: id });
        }
    }

    /// Keeps a downloaded song for its turn, dropping the oldest once there are
    /// more than [`AUDIO_CACHE_KEPT`]. Each is a whole MP3, so the map is not
    /// allowed to grow with the session.
    pub(super) fn keep_audio(&mut self, token: String, data: Vec<u8>) {
        self.audio_fetch.cache.keep(token, data);
    }

    pub(super) fn prefetch(&mut self) {
        if self.demo || !self.audio.has_device() {
            return;
        }
        if let Some(next) = self.queue.front()
            && self.audio_fetch.requested.insert(next.track_token.clone())
        {
            self.send_audio(next.track_token.clone(), next.audio_url.clone());
        }
    }

    /// Every frame: a download that never answers is given up on, so the
    /// player cannot sit on "buffering…" forever.
    pub(super) fn buffer_tick(&mut self) {
        let Some((token, since)) = &self.audio_fetch.buffering else { return };
        if since.elapsed() <= BUFFER_TIMEOUT {
            return;
        }
        let token = token.clone();
        crate::log::log!("download: no answer for {token} in {BUFFER_TIMEOUT:?}: skipping");
        self.status = "⚠ The song took too long to download: skipping".into();
        // `advance` bumps the generation while `buffering` is still set.
        self.advance();
    }

    /// Every frame: a sound device that went away is noticed, so the player
    /// does not sit on a frozen progress bar for ever.
    ///
    /// Device loss is a reported device error, or a stall longer than
    /// [`crate::audio`] allows. The device is opened again first (or dropped,
    /// so later songs use the silent clock) and only then is the stuck song skipped.
    pub(super) fn device_tick(&mut self) {
        let why = self.audio.take_device_error();
        let stalled = self.audio.stalled();
        if why.is_none() && !stalled {
            return;
        }
        if let Some(why) = why {
            crate::log::log!("device: {why}");
            self.status = format!("⚠ The sound device stopped ({why})");
        } else {
            crate::log::log!("device: the stream stopped moving: skipping");
            self.status = "⚠ The sound device stopped: skipping".into();
        }
        let playing = self.audio.is_active();
        self.audio.recover();
        if !self.audio.has_device() && !self.status.contains("playing silently") {
            self.status = format!("{} · playing silently", self.status);
        }
        if playing {
            self.advance();
        }
    }

    /// Starts the audio for `now`. `false`: it would not start, so the caller
    /// moves on to the next song.
    pub(super) fn play_now(&mut self, data: Option<Vec<u8>>) -> bool {
        let data = data.filter(|d| !d.is_empty());
        if let Some(song) = &self.now {
            crate::log::log!(
                "play: {} — {}, Pandora says {} s, {} bytes",
                song.title,
                song.artist,
                song.length,
                data.as_ref().map_or(0, Vec::len)
            );
        }
        match self.audio.start(data, self.now.as_ref().map_or(0.0, |s| s.gain), false) {
            Ok(()) => {
                // Pandora counts plays (and skips) from this report, like the website.
                // An episode uses `event/started`, not the station's `trackStarted`.
                if self.on_episode {
                    if let Some(source_id) = self.episode_source.clone() {
                        self.backend.send(Request::EpisodeStarted { source_id, index: self.episode_index });
                    }
                } else if let Some(song) = &self.now {
                    self.backend.send(Request::Started { track_token: song.track_token.clone() });
                }
                true
            }
            Err(e) => {
                crate::log::log!("play: failed: {e}");
                self.status = format!("⚠ {e}: skipping");
                false
            }
        }
    }

    pub(super) fn notify(&self, song: &Song) {
        // Tests make non-demo apps that play songs: without this they would
        // put a real notification on the screen for every one.
        if self.demo || cfg!(test) || !self.config.ui.notifications {
            return;
        }
        let mut command = std::process::Command::new("notify-send");
        command
            .args(["-a", "boxvibe", "-t", "4000", &song.title, &format!("{} — {}", song.artist, song.album)])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let _ = crate::proc::detached(command);
    }

    // --- Up Next and History ---

    pub(super) fn move_song(&mut self, by: i32) {
        let (cursor, len) = match self.pane {
            Pane::UpNext => (&mut self.next_cursor, self.queue.len()),
            Pane::History => (&mut self.history_cursor, self.history.len()),
            Pane::NowPlaying => return,
        };
        if len > 0 {
            *cursor = step(*cursor, len, by);
        }
    }

    /// The selected rows, kept inside their lists as songs come and go.
    pub fn next_index(&self) -> usize {
        self.next_cursor.min(self.queue.len().saturating_sub(1))
    }

    pub fn history_index(&self) -> usize {
        self.history_cursor.min(self.history.len().saturating_sub(1))
    }

    pub(super) fn song_mut(&mut self, token: &str) -> Option<&mut Song> {
        self.now.iter_mut().chain(self.history.iter_mut()).chain(self.queue.iter_mut()).find(|s| s.track_token == token)
    }

    pub(super) fn find_song(&self, token: &str) -> Option<&Song> {
        self.now.iter().chain(self.history.iter()).chain(self.queue.iter()).find(|s| s.track_token == token)
    }

    /// A podcast episode is what is playing (the Now Playing card says "show").
    pub fn playing_episode(&self) -> bool {
        self.on_episode
    }

    pub fn station_name(&self) -> Option<&str> {
        if self.on_episode {
            let show = self.now.as_ref().map(|song| song.artist.as_str()).filter(|name| !name.is_empty());
            if show.is_some() {
                return show;
            }
        }
        let id = self.current_station.as_deref()?;
        self.stations.iter().find(|s| s.id == id).map(|s| s.name.as_str())
    }

    pub(super) fn reply_playlist(&mut self, station_id: String, songs: Vec<Song>) {
        if self.current_station.as_deref() != Some(&station_id) {
            return;
        }
        self.in_flight.end_playlist();
        if songs.is_empty() && self.now.is_none() {
            self.status = "Pandora sent no songs for this station".into();
        }
        self.queue.extend(songs);
        if self.now.is_none() && !self.ad_break() {
            self.advance();
        } else if self.now.is_some() {
            self.prefetch();
        }
    }

    pub(super) fn reply_rated(&mut self, track_token: String, positive: bool, feedback_id: String) {
        if let Some(song) = self.song_mut(&track_token) {
            song.feedback_id = feedback_id;
        }
        let ic = crate::icons::icons(self.config.ui.icons);
        let what = if positive { format!("{} Liked", ic.up) } else { format!("{} Disliked", ic.down) };
        let title = self.find_song(&track_token).map(|s| s.title.clone()).unwrap_or_default();
        self.status = format!("{what}: {title}");
    }

    pub(super) fn reply_thumb_cleared(&mut self, track_token: String) {
        let title = self.song_mut(&track_token).map(|s| {
            s.feedback_id.clear();
            s.title.clone()
        });
        self.status = format!("Thumb taken back: {}", title.unwrap_or_default());
    }

    pub(super) fn reply_took_over(&mut self) {
        self.status = "Pandora was playing on another device with this account: boxvibe took over".into();
    }

    pub(super) fn reply_tired(&mut self, track_token: String) {
        let title = self.find_song(&track_token).map(|s| s.title.clone()).unwrap_or_default();
        let sleep = crate::icons::icons(self.config.ui.icons).sleep;
        self.status = format!("{sleep} Tired of {title}: Pandora won't play it for a month");
    }

    pub(super) fn reply_audio(&mut self, track_token: String, data: Vec<u8>, generation: u64) {
        if generation != self.audio_fetch.generation {
            self.note_late_download(&track_token);
            return;
        }
        let Some(data) = self.ad_audio_arrived(&track_token, data) else { return };
        // Success (or a late drop) must free the token so a later prefetch /
        // cache miss can ask again — failures already remove in AudioFailed.
        self.audio_fetch.requested.remove(&track_token);
        if self.audio_fetch.buffering.as_ref().is_some_and(|(t, _)| *t == track_token) {
            self.audio_fetch.buffering = None;
            if !self.play_now(Some(data)) {
                // The audio would not start: skip on, as `next_song` does.
                self.advance();
            }
        } else if self.queue.iter().any(|s| s.track_token == track_token) {
            self.keep_audio(track_token, data);
        }
    }

    pub(super) fn reply_audio_failed(&mut self, track_token: String, message: String, generation: u64) {
        if generation != self.audio_fetch.generation {
            self.note_late_download(&track_token);
            return;
        }
        if self.ad_audio_failed(&track_token, &message) {
            return;
        }
        self.audio_fetch.requested.remove(&track_token);
        if self.audio_fetch.buffering.as_ref().is_some_and(|(t, _)| *t == track_token) {
            crate::log::log!("download: failed for {track_token}: {message}");
            self.audio_fetch.buffering = None;
            self.status = format!("⚠ {message}: skipping");
            self.advance();
        }
    }

    /// A download that arrived after its generation was abandoned. Quiet when
    /// a newer song is already playing; otherwise say the bytes were dropped.
    fn note_late_download(&mut self, track_token: &str) {
        let moved_on = self.now.as_ref().is_some_and(|song| song.track_token != track_token);
        if moved_on {
            return;
        }
        let title = self
            .find_song(track_token)
            .map(|song| song.title.clone())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| track_token.to_owned());
        self.status = format!("⚠ Dropped a late download for {title}");
    }
}
