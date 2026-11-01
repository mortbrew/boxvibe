//! Themes live in vibekit; this keeps `crate::theme::*` working.

pub use vibekit::theme::*;

/// `~/.config/boxvibe/themes`.
pub fn user_dir() -> Option<std::path::PathBuf> {
    vibekit::theme::user_dir("boxvibe")
}
