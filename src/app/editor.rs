//! The station editor (`e`): the seeds a station is built from (add with
//! `a`, remove with `x`) and the thumbs you gave on it (take one back with
//! `x`). It is a popup with three tabs: Seeds, Thumbs Up and Thumbs Down. Everything it shows
//! comes from Pandora; every change asks first and refreshes the list from
//! Pandora when it is done.

use super::*;
use crate::pandora::{Feedback, Seed};

/// The editor's tabs (Tab / Shift+Tab or → / ← switch, in this order, round).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab {
    Seeds,
    ThumbsUp,
    ThumbsDown,
}

impl EditorTab {
    pub fn next(self) -> Self {
        match self {
            Self::Seeds => Self::ThumbsUp,
            Self::ThumbsUp => Self::ThumbsDown,
            Self::ThumbsDown => Self::Seeds,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Seeds => Self::ThumbsDown,
            Self::ThumbsUp => Self::Seeds,
            Self::ThumbsDown => Self::ThumbsUp,
        }
    }

    /// The thumbs list this tab shows: up (`true`) or down; none for Seeds.
    pub fn thumbs(self) -> Option<bool> {
        match self {
            Self::Seeds => None,
            Self::ThumbsUp => Some(true),
            Self::ThumbsDown => Some(false),
        }
    }
}

/// Something Pandora is giving us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loaded<T> {
    Loading,
    Ready(T),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedState {
    /// Not asked for yet (the tab was never opened).
    Idle,
    Loading,
    Ready,
    Failed(String),
}

/// The thumbs up (or down) of the station, a page at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbList {
    pub items: Vec<Feedback>,
    /// How many Pandora has in all.
    pub total: usize,
    pub state: FeedState,
    pub cursor: usize,
}

impl ThumbList {
    fn new() -> Self {
        Self { items: Vec::new(), total: 0, state: FeedState::Idle, cursor: 0 }
    }

    /// There are more pages to fetch.
    pub fn has_more(&self) -> bool {
        self.items.len() < self.total
    }
}

/// What is waiting for a `y`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorConfirm {
    RemoveSeed(Seed),
    DeleteThumb(Feedback),
}

/// The add-a-seed box inside the editor: the search box, plus the
/// suggestions shown while it is empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedAdd {
    pub search: StationSearch,
    /// What Pandora suggested (shown while the text is empty).
    pub suggestions: Vec<SearchHit>,
    /// The suggestions are what the list shows now.
    pub suggested: bool,
    /// The suggestions are on their way.
    pub suggesting: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    pub station: StationRef,
    pub tab: EditorTab,
    pub seeds: Loaded<Vec<Seed>>,
    pub seed_cursor: usize,
    /// The counts Pandora gives for the station (the list totals say it too once loaded).
    pub thumbs_up: Option<u32>,
    pub thumbs_down: Option<u32>,
    pub up: ThumbList,
    pub down: ThumbList,
    pub add: Option<SeedAdd>,
    pub confirm: Option<EditorConfirm>,
}

impl Editor {
    pub fn list(&self, positive: bool) -> &ThumbList {
        if positive { &self.up } else { &self.down }
    }

    fn list_mut(&mut self, positive: bool) -> &mut ThumbList {
        if positive { &mut self.up } else { &mut self.down }
    }

    /// The count shown for thumbs up / down: the list's total once it is
    /// loaded, else what the station details said.
    pub fn count(&self, positive: bool) -> Option<usize> {
        let list = self.list(positive);
        if list.state == FeedState::Ready {
            Some(list.total)
        } else if positive {
            self.thumbs_up.map(|n| n as usize)
        } else {
            self.thumbs_down.map(|n| n as usize)
        }
    }

    pub fn seed_list(&self) -> &[Seed] {
        match &self.seeds {
            Loaded::Ready(seeds) => seeds,
            _ => &[],
        }
    }
}

impl EditorConfirm {
    /// What the box says will happen.
    pub fn question(&self, station: &str) -> String {
        match self {
            Self::RemoveSeed(seed) => format!(
                "Remove the {} seed “{}” from the station “{station}”? Pandora stops picking songs from it.",
                seed.kind.to_lowercase(),
                seed.name
            ),
            Self::DeleteThumb(f) => {
                let song = if f.artist.is_empty() { f.title.clone() } else { format!("{} by {}", f.title, f.artist) };
                if f.positive {
                    format!("Take back your thumbs up on “{song}” (station “{station}”)? Pandora forgets you liked it.")
                } else {
                    format!(
                        "Take back your thumbs down on “{song}” (station “{station}”)? Pandora forgets you disliked it, so it can play again."
                    )
                }
            }
        }
    }
}

impl App {
    /// `e`: the editor for the selected station (Stations, a favorite, or the playing station on Now Playing).
    pub(super) fn open_editor(&mut self) {
        let Some((station, favorite)) = self.station_to_change() else {
            return self.no_station_here("e edits");
        };
        if !station.allow_edit_seeds {
            let message = format!("Pandora doesn't let you edit the seeds of “{}”", station.name);
            self.blocked(&station, "edited", message);
            return;
        }
        if self.session != Session::LoggedIn {
            self.status = "Log in first (Settings, Shift+O) to edit a station".into();
            return;
        }
        self.backend.send(Request::Seeds { station_id: station.id.clone() });
        let current = self.current_station.as_deref() == Some(station.id.as_str());
        self.backend.send(Request::StationDetails { station_id: station.id.clone(), current });
        self.editor = Some(Editor {
            station: StationRef { id: station.id, name: station.name, favorite },
            tab: EditorTab::Seeds,
            seeds: Loaded::Loading,
            seed_cursor: 0,
            thumbs_up: None,
            thumbs_down: None,
            up: ThumbList::new(),
            down: ThumbList::new(),
            add: None,
            confirm: None,
        });
    }

    fn ask_seeds(&mut self) {
        if let Some(ed) = self.editor.as_mut() {
            ed.seeds = Loaded::Loading;
            self.backend.send(Request::Seeds { station_id: ed.station.id.clone() });
        }
    }

    fn ask_counts(&mut self) {
        if let Some(ed) = &self.editor {
            let current = self.current_station.as_deref() == Some(ed.station.id.as_str());
            self.backend.send(Request::StationDetails { station_id: ed.station.id.clone(), current });
        }
    }

    /// Asks for a page of the thumbs up (or down), from `start`.
    fn ask_thumbs(&mut self, positive: bool, start: usize) {
        let Some(ed) = self.editor.as_mut() else { return };
        ed.list_mut(positive).state = FeedState::Loading;
        self.backend.send(Request::Feedback { station_id: ed.station.id.clone(), positive, start });
    }

    /// A thumbs tab was opened: the first page of its list, once.
    fn ensure_thumbs(&mut self) {
        let Some(ed) = &self.editor else { return };
        if let Some(positive) = ed.tab.thumbs()
            && ed.list(positive).state == FeedState::Idle
        {
            self.ask_thumbs(positive, 0);
        }
    }

    pub(super) fn editor_key(&mut self, key: KeyEvent) {
        let Some(ed) = self.editor.as_mut() else { return };
        if let Some(confirm) = ed.confirm.take() {
            // y does it; any other key is no, like the other confirm boxes.
            if matches!(key.code, KeyCode::Char('y' | 'Y')) {
                self.editor_confirmed(confirm);
            } else {
                self.status = "Kept it".into();
            }
            return;
        }
        if ed.add.is_some() {
            return self.seed_add_key(key);
        }
        match key.code {
            KeyCode::Esc => {
                self.status = format!("Closed the editor for “{}”", ed.station.name);
                self.editor = None;
            }
            KeyCode::Tab | KeyCode::Right => {
                ed.tab = ed.tab.next();
                self.ensure_thumbs();
            }
            KeyCode::BackTab | KeyCode::Left => {
                ed.tab = ed.tab.previous();
                self.ensure_thumbs();
            }
            KeyCode::Char('j') | KeyCode::Down => self.editor_move(1),
            KeyCode::Char('k') | KeyCode::Up => self.editor_move(-1),
            KeyCode::Char('a') if ed.tab == EditorTab::Seeds => self.open_seed_add(),
            KeyCode::Char('x') if ed.tab == EditorTab::Seeds => self.ask_remove_seed(),
            KeyCode::Char('x') => self.ask_delete_thumb(),
            _ => {}
        }
    }

    fn editor_move(&mut self, by: i32) {
        let Some(ed) = self.editor.as_mut() else { return };
        match ed.tab {
            EditorTab::Seeds => {
                let len = ed.seed_list().len();
                ed.seed_cursor = step(ed.seed_cursor, len, by);
            }
            EditorTab::ThumbsUp | EditorTab::ThumbsDown => {
                let positive = ed.tab == EditorTab::ThumbsUp;
                let list = ed.list_mut(positive);
                let len = list.items.len();
                let last = len.saturating_sub(1);
                let loading = list.state == FeedState::Loading;
                if by > 0 && list.cursor >= last && list.has_more() {
                    // The end of what is loaded: the next page, and stay here.
                    if !loading {
                        let start = len;
                        self.ask_thumbs(positive, start);
                    }
                    return;
                }
                list.cursor = step(list.cursor, len, by);
                // Close to the end: fetch ahead so the list doesn't stop.
                if list.has_more() && list.cursor + 3 >= len && !loading {
                    let start = len;
                    self.ask_thumbs(positive, start);
                }
            }
        }
    }

    // --- Seeds ---

    fn ask_remove_seed(&mut self) {
        let Some(ed) = self.editor.as_mut() else { return };
        let seeds = ed.seed_list();
        let Some(seed) = seeds.get(ed.seed_cursor.min(seeds.len().saturating_sub(1))).cloned() else {
            self.status = "No seed to remove".into();
            return;
        };
        if seeds.len() <= 1 {
            self.status = "A station needs at least one seed: add another one first, then remove this one".into();
        } else if seed.id.is_empty() {
            self.status = "Pandora gave this seed no id, so it can't be removed here".into();
        } else {
            ed.confirm = Some(EditorConfirm::RemoveSeed(seed));
        }
    }

    fn open_seed_add(&mut self) {
        let Some(ed) = self.editor.as_mut() else { return };
        if !matches!(ed.seeds, Loaded::Ready(_)) {
            self.status = "The seeds are still loading".into();
            return;
        }
        // Suggestions come from a seed Pandora knows by its music id.
        let from = ed.seed_list().iter().find(|s| !s.music_id.is_empty()).map(|s| s.music_id.clone());
        let suggesting = from.is_some();
        if let Some(seed_music_id) = from {
            self.backend.send(Request::SeedSuggestions { station_id: ed.station.id.clone(), seed_music_id });
        }
        ed.add = Some(SeedAdd { search: StationSearch::new(), suggestions: Vec::new(), suggested: false, suggesting });
    }

    fn seed_add_key(&mut self, key: KeyEvent) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(add) = ed.add.as_mut() else { return };
        let search = &mut add.search;
        let len = search.hits.len();
        match (search.mode, key.code) {
            (_, KeyCode::Esc) => {
                ed.add = None;
                self.status = "No seed added".into();
            }
            (SearchMode::Typing, KeyCode::Enter) => {
                let text = search.text.text().trim().to_owned();
                if search.waiting {
                    return;
                }
                if text.is_empty() {
                    // Nothing typed: Enter takes the suggestion the cursor is on.
                    if add.suggested && search.selected().is_some() {
                        self.add_selected_seed();
                    }
                    return;
                }
                if search.asked.as_deref() == Some(text.as_str()) && search.selected().is_some() {
                    return self.add_selected_seed();
                }
                search.asked = Some(text.clone());
                search.waiting = true;
                search.hits.clear();
                add.suggested = false;
                search.cursor = 0;
                search.error = None;
                self.backend.send(Request::Search { query: text });
            }
            (SearchMode::Typing, KeyCode::Down | KeyCode::Up | KeyCode::Tab) if len > 0 => {
                search.mode = SearchMode::Results;
            }
            (SearchMode::Typing, _) => {
                let before = search.text.text().to_owned();
                if search.text.key(key, MAX_SEARCH) && search.text.text() != before {
                    if search.text.text().trim().is_empty() {
                        // Emptied again: back to the suggestions.
                        search.hits = add.suggestions.clone();
                        add.suggested = !add.suggestions.is_empty();
                        search.asked = None;
                        search.waiting = false;
                        search.error = None;
                        search.cursor = 0;
                    } else if add.suggested {
                        // Typing replaces the suggestions.
                        search.hits.clear();
                        add.suggested = false;
                    }
                }
            }
            (SearchMode::Results, KeyCode::Enter) => self.add_selected_seed(),
            (SearchMode::Results, KeyCode::Char('j') | KeyCode::Down) => search.cursor = step(search.cursor, len, 1),
            (SearchMode::Results, KeyCode::Char('k') | KeyCode::Up) => search.cursor = step(search.cursor, len, -1),
            (SearchMode::Results, KeyCode::Char('/') | KeyCode::Tab | KeyCode::BackTab) => {
                search.mode = SearchMode::Typing;
            }
            (SearchMode::Results, _) => {}
        }
    }

    fn add_selected_seed(&mut self) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(hit) = ed.add.as_ref().and_then(|a| a.search.selected().cloned()) else { return };
        if ed.seed_list().iter().any(|s| s.id == hit.id) {
            self.status = format!("“{}” is already a seed of this station", hit.name);
            return;
        }
        ed.add = None;
        self.status = format!("Adding “{}” as a seed…", hit.name);
        self.backend.send(Request::AddSeed { station_id: ed.station.id.clone(), seed_id: hit.id, name: hit.name });
    }

    // --- Thumbs ---

    fn ask_delete_thumb(&mut self) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(positive) = ed.tab.thumbs() else { return };
        let list = ed.list(positive);
        match list.items.get(list.cursor.min(list.items.len().saturating_sub(1))).cloned() {
            Some(f) => ed.confirm = Some(EditorConfirm::DeleteThumb(f)),
            None => self.status = "No thumb to take back".into(),
        }
    }

    fn editor_confirmed(&mut self, confirm: EditorConfirm) {
        let Some(ed) = &self.editor else { return };
        let station_id = ed.station.id.clone();
        match confirm {
            EditorConfirm::RemoveSeed(seed) => {
                self.status = format!("Removing “{}”…", seed.name);
                self.backend.send(Request::RemoveSeed { station_id, seed_id: seed.id, name: seed.name });
            }
            EditorConfirm::DeleteThumb(f) => {
                self.status = format!("Taking back the thumb on “{}”…", f.title);
                self.backend.send(Request::DeleteFeedback { station_id, feedback_id: f.id, positive: f.positive });
            }
        }
    }

    // --- Pandora's answers ---

    /// Answers meant for the editor. `true`: it took the reply.
    pub(super) fn editor_reply(&mut self, reply: &Reply) -> bool {
        let Some(ed) = self.editor.as_mut() else { return false };
        match reply {
            Reply::Seeds { station_id, seeds } if *station_id == ed.station.id => {
                ed.seed_cursor = ed.seed_cursor.min(seeds.len().saturating_sub(1));
                ed.seeds = Loaded::Ready(seeds.clone());
            }
            Reply::StationDetails { station_id, details } if *station_id == ed.station.id => {
                ed.thumbs_up = details.thumbs_up;
                ed.thumbs_down = details.thumbs_down;
                // The details popup (v) is a different window: let it see this too.
                return false;
            }
            Reply::SeedSuggestions { station_id, hits } if *station_id == ed.station.id => {
                if let Some(add) = ed.add.as_mut() {
                    add.suggesting = false;
                    add.suggestions = hits.clone();
                    if add.search.text.text().trim().is_empty() && add.search.asked.is_none() {
                        add.search.hits = hits.clone();
                        add.suggested = !hits.is_empty();
                        add.search.cursor = 0;
                    }
                }
            }
            Reply::SeedAdded { station_id, name } if *station_id == ed.station.id => {
                ed.seed_cursor = 0;
                self.status = format!("Added “{name}” as a seed of “{}”", ed.station.name);
                self.ask_seeds();
                self.ask_counts();
            }
            Reply::SeedRemoved { station_id, name } if *station_id == ed.station.id => {
                self.status = format!("Removed “{name}” from “{}”", ed.station.name);
                self.ask_seeds();
                self.ask_counts();
            }
            Reply::Feedback { station_id, positive, start, page } if *station_id == ed.station.id => {
                let list = ed.list_mut(*positive);
                if *start == 0 {
                    list.items = page.items.clone();
                } else if *start == list.items.len() {
                    list.items.extend(page.items.iter().cloned());
                } else {
                    // An answer to an older question.
                    return true;
                }
                list.total = page.total.max(list.items.len());
                // A short page with more promised would ask again forever.
                if page.items.is_empty() {
                    list.total = list.items.len();
                }
                list.state = FeedState::Ready;
                list.cursor = list.cursor.min(list.items.len().saturating_sub(1));
            }
            Reply::FeedbackDeleted { station_id, positive } if *station_id == ed.station.id => {
                let positive = *positive;
                self.status = "Took the thumb back".into();
                self.ask_thumbs(positive, 0);
                self.ask_counts();
            }
            _ => return false,
        }
        true
    }

    /// A failed call the editor was waiting for. `true`: the message needs
    /// no status line (the editor shows it).
    pub(super) fn editor_failed(&mut self, what: &str, message: &str) -> bool {
        let Some(ed) = self.editor.as_mut() else { return false };
        match what {
            "seeds" => ed.seeds = Loaded::Failed(message.to_owned()),
            "thumbs history" => {
                for list in [&mut ed.up, &mut ed.down] {
                    if list.state == FeedState::Loading {
                        list.state = FeedState::Failed(message.to_owned());
                    }
                }
            }
            "search" => {
                let Some(add) = ed.add.as_mut() else { return false };
                add.search.waiting = false;
                add.search.error = Some(message.to_owned());
                return true;
            }
            // No suggestions is not worth a word: the box just stays a search box.
            "suggestions" => {
                if let Some(add) = ed.add.as_mut() {
                    add.suggesting = false;
                }
                return true;
            }
            _ => {}
        }
        false
    }

    /// The add box's search answer. `true`: it was for the editor.
    pub(super) fn editor_search_results(&mut self, query: &str, hits: &[SearchHit]) -> bool {
        let Some(add) = self.editor.as_mut().and_then(|e| e.add.as_mut()) else { return false };
        let search = &mut add.search;
        if search.asked.as_deref() != Some(query) {
            return true;
        }
        search.waiting = false;
        search.error = None;
        let same = search.text.text().trim() == query;
        search.mode = if hits.is_empty() || !same { SearchMode::Typing } else { SearchMode::Results };
        search.cursor = 0;
        search.hits = hits.to_vec();
        add.suggested = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::pandora::FEEDBACK_PAGE;
    use ratatui::crossterm::event::KeyModifiers;
    use std::time::Duration;

    const WAIT: Duration = Duration::from_millis(250);

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        app.settle(WAIT);
    }

    fn chr(app: &mut App, c: char) {
        press(app, KeyCode::Char(c));
    }

    /// The demo, on the Stations page, with the editor open on the first
    /// station that has one (Thumbprint Radio: 2 seeds, 37 thumbs up).
    fn editing() -> App {
        let mut app = App::new(Config::default(), None, &[], true);
        app.demo_warm_up(2, Duration::ZERO);
        app.settle(Duration::from_millis(300));
        app.on_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT));
        for _ in 0..8 {
            chr(&mut app, 'e');
            if app.editor.is_some() {
                break;
            }
            chr(&mut app, 'j');
        }
        assert!(app.editor.is_some(), "e opened the editor on a station that allows it");
        app
    }

    fn seed_names(app: &App) -> Vec<String> {
        app.editor.as_ref().unwrap().seed_list().iter().map(|s| s.name.clone()).collect()
    }

    #[test]
    fn e_opens_the_editor_only_on_stations_that_allow_it() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.demo_warm_up(2, Duration::ZERO);
        app.settle(Duration::from_millis(300));
        app.on_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT));
        // Shuffle and the genre stations: Pandora does not let their seeds be edited.
        for locked in [true, false] {
            for _ in 0..12 {
                app.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
            }
            let wanted =
                |app: &App| app.station_in_reach().is_some_and(|(s, _)| s.is_shuffle == locked && !s.allow_edit_seeds);
            for _ in 0..12 {
                if wanted(&app) {
                    break;
                }
                app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
            }
            assert!(wanted(&app), "a station without seeds to edit exists");
            let name = app.station_in_reach().unwrap().0.name;
            chr(&mut app, 'e');
            assert!(app.editor.is_none(), "{name}");
            assert!(app.status.contains("doesn't let you edit") && app.status.contains(&name), "{}", app.status);
            if !locked {
                break;
            }
        }
        let app = editing();
        let ed = app.editor.as_ref().unwrap();
        assert_eq!(ed.tab, EditorTab::Seeds);
        assert_eq!(seed_names(&app), ["M83", "Midnight City by M83"]);
        assert_eq!(ed.thumbs_up, Some(37), "the counts come from the station details");
    }

    #[test]
    fn esc_closes_and_tab_and_arrows_switch_the_three_tabs_round() {
        let mut app = editing();
        let tab = |app: &App| app.editor.as_ref().unwrap().tab;
        // Tab: Seeds → Thumbs Up → Thumbs Down → Seeds.
        press(&mut app, KeyCode::Tab);
        assert_eq!(tab(&app), EditorTab::ThumbsUp);
        press(&mut app, KeyCode::Tab);
        assert_eq!(tab(&app), EditorTab::ThumbsDown);
        press(&mut app, KeyCode::Tab);
        assert_eq!(tab(&app), EditorTab::Seeds);
        // Shift+Tab goes the other way round.
        press(&mut app, KeyCode::BackTab);
        assert_eq!(tab(&app), EditorTab::ThumbsDown);
        press(&mut app, KeyCode::BackTab);
        assert_eq!(tab(&app), EditorTab::ThumbsUp);
        press(&mut app, KeyCode::BackTab);
        assert_eq!(tab(&app), EditorTab::Seeds);
        // → is Tab and ← is Shift+Tab.
        press(&mut app, KeyCode::Right);
        assert_eq!(tab(&app), EditorTab::ThumbsUp);
        press(&mut app, KeyCode::Right);
        assert_eq!(tab(&app), EditorTab::ThumbsDown);
        press(&mut app, KeyCode::Left);
        assert_eq!(tab(&app), EditorTab::ThumbsUp);
        press(&mut app, KeyCode::Left);
        assert_eq!(tab(&app), EditorTab::Seeds);
        // The keys of the page behind it do nothing while it is open.
        chr(&mut app, 'N');
        assert!(app.editor.is_some());
        press(&mut app, KeyCode::Esc);
        assert!(app.editor.is_none());
    }

    #[test]
    fn a_adds_a_suggestion_with_enter_and_the_list_refreshes() {
        let mut app = editing();
        chr(&mut app, 'a');
        let add = app.editor.as_ref().unwrap().add.as_ref().expect("the add box");
        assert!(add.suggested && !add.search.hits.is_empty(), "suggestions show while the box is empty");
        let pick = add.search.hits[0].clone();
        press(&mut app, KeyCode::Enter);
        app.settle(WAIT);
        assert!(app.editor.as_ref().unwrap().add.is_none());
        assert_eq!(seed_names(&app)[0], pick.name, "the new seed is listed from Pandora's answer");
        assert_eq!(seed_names(&app).len(), 3);
        assert!(app.status.contains("Added"), "{}", app.status);
    }

    #[test]
    fn typing_replaces_the_suggestions_with_a_search_and_emptying_brings_them_back() {
        let mut app = editing();
        chr(&mut app, 'a');
        for c in "tame".chars() {
            chr(&mut app, c);
        }
        assert!(!app.editor.as_ref().unwrap().add.as_ref().unwrap().suggested);
        assert!(app.editor.as_ref().unwrap().add.as_ref().unwrap().search.hits.is_empty());
        press(&mut app, KeyCode::Enter);
        let search = &app.editor.as_ref().unwrap().add.as_ref().unwrap().search;
        assert_eq!(search.mode, SearchMode::Results);
        assert_eq!(search.hits[0].name, "Tame Impala");
        press(&mut app, KeyCode::Enter);
        assert_eq!(seed_names(&app)[0], "Tame Impala");
        // Back in a new box, typing and then deleting it all shows them again.
        chr(&mut app, 'a');
        chr(&mut app, 'x');
        assert!(!app.editor.as_ref().unwrap().add.as_ref().unwrap().suggested);
        press(&mut app, KeyCode::Backspace);
        assert!(app.editor.as_ref().unwrap().add.as_ref().unwrap().suggested);
        // Esc closes just the box; the editor stays.
        press(&mut app, KeyCode::Esc);
        assert!(app.editor.as_ref().is_some_and(|e| e.add.is_none()));
    }

    #[test]
    fn a_seed_that_is_already_there_is_not_added_twice() {
        let mut app = editing();
        chr(&mut app, 'a');
        for c in "m83".chars() {
            chr(&mut app, c);
        }
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        assert!(app.status.contains("already a seed"), "{}", app.status);
        assert_eq!(seed_names(&app).len(), 2);
    }

    #[test]
    fn x_asks_what_will_happen_then_removes_and_the_last_seed_stays() {
        let mut app = editing();
        chr(&mut app, 'x');
        let ed = app.editor.as_ref().unwrap();
        let question = ed.confirm.as_ref().expect("asks first").question(&ed.station.name);
        assert!(question.contains("Remove the artist seed “M83”"), "{question}");
        assert!(question.contains("stops picking songs"), "{question}");
        // Any other key says no.
        chr(&mut app, 'n');
        assert!(app.editor.as_ref().unwrap().confirm.is_none());
        assert_eq!(seed_names(&app).len(), 2);
        chr(&mut app, 'x');
        chr(&mut app, 'y');
        app.settle(WAIT);
        assert_eq!(seed_names(&app), ["Midnight City by M83"]);
        assert!(app.status.contains("Removed"), "{}", app.status);
        // The only seed left cannot go: no question, a message.
        chr(&mut app, 'x');
        assert!(app.editor.as_ref().unwrap().confirm.is_none());
        assert!(app.status.contains("at least one seed"), "{}", app.status);
        assert_eq!(seed_names(&app).len(), 1);
    }

    #[test]
    fn each_thumbs_tab_loads_its_own_list_when_first_opened_and_pages_through_it() {
        let mut app = editing();
        press(&mut app, KeyCode::Tab);
        let ed = app.editor.as_ref().unwrap();
        assert_eq!((ed.up.items.len(), ed.up.total), (FEEDBACK_PAGE, 37));
        assert!(
            ed.tab == EditorTab::ThumbsUp && ed.down.state == FeedState::Idle,
            "the other list waits until asked for"
        );
        // Walking down past the end of the loaded page fetches the next one.
        for _ in 0..FEEDBACK_PAGE + 2 {
            chr(&mut app, 'j');
        }
        let ed = app.editor.as_ref().unwrap();
        assert_eq!(ed.up.items.len(), 37, "the rest was appended");
        assert_eq!(ed.up.cursor, FEEDBACK_PAGE + 2);
        // At the very end it stops (no wrap-around while more could come, and nothing to load).
        for _ in 0..10 {
            chr(&mut app, 'j');
        }
        assert_eq!(app.editor.as_ref().unwrap().up.items.len(), 37);
        // The next tab is the thumbs down list, loaded now, not before.
        press(&mut app, KeyCode::Tab);
        app.settle(WAIT);
        let ed = app.editor.as_ref().unwrap();
        assert_eq!(ed.tab, EditorTab::ThumbsDown);
        assert_eq!(ed.down.total, ed.down.items.len());
        assert!(!ed.down.items.is_empty() && ed.down.items.iter().all(|f| !f.positive));
        // Back on the thumbs up tab, the list is kept (not asked for again).
        press(&mut app, KeyCode::BackTab);
        let ed = app.editor.as_ref().unwrap();
        assert_eq!((ed.tab, ed.up.items.len()), (EditorTab::ThumbsUp, 37));
    }

    #[test]
    fn t_no_longer_flips_anything() {
        let mut app = editing();
        chr(&mut app, 't');
        assert_eq!(app.editor.as_ref().unwrap().tab, EditorTab::Seeds);
        press(&mut app, KeyCode::Tab);
        chr(&mut app, 't');
        let ed = app.editor.as_ref().unwrap();
        assert_eq!(ed.tab, EditorTab::ThumbsUp);
        assert_eq!(ed.down.state, FeedState::Idle, "t did not open the thumbs down list");
        press(&mut app, KeyCode::Tab);
        chr(&mut app, 't');
        assert_eq!(app.editor.as_ref().unwrap().tab, EditorTab::ThumbsDown);
    }

    #[test]
    fn x_on_a_thumb_asks_then_deletes_and_refreshes_the_list_and_the_counts() {
        let mut app = editing();
        press(&mut app, KeyCode::Tab);
        chr(&mut app, 'j');
        let gone = app.editor.as_ref().unwrap().up.items[1].clone();
        chr(&mut app, 'x');
        let ed = app.editor.as_ref().unwrap();
        let question = ed.confirm.as_ref().expect("asks first").question(&ed.station.name);
        assert!(question.contains("Take back your thumbs up") && question.contains(&gone.title), "{question}");
        chr(&mut app, 'n');
        assert_eq!(app.editor.as_ref().unwrap().up.total, 37, "no: nothing changed");
        chr(&mut app, 'x');
        chr(&mut app, 'y');
        app.settle(WAIT);
        let ed = app.editor.as_ref().unwrap();
        assert_eq!(ed.up.total, 36);
        assert!(!ed.up.items.iter().any(|f| f.id == gone.id));
        assert_eq!(ed.thumbs_up, Some(36), "the count in the tab comes from the refreshed details");
        // On the thumbs down tab the question says the song may play again; y takes it back.
        press(&mut app, KeyCode::Tab);
        app.settle(WAIT);
        let before = app.editor.as_ref().unwrap().down.total;
        assert!(before > 0);
        let gone = app.editor.as_ref().unwrap().down.items[0].clone();
        chr(&mut app, 'x');
        let ed = app.editor.as_ref().unwrap();
        let question = ed.confirm.as_ref().unwrap().question(&ed.station.name);
        assert!(question.contains("thumbs down") && question.contains("can play again"), "{question}");
        chr(&mut app, 'y');
        app.settle(WAIT);
        let ed = app.editor.as_ref().unwrap();
        assert_eq!(ed.down.total, before - 1);
        assert!(!ed.down.items.iter().any(|f| f.id == gone.id));
        assert_eq!(ed.up.total, 36, "the other list is untouched");
    }

    #[test]
    fn failures_go_to_the_status_line_and_the_list_says_so() {
        let mut app = editing();
        app.on_reply(Reply::Failed { what: "thumbs history", message: "HTTP 500".into() });
        assert!(app.status.contains("thumbs history") && app.status.contains("HTTP 500"), "{}", app.status);
        app.on_reply(Reply::Failed { what: "seeds", message: "offline".into() });
        assert!(matches!(app.editor.as_ref().unwrap().seeds, Loaded::Failed(_)));
        assert!(app.status.contains("seeds"), "{}", app.status);
        // A failed thumbs page marks only the list that was loading.
        press(&mut app, KeyCode::Tab);
        app.editor.as_mut().unwrap().up.state = FeedState::Loading;
        app.on_reply(Reply::Failed { what: "thumbs history", message: "HTTP 500".into() });
        assert!(matches!(app.editor.as_ref().unwrap().up.state, FeedState::Failed(_)));
        // No suggestions is not an error worth the status line.
        chr(&mut app, 'a');
        press(&mut app, KeyCode::BackTab);
        chr(&mut app, 'a');
        app.status.clear();
        app.on_reply(Reply::Failed { what: "suggestions", message: "Service method not found (HTTP 404)".into() });
        assert!(app.status.is_empty(), "{}", app.status);
    }

    #[test]
    fn failed_suggestions_leave_the_add_box_quiet_and_a_plain_search_box() {
        let mut app = editing();
        chr(&mut app, 'a');
        app.editor.as_mut().unwrap().add.as_mut().unwrap().suggesting = true;
        app.status.clear();
        app.on_reply(Reply::Failed { what: "suggestions", message: "Service method not found (HTTP 404)".into() });
        assert!(app.status.is_empty(), "{}", app.status);
        let add = app.editor.as_ref().unwrap().add.as_ref().unwrap();
        assert!(add.search.error.is_none() && !add.suggesting);
        // Typing still searches afterwards.
        chr(&mut app, 'm');
        assert!(app.editor.as_ref().unwrap().add.is_some());
    }

    #[test]
    fn answers_for_another_station_or_a_closed_editor_are_ignored() {
        let mut app = editing();
        let before = seed_names(&app);
        app.on_reply(Reply::Seeds { station_id: "someone-else".into(), seeds: Vec::new() });
        assert_eq!(seed_names(&app), before);
        press(&mut app, KeyCode::Esc);
        app.on_reply(Reply::Seeds { station_id: "1".into(), seeds: Vec::new() });
        app.on_reply(Reply::SeedAdded { station_id: "1".into(), name: "x".into() });
        assert!(app.editor.is_none());
    }
}
