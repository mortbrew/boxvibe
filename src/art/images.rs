//! The downloaded covers kept in memory.
//!
//! Each entry is a decoded image, so the map must not grow with the session.
//! The order the covers arrived in is kept beside the map so the oldest can go,
//! and the two must never disagree: a url left in the order after its image is
//! gone would evict a cover that is still cached, and a url missing from the
//! order would never be evicted at all.
//!
//! Keeping them in one type is what makes that impossible.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use image::DynamicImage;

/// How many covers are kept in memory.
pub const KEPT: usize = 24;

#[derive(Default)]
pub struct ImageCache {
    images: HashMap<String, Arc<DynamicImage>>,
    /// The order `images` was filled in, oldest first.
    order: VecDeque<String>,
}

impl ImageCache {
    pub fn get(&self, url: &str) -> Option<&Arc<DynamicImage>> {
        self.images.get(url)
    }

    pub fn contains(&self, url: &str) -> bool {
        self.images.contains_key(url)
    }

    /// Keeps `image` for `url`, dropping the oldest once there are more than
    /// [`KEPT`].
    pub fn keep(&mut self, url: String, image: Arc<DynamicImage>) {
        self.images.insert(url.clone(), image);
        // A cover that arrives twice must not take two places in the order.
        self.order.retain(|u| *u != url);
        self.order.push_back(url);
        while self.order.len() > KEPT {
            let Some(oldest) = self.order.pop_front() else { break };
            self.images.remove(&oldest);
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.images.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cover() -> Arc<DynamicImage> {
        Arc::new(DynamicImage::new_rgb8(1, 1))
    }

    fn cache(urls: &[&str]) -> ImageCache {
        let mut cache = ImageCache::default();
        for url in urls {
            cache.keep((*url).to_owned(), cover());
        }
        cache
    }

    #[test]
    fn only_the_newest_are_kept() {
        let urls: Vec<String> = (0..KEPT + 4).map(|i| format!("u{i}")).collect();
        let refs: Vec<&str> = urls.iter().map(String::as_str).collect();
        let cache = cache(&refs);
        assert_eq!(cache.len(), KEPT);
        assert!(!cache.contains("u0"), "the oldest went");
        assert!(cache.contains(&format!("u{}", KEPT + 3)));
    }

    #[test]
    fn keeping_the_same_cover_again_takes_one_place() {
        // The bug this type exists to prevent: a url left twice in the order
        // would evict a cover that is still cached on the next arrival.
        let mut cache = cache(&["a", "b", "c"]);
        cache.keep("a".to_owned(), cover());
        assert_eq!(cache.len(), 3, "no duplicate entry");
        // Fill to the cap; nothing may be lost early because of the repeat.
        for i in 0..KEPT - 3 {
            cache.keep(format!("x{i}"), cover());
        }
        assert_eq!(cache.len(), KEPT);
        assert!(cache.contains("a"), "re-kept, so it is the newest");
        assert!(cache.contains("b") && cache.contains("c"));
    }
}
