//! Requests that are in flight, so a second one is not sent on top.
//!
//! Both flags mean the same thing: a request has been sent and its answer has
//! not arrived. They were loose booleans, set in one place and cleared in four
//! or five others — every failure path had to remember to clear them, and a
//! path that forgot left the feature dead for the rest of the session (a
//! station that could never be made again, a queue that never refilled).
//!
//! Keeping them in one type does not make forgetting impossible, but it puts
//! the whole lifecycle in one file, so the set and the clears are read
//! together instead of being scattered across `app.rs` and `playback.rs`.

#[derive(Default)]
pub struct InFlight {
    /// A station is being made: a second Enter waits for it.
    creating_station: bool,
    /// A playlist was asked for: no second ask until it arrives.
    playlist: bool,
}

impl InFlight {
    /// Whether a station is being made right now.
    #[cfg(test)]
    pub fn creating_station(&self) -> bool {
        self.creating_station
    }

    /// Marks a station as being made. `false`: one already is.
    pub fn begin_station(&mut self) -> bool {
        if self.creating_station {
            return false;
        }
        self.creating_station = true;
        true
    }

    /// The station arrived, or Pandora refused: either way it is not in flight.
    pub fn end_station(&mut self) {
        self.creating_station = false;
    }

    /// Whether a playlist is in flight.
    #[cfg(test)]
    pub fn playlist(&self) -> bool {
        self.playlist
    }

    /// Marks a playlist as asked for. `false`: one already is.
    pub fn begin_playlist(&mut self) -> bool {
        if self.playlist {
            return false;
        }
        self.playlist = true;
        true
    }

    /// The playlist arrived, or the ask failed: either way it is not in flight.
    pub fn end_playlist(&mut self) {
        self.playlist = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_station_is_refused_while_one_is_being_made() {
        let mut f = InFlight::default();
        assert!(f.begin_station());
        assert!(f.creating_station());
        assert!(!f.begin_station(), "one at a time");
        f.end_station();
        assert!(!f.creating_station());
        assert!(f.begin_station(), "free again once it arrived");
    }

    #[test]
    fn a_second_playlist_is_refused_while_one_is_in_flight() {
        let mut f = InFlight::default();
        assert!(f.begin_playlist());
        assert!(f.playlist());
        assert!(!f.begin_playlist(), "one at a time");
        f.end_playlist();
        assert!(f.begin_playlist(), "free again once it arrived");
    }

    #[test]
    fn the_two_are_independent() {
        // Making a station must not block the queue refilling, and vice versa.
        let mut f = InFlight::default();
        assert!(f.begin_station());
        assert!(f.begin_playlist(), "a playlist is not blocked by a station");
        f.end_station();
        assert!(f.playlist(), "ending one leaves the other alone");
    }

    #[test]
    fn ending_twice_is_harmless() {
        // Every failure path clears, and some run after a success already did.
        let mut f = InFlight::default();
        f.end_station();
        f.end_playlist();
        assert!(!f.creating_station() && !f.playlist());
    }
}
