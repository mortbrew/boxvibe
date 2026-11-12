//! Where the Browse page is and where Esc goes back to.
//!
//! Three things have to agree at all times: which View-all page is on screen
//! (`path`, or none for the teaser), what that page holds (`state`), and the
//! page underneath it (`parent`) so Esc can return there whole — its rows,
//! its path and its cursor together.
//!
//! Kept as loose fields they drifted: restoring a parent meant remembering to
//! put back all three by hand at every call site. Here the snapshot travels as
//! one value, so a half-restored page cannot happen.

use super::{BrowsePlace, BrowseState};

#[derive(Default)]
pub struct BrowsePages {
    /// The main `/browse` teaser. View-all paths are read from here.
    directory: BrowseState,
    /// View-all path on screen or in flight. `None`: the page is the teaser.
    path: Option<String>,
    /// Page to restore when Esc leaves a category shelf.
    parent: Option<BrowsePlace>,
}

impl BrowsePages {
    pub fn directory(&self) -> &BrowseState {
        &self.directory
    }

    pub fn set_directory(&mut self, state: BrowseState) {
        self.directory = state;
    }

    /// The View-all path on screen or in flight; `None` for the teaser.
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// Whether `path` is the page on screen or in flight.
    pub fn is_on(&self, path: &str) -> bool {
        self.path.as_deref() == Some(path)
    }

    /// Back to the teaser (no View-all page).
    pub fn show_teaser(&mut self) {
        self.path = None;
    }

    /// Opens a View-all page (or marks one in flight).
    pub fn open(&mut self, path: String) {
        self.path = Some(path);
    }

    /// Forgets both where we are and where we came from.
    pub fn reset(&mut self) {
        self.path = None;
        self.parent = None;
    }

    /// Remembers the page on screen so Esc can return to it whole.
    pub fn remember(&mut self, state: BrowseState, cursor: usize) {
        self.parent = Some(BrowsePlace { state, path: self.path.clone(), cursor });
    }

    /// The page Esc returns to: its rows and its cursor (the path comes back too).
    pub fn back(&mut self) -> Option<(BrowseState, usize)> {
        let place = self.parent.take()?;
        self.path = place.path;
        Some((place.state, place.cursor))
    }

    pub fn can_go_back(&self) -> bool {
        self.parent.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn going_back_restores_the_path_with_the_rows() {
        let mut pages = BrowsePages::default();
        pages.open("/browse/music/genres".into());
        pages.set_directory(BrowseState::Loaded(Vec::new()));
        pages.remember(BrowseState::Loaded(Vec::new()), 4);
        pages.open("/browse/music/genres/pop".into());
        assert!(pages.can_go_back());
        let (state, cursor) = pages.back().expect("a page underneath");
        assert!(matches!(state, BrowseState::Loaded(_)));
        assert_eq!(cursor, 4);
        assert_eq!(pages.path(), Some("/browse/music/genres"), "the path came back too");
        assert!(!pages.can_go_back(), "only one level");
    }

    #[test]
    fn going_back_from_a_shelf_opened_on_the_teaser_returns_to_the_teaser() {
        let mut pages = BrowsePages::default();
        pages.set_directory(BrowseState::Loaded(Vec::new()));
        pages.remember(BrowseState::Loaded(Vec::new()), 2);
        pages.open("/browse/music/decades".into());
        pages.back().expect("a page underneath");
        assert_eq!(pages.path(), None, "the teaser has no View-all path");
    }

    #[test]
    fn resetting_forgets_the_page_underneath() {
        let mut pages = BrowsePages::default();
        pages.open("/a".into());
        pages.remember(BrowseState::Loading, 0);
        pages.reset();
        assert_eq!(pages.path(), None);
        assert!(!pages.can_go_back());
        assert!(pages.back().is_none());
    }
}
