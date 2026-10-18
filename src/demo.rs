//! `--demo`: made-up stations and songs, no network and no sound, so the
//! interface can be tried (and screenshotted) without a Pandora account.

use crate::pandora::{AudioFormat, Rating, Song, Station};

pub fn stations() -> Vec<Station> {
    let names = [
        ("Shuffle", true, false),
        ("Thumbprint Radio", false, false),
        ("M83 Radio", false, false),
        ("Lo-Fi Beats", false, false),
        ("Classic Rock Radio", false, false),
        ("90s Alternative", false, false),
        ("Jazz Vibes", false, false),
        ("Synthwave Radio", false, true),
        ("Today's Hits", false, false),
    ];
    names
        .iter()
        .enumerate()
        .map(|(i, (name, shuffle, shared))| Station {
            id: format!("demo-{i}"),
            name: (*name).into(),
            is_shuffle: *shuffle,
            is_shared: *shared,
            art_url: format!("https://example.invalid/station/{i}.jpg"),
        })
        .collect()
}

const SONGS: [(&str, &str, &str, u32); 12] = [
    ("Midnight City", "M83", "Hurry Up, We're Dreaming", 243),
    ("Let It Happen", "Tame Impala", "Currents", 467),
    ("Kids", "MGMT", "Oracular Spectacular", 302),
    ("The Mother We Share", "CHVRCHES", "The Bones of What You Believe", 192),
    ("1901", "Phoenix", "Wolfgang Amadeus Phoenix", 193),
    ("Digital Love", "Daft Punk", "Discovery", 301),
    ("Dreams", "Fleetwood Mac", "Rumours", 257),
    ("Electric Feel", "MGMT", "Oracular Spectacular", 229),
    ("Nightcall", "Kavinsky", "OutRun", 258),
    ("Feel It Still", "Portugal. The Man", "Woodstock", 163),
    ("Instant Crush", "Daft Punk", "Random Access Memories", 337),
    ("Pumped Up Kicks", "Foster the People", "Torches", 239),
];

/// Four songs for `station_id`, the `round`-th batch (like a `getFragment`).
pub fn playlist(station_id: &str, round: usize) -> Vec<Song> {
    (0..4)
        .map(|i| {
            let n = (round * 4 + i) % SONGS.len();
            let (title, artist, album, length) = SONGS[n];
            Song {
                title: title.into(),
                artist: artist.into(),
                album: album.into(),
                track_token: format!("demo-track-{round}-{i}"),
                station_id: station_id.into(),
                audio_url: String::new(),
                audio_format: AudioFormat::Mp3,
                cover_art_url: format!("https://example.invalid/art/{n}.jpg"),
                detail_url: String::new(),
                length,
                gain: 0.0,
                rating: if n == 0 { Rating::Up } else { Rating::None },
            }
        })
        .collect()
}
