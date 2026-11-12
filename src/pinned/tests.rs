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

#[test]
fn a_saved_pin_file_is_private_and_leaves_no_temporary_behind() {
    let dir = crate::config::temp_dir("pinned-private");
    let path = dir.join("pinned.toml");
    save(&path, &[]).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600, "owner only");
    }
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "the temporary file is renamed away: {leftovers:?}");
}
