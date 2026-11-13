//! Playback: tuning a station, the queue, the song playing, and the audio
//! behind it. Split out of `app.rs`; these are still `App` methods.

use super::*;

impl App {
    pub(super) fn tune(&mut self, index: usize) {
        let Some(station) = self.stations.get(index).cloned() else { return };
        if let Some(song) = self.now.take() {
            self.push_history(song);
        }
        self.audio.stop();
        self.ad = AdPhase::Off;
        self.queue.clear();
        self.audio_cache.clear();
        self.requested.clear();
        // Held dislikes are for songs of the station being left.
        self.held_dislikes.clear();
        self.buffering = None;
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
        if let Some(song) = self.now.take() {
            self.push_history(song);
        }
        self.audio.stop();
        self.buffering = None;
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
            self.audio_cache.forget(&song.track_token);
            self.push_history(song);
        }
        loop {
            let Some(song) = self.queue.pop_front() else {
                self.request_more();
                return;
            };
            let token = song.track_token.clone();
            let url = song.audio_url.clone();
            self.notify(&song);
            // What Pandora really sent for this song, which is not always what was
            // asked for: a free plan gets 64 kbps HE-AAC whatever `high` says.
            self.stream = Some((song.audio_format, song.bitrate));
            self.now = Some(song);
            let data = if let Some(data) = self.audio_cache.take(&token) {
                Some(data)
            } else if self.demo || !self.audio.has_device() {
                None
            } else {
                // The download answers later and calls `play_now` itself.
                self.buffering = Some((token.clone(), Instant::now()));
                if self.requested.insert(token.clone()) {
                    self.backend.send(Request::Audio { track_token: token, url });
                }
                break;
            };
            if self.play_now(data) {
                break;
            }
            // The audio would not start: this song is done, so try the next one.
            if let Some(song) = self.now.take() {
                self.push_history(song);
            }
            self.buffering = None;
            self.audio.stop();
        }
        if self.queue.len() < 2 {
            self.request_more();
        }
        self.prefetch();
    }

    pub(super) fn request_more(&mut self) {
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
        self.audio_cache.keep(token, data);
    }

    pub(super) fn prefetch(&mut self) {
        if self.demo || !self.audio.has_device() {
            return;
        }
        if let Some(next) = self.queue.front()
            && self.requested.insert(next.track_token.clone())
        {
            self.backend.send(Request::Audio { track_token: next.track_token.clone(), url: next.audio_url.clone() });
        }
    }

    /// Every frame: a download that never answers is given up on, so the
    /// player cannot sit on "buffering…" forever.
    pub(super) fn buffer_tick(&mut self) {
        let Some((token, since)) = &self.buffering else { return };
        if since.elapsed() <= BUFFER_TIMEOUT {
            return;
        }
        let token = token.clone();
        crate::log::log!("download: no answer for {token} in {BUFFER_TIMEOUT:?}: skipping");
        self.buffering = None;
        self.requested.remove(&token);
        self.status = "⚠ The song took too long to download: skipping".into();
        self.advance();
    }

    /// Every frame: a sound device that went away is noticed, so the player
    /// does not sit on a frozen progress bar for ever.
    ///
    /// A dead stream reports nothing through [`Audio::finished`]: its queue
    /// still holds the song, so it never looks empty, and its position stops
    /// advancing because nothing is pulling samples. Both are checked here.
    pub(super) fn device_tick(&mut self) {
        if let Some(why) = self.audio.take_device_error() {
            crate::log::log!("device: {why}");
            self.status = format!("⚠ The sound device stopped ({why})");
        }
        if !self.audio.stalled() {
            return;
        }
        crate::log::log!("device: the stream stopped moving: skipping");
        self.status = "⚠ The sound device stopped: skipping".into();
        self.advance();
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
                if let Some(song) = &self.now {
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
}
