//! Self-update lives in vibekit; this pins it to boxvibe's releases.

use vibekit::update::Release;
pub use vibekit::update::{Checked, Progress, Replace, Update, is_dev_build};

/// boxvibe on GitHub.
pub const RELEASE: Release = Release { name: "boxvibe", repo: "mortbrew/boxvibe", version: env!("CARGO_PKG_VERSION") };

/// Asks GitHub for the newest release.
pub fn check() -> Checked {
    RELEASE.check()
}

/// One user-facing line of a release: the bold headline and what follows it.
///
/// About draws the two differently, so they are kept apart here instead of being
/// flattened into one paragraph with the `**` markers stripped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Note {
    /// The sentence the changelog had in bold: what changed. Empty when it had none.
    pub lead: String,
    /// The rest of the line: how it works. Empty when the changelog said no more.
    pub detail: String,
}

impl Note {
    /// Reads one changelog bullet: `**lead** detail`, with the code backticks dropped
    /// so none of them reach the screen.
    pub fn parse(line: &str) -> Self {
        let line = line.trim().replace('`', "");
        if let Some(rest) = line.strip_prefix("**")
            && let Some((lead, after)) = rest.split_once("**")
        {
            return Self { lead: lead.trim().to_owned(), detail: after.trim().to_owned() };
        }
        Self { lead: String::new(), detail: line }
    }

    /// The whole note as one line, for tests and for anywhere that has no room to style it.
    pub fn plain(&self) -> String {
        match (self.lead.is_empty(), self.detail.is_empty()) {
            (true, _) => self.detail.clone(),
            (false, true) => self.lead.clone(),
            (false, false) => format!("{} {}", self.lead, self.detail),
        }
    }
}

/// The user-facing lines of `version` in a changelog: Added, Changed and Fixed.
/// Internal notes stay out. An empty list means the section was not there.
pub fn changelog_notes(text: &str, version: &str) -> Vec<Note> {
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
            let note = Note::parse(rest);
            if !note.plain().is_empty() {
                notes.push(note);
            }
            if notes.len() == 8 {
                break;
            }
        }
    }
    notes
}

/// `CHANGELOG.md` from the `v{version}` tag. Quiet when GitHub has nothing to say.
pub fn release_notes(version: &str) -> Vec<Note> {
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
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].lead, "A podcast episode plays.");
        assert_eq!(notes[0].detail, "Enter starts it.");
        assert_eq!(notes[1].lead, "Settings values sit beside their labels.");
        assert!(notes[1].detail.is_empty(), "a one-sentence note has no detail");
        assert_eq!(changelog_notes(LOG, "0.6.3")[0].plain(), "Songs are no longer skipped.");
        assert!(changelog_notes(LOG, "0.6.40").is_empty());
        assert!(changelog_notes(LOG, "9.9.9").is_empty());
    }

    #[test]
    fn a_note_keeps_its_code_words_but_not_the_backticks() {
        let note = Note::parse("**The filter is shown.** `[account] user_agent` still wins.");
        assert_eq!(note.lead, "The filter is shown.");
        assert_eq!(note.detail, "[account] user_agent still wins.");
        assert!(!note.plain().contains('`'), "no backticks reach the screen: {}", note.plain());
        // A line with no bold is all detail, and still loses its backticks.
        let plain = Note::parse("A `csrftoken` cookie is kept.");
        assert!(plain.lead.is_empty());
        assert_eq!(plain.detail, "A csrftoken cookie is kept.");
        // Bold that is never closed is not a headline: the line stays as it is.
        let unclosed = Note::parse("**Half a sentence.");
        assert!(unclosed.lead.is_empty());
        assert_eq!(unclosed.detail, "**Half a sentence.");
    }
}
