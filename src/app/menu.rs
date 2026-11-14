//! Cached main-menu rows and Browse pin labels.
//!
//! Owned separately from [`App`] so draws borrow slices without cloning, and
//! so the epoch / rebuild invariant stays in one place.

use super::*;

/// The left-panel menu model: item list, Browse pin labels, and the epoch
/// that says they match the current config / session / recent state.
#[derive(Default)]
pub struct MenuModel {
    items: Vec<MenuItem>,
    browse_pins: Vec<(String, String)>,
    epoch: u64,
}

impl MenuModel {
    pub fn items(&self) -> &[MenuItem] {
        &self.items
    }

    pub fn browse_pins(&self) -> &[(String, String)] {
        &self.browse_pins
    }

    /// Rebuilds both caches for `epoch` (callers compute pins and items).
    pub fn refresh(&mut self, epoch: u64, browse_pins: Vec<(String, String)>, items: Vec<MenuItem>) {
        self.browse_pins = browse_pins;
        self.items = items;
        self.epoch = epoch;
    }

    pub fn is_current(&self, epoch: u64) -> bool {
        self.epoch == epoch
    }
}
