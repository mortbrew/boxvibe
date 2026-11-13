//! The View-all pages already fetched, keyed by their `viewAll.webUrl`.
//!
//! Each page is a whole section list, so the map must not grow with the
//! session. The order the pages arrived in is kept beside the map so the
//! oldest can go, and the two must never disagree — a path left in the order
//! after its page is gone would evict a page that is still cached.
//!
//! One page is special: the one on screen. Dropping it would empty the page
//! the user is reading, so it is never the one evicted.

use std::collections::{HashMap, VecDeque};

use crate::pandora::BrowseSection;

/// How many View-all pages are kept.
pub const KEPT: usize = 24;

#[derive(Default)]
pub struct ViewAllCache {
    pages: HashMap<String, Vec<BrowseSection>>,
    /// The order `pages` was filled in, oldest first.
    order: VecDeque<String>,
}

impl ViewAllCache {
    pub fn get(&self, path: &str) -> Option<&Vec<BrowseSection>> {
        self.pages.get(path)
    }

    /// Remembers `sections` for `path`, dropping the oldest page once there are
    /// more than [`KEPT`]. `on_screen` is never dropped.
    pub fn keep(&mut self, path: String, sections: Vec<BrowseSection>, on_screen: Option<&str>) {
        if self.pages.insert(path.clone(), sections).is_none() {
            self.order.push_back(path);
        }
        while self.order.len() > KEPT {
            // The oldest page that is not the one on screen.
            let Some(at) = self.order.iter().position(|p| on_screen != Some(p.as_str())) else {
                break;
            };
            let gone = self.order.remove(at).expect("just found");
            self.pages.remove(&gone);
        }
    }

    /// Forgets `path` (its fetch failed, so it is asked for again).
    pub fn forget(&mut self, path: &str) {
        self.pages.remove(path);
        self.order.retain(|p| p != path);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    #[cfg(test)]
    pub fn contains(&self, path: &str) -> bool {
        self.pages.contains_key(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(paths: &[&str]) -> ViewAllCache {
        let mut cache = ViewAllCache::default();
        for path in paths {
            cache.keep((*path).to_owned(), Vec::new(), None);
        }
        cache
    }

    #[test]
    fn only_the_newest_are_kept() {
        let mut cache = ViewAllCache::default();
        for i in 0..KEPT + 4 {
            cache.keep(format!("/p{i}"), Vec::new(), None);
        }
        assert_eq!(cache.len(), KEPT);
        assert!(!cache.contains("/p0"), "the oldest went");
        assert!(cache.contains(&format!("/p{}", KEPT + 3)));
    }

    #[test]
    fn the_page_on_screen_is_never_dropped() {
        let mut cache = ViewAllCache::default();
        cache.keep("/on-screen".into(), Vec::new(), Some("/on-screen"));
        for i in 0..KEPT + 4 {
            cache.keep(format!("/p{i}"), Vec::new(), Some("/on-screen"));
        }
        assert!(cache.contains("/on-screen"), "the page being read stays");
        assert_eq!(cache.len(), KEPT);
    }

    #[test]
    fn forgetting_a_page_leaves_no_trace_in_the_order() {
        // A path left in the order after its page was forgotten would evict a
        // page that is still cached.
        let mut cache = cache(&["/a", "/b", "/c"]);
        cache.forget("/a");
        assert!(!cache.contains("/a"));
        for i in 0..KEPT - 2 {
            cache.keep(format!("/f{i}"), Vec::new(), None);
        }
        assert_eq!(cache.len(), KEPT, "the forgotten page did not count against the cap");
        assert!(cache.contains("/b") && cache.contains("/c"), "a cached page was not evicted by a stale path");
    }

    #[test]
    fn keeping_the_same_page_again_does_not_duplicate_it() {
        let mut cache = cache(&["/a", "/b"]);
        cache.keep("/a".into(), Vec::new(), None);
        assert_eq!(cache.len(), 2);
        for i in 0..KEPT - 2 {
            cache.keep(format!("/f{i}"), Vec::new(), None);
        }
        assert_eq!(cache.len(), KEPT, "no duplicate entry counted against the cap");
    }
}
