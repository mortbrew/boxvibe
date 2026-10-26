//! `--demo`: made-up stations and songs, no network and no sound, so the
//! interface can be tried (and screenshotted) without a Pandora account.

use std::collections::HashMap;

use crate::pandora::{
    AudioFormat, FEEDBACK_PAGE, Feedback, FeedbackPage, Genre, GenreCategory, HitKind, Rating, SearchHit, Seed, Song,
    Station, StationDetails,
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
            allow_edit_seeds: !*shuffle && *name != "Today's Hits",
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

/// The demo's answer to the `n`-th "is an ad due?": every third time a made-up
/// pod of two short ads (nothing to download or report; the demo plays silently).
pub fn ads(n: usize) -> Vec<crate::ads::Ad> {
    #[allow(clippy::manual_is_multiple_of)] // is_multiple_of needs Rust 1.91; the MSRV is 1.90
    let none = n % 3 != 0;
    if none {
        return Vec::new();
    }
    [("Sparkle Seltzer: taste the demo", 6.0), ("Acme Umbrellas: for every forecast", 4.0)]
        .iter()
        .enumerate()
        .map(|(i, (_, length))| crate::ads::Ad {
            token: format!("pod:1/2/{}", i + 1),
            request_meta: "demo".into(),
            length: *length,
            audio_url: format!("https://ads.example.com/demo/{}.mp4", i + 1),
            events: Vec::new(),
            monocle: String::new(),
        })
        .collect()
}

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

/// The seeds a demo station starts with: (kind, name, pandoraId, musicId).
const SEEDS: [(&str, &str, &str, &str); 3] = [
    ("Artist", "M83", "AR:demo-m83", "R-m83"),
    ("Song", "Midnight City by M83", "TR:demo-midnight", "T-midnight"),
    ("Genre", "Synthwave", "GE:demo-synthwave", ""),
];

fn seed(kind: &'static str, name: &str, id: &str, music_id: &str) -> Seed {
    Seed { kind, name: name.into(), id: id.into(), music_id: music_id.into() }
}

fn station_number(station_id: &str) -> usize {
    station_id.strip_prefix("demo-").and_then(|n| n.parse::<usize>().ok()).unwrap_or(0)
}

/// Made-up details for a demo station (the real ones come from Pandora).
pub fn station_details(station_id: &str) -> StationDetails {
    let n = station_number(station_id);
    StationDetails {
        seeds: SEEDS.iter().take(1 + n % 3).map(|(k, name, id, music)| seed(k, name, id, music)).collect(),
        thumbs_up: Some(thumbs_total(station_id, true) as u32),
        thumbs_down: Some(thumbs_total(station_id, false) as u32),
        genres: vec!["Electronic".into(), "Dance".into()],
        created: "2024-03-14T09:30:00.000-04:00".into(),
        last_played: "2026-10-23T21:05:00.000-04:00".into(),
        creator: String::new(),
    }
}

/// How many thumbs the demo station starts with (the thumbs-up list is more
/// than a page long, so paging can be tried).
fn thumbs_total(station_id: &str, positive: bool) -> usize {
    let n = station_number(station_id);
    if positive { 34 + 3 * n } else { 2 + n % 4 }
}

const THUMB_SONGS: [(&str, &str, &str); 6] = [
    ("Midnight City", "M83", "Hurry Up, We're Dreaming"),
    ("Kids", "MGMT", "Oracular Spectacular"),
    ("Digital Love", "Daft Punk", "Discovery"),
    ("Let It Happen", "Tame Impala", "Currents"),
    ("1901", "Phoenix", "Wolfgang Amadeus Phoenix"),
    ("Dreams", "Fleetwood Mac", "Rumours"),
];

/// The made-up account's seeds and thumbs: what the editor changes in `--demo`.
#[derive(Default)]
pub struct Editing {
    seeds: HashMap<String, Vec<Seed>>,
    thumbs: HashMap<(String, bool), Vec<Feedback>>,
}

impl Editing {
    pub fn seeds(&mut self, station_id: &str) -> &mut Vec<Seed> {
        self.seeds.entry(station_id.to_owned()).or_insert_with(|| station_details(station_id).seeds)
    }

    /// Adds the artist or song `pandora_id` from the catalogue; returns its name.
    pub fn add_seed(&mut self, station_id: &str, pandora_id: &str) -> Result<String, String> {
        let (_, kind, name, by) =
            CATALOGUE.iter().find(|(id, ..)| *id == pandora_id).ok_or("no such artist or song")?;
        let seeds = self.seeds(station_id);
        if seeds.iter().any(|s| s.id == pandora_id) {
            return Err("that is already a seed of this station".into());
        }
        let (kind, label) = match kind {
            HitKind::Artist => ("Artist", (*name).to_owned()),
            HitKind::Song => ("Song", format!("{name} by {by}")),
        };
        // Like Pandora: the new seed comes first.
        seeds.insert(0, seed(kind, &label, pandora_id, &pandora_id.replace("AR:", "R-")));
        Ok(label)
    }

    pub fn remove_seed(&mut self, station_id: &str, pandora_id: &str) -> Result<String, String> {
        let seeds = self.seeds(station_id);
        let at = seeds.iter().position(|s| s.id == pandora_id).ok_or("that seed is not on this station")?;
        if seeds.len() == 1 {
            return Err("a station needs at least one seed".into());
        }
        Ok(seeds.remove(at).name)
    }

    /// Artists to suggest: the catalogue's, minus what the station has.
    pub fn suggestions(&mut self, station_id: &str) -> Vec<SearchHit> {
        let have: Vec<String> = self.seeds(station_id).iter().map(|s| s.id.clone()).collect();
        CATALOGUE
            .iter()
            .filter(|(id, kind, ..)| *kind == HitKind::Artist && !have.iter().any(|h| h == id))
            .take(6)
            .map(|(id, kind, name, by)| SearchHit {
                id: (*id).into(),
                kind: *kind,
                name: (*name).into(),
                by: (*by).into(),
            })
            .collect()
    }

    fn thumbs(&mut self, station_id: &str, positive: bool) -> &mut Vec<Feedback> {
        self.thumbs.entry((station_id.to_owned(), positive)).or_insert_with(|| {
            (0..thumbs_total(station_id, positive))
                .map(|i| {
                    let (title, artist, album) = THUMB_SONGS[i % THUMB_SONGS.len()];
                    Feedback {
                        id: format!("demo-{station_id}-{positive}-{i}"),
                        positive,
                        title: title.into(),
                        artist: artist.into(),
                        album: album.into(),
                        date: format!("2026-10-{:02}T20:15:00.000-04:00", 24 - (i % 20)),
                    }
                })
                .collect()
        })
    }

    pub fn feedback(&mut self, station_id: &str, positive: bool, start: usize) -> FeedbackPage {
        let all = self.thumbs(station_id, positive);
        FeedbackPage { total: all.len(), items: all.iter().skip(start).take(FEEDBACK_PAGE).cloned().collect() }
    }

    pub fn delete_feedback(&mut self, station_id: &str, feedback_id: &str, positive: bool) -> Result<(), String> {
        let all = self.thumbs(station_id, positive);
        let at = all.iter().position(|f| f.id == feedback_id).ok_or("that thumb is already gone")?;
        all.remove(at);
        Ok(())
    }

    /// The counts of `station_details`, after the edits.
    pub fn details(&mut self, station_id: &str) -> StationDetails {
        let mut details = station_details(station_id);
        details.seeds = self.seeds(station_id).clone();
        details.thumbs_up = Some(self.thumbs(station_id, true).len() as u32);
        details.thumbs_down = Some(self.thumbs(station_id, false).len() as u32);
        details
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
    let (name, name_is_genre) = if let Some((_, _, name, _)) = CATALOGUE.iter().find(|(id, ..)| *id == pandora_id) {
        (format!("{name} Radio"), false)
    } else {
        (GENRES.iter().flat_map(|(_, g)| g.iter()).find(|(id, ..)| *id == pandora_id)?.1.to_owned(), true)
    };
    Some(Station {
        id: format!("demo-new-{existing}"),
        name,
        is_shuffle: false,
        is_shared: false,
        art_url: String::new(),
        allow_rename: !name_is_genre,
        allow_delete: true,
        allow_edit_seeds: !name_is_genre,
    })
}
