//! Recently Played: the local list of what was played, its menu entry, and
//! playing a row back. Split out of `app.rs`; these are still `App` methods.

use super::*;

impl App {
    /// How many rows the menu's recently played entry takes: one, or none
    /// while the list is empty (FAVORITES hides the same way).
    pub fn recent_row(&self) -> bool {
        !self.recent.is_empty()
    }

    /// Asks Pandora for the recently played list, once after login.
    pub(super) fn ask_recently_played(&mut self) {
        if self.recent_state.needs_first_ask() {
            self.refresh_recently_played();
        }
    }

    /// One fetch for a login, a station start, or opening the page. A second
    /// event while that fetch is in flight waits for it, then sends once more.
    /// The list itself still comes only from Pandora's reply.
    pub(super) fn refresh_recently_played(&mut self) {
        if self.session != Session::LoggedIn {
            return;
        }
        let Some(generation) = self.recent_state.begin() else { return };
        self.backend.send(Request::RecentlyPlayed { generation });
    }

    pub(super) fn finish_recent_request(&mut self) {
        if self.recent_state.finish() {
            self.refresh_recently_played();
        }
    }

    /// The reply belongs to the login that asked for it.
    pub(super) fn accept_recent(&self, generation: u64) -> bool {
        self.recent_state.accepts(generation) && self.session == Session::LoggedIn
    }

    /// Drops the list and invalidates an answer still in flight.
    pub(super) fn forget_recent(&mut self) {
        let had = self.recent_row();
        self.recent_state.reset();
        self.recent.clear();
        self.recent_cursor = 0;
        self.recent_hidden.clear();
        if had {
            self.refresh_menu_cache();
        }
    }

    pub(super) fn filter_recent(&self, sources: Vec<RecentSource>) -> Vec<RecentSource> {
        sources.into_iter().filter(|s| !self.recent_hidden.contains(&s.id)).collect()
    }

    pub(crate) fn forget_recent_row(&mut self, at: usize) {
        let Some(source) = self.recent.get(at).cloned() else { return };
        self.recent_hidden.insert(source.id.clone());
        self.recent.retain(|s| s.id != source.id);
        self.recent_cursor = self.recent_cursor.min(self.recent.len().saturating_sub(1));
        if self.recent.is_empty() {
            self.refresh_menu_cache();
        }
        self.status = format!(
            "Removed “{}” from Recently Played here (this device only; Pandora's history is unchanged)",
            self.recent_title(&source)
        );
    }

    /// The menu's RECENTLY PLAYED entry: the list of recent sources.
    pub(super) fn show_recently_played(&mut self) {
        self.view = View::Recent;
        self.focus = Focus::Content;
        self.point_menu_at_view();
        self.recent_cursor = self.recent_cursor.min(self.recent.len().saturating_sub(1));
        self.refresh_recently_played();
    }

    /// A recent entry's title. A station's own fragment selects no name, so
    /// the station list gives it one; anything else uses `name`, then the
    /// artist, then the id rather than a blank row.
    pub fn recent_title(&self, source: &RecentSource) -> String {
        if let Some(station) = self.stations.iter().find(|s| station_ids_match(&s.id, &source.id)) {
            return station.name.clone();
        }
        if !source.name.is_empty() {
            return source.name.clone();
        }
        if !source.by.is_empty() {
            return source.by.clone();
        }
        source.id.clone()
    }

    /// Enter on a recent entry: a station plays. Anything else (a track, an
    /// album, a playlist) needs a station to play from, which Pandora's answer
    /// does not name, so say so instead of guessing one.
    pub(super) fn play_recent(&mut self) {
        let Some(source) = self.recent.get(self.recent_cursor.min(self.recent.len().saturating_sub(1))).cloned() else {
            return;
        };
        if source.kind != "ST" {
            self.status = format!("“{}” is not a station: play it from Pandora", self.recent_title(&source));
            return;
        }
        match self.stations.iter().position(|s| station_ids_match(&s.id, &source.id)) {
            Some(at) => self.tune(at),
            None if self.stations.is_empty() => self.status = "The stations are still loading".into(),
            None => self.status = format!("“{}” isn't one of your stations any more", self.recent_title(&source)),
        }
    }

    pub(super) fn reply_recent(&mut self, generation: u64, sources: Vec<RecentSource>) {
        if !self.accept_recent(generation) {
            return;
        }
        // A new row is inserted above Pinned. Stay on the menu entry
        // the cursor was already on, and on the same recent source.
        let had = self.recent_row();
        let selected = self.menu_selected();
        let keep = self.recent.get(self.recent_cursor).map(|s| s.id.clone());
        self.recent = self.filter_recent(sources);
        if let Some(id) = keep
            && let Some(at) = self.recent.iter().position(|s| s.id == id)
        {
            self.recent_cursor = at;
        } else {
            self.recent_cursor = self.recent_cursor.min(self.recent.len().saturating_sub(1));
        }
        if !had && self.recent_row() {
            self.refresh_menu_cache();
            let items = self.menu_items();
            if let Some(i) = items.iter().position(|m| *m == selected) {
                self.menu_cursor = i;
            }
        }
        self.finish_recent_request();
    }

    pub(super) fn reply_recent_failed(&mut self, generation: u64, message: String) {
        if !self.accept_recent(generation) {
            return;
        }
        self.status = format!("⚠ Recently Played: {message}");
        self.finish_recent_request();
    }
}
