//! Settings → Theme Select: the theme picker popup. Two tabs, like the
//! station editor's: Search (first) filters every scheme GitHub lists as you
//! type, fuzzy-ranked like fzf, and Enter downloads, saves and uses one;
//! Installed lists the built-in and saved themes, Enter uses one and `x`
//! deletes a saved one (after asking). The popup stays open until Esc.

use super::*;
use crate::theme_store::{Entry, Msg};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32Str};

/// The picker's tabs (Tab / Shift+Tab switch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeTab {
    Search,
    Installed,
}

/// One row of the Installed tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledTheme {
    /// The config name (`gruvbox`, `omarchy-nord`).
    pub name: String,
    /// Built-in themes come with boxvibe and can't be deleted.
    pub built_in: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThemePicker {
    pub tab: ThemeTab,
    /// What is typed in Search.
    pub query: String,
    /// Every scheme GitHub listed.
    pub catalog: Vec<Entry>,
    /// `catalog` indexes that match `query`, best first.
    pub hits: Vec<usize>,
    pub search_cursor: usize,
    pub installed_cursor: usize,
    /// Waiting for the list from GitHub.
    pub loading: bool,
    /// Why there is no list.
    pub error: Option<String>,
    /// The scheme being downloaded.
    pub downloading: Option<String>,
    /// The saved theme `x` asked to delete.
    pub confirm_delete: Option<String>,
}

impl ThemePicker {
    fn new() -> Self {
        Self {
            tab: ThemeTab::Search,
            query: String::new(),
            catalog: Vec::new(),
            hits: Vec::new(),
            search_cursor: 0,
            installed_cursor: 0,
            loading: true,
            error: None,
            downloading: None,
            confirm_delete: None,
        }
    }

    /// Ranks the catalog against the query (fzf-style fuzzy match on the
    /// name and source); an empty query lists everything by name.
    fn refilter(&mut self) {
        self.hits = rank(&self.catalog, &self.query);
        self.search_cursor = 0;
    }
}

/// `catalog` indexes matching `query`, best match first (ties by name).
pub fn rank(catalog: &[Entry], query: &str) -> Vec<usize> {
    if query.trim().is_empty() {
        let mut all: Vec<usize> = (0..catalog.len()).collect();
        all.sort_by_cached_key(|i| catalog[*i].name.to_lowercase());
        return all;
    }
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, usize)> = catalog
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let hay = format!("{} {}", e.name, e.source.label());
            pattern.score(Utf32Str::new(&hay, &mut buf), &mut matcher).map(|s| (s, i))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| catalog[a.1].name.cmp(&catalog[b.1].name)));
    scored.into_iter().map(|(_, i)| i).collect()
}

impl App {
    /// Settings → Theme Select: opens the picker on Search and asks GitHub
    /// for the list (in the background).
    pub(super) fn open_themes(&mut self) {
        self.theme_job = Some(crate::theme_store::fetch_catalog(self.demo));
        self.theme_picker = Some(ThemePicker::new());
    }

    /// The Installed tab's rows: built-in themes, then the saved ones.
    pub fn installed_themes(&self) -> Vec<InstalledTheme> {
        crate::theme::names()
            .filter(|n| *n != "omarchy")
            .map(|n| InstalledTheme { name: n.into(), built_in: true })
            .chain(self.user_themes.iter().map(|(n, _)| InstalledTheme { name: n.clone(), built_in: false }))
            .collect()
    }

    /// Whether a catalog entry is already saved.
    pub fn is_installed(&self, entry: &Entry) -> bool {
        let slug = entry.slug();
        self.user_themes.iter().any(|(n, _)| *n == slug)
    }

    pub(super) fn theme_picker_key(&mut self, key: KeyEvent) {
        let Some(picker) = self.theme_picker.as_mut() else { return };
        if let Some(name) = picker.confirm_delete.clone() {
            match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    picker.confirm_delete = None;
                    self.delete_theme(&name);
                }
                KeyCode::Char('n') | KeyCode::Esc => picker.confirm_delete = None,
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Esc => {
                self.theme_picker = None;
                return;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                picker.tab = match picker.tab {
                    ThemeTab::Search => ThemeTab::Installed,
                    ThemeTab::Installed => ThemeTab::Search,
                };
                return;
            }
            _ => {}
        }
        match picker.tab {
            ThemeTab::Search => {
                let len = picker.hits.len();
                match key.code {
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        picker.query.clear();
                        picker.refilter();
                    }
                    KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        picker.query.push(c);
                        picker.refilter();
                    }
                    KeyCode::Backspace => {
                        picker.query.pop();
                        picker.refilter();
                    }
                    KeyCode::Down => picker.search_cursor = step(picker.search_cursor, len, 1),
                    KeyCode::Up => picker.search_cursor = step(picker.search_cursor, len, -1),
                    KeyCode::PageDown => picker.search_cursor = (picker.search_cursor + 10).min(len.saturating_sub(1)),
                    KeyCode::PageUp => picker.search_cursor = picker.search_cursor.saturating_sub(10),
                    KeyCode::Enter => {
                        let Some(entry) = picker.hits.get(picker.search_cursor).map(|i| picker.catalog[*i].clone())
                        else {
                            return;
                        };
                        let busy = picker.downloading.is_some();
                        if self.is_installed(&entry) {
                            self.use_theme(&entry.slug());
                        } else if busy {
                            self.status = "Still downloading the last one…".into();
                        } else {
                            self.status = format!("Downloading {}…", entry.name);
                            if let Some(picker) = self.theme_picker.as_mut() {
                                picker.downloading = Some(entry.name.clone());
                            }
                            self.theme_job = Some(crate::theme_store::fetch_theme(entry, self.demo));
                        }
                    }
                    _ => {}
                }
            }
            ThemeTab::Installed => {
                let rows = self.installed_themes();
                let Some(picker) = self.theme_picker.as_mut() else { return };
                let len = rows.len();
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        picker.installed_cursor = step(picker.installed_cursor, len, 1)
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        picker.installed_cursor = step(picker.installed_cursor, len, -1)
                    }
                    KeyCode::Home | KeyCode::Char('g') => picker.installed_cursor = 0,
                    KeyCode::End | KeyCode::Char('G') => picker.installed_cursor = len.saturating_sub(1),
                    KeyCode::Enter => {
                        if let Some(row) = rows.get(picker.installed_cursor) {
                            self.use_theme(&row.name);
                        }
                    }
                    KeyCode::Char('x') | KeyCode::Delete => match rows.get(picker.installed_cursor) {
                        Some(row) if row.built_in => {
                            self.status =
                                format!("{} comes with boxvibe: it can't be deleted", theme::label(&row.name));
                        }
                        Some(row) => picker.confirm_delete = Some(row.name.clone()),
                        None => {}
                    },
                    _ => {}
                }
            }
        }
    }

    /// Deletes a saved theme; one in use falls back to the default.
    fn delete_theme(&mut self, name: &str) {
        if let Some(dir) = &self.themes_dir
            && let Err(e) = crate::theme_store::remove(dir, name)
        {
            self.status = format!("⚠ Couldn't delete {name}: {e}");
            return;
        }
        self.user_themes.retain(|(n, _)| n != name);
        let len = self.installed_themes().len();
        if let Some(picker) = self.theme_picker.as_mut() {
            picker.installed_cursor = picker.installed_cursor.min(len.saturating_sub(1));
        }
        if self.config.ui.theme_select == name {
            self.use_theme(theme::FALLBACK);
            self.status = format!("Deleted {name}; Theme Select is {} again", theme::label(theme::FALLBACK));
        } else {
            self.status = format!("Deleted {name}");
        }
    }

    /// A theme list or download arrived.
    pub(super) fn theme_tick(&mut self) {
        let Some(msg) = self.theme_job.as_ref().and_then(|r| r.try_recv().ok()) else { return };
        self.theme_job = None;
        match msg {
            Msg::Catalog(result) => {
                let Some(picker) = self.theme_picker.as_mut() else { return };
                picker.loading = false;
                match result {
                    Ok(entries) => {
                        picker.catalog = entries;
                        picker.refilter();
                    }
                    Err(e) => picker.error = Some(e),
                }
            }
            Msg::Downloaded(entry, result) => {
                if let Some(picker) = self.theme_picker.as_mut() {
                    picker.downloading = None;
                }
                let text = match result {
                    Ok(text) => text,
                    Err(e) => {
                        self.status = format!("⚠ Couldn't get {}: {e}", entry.name);
                        return;
                    }
                };
                let slug = entry.slug();
                let Some(palette) = crate::theme::parse_any(&text) else {
                    self.status = format!("⚠ {} isn't a colour scheme boxvibe can read", entry.name);
                    return;
                };
                if let Some(dir) = &self.themes_dir
                    && let Err(e) = crate::theme_store::save(dir, &slug, &text)
                {
                    self.status = format!("⚠ Couldn't save {}: {e}", entry.name);
                    return;
                }
                self.user_themes.retain(|(n, _)| *n != slug);
                self.user_themes.push((slug.clone(), palette));
                self.user_themes.sort_by(|a, b| a.0.cmp(&b.0));
                self.use_theme(&slug);
            }
        }
    }
}
