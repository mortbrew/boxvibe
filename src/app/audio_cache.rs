//! The downloaded songs kept for their turn in the queue.
//!
//! Each entry is a whole MP3, so the map must not grow with the session. The
//! order the songs arrived in is kept beside the map so the oldest can go, and
//! the two must never disagree: a token left in the order after its bytes are
//! gone would evict a song that is still cached, and a token missing from the
//! order would never be evicted at all.
//!
//! Keeping them in one type is what makes that impossible. Before, every
//! `audio_cache.remove` had to be paired with an `audio_order.retain` by hand,
//! and one of them was not.

use std::collections::{HashMap, VecDeque};

/// How many downloaded songs are kept. Each is a whole MP3.
pub const KEPT: usize = 3;

#[derive(Default)]
pub struct AudioCache {
    data: HashMap<String, Vec<u8>>,
    /// The order `data` was filled in, oldest first.
    order: VecDeque<String>,
}

impl AudioCache {
    /// Keeps `data` for `token`, dropping the oldest once there are more than
    /// [`KEPT`].
    pub fn keep(&mut self, token: String, data: Vec<u8>) {
        self.data.insert(token.clone(), data);
        self.order.retain(|t| *t != token);
        self.order.push_back(token);
        while self.order.len() > KEPT {
            let Some(oldest) = self.order.pop_front() else { break };
            self.data.remove(&oldest);
        }
    }

    /// Takes the song for `token`, if it is still here.
    pub fn take(&mut self, token: &str) -> Option<Vec<u8>> {
        let data = self.data.remove(token)?;
        self.order.retain(|t| t != token);
        Some(data)
    }

    /// Forgets `token` without returning it (the song is no longer wanted).
    pub fn forget(&mut self, token: &str) {
        self.data.remove(token);
        self.order.retain(|t| t != token);
    }

    pub fn clear(&mut self) {
        self.data.clear();
        self.order.clear();
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[cfg(test)]
    pub fn contains(&self, token: &str) -> bool {
        self.data.contains_key(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(tokens: &[&str]) -> AudioCache {
        let mut cache = AudioCache::default();
        for token in tokens {
            cache.keep((*token).to_owned(), vec![1]);
        }
        cache
    }

    #[test]
    fn only_the_newest_are_kept() {
        let cache = cache(&["a", "b", "c", "d"]);
        assert_eq!(cache.len(), KEPT);
        assert!(!cache.contains("a"), "the oldest went");
        assert!(cache.contains("d"));
    }

    #[test]
    fn taking_a_song_leaves_no_trace_in_the_order() {
        // The bug this type exists to prevent: a token left in the order after
        // its bytes were taken would evict a song that is still cached.
        let mut cache = cache(&["a", "b", "c"]);
        assert_eq!(cache.take("a"), Some(vec![1]));
        assert_eq!(cache.take("a"), None, "gone once taken");
        // Room for one more without losing a song that is still here.
        cache.keep("d".to_owned(), vec![1]);
        assert_eq!(cache.len(), KEPT);
        assert!(cache.contains("b") && cache.contains("c") && cache.contains("d"));
    }

    #[test]
    fn forgetting_a_song_leaves_no_trace_in_the_order() {
        let mut cache = cache(&["a", "b", "c"]);
        cache.forget("a");
        assert!(!cache.contains("a"));
        cache.keep("d".to_owned(), vec![1]);
        assert_eq!(cache.len(), KEPT, "the forgotten one did not count against the cap");
        assert!(cache.contains("b") && cache.contains("c") && cache.contains("d"));
    }

    #[test]
    fn keeping_the_same_song_again_moves_it_to_the_newest() {
        let mut cache = cache(&["a", "b", "c"]);
        cache.keep("a".to_owned(), vec![2]);
        assert_eq!(cache.len(), KEPT, "no duplicate entry");
        // `b` is now the oldest, so it goes rather than `a`.
        cache.keep("d".to_owned(), vec![1]);
        assert!(!cache.contains("b"));
        assert!(cache.contains("a"), "re-kept, so it is the newest");
    }

    #[test]
    fn clearing_empties_both_halves() {
        let mut cache = cache(&["a", "b"]);
        cache.clear();
        assert_eq!(cache.len(), 0);
        // Nothing stale left to evict a fresh song.
        for token in ["x", "y", "z"] {
            cache.keep(token.to_owned(), vec![1]);
        }
        assert_eq!(cache.len(), KEPT);
    }
}
