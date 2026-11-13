//! Settings → BOXVIBE → App menu entry: a `boxvibe.desktop` file so app
//! launchers (walker on Omarchy, GNOME, KDE, ...) list boxvibe. It is a file
//! on disk, not a config key: the setting reads the disk every time.

use std::io;
use std::path::{Path, PathBuf};

use vibekit::desktop::Entry;

/// The line that marks a file as boxvibe's own; only such a file is deleted.
pub const MARKER: &str = "X-Boxvibe-Generated=true";

const BOXVIBE: Entry = Entry {
    id: "boxvibe",
    name: "boxvibe",
    comment: "Pandora in the terminal",
    marker: MARKER,
    icon: "multimedia-audio-player",
    categories: "Audio;AudioVideo;Player;",
    keywords: "pandora;music;radio;",
};

/// `$XDG_DATA_HOME/applications`, else `~/.local/share/applications`.
pub fn applications_dir() -> Option<PathBuf> {
    Entry::applications_dir()
}

pub fn path_in(dir: &Path) -> PathBuf {
    BOXVIBE.path_in(dir)
}

pub fn installed(dir: &Path) -> bool {
    BOXVIBE.installed(dir)
}

/// `exe` as one Exec argument (see [`vibekit::desktop::Entry::exec_arg`]).
#[cfg(test)]
pub fn exec_arg(exe: &Path) -> String {
    Entry::exec_arg(exe)
}

/// Writes the entry (replacing one boxvibe wrote) for `exe`.
pub fn install(dir: &Path, exe: &Path) -> io::Result<PathBuf> {
    BOXVIBE.install(dir, exe)
}

/// Deletes the entry, only if boxvibe wrote it.
pub fn remove(dir: &Path) -> io::Result<PathBuf> {
    BOXVIBE.remove(dir)
}

/// The running binary, symlinks resolved.
pub fn current_exe() -> io::Result<PathBuf> {
    vibekit::desktop::current_exe()
}

#[cfg(test)]
mod tests;
