//! Self-update lives in vibekit; this pins it to boxvibe's releases.

use vibekit::update::Release;
pub use vibekit::update::{Checked, Progress, Replace, Update, is_dev_build};

/// boxvibe on GitHub.
pub const RELEASE: Release = Release { name: "boxvibe", repo: "mortbrew/boxvibe", version: env!("CARGO_PKG_VERSION") };

/// Asks GitHub for the newest release.
pub fn check() -> Checked {
    RELEASE.check()
}

/// The user-facing lines of `version` in a changelog: Added, Changed and Fixed.
/// Internal notes stay out. An empty list means the section was not there.
pub fn changelog_notes(text: &str, version: &str) -> Vec<String> {
    let mut in_section = false;
    let mut keep = false;
    let mut notes = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            if in_section {
                break;
            }
            let title = rest.trim();
            in_section = title == version || title.starts_with(&format!("{version} "));
            keep = false;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some(rest) = line.strip_prefix("### ") {
            keep = matches!(rest.trim(), "Added" | "Changed" | "Fixed");
            continue;
        }
        if keep && let Some(rest) = line.strip_prefix("- ") {
            let note = rest.replace("**", "");
            let note = note.trim();
            if !note.is_empty() {
                notes.push(note.to_owned());
            }
            if notes.len() == 8 {
                break;
            }
        }
    }
    notes
}

/// `CHANGELOG.md` from the `v{version}` tag. Quiet when GitHub has nothing to say.
pub fn release_notes(version: &str) -> Vec<String> {
    let url = format!("https://raw.githubusercontent.com/{}/v{version}/CHANGELOG.md", RELEASE.repo);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .user_agent(format!("boxvibe/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let Ok(reply) = agent.get(&url).call() else { return Vec::new() };
    let Ok(text) = reply.into_body().read_to_string() else { return Vec::new() };
    changelog_notes(&text, version)
}

/// Downloads, checks and swaps in `version` from `url`.
pub fn install(url: &str, version: &str, progress: impl FnMut(u8)) -> Result<std::path::PathBuf, String> {
    RELEASE.install(url, version, progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "\
## Unreleased

## 0.6.4 (2026-11-20)

### Added
- **A podcast episode plays.** Enter starts it.

### Fixed
- **Settings values sit beside their labels.**

### Internal
- syom 0.10.3 plays a fractional edit list.

## 0.6.3 (2026-11-13)

### Fixed
- **Songs are no longer skipped.**
";

    #[test]
    fn changelog_notes_are_the_added_and_fixed_lines_of_that_version() {
        let notes = changelog_notes(LOG, "0.6.4");
        assert_eq!(
            notes,
            vec![
                "A podcast episode plays. Enter starts it.".to_owned(),
                "Settings values sit beside their labels.".to_owned()
            ]
        );
        assert_eq!(changelog_notes(LOG, "0.6.3"), vec!["Songs are no longer skipped.".to_owned()]);
        assert!(changelog_notes(LOG, "0.6.40").is_empty());
        assert!(changelog_notes(LOG, "9.9.9").is_empty());
    }
}
