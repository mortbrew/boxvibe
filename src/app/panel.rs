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
    /// Settings → More themes: Enter downloads, saves and uses the scheme.
    /// `/` types a filter over `all`; `items` is what matches.
    Themes { all: Vec<PanelItem>, filter: String, typing: bool },
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
            PanelKind::Themes { filter, typing, all } => match (filter.is_empty(), typing) {
                (true, false) => format!("More Themes · {}", all.len()),
                _ => format!("More Themes · /{filter}{}", if *typing { "▏" } else { "" }),
            },
        }
    }

    pub fn list_heading(&self) -> &'static str {
        match self.kind {
            PanelKind::Artist { .. } => "Similar Artists",
            PanelKind::Modes { .. } => "Modes",
            PanelKind::Themes { .. } => "Schemes (● saved)",
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
        // Themes never fail through the backend: see `theme_tick`.
        if !ours {
            return false;
        }
        if panel.loading {
            panel.loading = false;
            panel.intro = match panel.kind {
                PanelKind::Artist { .. } => format!("No artist page from Pandora: {message}"),
                PanelKind::Modes { .. } => format!("No station modes: {message}"),
                PanelKind::Themes { .. } => format!("No themes: {message}"),
            };
        } else {
            self.status = format!("⚠ {message}");
        }
        true
    }

    pub(super) fn panel_key(&mut self, key: KeyEvent) {
        let Some(panel) = self.panel.as_mut() else { return };
        if let PanelKind::Themes { all, filter, typing: typing @ true } = &mut panel.kind {
            match key.code {
                KeyCode::Char(c) => filter.push(c),
                KeyCode::Backspace => {
                    filter.pop();
                }
                KeyCode::Esc => {
                    filter.clear();
                    *typing = false;
                }
                KeyCode::Enter => *typing = false,
                KeyCode::Down => panel.cursor = step(panel.cursor, panel.items.len(), 1),
                KeyCode::Up => panel.cursor = step(panel.cursor, panel.items.len(), -1),
                _ => {}
            }
            if matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Esc) {
                panel.items = matching(all, filter);
                panel.cursor = 0;
            }
            return;
        }
        let len = panel.items.len();
        match key.code {
            KeyCode::Char('/') if matches!(panel.kind, PanelKind::Themes { .. }) => {
                if let PanelKind::Themes { typing, .. } = &mut panel.kind {
                    *typing = true;
                }
            }
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
                    PanelKind::Themes { .. } => self.pick_theme(item),
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

    /// Settings → More themes: lists both sources (in the background).
    pub(super) fn open_themes(&mut self) {
        self.theme_job = Some(crate::theme_store::fetch_catalog(self.demo));
        self.panel = Some(Panel {
            kind: PanelKind::Themes { all: Vec::new(), filter: String::new(), typing: false },
            intro: "Asking GitHub for iTerm2-Color-Schemes and Omarchy's themes…".into(),
            items: Vec::new(),
            cursor: 0,
            loading: true,
            link: String::new(),
        });
    }

    fn pick_theme(&mut self, item: PanelItem) {
        if item.current {
            self.use_theme(&item.id);
            return;
        }
        let Some(entry) = self.theme_entries.iter().find(|e| e.slug() == item.id).cloned() else { return };
        self.status = format!("Downloading {}…", entry.name);
        self.theme_job = Some(crate::theme_store::fetch_theme(entry, self.demo));
    }

    /// A theme list or download arrived.
    pub(super) fn theme_tick(&mut self) {
        use crate::theme_store::Msg;
        let Some(msg) = self.theme_job.as_ref().and_then(|r| r.try_recv().ok()) else { return };
        self.theme_job = None;
        match msg {
            Msg::Catalog(Ok(entries)) => {
                let saved: Vec<String> = self.user_themes.iter().map(|(n, _)| n.clone()).collect();
                let all: Vec<PanelItem> = entries
                    .iter()
                    .map(|e| {
                        let id = e.slug();
                        PanelItem {
                            label: e.name.clone(),
                            note: e.source.label().into(),
                            current: saved.contains(&id),
                            id,
                            locked: false,
                        }
                    })
                    .collect();
                self.theme_entries = entries;
                if let Some(panel) = self.panel.as_mut()
                    && let PanelKind::Themes { all: list, filter, .. } = &mut panel.kind
                {
                    panel.loading = false;
                    panel.intro = if all.is_empty() {
                        "GitHub listed no themes.".into()
                    } else {
                        "Enter downloads, saves and uses one · / filters".into()
                    };
                    panel.items = matching(&all, filter);
                    *list = all;
                    panel.cursor = 0;
                }
            }
            Msg::Catalog(Err(e)) => {
                if let Some(panel) = self.panel.as_mut().filter(|p| matches!(p.kind, PanelKind::Themes { .. })) {
                    panel.loading = false;
                    panel.intro = format!("No theme list: {e}");
                }
            }
            Msg::Downloaded(entry, Ok(text)) => {
                let slug = entry.slug();
                let Some(palette) = crate::theme::parse_any(&text) else { return };
                if let Some(dir) = &self.themes_dir
                    && let Err(e) = crate::theme_store::save(dir, &slug, &text)
                {
                    self.status = format!("⚠ Couldn't save {}: {e}", entry.name);
                    return;
                }
                self.user_themes.retain(|(n, _)| *n != slug);
                self.user_themes.push((slug.clone(), palette));
                self.user_themes.sort_by(|a, b| a.0.cmp(&b.0));
                if let Some(panel) = self.panel.as_mut()
                    && let PanelKind::Themes { all, .. } = &mut panel.kind
                {
                    for item in all.iter_mut().chain(panel.items.iter_mut()).filter(|i| i.id == slug) {
                        item.current = true;
                    }
                }
                self.use_theme(&slug);
            }
            Msg::Downloaded(entry, Err(e)) => self.status = format!("⚠ Couldn't get {}: {e}", entry.name),
        }
    }
}

/// The items whose name or source has `filter` in it (any case).
fn matching(all: &[PanelItem], filter: &str) -> Vec<PanelItem> {
    let f = filter.to_lowercase();
    all.iter()
        .filter(|i| f.is_empty() || i.label.to_lowercase().contains(&f) || i.note.to_lowercase().contains(&f))
        .cloned()
        .collect()
}
