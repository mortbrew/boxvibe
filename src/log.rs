//! A log of what playback did, for working out why a song skipped.
//!
//! Written to `~/.cache/boxvibe/boxvibe.log` (XDG), never to the terminal: the
//! interface owns the screen. Past [`CAP`] the log moves to `.old` and starts
//! again, so it can be left on.
//!
//! `BOXVIBE_LOG=off` silences it; `BOXVIBE_LOG=<file>` writes somewhere else
//! (useful when the cache directory is not writable).
//!
//! What is worth a line: each song as it starts (what Pandora sent, what it
//! decoded to, whether it was resampled), each song as it ends (how far it got
//! against how long it should have been), and every reason a song was skipped.
//! A skip with no error message is the interesting case: it means the decoder
//! produced a source that ended at once, so the bytes are logged too.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Past this the log moves to `.old` and starts again.
const CAP: u64 = 512 * 1024;

/// Where the log goes, worked out once: `None` when it is switched off.
fn path() -> Option<&'static PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| {
        let setting = std::env::var("BOXVIBE_LOG").unwrap_or_default();
        match setting.trim() {
            "off" | "0" | "no" => None,
            // Tests make demo apps that play songs: they would fill the real
            // log with made-up titles, so they write nothing unless asked.
            "" if cfg!(test) => None,
            "" => directories::ProjectDirs::from("", "", "boxvibe").map(|d| d.cache_dir().join("boxvibe.log")),
            file => Some(PathBuf::from(file)),
        }
    })
    .as_ref()
}

/// Adds a line to the log, stamped with the time (UTC).
///
/// Errors writing it are ignored: a log is not worth a crash or a message.
pub fn note(args: std::fmt::Arguments<'_>) {
    let Some(path) = path() else { return };
    write(path, &format!("{} {args}", crate::ads::iso_utc(std::time::SystemTime::now())));
}

fn write(path: &Path, line: &str) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > CAP) {
        let _ = std::fs::rename(path, path.with_extension("log.old"));
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // One line, whatever was handed in: a newline in a title would split it.
    let line: String = line.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

/// The first bytes of a payload, as hex, for telling containers apart.
pub fn head(data: &[u8]) -> String {
    data.iter().take(16).map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// `log!(...)`: a line in the playback log.
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::log::note(format_args!($($arg)*))
    };
}
pub(crate) use log;

#[cfg(test)]
mod tests;
