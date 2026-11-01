//! Self-update lives in vibekit; this pins it to boxvibe's releases.

use vibekit::update::Release;
pub use vibekit::update::{Checked, Progress, Replace, Update, is_dev_build};

/// boxvibe on GitHub.
pub const RELEASE: Release = Release { name: "boxvibe", repo: "mortbrew/boxvibe", version: env!("CARGO_PKG_VERSION") };

/// Asks GitHub for the newest release.
pub fn check() -> Checked {
    RELEASE.check()
}

/// Downloads, checks and swaps in `version` from `url`.
pub fn install(url: &str, version: &str, progress: impl FnMut(u8)) -> Result<std::path::PathBuf, String> {
    RELEASE.install(url, version, progress)
}
