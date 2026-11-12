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
