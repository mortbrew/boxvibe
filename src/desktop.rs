//! Settings → BOXVIBE → App menu entry: a `boxvibe.desktop` file so app
//! launchers (walker on Omarchy, GNOME, KDE, ...) list boxvibe. It is a file
//! on disk, not a config key: the setting reads the disk every time.

use std::io;
use std::path::{Path, PathBuf};

/// The line that marks a file as boxvibe's own; only such a file is deleted.
pub const MARKER: &str = "X-Boxvibe-Generated=true";

/// `$XDG_DATA_HOME/applications`, else `~/.local/share/applications`.
pub fn applications_dir() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(data.join("applications"))
}

pub fn path_in(dir: &Path) -> PathBuf {
    dir.join("boxvibe.desktop")
}

pub fn installed(dir: &Path) -> bool {
    path_in(dir).is_file()
}

/// `exe` as one Exec argument, quoted and escaped as the desktop entry spec
/// asks: inside double quotes `"` `` ` `` `$` `\` get a backslash, then the
/// value's own escaping doubles every backslash; `%` is written `%%`.
pub fn exec_arg(exe: &Path) -> String {
    let s = exe.to_string_lossy();
    let reserved = |c: char| " \t\n\"'\\><~|&;$*?#()`".contains(c);
    let s = if s.contains(reserved) {
        let mut q = String::from("\"");
        for c in s.chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                q.push('\\');
            }
            q.push(c);
        }
        q.push('"');
        q.replace('\\', "\\\\")
    } else {
        s.into_owned()
    };
    s.replace('%', "%%")
}

pub fn contents(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=boxvibe\n\
         Comment=Pandora in the terminal\n\
         Exec={}\n\
         Icon=multimedia-audio-player\n\
         Terminal=true\n\
         Categories=Audio;AudioVideo;Player;\n\
         Keywords=pandora;music;radio;\n\
         {MARKER}\n",
        exec_arg(exe)
    )
}

/// Writes the entry (replacing one boxvibe wrote) for `exe`.
pub fn install(dir: &Path, exe: &Path) -> io::Result<PathBuf> {
    let path = path_in(dir);
    if path.exists() && !generated(&path) {
        return Err(io::Error::other("a boxvibe.desktop boxvibe didn't write is already there"));
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(&path, contents(exe))?;
    refresh(dir);
    Ok(path)
}

/// Deletes the entry, only if boxvibe wrote it.
pub fn remove(dir: &Path) -> io::Result<PathBuf> {
    let path = path_in(dir);
    if !generated(&path) {
        return Err(io::Error::other("boxvibe didn't write it, so it is left alone"));
    }
    std::fs::remove_file(&path)?;
    refresh(dir);
    Ok(path)
}

fn generated(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|s| s.lines().any(|l| l.trim() == MARKER))
}

/// Lets launchers that use the cache see the change; fine if it's missing.
fn refresh(dir: &Path) {
    let _ = std::process::Command::new("update-desktop-database")
        .arg(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// The running binary, symlinks resolved.
pub fn current_exe() -> io::Result<PathBuf> {
    std::env::current_exe()?.canonicalize()
}

#[cfg(test)]
mod tests;
