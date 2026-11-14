//! Station list, favorites, create/search, rename, and delete. Split out of
//! `app.rs`; these are still `App` methods.

use super::*;

impl App {
    /// Shift+X: plays Pandora's Shuffle, which mixes songs from the account's
    /// stations (Pandora makes it if the account has none yet).
    pub(super) fn play_shuffle(&mut self) {
        if let Some(at) = self.stations.iter().position(|s| s.is_shuffle) {
            if self.current_station.as_deref() == Some(self.stations[at].id.as_str()) {
                self.status = "Already playing Shuffle".into();
                return;
            }
            self.stations_cursor = at;
            self.on_add_row = false;
            self.tune(at);
            return;
        }
        self.status = "Asking Pandora for your Shuffle station…".into();
        self.backend.send(Request::Shuffle);
    }

    pub(super) fn shuffle_arrived(&mut self, station: Station) {
        let at = match self.stations.iter().position(|s| s.id == station.id) {
            Some(at) => at,
            None => {
                self.stations.insert(0, station);
                0
            }
        };
        self.stations_cursor = at;
        self.on_add_row = false;
        self.tune(at);
    }

    pub(super) fn station_created(&mut self, station: Station) {
        self.in_flight.end_station();
        let at = match self.stations.iter().position(|s| s.id == station.id) {
            Some(at) => at,
            None => {
                // Newest first, after Shuffle (which stays on top).
                let at = usize::from(self.stations.first().is_some_and(|s| s.is_shuffle));
                self.stations.insert(at, station);
                at
            }
        };
        self.on_add_row = false;
        self.stations_cursor = at;
        self.tune(at);
    }

    pub fn stations_index(&self) -> usize {
        self.stations_cursor.min(self.stations.len().saturating_sub(1))
    }

    pub fn is_favorite(&self, id: &str) -> bool {
        self.config.ui.favorites.iter().any(|f| f.id == id)
    }

    /// `f`: stars or unstars the station under the cursor (the list, a
    /// favorite in the menu) or else the one playing; saved as `[playback] favorites`.
    pub(super) fn toggle_favorite(&mut self) {
        let (id, name) = match (self.focus, self.view, self.menu_selected()) {
            (Focus::Content, View::Stations, _) if self.on_add_row => {
                self.status = "f stars a station: move onto one".into();
                return;
            }
            (Focus::Content, View::Stations, _) => match self.stations.get(self.stations_index()) {
                Some(s) => (s.id.clone(), s.name.clone()),
                None => return,
            },
            (Focus::Menu, _, MenuItem::Favorite(i)) => {
                let f = &self.config.ui.favorites[i];
                (f.id.clone(), f.name.clone())
            }
            _ => match self.current_station.as_ref().and_then(|id| self.stations.iter().find(|s| &s.id == id)) {
                Some(s) => (s.id.clone(), s.name.clone()),
                None => {
                    self.status = "Open Stations to pick one to favorite".into();
                    return;
                }
            },
        };
        let selected = self.menu_selected();
        if let Some(at) = self.config.ui.favorites.iter().position(|f| f.id == id) {
            self.config.ui.favorites.remove(at);
            self.status = format!("Removed {name} from Favorites");
        } else {
            self.config.ui.favorites.push(config::Favorite { id, name: name.clone() });
            self.status = format!("{name} added to Favorites");
        }
        self.refresh_menu_cache();
        // The menu cursor stays on its entry (or the one that took its place).
        if let Some(i) = self.menu_items().iter().position(|m| *m == selected) {
            self.menu_cursor = i;
        }
        self.menu_cursor = self.menu_cursor.min(self.menu_items().len() - 1);
        self.save_favorites();
    }

    pub(super) fn save_favorites(&mut self) {
        let mut list = toml_edit::Array::new();
        for f in &self.config.ui.favorites {
            let mut t = toml_edit::InlineTable::new();
            t.insert("id", f.id.as_str().into());
            t.insert("name", f.name.as_str().into());
            list.push(t);
        }
        self.save(&[("playback", "favorites", toml_edit::Value::Array(list))]);
    }

    /// Pandora removed a station: it goes from the list, Favorites and,
    /// if it was playing, the player.
    pub(super) fn station_removed(&mut self, id: &str) {
        let name = self.stations.iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_default();
        let selected = self.menu_selected();
        self.stations.retain(|s| s.id != id);
        if self.config.ui.favorites.iter().any(|f| f.id == id) {
            self.config.ui.favorites.retain(|f| f.id != id);
            self.save_favorites();
        }
        if self.current_station.as_deref() == Some(id) {
            self.audio.stop();
            self.now = None;
            self.queue.clear();
            self.audio_fetch.cache.clear();
            self.audio_fetch.buffering = None;
            self.bump_audio_generation();
            self.in_flight.end_playlist();
            self.current_station = None;
            if self.view == View::Player {
                self.view = View::Stations;
            }
        }
        self.refresh_menu_cache();
        self.stations_cursor = self.stations_cursor.min(self.stations.len().saturating_sub(1));
        let items = self.menu_items();
        self.menu_cursor = items.iter().position(|m| *m == selected).unwrap_or(self.menu_cursor).min(items.len() - 1);
        self.status = format!("Deleted “{name}” from your Pandora account");
    }

    pub(super) fn reply_stations(&mut self, stations: Vec<Station>) {
        self.stations = stations;
        self.on_add_row = self.stations.is_empty();
        let last = &self.config.pandora.last_station;
        let at = self.stations.iter().position(|s| &s.id == last);
        self.stations_cursor = at.unwrap_or(0);
        // One call per session, after the list: a station entry carries
        // no name, so the row joins names from `stations`.
        self.ask_recently_played();
        // Favorites keep the names Pandora uses now, and one whose
        // station Pandora no longer has goes -- but only on a list we
        // can trust: a logged-in account that really has stations. An
        // empty list is a logged-out or brand-new account, and pruning
        // on it would throw away every star.
        let mut renamed = false;
        let trusted = self.session == Session::LoggedIn && !self.stations.is_empty();
        let before = self.config.ui.favorites.len();
        if trusted {
            self.config.ui.favorites.retain(|f| self.stations.iter().any(|s| s.id == f.id));
        }
        let pruned = before - self.config.ui.favorites.len();
        for f in &mut self.config.ui.favorites {
            if let Some(s) = self.stations.iter().find(|s| s.id == f.id)
                && s.name != f.name
            {
                f.name = s.name.clone();
                renamed = true;
            }
        }
        if renamed || pruned > 0 {
            self.save_favorites();
        }
        if self.stations.is_empty() {
            self.status = "This account has no stations yet: create one on pandora.com".into();
        } else if self.config.ui.autoplay_last_station && self.current_station.is_none() {
            // Shuffle comes first in the list: start on a real station.
            if at.is_none() && self.stations.len() > 1 && self.stations[0].is_shuffle {
                self.stations_cursor = 1;
            }
            self.tune(self.stations_cursor);
        } else if self.current_station.is_none() && self.status.is_empty() {
            // Startup always stays on Now Playing (idle); Stations is opened by hand.
            self.status = "Nothing playing: open My Stations (Shift+S) to pick one".into();
        }
        // Last, so autoplay's "Tuning in to …" does not hide it.
        if pruned > 0 {
            self.status = match pruned {
                1 => "A favorite was removed: that station is gone from Pandora".into(),
                n => format!("{n} favorites were removed: those stations are gone from Pandora"),
            };
        }
    }

    pub(super) fn reply_station_renamed(&mut self, station_id: String, name: String) {
        if let Some(s) = self.stations.iter_mut().find(|s| s.id == station_id) {
            s.name = name.clone();
        }
        let mut starred = false;
        for f in self.config.ui.favorites.iter_mut().filter(|f| f.id == station_id) {
            f.name = name.clone();
            starred = true;
        }
        if starred {
            self.save_favorites();
        }
        self.status = format!("Station renamed to “{name}” on your Pandora account");
    }

    pub(super) fn reply_station_details(&mut self, station_id: String, details: StationDetails) {
        if let Some(d) = self.details_mut().filter(|d| d.station_id == station_id) {
            d.from_pandora = Some(Ok(details));
        }
    }

    pub(super) fn reply_search_results(&mut self, query: String, hits: Vec<SearchHit>) {
        if self.editor_search_results(&query, &hits) {
            return;
        }
        if let Some(search) = self.add_station.as_mut().filter(|s| s.asked.as_deref() == Some(query.as_str())) {
            search.waiting = false;
            search.error = None;
            // Typing on while it searched keeps the keys in the text.
            let same = search.text.text().trim() == query;
            search.mode = if hits.is_empty() || !same { SearchMode::Typing } else { SearchMode::Results };
            search.cursor = search.prefer.and_then(|k| hits.iter().position(|h| h.kind == k)).unwrap_or(0);
            search.hits = hits;
        }
    }
}
