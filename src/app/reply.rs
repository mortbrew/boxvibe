//! Backend replies: the thin switchboard that routes each `Reply` to its
//! domain. Split out of `app.rs`; these are still `App` methods.

use super::*;

impl App {
    pub(crate) fn on_reply(&mut self, reply: Reply) {
        self.mark_draw();
        if self.editor_reply(&reply) {
            return;
        }
        match reply {
            Reply::LoggedIn { hifi } => self.reply_logged_in(hifi),
            Reply::LoginFailed { message, wrong_password } => self.reply_login_failed(message, wrong_password),
            Reply::Keychain(reply) => self.on_keychain(reply),
            Reply::Stations(stations) => self.reply_stations(stations),
            Reply::StationRenamed { station_id, name } => self.reply_station_renamed(station_id, name),
            Reply::StationRemoved { station_id } => self.station_removed(&station_id),
            Reply::StationDetails { station_id, details } => self.reply_station_details(station_id, details),
            Reply::SearchResults { query, hits } => self.reply_search_results(query, hits),
            Reply::StationCreated(station) => self.station_created(station),
            Reply::Artist(info) => self.artist_loaded(info),
            Reply::Shuffle(station) => self.shuffle_arrived(station),
            Reply::StationModes { station_id, modes, set } => self.modes_loaded(&station_id, modes, set),
            Reply::Browse { path, sections } => self.browse_loaded(path, sections),
            Reply::BrowseFailed { path, message } => {
                let index = path.is_none();
                self.note_browse_failure(path, message.clone());
                if index {
                    self.status = format!("⚠ browse: {message}");
                }
            }
            Reply::Playlist { station_id, songs } => self.reply_playlist(station_id, songs),
            Reply::Rated { track_token, positive, feedback_id } => self.reply_rated(track_token, positive, feedback_id),
            Reply::ThumbCleared { track_token } => self.reply_thumb_cleared(track_token),
            Reply::TookOver => self.reply_took_over(),
            Reply::Tired { track_token } => self.reply_tired(track_token),
            Reply::AdList { station_id, ads } => self.ad_list_arrived(&station_id, ads),
            Reply::Audio { track_token, data, generation } => self.reply_audio(track_token, data, generation),
            Reply::Art { url, image } => self.art.arrived(url, image),
            Reply::AudioFailed { track_token, message, generation } => {
                self.reply_audio_failed(track_token, message, generation)
            }
            // The editor took these (or it was closed meanwhile).
            Reply::Seeds { .. }
            | Reply::SeedSuggestions { .. }
            | Reply::SeedAdded { .. }
            | Reply::SeedRemoved { .. }
            | Reply::Feedback { .. }
            | Reply::FeedbackDeleted { .. } => {}
            Reply::Recent { generation, sources } => self.reply_recent(generation, sources),
            Reply::RecentFailed { generation, message } => self.reply_recent_failed(generation, message),
            Reply::Failed { what, message } => self.reply_failed(what, message),
            Reply::UpdateFound(update) => self.reply_update_found(update),
            Reply::UpToDate => self.up_to_date = self.update.is_none(),
            Reply::UpdateProgress(percent) => self.reply_update_progress(percent),
            Reply::SelfUpdated(Ok(version)) => {
                self.status = format!("✔ boxvibe {version} installed · Restart Now in About opens it");
                self.self_update = Progress::Done(version);
            }
            Reply::SelfUpdated(Err(error)) => {
                self.status = format!("⚠ Could not update: {error}");
                self.self_update = Progress::Failed(error);
            }
        }
    }

    fn reply_failed(&mut self, what: Failed, message: String) {
        if self.editor_failed(what, &message) || self.panel_failed(what, &message) {
            return;
        }
        match what {
            Failed::RecentlyPlayed => {
                if self.recent_state.loading() {
                    self.status = format!("⚠ Recently Played: {message}");
                    self.finish_recent_request();
                }
                return;
            }
            Failed::Shuffle => {
                self.status = format!("⚠ No Shuffle: {message}");
                return;
            }
            Failed::Ads => {
                self.ad_ask_failed(&message);
                return;
            }
            Failed::Playlist => self.in_flight.end_playlist(),
            Failed::Search => {
                if let Some(search) = self.add_station.as_mut() {
                    search.waiting = false;
                    search.error = Some(message);
                    return;
                }
            }
            Failed::CreateStation => self.in_flight.end_station(),
            Failed::Details => {
                if let Some(d) = self.details_mut().filter(|d| d.from_pandora.is_none()) {
                    d.from_pandora = Some(Err(message.clone()));
                    return;
                }
            }
            _ => {}
        }
        self.status = format!("⚠ {}: {message}", what.what());
    }

    fn reply_update_found(&mut self, update: Update) {
        self.status = format!(
            "{} boxvibe {} available · see About",
            crate::icons::icons(self.config.ui.icons).update,
            update.version
        );
        self.update = Some(update);
    }

    fn reply_update_progress(&mut self, percent: u8) {
        if let (Progress::Downloading(_), Some(update)) = (&self.self_update, &self.update) {
            self.self_update = Progress::Downloading(percent);
            self.status = format!("⬇ Updating boxvibe to {}… {percent}%", update.version);
        }
    }
}
