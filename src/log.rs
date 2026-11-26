//! A log of what playback and the Pandora calls did, for working out why a song
//! skipped or a login was refused.
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
//! produced a source that ended at once, so the bytes are logged too. A failed
//! Pandora call adds its endpoint, HTTP status and kind of failure, and a login
//! says whether it worked, so a bot check can be told apart from a bad password
//! after the fact. Neither the request nor the answer is written: the login body
//! carries the password and a block page carries the account name.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;

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
    enqueue(path, CAP, &format!("{} {args}", crate::time::iso_utc(std::time::SystemTime::now())));
}

/// Lines `try_send` could not take. The next line that does fit also writes how many were lost.
static DROPPED: AtomicU64 = AtomicU64::new(0);

/// Hands a line to the log thread. A full queue drops the line and counts it:
/// the UI thread does not wait on disk. When a later line fits, one note says
/// how many were dropped while the queue was full.
pub fn enqueue(path: &Path, cap: u64, line: &str) {
    let tx = writer();
    if tx.try_send((path.to_path_buf(), cap, line.to_owned())).is_err() {
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let n = DROPPED.swap(0, Ordering::Relaxed);
    if n > 0 {
        let note = format!("dropped {n} log lines while the log was busy");
        if tx.try_send((path.to_path_buf(), cap, note)).is_err() {
            DROPPED.fetch_add(n.saturating_add(1), Ordering::Relaxed);
        }
    }
}

fn writer() -> &'static mpsc::SyncSender<(PathBuf, u64, String)> {
    static TX: OnceLock<mpsc::SyncSender<(PathBuf, u64, String)>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<(PathBuf, u64, String)>(64);
        let _ = std::thread::Builder::new().name("log".into()).spawn(move || {
            while let Ok((path, cap, line)) = rx.recv() {
                append(&path, cap, &line);
            }
        });
        tx
    })
}

/// Adds one line to the log at `path`, moving it to `.old` first when it has
/// grown past `cap`. Control characters become spaces, so a newline in a title
/// cannot split the line. Errors are ignored: a log is not worth a crash.
pub fn append(path: &Path, cap: u64, line: &str) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > cap) {
        let _ = std::fs::rename(path, path.with_extension("log.old"));
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
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
