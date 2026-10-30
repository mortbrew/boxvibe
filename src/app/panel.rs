//! List popups fed by Pandora: an artist's bio and similar artists (`.`
//! menu), and a station's modes (`m`). Both load in the background, show a
//! line while waiting, and say plainly when Pandora has nothing.

use super::*;
use crate::pandora::{ArtistInfo, StationModes};

/// What a panel is about, and what Enter does there.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelKind {
    /// Enter starts a station from the highlighted similar artist.
    Artist { name: String },
    /// Enter switches the station to the highlighted mode.
    Modes { station: StationRef },
}

/// One row: what it says, a note under it (muted), and the id Enter uses.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelItem {
    pub label: String,
    pub note: String,
    pub id: String,
    /// Can't be picked (a mode for Premium only): shown dimmed.
    pub locked: bool,
    /// The mode that is on.
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    pub kind: PanelKind,
    /// The text above the list: the bio, or "loading…", or why there is nothing.
    pub intro: String,
    pub items: Vec<PanelItem>,
    pub cursor: usize,
    pub loading: bool,
    /// The artist's Pandora page, when known.
    pub link: String,
}

impl Panel {
    pub fn title(&self) -> String {
        match &self.kind {
            PanelKind::Artist { name } => format!("Artist · {name}"),
            PanelKind::Modes { station } => format!("Station Mode · {}", station.name),
        }
    }

    pub fn list_heading(&self) -> &'static str {
        match self.kind {
            PanelKind::Artist { .. } => "Similar Artists",
            PanelKind::Modes { .. } => "Modes",
        }
    }
}

impl App {
    /// `.` → "Similar Artists & Bio": the artist page of the song's artist.
    pub(super) fn open_artist(&mut self, song: &Song) {
        let name = song.artist.trim().to_owned();
        self.panel = Some(Panel {
            kind: PanelKind::Artist { name: name.clone() },
            intro: "Loading the artist page…".into(),
            items: Vec::new(),
            cursor: 0,
            loading: true,
            link: String::new(),
        });
        self.backend.send(Request::Artist { music_id: song.artist_music_id.clone(), name });
    }

    pub(super) fn artist_loaded(&mut self, info: ArtistInfo) {
        let Some(panel) = self.panel.as_mut() else { return };
        let PanelKind::Artist { name } = &panel.kind else { return };
        if !info.name.is_empty() && !info.name.eq_ignore_ascii_case(name) {
            return; // An answer for an artist closed meanwhile.
        }
        panel.loading = false;
        panel.link = info.detail_url.clone();
        panel.intro = if info.bio.is_empty() { "Pandora sent no bio for this artist.".into() } else { info.bio };
        panel.items = info
            .similar
            .into_iter()
            .map(|h| PanelItem { label: h.name, note: String::new(), id: h.id, locked: false, current: false })
            .collect();
        panel.cursor = 0;
    }

    /// `m`: the modes of the station under the cursor (Stations), in the
    /// editor, or else the one playing.
    pub(super) fn open_modes(&mut self, station: Option<StationRef>) {
        let station = station.or_else(|| {
            let id = self.current_station.as_deref()?;
            let s = self.stations.iter().find(|s| s.id == id)?;
            Some(StationRef { id: s.id.clone(), name: s.name.clone(), favorite: false })
        });
        let Some(station) = station else {
            self.status = "m picks a station mode: play or pick a station first".into();
            return;
        };
        if self.stations.iter().any(|s| s.id == station.id && s.is_shuffle) {
            self.status = "Shuffle has no modes".into();
            return;
        }
        self.backend.send(Request::StationModes { station_id: station.id.clone() });
        self.panel = Some(Panel {
            kind: PanelKind::Modes { station },
            intro: "Asking Pandora which modes this station has…".into(),
            items: Vec::new(),
            cursor: 0,
            loading: true,
            link: String::new(),
        });
    }

    pub(super) fn modes_loaded(&mut self, station_id: &str, modes: StationModes, set: bool) {
        let mut said = None;
        if let Some(panel) = self.panel.as_mut()
            && let PanelKind::Modes { station } = &panel.kind
            && station.id == station_id
        {
            panel.loading = false;
            panel.intro = if modes.modes.is_empty() {
                "Pandora offers no modes for this station.".into()
            } else {
                "Enter switches; new songs follow the mode.".into()
            };
            panel.items = modes
                .modes
                .iter()
                .map(|m| PanelItem {
                    label: m.name.clone(),
                    note: if m.premium_only && !m.available {
                        format!("{} (Premium)", m.description)
                    } else {
                        m.description.clone()
                    },
                    id: m.id.to_string(),
                    locked: !m.available,
                    current: modes.current == Some(m.id),
                })
                .collect();
            if let Some(at) = panel.items.iter().position(|i| i.current) {
                panel.cursor = at;
            }
            if set {
                let name = panel.items.iter().find(|i| i.current).map(|i| i.label.clone()).unwrap_or_default();
                said = Some(format!("“{}” is now in {name} mode", station.name));
            }
        }
        if let Some(text) = said {
            self.status = text;
            // Songs already fetched were picked in the old mode: fetch afresh.
            if self.current_station.as_deref() == Some(station_id) {
                for song in std::mem::take(&mut self.queue) {
                    self.audio_cache.remove(&song.track_token);
                    self.requested.remove(&song.track_token);
                }
                self.next_cursor = 0;
                self.request_more();
            }
        }
    }

    /// A failed artist or mode call: said in the panel (it stays open).
    pub(super) fn panel_failed(&mut self, what: &str, message: &str) -> bool {
        let Some(panel) = self.panel.as_mut() else { return false };
        let ours =
            matches!((&panel.kind, what), (PanelKind::Artist { .. }, "artist") | (PanelKind::Modes { .. }, "mode"));
        if !ours {
            return false;
        }
        if panel.loading {
            panel.loading = false;
            panel.intro = match panel.kind {
                PanelKind::Artist { .. } => format!("No artist page from Pandora: {message}"),
                PanelKind::Modes { .. } => format!("No station modes: {message}"),
            };
        } else {
            self.status = format!("⚠ {message}");
        }
        true
    }

    pub(super) fn panel_key(&mut self, key: KeyEvent) {
        let Some(panel) = self.panel.as_mut() else { return };
        let len = panel.items.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.panel = None,
            KeyCode::Char('j') | KeyCode::Down => panel.cursor = step(panel.cursor, len, 1),
            KeyCode::Char('k') | KeyCode::Up => panel.cursor = step(panel.cursor, len, -1),
            KeyCode::Home | KeyCode::Char('g') => panel.cursor = 0,
            KeyCode::End | KeyCode::Char('G') => panel.cursor = len.saturating_sub(1),
            KeyCode::Enter => {
                let Some(item) = panel.items.get(panel.cursor).cloned() else { return };
                match panel.kind.clone() {
                    PanelKind::Artist { .. } => {
                        self.panel = None;
                        self.create_station(&item.id, &item.label);
                    }
                    PanelKind::Modes { station } => {
                        if item.locked {
                            self.status = format!("{} isn't available on this account", item.label);
                        } else if item.current {
                            self.status = format!("Already in {} mode", item.label);
                        } else if let Ok(mode_id) = item.id.parse() {
                            self.status = format!("Switching to {}…", item.label);
                            self.backend.send(Request::SetStationMode { station_id: station.id, mode_id });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
