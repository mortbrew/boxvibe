//! `--demo`: made-up stations and songs, no network and no sound, so the
//! interface can be tried (and screenshotted) without a Pandora account.

use crate::pandora::{
    AudioFormat, Genre, GenreCategory, HitKind, Rating, SearchHit, Seed, Song, Station, StationDetails,
};

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
            // Shuffle can't be renamed or deleted; "Today's Hits" is a genre
            // station like Pandora's: it can be deleted but not renamed.
            allow_rename: !*shuffle && *name != "Today's Hits",
            allow_delete: !*shuffle,
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

/// Made-up details for a demo station (the real ones come from Pandora).
pub fn station_details(station_id: &str) -> StationDetails {
    let n = station_id.strip_prefix("demo-").and_then(|n| n.parse::<usize>().ok()).unwrap_or(0);
    let seeds = [("Artist", "M83"), ("Song", "Midnight City by M83"), ("Genre", "Synthwave")];
    StationDetails {
        seeds: seeds.iter().take(1 + n % 3).map(|(kind, name)| Seed { kind, name: (*name).into() }).collect(),
        thumbs_up: Some(12 + n as u32),
        thumbs_down: Some(n as u32 % 4),
        genres: vec!["Electronic".into(), "Dance".into()],
        created: "2024-03-14T09:30:00.000-04:00".into(),
        last_played: "2026-10-23T21:05:00.000-04:00".into(),
        creator: String::new(),
    }
}

/// The made-up catalogue the demo search looks in: (id, kind, name, artist).
const CATALOGUE: [(&str, HitKind, &str, &str); 14] = [
    ("AR:demo-m83", HitKind::Artist, "M83", ""),
    ("AR:demo-mgmt", HitKind::Artist, "MGMT", ""),
    ("AR:demo-daft", HitKind::Artist, "Daft Punk", ""),
    ("AR:demo-tame", HitKind::Artist, "Tame Impala", ""),
    ("AR:demo-phoenix", HitKind::Artist, "Phoenix", ""),
    ("AR:demo-fleetwood", HitKind::Artist, "Fleetwood Mac", ""),
    ("AR:demo-chvrches", HitKind::Artist, "CHVRCHES", ""),
    ("TR:demo-midnight", HitKind::Song, "Midnight City", "M83"),
    ("TR:demo-kids", HitKind::Song, "Kids", "MGMT"),
    ("TR:demo-digital", HitKind::Song, "Digital Love", "Daft Punk"),
    ("TR:demo-happen", HitKind::Song, "Let It Happen", "Tame Impala"),
    ("TR:demo-1901", HitKind::Song, "1901", "Phoenix"),
    ("TR:demo-dreams", HitKind::Song, "Dreams", "Fleetwood Mac"),
    ("TR:demo-nightcall", HitKind::Song, "Nightcall", "Kavinsky"),
];

/// Artists and songs whose name or artist has `query` in it.
pub fn search(query: &str) -> Vec<SearchHit> {
    let q = query.trim().to_lowercase();
    CATALOGUE
        .iter()
        .filter(|(_, _, name, by)| {
            !q.is_empty() && (name.to_lowercase().contains(&q) || by.to_lowercase().contains(&q))
        })
        .map(|(id, kind, name, by)| SearchHit { id: (*id).into(), kind: *kind, name: (*name).into(), by: (*by).into() })
        .collect()
}

/// (id, name, description) of a demo genre station.
type DemoGenre = (&'static str, &'static str, &'static str);

const GENRES: [(&str, &[DemoGenre]); 3] = [
    (
        "Pop",
        &[
            ("GE:demo-hits", "Today's Hits", "The biggest songs right now."),
            ("GE:demo-indie", "Indie Pop", "Bedroom dreams and big choruses."),
        ],
    ),
    ("Electronic", &[("GE:demo-synthwave", "Synthwave", "Neon, night drives and analogue synths.")]),
    (
        "Rock",
        &[("GE:demo-classic", "Classic Rock", ""), ("GE:demo-alt90s", "90s Alternative", "Loud guitars, flannel.")],
    ),
];

pub fn genres() -> Vec<GenreCategory> {
    GENRES
        .iter()
        .map(|(name, genres)| GenreCategory {
            name: (*name).into(),
            genres: genres
                .iter()
                .map(|(id, name, description)| Genre {
                    id: (*id).into(),
                    name: (*name).into(),
                    description: (*description).into(),
                })
                .collect(),
        })
        .collect()
}

/// The station Pandora would make from a hit or a genre (`None`: not in the demo catalogue).
pub fn new_station(pandora_id: &str, existing: usize) -> Option<Station> {
    let name = if let Some((_, _, name, _)) = CATALOGUE.iter().find(|(id, ..)| *id == pandora_id) {
        format!("{name} Radio")
    } else {
        GENRES.iter().flat_map(|(_, g)| g.iter()).find(|(id, ..)| *id == pandora_id)?.1.to_owned()
    };
    Some(Station {
        id: format!("demo-new-{existing}"),
        name,
        is_shuffle: false,
        is_shared: false,
        art_url: String::new(),
        allow_rename: true,
        allow_delete: true,
    })
}
