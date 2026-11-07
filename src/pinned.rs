//! Pinned songs (Shift+I): a list kept in `~/.local/share/boxvibe/pinned.toml`,
//! apart from the config. Only what Pandora showed about the song is kept
//! (title, artist, album, cover link, Pandora page, the day it was pinned);
//! never any audio.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::pandora::Song;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Pin {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub cover_url: String,
    /// The song's Pandora page.
    pub link: String,
    /// `2026-10-29` (UTC).
    pub pinned: String,
}

impl Pin {
    pub fn of(song: &Song, today: &str) -> Self {
        Self {
            title: song.title.clone(),
            artist: song.artist.clone(),
            album: song.album.clone(),
            cover_url: song.cover_art_url.clone(),
            link: song.detail_url.clone(),
            pinned: today.to_owned(),
        }
    }

    /// The same song: same title and artist (Pandora's links change).
    pub fn same(&self, song: &Song) -> bool {
        self.title == song.title && self.artist == song.artist
    }
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    pin: Vec<Pin>,
}

/// `$XDG_DATA_HOME/boxvibe/pinned.toml` (`~/.local/share/boxvibe/pinned.toml`).
pub fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "boxvibe").map(|d| d.data_dir().join("pinned.toml"))
}

/// The pins, newest first. A missing or broken file is an empty list (a
/// broken one is kept aside as `pinned.toml.bad` before anything is saved).
pub fn load(path: &Path) -> Vec<Pin> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    match toml::from_str::<File>(&text) {
        Ok(file) => file.pin,
        Err(_) => {
            let _ = std::fs::rename(path, path.with_extension("toml.bad"));
            Vec::new()
        }
    }
}

pub fn save(path: &Path, pins: &[Pin]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text =
        String::from("# boxvibe's pinned songs (Shift+I pins, x on the Pinned page unpins). No audio is kept.\n\n");
    text.push_str(&toml::to_string(&File { pin: pins.to_vec() }).map_err(std::io::Error::other)?);
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

/// Today, `2026-10-29` (UTC).
pub fn today() -> String {
    crate::ads::iso_utc(std::time::SystemTime::now())[..10].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_round_trip_and_a_broken_file_is_an_empty_list() {
        let dir = crate::config::temp_dir("pinned");
        let path = dir.join("pinned.toml");
        assert!(load(&path).is_empty(), "no file yet");
        let song = Song {
            title: "Song".into(),
            artist: "Band".into(),
            album: "Record".into(),
            cover_art_url: "https://example.com/c.jpg".into(),
            detail_url: "https://www.pandora.com/artist/band/record/song".into(),
            audio_url: "https://example.com/audio.mp3".into(),
            ..Song::default()
        };
        let pins = vec![Pin::of(&song, "2026-10-29")];
        save(&path, &pins).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[[pin]]") && text.contains("pinned = \"2026-10-29\""), "{text}");
        assert!(!text.contains("audio.mp3"), "no audio link is kept: {text}");
        assert_eq!(load(&path), pins);
        assert!(pins[0].same(&song));
        std::fs::write(&path, "not [toml").unwrap();
        assert!(load(&path).is_empty());
        assert!(path.with_extension("toml.bad").exists());
        assert_eq!(today().len(), 10);
    }
}
