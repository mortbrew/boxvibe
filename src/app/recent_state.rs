//! The bookkeeping behind Recently Played's fetches.
//!
//! Three things have to stay in step, and getting them wrong is silent: a
//! reply from an older login overwrites a newer list, or a second request
//! while one is in flight is dropped and the list never refreshes.
//!
//! - `generation` names the request in flight. A reply carries the generation
//!   it was asked under, and one that does not match is stale.
//! - `loading` says a request is out. A second ask while it is set is not sent;
//!   it sets `dirty` instead, and the reply sends once more.
//! - `asked` says the login's first fetch has gone, so it is not repeated.
//!
//! They live together so a caller cannot set one without the others.

/// What is known about the request in flight.
#[derive(Default)]
pub struct RecentState {
    /// Bumped on each ask and on logout. A reply from an older one is dropped.
    generation: u64,
    /// A request is out.
    loading: bool,
    /// An ask arrived while one was in flight: send once more when it lands.
    dirty: bool,
    /// The login's first fetch has been sent. Logout clears it.
    asked: bool,
}

impl RecentState {
    /// Whether the login's first fetch still has to go.
    pub fn needs_first_ask(&self) -> bool {
        !self.asked
    }

    pub fn loading(&self) -> bool {
        self.loading
    }

    /// Starts a request: names it, and clears the waiting flag.
    ///
    /// `None` when one is already out — the caller then sets nothing, because
    /// this records that another ask is waiting instead.
    pub fn begin(&mut self) -> Option<u64> {
        if self.loading {
            self.dirty = true;
            return None;
        }
        self.dirty = false;
        self.asked = true;
        self.loading = true;
        self.generation = self.generation.wrapping_add(1);
        Some(self.generation)
    }

    /// The reply landed. `true`: an ask arrived meanwhile, so send once more.
    pub fn finish(&mut self) -> bool {
        self.loading = false;
        std::mem::take(&mut self.dirty)
    }

    /// Whether a reply belongs to the request in flight.
    pub fn accepts(&self, generation: u64) -> bool {
        generation == self.generation
    }

    /// Logout, or a fresh list: invalidates an answer still in flight and
    /// forgets that anything was asked.
    pub fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.loading = false;
        self.dirty = false;
        self.asked = false;
    }

    #[cfg(test)]
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_ask_while_one_is_out_waits_for_it() {
        let mut state = RecentState::default();
        let first = state.begin().expect("the first ask goes");
        assert!(state.begin().is_none(), "the second waits");
        assert!(state.finish(), "and is sent once the reply lands");
        assert!(!state.finish(), "only once");
        assert!(state.accepts(first), "the reply still names the request");
    }

    #[test]
    fn a_reply_from_an_older_request_is_refused() {
        let mut state = RecentState::default();
        let old = state.begin().unwrap();
        state.finish();
        let new = state.begin().unwrap();
        assert!(!state.accepts(old), "the older login's answer is stale");
        assert!(state.accepts(new));
    }

    #[test]
    fn resetting_invalidates_what_is_in_flight() {
        let mut state = RecentState::default();
        let generation = state.begin().unwrap();
        state.reset();
        assert!(!state.accepts(generation), "logout drops the answer on its way");
        assert!(!state.loading());
        assert!(state.needs_first_ask(), "the next login asks again");
    }

    #[test]
    fn the_first_ask_is_sent_once_per_login() {
        let mut state = RecentState::default();
        assert!(state.needs_first_ask());
        state.begin().unwrap();
        assert!(!state.needs_first_ask());
        state.finish();
        assert!(!state.needs_first_ask(), "a refresh does not re-arm it");
        state.reset();
        assert!(state.needs_first_ask());
    }
}
