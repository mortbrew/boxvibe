//! `--demo`: made-up stations and songs, no network and no sound, so the
//! interface can be tried (and screenshotted) without a Pandora account.

use std::collections::HashMap;

use crate::pandora::{
    ArtistInfo, AudioFormat, BrowseItem, BrowseSection, FEEDBACK_PAGE, Feedback, FeedbackPage, HitKind, Rating,
    SearchHit, Seed, Song, Station, StationDetails, StationMode, StationModes,
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
                bitrate: 192,
                cover_art_url: format!("https://example.invalid/art/{n}.jpg"),
                detail_url: String::new(),
                artist_music_id: format!("R-demo-{}", artist.to_lowercase().replace(' ', "-")),
                length,
                gain: 0.0,
                rating: if n == 0 { Rating::Up } else { Rating::None },
                feedback_id: if n == 0 { format!("demo-fb-{round}-{i}") } else { String::new() },
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

fn browse_item(name: &str, detail: &str, pandora_id: &str, station_factory: &str) -> BrowseItem {
    BrowseItem {
        name: name.into(),
        detail: detail.into(),
        pandora_id: pandora_id.into(),
        station_factory: station_factory.into(),
        link: None,
    }
}

fn linked_shelf(name: &str, pandora_id: &str, path: &str) -> BrowseItem {
    BrowseItem {
        name: name.into(),
        detail: String::new(),
        pandora_id: pandora_id.into(),
        station_factory: String::new(),
        link: Some(path.into()),
    }
}

fn browse_section(id: &str, title: &str, view_all_path: Option<&str>, items: Vec<BrowseItem>) -> BrowseSection {
    BrowseSection { id: id.into(), title: title.into(), items, view_all_path: view_all_path.map(str::to_owned) }
}

/// A small Browse directory for the demo (same section titles as live Pandora).
///
/// `view_all_path` matches the live teaser's `viewAll.webUrl`. Artists Picked
/// For You has none.
pub fn browse() -> Vec<BrowseSection> {
    vec![
        browse_section(
            "BS:1:3:1",
            "Artists Picked For You",
            None,
            vec![browse_item("M83", "", "AR:demo-m83", "SF:16722:demo-m83")],
        ),
        browse_section(
            "BS:1:8",
            "New Music",
            Some("/browse/music/new-music"),
            vec![browse_item("Midnight City", "M83", "AL:demo-album", "")],
        ),
        browse_section(
            "BS:1:2:1",
            "Browse Genres",
            Some("/browse/music/genres"),
            vec![browse_item("Synthwave", "1.2M Listeners", "GE:demo-synthwave", "SF:18245:demo-synthwave")],
        ),
        browse_section(
            "BS:1:12",
            "Featured Playlists",
            Some("/browse/music/featured-playlists"),
            vec![browse_item("The Drop", "9 songs", "PL:demo:1", "")],
        ),
        browse_section(
            "BS:1:15",
            "Trendsetters: Artists on the rise",
            Some("/browse/music/artists-on-the-rise"),
            vec![browse_item("MGMT", "#1 This Week", "AR:demo-mgmt", "SF:16722:demo-mgmt")],
        ),
        browse_section(
            "BS:1:6:1",
            "Decades",
            Some("/browse/music/decades"),
            vec![browse_item("2010s", "", "GE:demo-2010s", "SF:18245:demo-2010s")],
        ),
        browse_section(
            "BS:1:38",
            "Recommended Podcasts",
            Some("/browse/podcasts"),
            vec![browse_item("Crime Junkie", "Audiochuck", "PC:demo", "")],
        ),
        browse_section(
            "BS:1:13",
            "Top artists",
            Some("/browse/music/top-artists"),
            vec![browse_item("Tame Impala", "30M listeners", "AR:demo-tame", "SF:16722:demo-tame")],
        ),
        browse_section(
            "BS:1:7",
            "Moods and activities",
            Some("/browse/music/activities"),
            vec![browse_item("Happy", "", "BR:demo:happy", "")],
        ),
        browse_section(
            "BS:1:14",
            "Top stations",
            Some("/browse/music/top-stations"),
            vec![browse_item("Today's Hits", "870K listeners", "GE:demo-hits", "SF:18245:demo-hits")],
        ),
    ]
}

/// `None`: the main teaser. `Some(path)`: that section's View-all page.
pub fn browse_at(path: Option<&str>) -> Vec<BrowseSection> {
    let Some(path) = path else { return browse() };
    match path {
        "/browse/music/new-music" => new_music_view_all(),
        "/browse/music/genres" => genres_view_all(),
        "/browse/music/decades" => decades_view_all(),
        "/browse/music/activities" => moods_view_all(),
        "/browse/music/genres/pop" => {
            station_page("GE:pop", "Pop", &[("Today's Hits", "GE:demo-hits"), ("Indie Pop", "GE:demo-indie")])
        }
        "/browse/music/genres/electronic" => {
            station_page("GE:electronic", "Electronic", &[("Synthwave", "GE:demo-synthwave")])
        }
        "/browse/music/genres/rock" => station_page(
            "GE:rock",
            "Rock",
            &[("Classic Rock", "GE:demo-classic"), ("90s Alternative", "GE:demo-alt90s")],
        ),
        "/browse/music/decades/2010s" => station_page("GE:2010s", "2010s", &[("Classic Rock", "GE:demo-classic")]),
        "/browse/music/activities/happy" => station_page("GE:happy", "Happy", &[("Indie Pop", "GE:demo-indie")]),
        "/browse/music/genres/empty" => vec![browse_section("GE:empty", "Empty", None, vec![])],
        "/browse/music/featured-playlists" => featured_playlists_view_all(),
        "/browse/music/featured-playlists/the-drop" => {
            playlist_page("PL:drop", "The Drop", &[("Midnight City", "M83"), ("Kids", "MGMT")])
        }
        "/browse/podcasts" => podcasts_view_all(),
        "/browse/podcasts/crime-junkie" => {
            podcast_page("PC:demo", "Crime Junkie", &[("Latest episode", "This week"), ("Back catalog", "Archive")])
        }
        other => match browse().into_iter().find(|s| s.view_all_path.as_deref() == Some(other)) {
            Some(section) => vec![expand_view_all(section)],
            None => Vec::new(),
        },
    }
}

/// New Music's View-all is several sections, longer than the one-row teaser.
fn new_music_view_all() -> Vec<BrowseSection> {
    // Ids are demo genres so Enter can start them. Live rows are real GE: ids.
    let stations = [
        ("Today's Hits", "GE:demo-hits"),
        ("Indie Pop", "GE:demo-indie"),
        ("Synthwave", "GE:demo-synthwave"),
        ("Classic Rock", "GE:demo-classic"),
        ("90s Alternative", "GE:demo-alt90s"),
    ];
    let recommended = ["Queen", "Tom Petty", "Bryan Adams", "Madonna", "Prince"];
    vec![
        browse_section(
            "BS:8:2",
            "New Music Stations",
            None,
            stations.iter().map(|(name, id)| browse_item(name, "", id, "")).collect(),
        ),
        browse_section(
            "BS:8:40",
            "Recommended new releases",
            None,
            recommended.iter().map(|name| browse_item(name, "Album", "AL:demo-rec", "")).collect(),
        ),
        browse_section(
            "BS:8:61",
            "All New Albums",
            None,
            (0..10)
                .map(|n| browse_item(&format!("New Album {n}"), "This week", &format!("AL:demo-new-{n}"), ""))
                .collect(),
        ),
    ]
}

/// Genres View-all: category shelves. Named rows carry the next page's site path.
/// A shelf without a path does not invent one from its `BR:` id.
fn genres_view_all() -> Vec<BrowseSection> {
    let mut page = category_page(
        "BS:50",
        "Browse Genres",
        "BR:50",
        &[
            ("Pop", "/browse/music/genres/pop"),
            ("Electronic", "/browse/music/genres/electronic"),
            ("Rock", "/browse/music/genres/rock"),
        ],
    );
    page[0].items.insert(0, browse_item("Synthwave", "1.2M Listeners", "GE:demo-synthwave", "SF:18245:demo-synthwave"));
    page
}

fn decades_view_all() -> Vec<BrowseSection> {
    category_page("BS:6", "Browse Decades", "BR:6", &[("2010s", "/browse/music/decades/2010s")])
}

fn moods_view_all() -> Vec<BrowseSection> {
    category_page("BS:7", "Browse Moods & Activities", "BR:7", &[("Happy", "/browse/music/activities/happy")])
}

fn category_page(id: &str, title: &str, id_prefix: &str, named: &[(&str, &str)]) -> Vec<BrowseSection> {
    let mut items: Vec<BrowseItem> = named
        .iter()
        .enumerate()
        .map(|(n, (name, path))| linked_shelf(name, &format!("{id_prefix}:{n}"), path))
        .collect();
    items.push(browse_item("No page", "", &format!("{id_prefix}:none"), ""));
    vec![expand_view_all(browse_section(id, title, None, items))]
}

fn featured_playlists_view_all() -> Vec<BrowseSection> {
    category_page("BS:12", "Featured Playlists", "BR:12", &[("The Drop", "/browse/music/featured-playlists/the-drop")])
}

fn podcasts_view_all() -> Vec<BrowseSection> {
    category_page("BS:38", "Recommended Podcasts", "BR:38", &[("Crime Junkie", "/browse/podcasts/crime-junkie")])
}

fn playlist_page(id: &str, title: &str, tracks: &[(&str, &str)]) -> Vec<BrowseSection> {
    let items = tracks
        .iter()
        .enumerate()
        .map(|(n, (name, artist))| browse_item(name, artist, &format!("AL:pl:{n}"), ""))
        .collect();
    vec![browse_section(id, title, None, items)]
}

fn podcast_page(id: &str, title: &str, episodes: &[(&str, &str)]) -> Vec<BrowseSection> {
    let items = episodes
        .iter()
        .enumerate()
        .map(|(n, (name, detail))| browse_item(name, detail, &format!("PE:pc:{n}"), ""))
        .collect();
    vec![browse_section(id, title, None, items)]
}

fn station_page(id: &str, title: &str, rows: &[(&str, &str)]) -> Vec<BrowseSection> {
    let items = rows
        .iter()
        .map(|(name, pandora_id)| browse_item(name, "", pandora_id, &pandora_id.replacen("GE:", "SF:", 1)))
        .collect();
    vec![browse_section(id, title, None, items)]
}

fn expand_view_all(section: BrowseSection) -> BrowseSection {
    let mut items = section.items.clone();
    let title = section.title.clone();
    items.push(browse_item(&format!("{title} extra"), "View all", "AR:view-all-extra", "SF:16722:view-all-extra"));
    items.push(browse_item(&format!("{title} also"), "View all", "AR:view-all-also", "SF:16722:view-all-also"));
    BrowseSection { items, view_all_path: None, ..section }
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
        allow_rename: !name_is_genre,
        allow_delete: true,
        allow_edit_seeds: !name_is_genre,
    })
}

/// A demo artist page: a short bio and four similar artists from the catalogue.
pub fn artist(name: &str) -> ArtistInfo {
    ArtistInfo {
        name: name.to_owned(),
        bio: format!(
            "{name} is a demo artist: boxvibe made this bio up so the panel can be tried without an account. \
             Real bios come from Pandora's artist pages when it sends one."
        ),
        detail_url: format!("https://www.pandora.com/artist/{}", name.to_lowercase().replace(' ', "-")),
        similar: CATALOGUE
            .iter()
            .filter(|(_, kind, n, _)| *kind == HitKind::Artist && !n.eq_ignore_ascii_case(name))
            .take(4)
            .map(|(id, kind, n, by)| SearchHit { id: (*id).into(), kind: *kind, name: (*n).into(), by: (*by).into() })
            .collect(),
    }
}

/// Pandora's six modes, as the old app API listed them; `current` is on.
pub fn modes(current: i64) -> StationModes {
    let all: [(&str, &str, bool); 6] = [
        ("My Station", "The station experience you know and love.", false),
        ("Crowd Faves", "You'll hear the most Thumbed Up songs by other listeners.", false),
        ("Discovery", "You'll hear more artists who don't usually play on this station.", false),
        ("Deep Cuts", "You'll hear less familiar songs from station artists.", false),
        ("Newly Released", "You'll hear the newest releases from station artists.", false),
        ("Artist Only", "You'll only hear songs from the station artist.", true),
    ];
    StationModes {
        current: Some(current),
        modes: all
            .iter()
            .enumerate()
            .map(|(i, (name, description, premium))| StationMode {
                id: i as i64,
                name: (*name).into(),
                description: (*description).into(),
                available: !premium,
                premium_only: *premium,
            })
            .collect(),
    }
}
