//! In-flight and cached audio downloads for the player.
//!
//! Generation, buffering, the bytes cache, and the "already asked" set move
//! together: bumping the generation must clear `requested`, and a successful
//! download must clear the token from both cache bookkeeping and that set.

use std::collections::HashSet;
use std::time::Instant;

use super::audio_cache::AudioCache;

/// Download pipeline state owned by [`super::App`], separate from the UI bag.
#[derive(Default)]
pub struct AudioFetch {
    /// Invalidates queued / late audio downloads (see bump on station change).
    pub generation: u64,
    /// The song whose audio is downloading, and since when.
    pub buffering: Option<(String, Instant)>,
    /// Downloaded songs kept for their turn.
    pub cache: AudioCache,
    /// Tokens already sent to the audio worker this generation.
    pub requested: HashSet<String>,
}

impl AudioFetch {
    /// Advance the generation and drop in-flight request bookkeeping.
    pub fn bump(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.requested.clear();
        self.generation
    }
}
