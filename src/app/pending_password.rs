//! Where the password in memory came from, and whether the keychain has it yet.
//!
//! Two booleans used to say this, and one implied the other: a password read
//! from `config.toml` is always also unsaved. Written as an enum, "from the
//! file but already saved" cannot be spelled, so the file is never asked to
//! let go of a password the keychain does not hold.
//!
//! The save is asynchronous, so there is a state between asking and knowing:
//! [`PendingPassword::InFileOnly`] is a password whose save is on its way while
//! `config.toml` still holds a copy. Only a confirmed save clears it.

/// The password in memory that the keychain does not have yet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PendingPassword {
    /// Nothing to save.
    #[default]
    None,
    /// Typed in this session: save it once Pandora takes it.
    Typed,
    /// Read from `config.toml` by an older boxvibe: save it, then let the file
    /// drop it — but only once the keychain really has it.
    FromFile,
    /// The save has been sent; `config.toml` still holds a copy until it lands.
    InFileOnly,
}

impl PendingPassword {
    /// A password was typed and is not saved yet.
    pub fn typed(&mut self) {
        *self = Self::Typed;
    }

    /// The password came from `config.toml`.
    pub fn read_from_file(&mut self) {
        *self = Self::FromFile;
    }

    /// Whether a save still has to be sent.
    pub fn needs_saving(self) -> bool {
        matches!(self, Self::Typed | Self::FromFile)
    }

    /// Whether `config.toml` still holds a copy that should go once the
    /// keychain has it.
    pub fn in_file(self) -> bool {
        matches!(self, Self::FromFile | Self::InFileOnly)
    }

    /// The save has been sent. A password from the file stays marked until the
    /// keychain confirms it; a typed one is done.
    pub fn sent(&mut self) {
        *self = if self.in_file() { Self::InFileOnly } else { Self::None };
    }

    /// The keychain has it now (or it is gone): nothing left to do.
    pub fn done(&mut self) {
        *self = Self::None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_from_the_file_is_also_unsaved() {
        let mut pending = PendingPassword::default();
        assert!(!pending.needs_saving() && !pending.in_file());
        pending.read_from_file();
        assert!(pending.needs_saving(), "the keychain does not have it");
        assert!(pending.in_file(), "and config.toml still does");
    }

    #[test]
    fn a_typed_password_is_not_in_the_file() {
        let mut pending = PendingPassword::default();
        pending.typed();
        assert!(pending.needs_saving());
        assert!(!pending.in_file(), "nothing to remove from config.toml");
    }

    #[test]
    fn sending_the_save_leaves_a_file_copy_marked_until_it_lands() {
        // The bug this state exists to prevent: clearing everything when the
        // save was merely sent left config.toml holding the password for good.
        let mut pending = PendingPassword::default();
        pending.read_from_file();
        pending.sent();
        assert!(!pending.needs_saving(), "the save is on its way");
        assert!(pending.in_file(), "but the file still has a copy");
        pending.done();
        assert!(!pending.in_file(), "confirmed: the file may drop it");
    }

    #[test]
    fn sending_a_typed_password_is_the_end_of_it() {
        let mut pending = PendingPassword::default();
        pending.typed();
        pending.sent();
        assert!(!pending.needs_saving() && !pending.in_file());
    }
}
