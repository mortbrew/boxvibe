//! Settings → Theme Select: the theme picker popup. Two tabs, like the
//! station editor's: Search (first) filters the built-in themes and every
//! scheme GitHub lists as you type, fuzzy-ranked like fzf, hiding the ones
//! already saved; Enter uses a built-in one at once or downloads, saves and
//! uses one. Installed lists only the saved themes: Enter uses one and `x`
//! deletes one (after asking). The popup stays open until Esc.

use super::*;
use crate::theme_store::{Msg, Source};

use vibekit::picker::Action;
pub use vibekit::picker::ThemePicker;

impl App {
    /// Settings → Theme Select: opens the picker on Search and asks GitHub
    /// for the list (in the background).
    pub(super) fn open_themes(&mut self) {
        self.theme_job = Some(crate::theme_store::fetch_catalog(self.demo));
        let mut picker = ThemePicker::new();
        picker.refilter(&self.user_themes);
        self.theme_picker = Some(picker);
    }

    /// The Installed tab's rows: the themes you downloaded or saved.
    pub fn installed_themes(&self) -> Vec<String> {
        self.user_themes.iter().map(|(n, _)| n.clone()).collect()
    }

    pub(super) fn theme_picker_key(&mut self, key: KeyEvent) {
        let installed = self.installed_themes();
        let Some(picker) = self.theme_picker.as_mut() else { return };
        match picker.key(key, &self.user_themes, &installed) {
            Action::None => {}
            Action::Close => self.theme_picker = None,
            Action::Use(slug) => self.use_theme(&slug),
            Action::Busy => self.status = "Still downloading the last one…".into(),
            Action::Download(entry) => {
                self.status = format!("Downloading {}…", entry.name);
                self.theme_job = Some(crate::theme_store::fetch_theme(entry, self.demo));
            }
            Action::Delete(name) => self.delete_theme(&name),
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
            picker.refilter_keep(&self.user_themes); // back in Search
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
                        // iTerm2-Color-Schemes only: Omarchy's themes come with Auto Mode.
                        picker.catalog.extend(entries.into_iter().filter(|e| e.source != Source::Omarchy));
                        picker.refilter(&self.user_themes);
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
                if let Some(picker) = self.theme_picker.as_mut() {
                    picker.refilter_keep(&self.user_themes); // gone from Search
                }
                self.use_theme(&slug);
            }
        }
    }
}
