//! The Browse pages: Pandora's directory of genres and curated lists, the
//! pinned sections on the main menu, and the View All pages opened from a row.
//! Split out of `app.rs`; these are still `App` methods.

use super::*;

impl App {
    /// Browse sections pinned on the main menu: `(key, title)`, known first then extras.
    pub fn browse_menu_pins(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for &(key, title) in config::BROWSE_SECTIONS {
            if self.config.browse.pinned_key(key) {
                out.push((key.to_owned(), title.to_owned()));
                seen.insert(key);
            }
        }
        for (key, on) in &self.config.browse.sections {
            if *on && !seen.contains(key.as_str()) {
                let title = key
                    .split('_')
                    .filter(|p| !p.is_empty())
                    .map(|p| {
                        let mut c = p.chars();
                        match c.next() {
                            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                            None => String::new(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                out.push((key.clone(), title));
            }
        }
        out
    }

    /// Full teaser (every section). No menu entry; tests use it to check pins do not hide rows.
    #[cfg(test)]
    pub(crate) fn show_browse(&mut self) {
        self.browse_filter = None;
        self.open_browse_view();
    }

    /// Main-menu pin (or `G` for Browse Genres): one section's list.
    pub(super) fn show_browse_pin(&mut self, at: usize) {
        let Some((_, title)) = self.browse_menu_pins().get(at).cloned() else {
            return;
        };
        self.show_browse_section(&title);
    }

    /// Open one section (`G` → Browse Genres). A `view_all_path` loads that full page.
    pub(crate) fn show_browse_section(&mut self, title: &str) {
        self.browse_filter = Some(title.to_owned());
        self.open_browse_view();
    }

    fn open_browse_view(&mut self) {
        self.browse_parent = None;
        self.view = View::Browse;
        self.focus = Focus::Content;
        self.point_menu_at_view();
        self.sync_browse_page();
    }

    /// Show the teaser, or fetch the section's View-all page once the teaser is known.
    fn sync_browse_page(&mut self) {
        match &self.browse_directory {
            BrowseState::Idle => self.start_directory_fetch(),
            BrowseState::Loading => {
                self.browse_path = None;
                self.browse = BrowseState::Loading;
            }
            BrowseState::Failed(why) => {
                self.browse_path = None;
                self.browse = BrowseState::Failed(why.clone());
            }
            BrowseState::Loaded(_) => {
                if let Some(path) = self.wanted_view_all_path() {
                    self.fetch_view_all(path);
                } else {
                    self.show_directory_rows();
                }
            }
        }
    }

    fn start_directory_fetch(&mut self) {
        if self.session != Session::LoggedIn {
            self.status = "Log in first (Settings, Shift+O) to browse".into();
            self.browse = BrowseState::Idle;
            return;
        }
        self.browse_directory = BrowseState::Loading;
        self.browse_path = None;
        self.browse = BrowseState::Loading;
        self.backend.send(Request::Browse { path: None });
    }

    /// The open pin's `viewAll.webUrl`, once the main teaser has been parsed.
    fn wanted_view_all_path(&self) -> Option<String> {
        let want = self.browse_filter.as_ref()?;
        let BrowseState::Loaded(sections) = &self.browse_directory else { return None };
        sections.iter().find(|s| same_browse_section(s, want)).and_then(|s| s.view_all_path.clone())
    }

    fn show_directory_rows(&mut self) {
        self.browse_path = None;
        if let BrowseState::Loaded(sections) = &self.browse_directory {
            self.browse = BrowseState::Loaded(sections.clone());
        }
        self.place_browse_cursor();
    }

    fn fetch_view_all(&mut self, path: String) {
        if let Some(sections) = self.view_all.get(&path) {
            self.browse_path = Some(path);
            self.browse = BrowseState::Loaded(sections.clone());
            self.place_browse_cursor();
            return;
        }
        if self.browse_path.as_deref() == Some(path.as_str()) && matches!(self.browse, BrowseState::Loading) {
            return;
        }
        self.browse_path = Some(path.clone());
        self.browse = BrowseState::Loading;
        self.backend.send(Request::Browse { path: Some(path) });
    }

    fn place_browse_cursor(&mut self) {
        self.browse_cursor = self.browse_rows().iter().position(|r| matches!(r, BrowseRow::Item(_))).unwrap_or(0);
    }

    pub(super) fn browse_loaded(&mut self, path: Option<String>, sections: Vec<BrowseSection>) {
        match path {
            None => {
                self.browse_directory = BrowseState::Loaded(sections);
                if self.view == View::Browse {
                    self.sync_browse_page();
                }
            }
            Some(path) => {
                self.keep_view_all(path.clone(), sections.clone());
                if self.browse_path.as_deref() == Some(path.as_str()) {
                    self.browse = BrowseState::Loaded(sections);
                    self.place_browse_cursor();
                }
            }
        }
    }

    /// Remembers a View-all page, dropping the oldest once there are more than
    /// [`VIEW_ALL_KEPT`]. A page on screen is never the one dropped.
    fn keep_view_all(&mut self, path: String, sections: Vec<BrowseSection>) {
        if self.view_all.insert(path.clone(), sections).is_none() {
            self.view_all_order.push_back(path);
        }
        while self.view_all_order.len() > VIEW_ALL_KEPT {
            // The oldest page that is not the one on screen.
            let Some(at) = self.view_all_order.iter().position(|p| self.browse_path.as_deref() != Some(p.as_str()))
            else {
                break;
            };
            let gone = self.view_all_order.remove(at).expect("just found");
            self.view_all.remove(&gone);
        }
    }

    pub(super) fn note_browse_failure(&mut self, path: Option<String>, message: String) {
        match path {
            None => {
                self.browse_directory = BrowseState::Failed(message.clone());
                if self.view == View::Browse && self.browse_path.is_none() {
                    self.browse = BrowseState::Failed(message);
                }
            }
            Some(path) => {
                if self.browse_path.as_deref() != Some(path.as_str()) {
                    return;
                }
                if let Some(parent) = self.browse_parent.take() {
                    self.browse = parent.state;
                    self.browse_path = parent.path;
                    self.browse_cursor = parent.cursor;
                    self.status = format!("That page didn't load ({message})");
                    return;
                }
                if let BrowseState::Loaded(sections) = &self.browse_directory {
                    self.browse_path = None;
                    self.browse = BrowseState::Loaded(sections.clone());
                    self.place_browse_cursor();
                    self.status = format!("View-all didn't load ({message}): showing the short list");
                } else {
                    self.browse = BrowseState::Failed(message);
                }
            }
        }
    }

    fn retry_browse(&mut self) {
        if let Some(path) = self.browse_path.clone() {
            self.view_all.remove(&path);
            self.view_all_order.retain(|p| *p != path);
            self.browse = BrowseState::Idle;
            self.fetch_view_all(path);
            return;
        }
        self.browse_directory = BrowseState::Idle;
        self.browse = BrowseState::Idle;
        self.open_browse_view();
    }

    /// Rows for the Browse page.
    ///
    /// A View-all page lists every section it returned (New Music is several).
    /// The teaser lists every section, or one pin's rows when a pin has no View-all.
    pub fn browse_rows(&self) -> Vec<BrowseRow<'_>> {
        let BrowseState::Loaded(sections) = &self.browse else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for section in sections {
            if self.browse_path.is_none()
                && let Some(want) = &self.browse_filter
                && !same_browse_section(section, want)
            {
                continue;
            }
            rows.push(BrowseRow::Header(&section.title));
            for item in &section.items {
                rows.push(BrowseRow::Item(item));
            }
        }
        rows
    }

    pub fn browse_index(&self) -> usize {
        self.browse_cursor.min(self.browse_rows().len().saturating_sub(1))
    }

    /// Enter on a Browse item: start a station, or open a category page the row names.
    pub(super) fn activate_browse_row(&mut self) {
        let retry = matches!(self.browse, BrowseState::Idle | BrowseState::Failed(_));
        let loading = matches!(self.browse, BrowseState::Loading);
        let picked = self.browse_rows().get(self.browse_index()).map(|row| match row {
            BrowseRow::Header(_) => BrowsePick::Header,
            BrowseRow::Item(item) => {
                if let Some(id) = item.create_id() {
                    BrowsePick::Station(id.to_owned(), item.name.clone())
                } else if let Some(path) = item.link.clone() {
                    BrowsePick::Page(path)
                } else {
                    BrowsePick::Unsupported(item.name.clone())
                }
            }
        });
        match (loading, retry, picked) {
            (true, _, _) => {}
            (_, true, _) => self.retry_browse(),
            (_, _, Some(BrowsePick::Station(id, name))) => self.create_station(&id, &name),
            (_, _, Some(BrowsePick::Page(path))) => self.open_browse_link(path),
            (_, _, Some(BrowsePick::Unsupported(name))) => {
                let pandora_id = self.browse_rows().get(self.browse_index()).and_then(|r| match r {
                    BrowseRow::Item(item) => Some(item.pandora_id.clone()),
                    _ => None,
                });
                let hint = pandora_id.as_deref().and_then(|id| {
                    if id.starts_with("PL:") {
                        Some("playlists are browse-only in boxvibe right now")
                    } else if id.starts_with("PC:") || id.starts_with("PE:") {
                        Some("podcasts are browse-only in boxvibe right now")
                    } else {
                        None
                    }
                });
                self.status = if let Some(why) = hint {
                    format!("“{name}” can't play here ({why})")
                } else {
                    format!("“{name}” can't start a station here (playlist, podcast, album, mood or category)")
                };
            }
            (_, _, Some(BrowsePick::Header)) => self.status = "That's a heading. Pick a row under it.".into(),
            _ => {}
        }
    }

    /// A category shelf's site path. The page underneath is kept for Esc.
    fn open_browse_link(&mut self, path: String) {
        if self.browse_path.as_deref() == Some(path.as_str()) {
            return;
        }
        if matches!(self.browse, BrowseState::Loaded(_)) {
            self.browse_parent = Some(BrowsePlace {
                state: self.browse.clone(),
                path: self.browse_path.clone(),
                cursor: self.browse_cursor,
            });
        }
        self.fetch_view_all(path);
    }

    /// Esc on a category page returns to the page that opened it.
    pub(super) fn browse_back(&mut self) -> bool {
        if self.view != View::Browse || self.focus != Focus::Content {
            return false;
        }
        let Some(parent) = self.browse_parent.take() else { return false };
        self.browse = parent.state;
        self.browse_path = parent.path;
        self.browse_cursor = parent.cursor;
        true
    }

    pub fn browse_can_go_back(&self) -> bool {
        self.view == View::Browse && self.focus == Focus::Content && self.browse_parent.is_some()
    }
}
