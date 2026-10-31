//! Parsing tests on hand-written answers shaped like the docs and the live
//! site, and a fake Pandora on localhost for the login, retry and takeover
//! logic (no real account needed).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use serde_json::json;

use super::*;

#[test]
fn quality_remaps_old_values() {
    assert_eq!(Quality::parse("high"), Quality::High);
    assert_eq!(Quality::parse("standard"), Quality::Standard);
    assert_eq!(Quality::parse("low"), Quality::Standard, "old HE-AAC value");
    assert_eq!(Quality::parse("Medium"), Quality::Standard, "old HE-AAC value");
    assert_eq!(Quality::parse("nonsense"), Quality::High);
    assert_eq!(Quality::High.audio_format(true), "mp3-hifi");
    assert_eq!(Quality::High.audio_format(false), "mp3", "no hifi on this plan");
    assert_eq!(Quality::Standard.audio_format(true), "mp3");
}

#[test]
fn stations_parse_with_the_biggest_art() {
    let v = json!({
        "stationId": "12345", "name": "Chill Out Radio", "isShuffle": false, "isShared": true,
        "art": [ { "url": "https://a/90.jpg", "size": 90 }, { "url": "https://a/1080.jpg", "size": 1080 },
                 { "url": "https://a/500.jpg", "size": 500 } ]
    });
    let s = parse_station(&v).unwrap();
    assert_eq!((s.id.as_str(), s.name.as_str(), s.is_shared), ("12345", "Chill Out Radio", true));
    assert_eq!(s.art_url, "https://a/1080.jpg");
    assert_eq!(parse_station(&json!({ "name": "no id" })), None);
    let n = parse_station(&json!({ "stationId": 42, "name": "Shuffle", "isShuffle": true })).unwrap();
    assert_eq!((n.id.as_str(), n.is_shuffle, n.art_url.as_str()), ("42", true, ""));
}

fn track(title: &str, kind: &str) -> Value {
    json!({
        "songTitle": title, "artistName": "Niall Horan", "albumTitle": "Slow Hands (Single)",
        "trackType": kind, "trackToken": format!("tok-{title}"), "stationId": "st", "trackLength": 188,
        "fileGain": "-3.75", "rating": 0, "audioEncoding": "mp3-hifi",
        "audioURL": "https://audio.example/access/1?version=5&token=x",
        "songDetailURL": "https://www.pandora.com/artist/x",
        "albumArt": [ { "url": "https://art/130.jpg", "size": 130 }, { "url": "https://art/640.jpg", "size": 640 } ]
    })
}

#[test]
fn fragment_tracks_parse_and_non_songs_are_dropped() {
    let s = parse_track(&track("Slow Hands", "Track"), "fallback").unwrap();
    assert_eq!((s.title.as_str(), s.album.as_str(), s.length), ("Slow Hands", "Slow Hands (Single)", 188));
    assert_eq!((s.track_token.as_str(), s.station_id.as_str()), ("tok-Slow Hands", "st"));
    assert_eq!((s.audio_format, s.cover_art_url.as_str()), (AudioFormat::Mp3, "https://art/640.jpg"));
    let sizes = json!([{ "url": "u90", "size": 90 }, { "url": "u1080", "size": 1080 }, { "url": "u500", "size": 500 }, { "url": "u640", "size": 640 }]);
    assert_eq!(art_for_box(Some(&sizes)), "u500", "the smallest at least 500 px");
    assert_eq!(art_for_box(Some(&json!([{ "url": "u130", "size": 130 }]))), "u130", "else the largest");
    assert_eq!(art_for_box(None), "");
    assert!((s.gain + 3.75).abs() < 1e-4);
    assert_eq!(s.rating, Rating::None);

    assert_eq!(parse_track(&track("An ad", "Ad"), "st"), None, "ads");
    assert_eq!(parse_track(&track("Hello", "ArtistMessage"), "st"), None, "artist messages");
    assert_eq!(parse_track(&track("Curator Message: summer", "Track"), "st"), None, "curator messages");
    let mut no_audio = track("No audio", "Track");
    no_audio["audioURL"] = json!("");
    assert_eq!(parse_track(&no_audio, "st"), None);

    let mut liked = track("Liked", "Track");
    liked["rating"] = json!(1);
    liked["audioEncoding"] = json!("aacplus");
    liked.as_object_mut().unwrap().remove("stationId");
    let l = parse_track(&liked, "fallback").unwrap();
    assert_eq!((l.rating, l.audio_format, l.station_id.as_str()), (Rating::Up, AudioFormat::Aac, "fallback"));
}

#[test]
fn errors_are_classified() {
    // Answers seen live from www.pandora.com (Oct 2026).
    let expired = classify(401, r#"{"message":"Invalid Auth Token","errorCode":1001,"errorString":"INVALID_REQUEST"}"#);
    assert!(expired.is_auth_expired());
    let blocked = classify(
        403,
        r#"{"message":"Invalid request","errorCode":1215,"errorString":"s2s_high_score","appId":"PXabc","blockScript":"/x/captcha"}"#,
    );
    assert_eq!(blocked, Error::BotCheck(403));
    assert!(!blocked.is_auth_expired());
    // From the docs and other clients.
    let wrong = r#"{"message":"Invalid username and/or password","errorCode":0,"errorString":"AUTH_INVALID_USERNAME_PASSWORD"}"#;
    assert_eq!(classify(400, wrong), Error::WrongLogin);
    assert_eq!(classify(429, r#"{"errorString":"STREAM_VIOLATION","errorCode":0}"#), Error::StreamViolation);
    assert_eq!(classify(400, r#"{"errorString":"COUNTRY_CODE_INVALID","errorCode":1028}"#), Error::Country);
    let other = classify(500, "<html>oops</html>");
    assert_eq!(other.to_string(), "Pandora answered HTTP 500");
}

#[test]
fn csrf_cookie_is_found() {
    assert_eq!(
        csrf_from_cookie("csrftoken=dded938ac0c6091b;Path=/;Domain=.pandora.com;Secure").as_deref(),
        Some("dded938ac0c6091b")
    );
    assert_eq!(csrf_from_cookie("v2regbstage=;Path=/"), None);
    assert_eq!(made_up_csrf().len(), 16);
}

// --- A fake Pandora ---

#[derive(Debug, Clone)]
struct Seen {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Value,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

type Handler = Box<dyn FnMut(&Seen) -> (u16, String) + Send>;

/// Serves `handler` on localhost; answers every connection once and closes it.
fn fake_pandora(mut handler: Handler) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (method, path) = (parts.next().unwrap_or("").to_owned(), parts.next().unwrap_or("").to_owned());
            let mut headers = Vec::new();
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                let h = h.trim_end();
                if h.is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    headers.push((k.trim().to_owned(), v.trim().to_owned()));
                }
            }
            let length = headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, v)| v.parse().ok())
                .unwrap_or(0);
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let request = Seen { method, path, headers, body: serde_json::from_slice(&body).unwrap_or(Value::Null) };
            let (status, text) = handler(&request);
            log.lock().unwrap().push(request.clone());
            let extra =
                if request.method == "HEAD" { "Set-Cookie: csrftoken=fakecsrf123;Path=/;Secure\r\n" } else { "" };
            let body = if request.method == "HEAD" { String::new() } else { text };
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (base, seen)
}

fn login_ok(token: &str) -> (u16, String) {
    let v = json!({ "authToken": token, "listenerId": "1", "webname": "me",
                    "config": { "branding": "PandoraPlus", "flags": ["noAds", HIFI_FLAG] } });
    (200, v.to_string())
}

#[test]
fn login_sends_the_site_headers_and_reads_the_flags() {
    let (base, seen) = fake_pandora(Box::new(|r| match r.path.as_str() {
        "/api/v1/auth/login" => login_ok("tok1"),
        _ => (200, "{}".into()),
    }));
    let mut c = Client::with_base(&base, "");
    c.login("me@example.com", "pw").unwrap();
    assert!(c.account.hifi());
    assert_eq!(c.account.branding, "PandoraPlus");
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "HEAD", "csrftoken first");
    let login = &seen[1];
    assert_eq!(login.header("X-CsrfToken"), Some("fakecsrf123"));
    assert_eq!(login.header("Cookie"), Some("csrftoken=fakecsrf123"));
    assert_eq!(login.header("User-Agent"), Some(DEFAULT_USER_AGENT));
    assert_eq!(login.header("Origin"), Some("https://www.pandora.com"));
    assert_eq!(login.header("X-AuthToken"), None);
    assert_eq!(
        login.body,
        json!({ "username": "me@example.com", "password": "pw", "keepLoggedIn": true, "existingAuthToken": null })
    );
}

#[test]
fn an_expired_login_logs_in_again_once() {
    let mut logins = 0;
    let (base, seen) = fake_pandora(Box::new(move |r| match r.path.as_str() {
        "/api/v1/auth/login" => {
            logins += 1;
            login_ok(&format!("tok{logins}"))
        }
        "/api/v1/station/getStations" if r.header("X-AuthToken") == Some("tok1") => {
            (401, r#"{"message":"Invalid Auth Token","errorCode":1001,"errorString":"INVALID_REQUEST"}"#.into())
        }
        "/api/v1/station/getStations" => {
            (200, json!({ "totalStations": 1, "stations": [ { "stationId": "9", "name": "Jazz" } ] }).to_string())
        }
        _ => (200, "{}".into()),
    }));
    let mut c = Client::with_base(&base, "");
    let login = ("me".to_owned(), "pw".to_owned());
    c.login(&login.0, &login.1).unwrap();
    let mut took_over = false;
    let stations = with_retries(&mut c, Some(&login), &mut took_over, |c| c.stations()).unwrap();
    assert_eq!(stations.len(), 1);
    assert!(!took_over);
    let paths: Vec<String> = seen.lock().unwrap().iter().map(|s| s.path.clone()).collect();
    assert_eq!(paths.iter().filter(|p| p.ends_with("auth/login")).count(), 2);
}

#[test]
fn a_login_that_keeps_expiring_gives_up() {
    let (base, _) = fake_pandora(Box::new(|r| match r.path.as_str() {
        "/api/v1/auth/login" => login_ok("tok"),
        _ => (401, r#"{"errorCode":1001,"errorString":"INVALID_REQUEST","message":"Invalid Auth Token"}"#.into()),
    }));
    let mut c = Client::with_base(&base, "");
    let login = ("me".to_owned(), "pw".to_owned());
    c.login(&login.0, &login.1).unwrap();
    let err = with_retries(&mut c, Some(&login), &mut false, |c| c.stations()).unwrap_err();
    assert!(err.is_auth_expired());
}

#[test]
fn another_device_streaming_is_taken_over_once() {
    let mut taken = false;
    let (base, seen) = fake_pandora(Box::new(move |r| match r.path.as_str() {
        "/api/v1/auth/login" => login_ok("tok"),
        "/api/v1/station/playbackResumed" => {
            taken = r.body == json!({ "forceActive": true });
            (200, "{}".into())
        }
        "/api/v1/playlist/getFragment" if !taken => (429, r#"{"errorString":"STREAM_VIOLATION","errorCode":0}"#.into()),
        "/api/v1/playlist/getFragment" => {
            (200, json!({ "tracks": [ track("Song", "Track"), track("Ad", "Ad") ] }).to_string())
        }
        _ => (200, "{}".into()),
    }));
    let mut c = Client::with_base(&base, "");
    let login = ("me".to_owned(), "pw".to_owned());
    c.login(&login.0, &login.1).unwrap();
    let mut took_over = false;
    let songs = with_retries(&mut c, Some(&login), &mut took_over, |c| c.fragment("st")).unwrap();
    assert_eq!(songs.len(), 1, "the ad is dropped");
    assert!(took_over);
    let seen = seen.lock().unwrap();
    let fragments: Vec<&Seen> = seen.iter().filter(|s| s.path.ends_with("getFragment")).collect();
    assert_eq!(fragments[0].body["audioFormat"], "mp3-hifi", "the plan allows it");
    assert_eq!(fragments[0].body["isStationStart"], true);
    assert_eq!(fragments[1].header("X-AuthToken"), Some("tok"));
}

#[test]
fn later_fragments_are_not_station_starts_and_plain_plans_get_mp3() {
    let (base, seen) = fake_pandora(Box::new(|r| match r.path.as_str() {
        "/api/v1/auth/login" => (200, json!({ "authToken": "t", "config": { "flags": [] } }).to_string()),
        _ => (200, json!({ "tracks": [] }).to_string()),
    }));
    let mut c = Client::with_base(&base, "");
    c.login("me", "pw").unwrap();
    c.fragment("st").unwrap();
    c.fragment("st").unwrap();
    let seen = seen.lock().unwrap();
    let f: Vec<&Seen> = seen.iter().filter(|s| s.path.ends_with("getFragment")).collect();
    assert_eq!((f[0].body["isStationStart"].clone(), f[1].body["isStationStart"].clone()), (json!(true), json!(false)));
    assert_eq!(f[0].body["audioFormat"], "mp3");
}

#[test]
fn a_blocked_login_says_so() {
    let (base, _) = fake_pandora(Box::new(|_| {
        (403, r#"{"message":"Invalid request","errorCode":1215,"errorString":"s2s_high_score","appId":"PXx","blockScript":"/c"}"#.into())
    }));
    let mut c = Client::with_base(&base, "");
    let err = c.login("me", "pw").unwrap_err();
    assert_eq!(err, Error::BotCheck(403));
    assert!(err.to_string().contains("bot check"));
}

#[test]
fn stations_say_whether_they_can_be_renamed_or_removed() {
    let made =
        parse_station(&json!({ "stationId": "1", "name": "Mine", "allowRename": true, "allowDelete": true })).unwrap();
    assert!(made.allow_rename && made.allow_delete);
    let genre = parse_station(&json!({ "stationId": "2", "name": "Chill", "allowRename": false, "allowDelete": true }))
        .unwrap();
    assert!(!genre.allow_rename && genre.allow_delete);
    // Nothing said: allowed, except for Shuffle, which Pandora never lets go.
    let plain = parse_station(&json!({ "stationId": "3", "name": "Jazz" })).unwrap();
    assert!(plain.allow_rename && plain.allow_delete);
    let shuffle = parse_station(&json!({ "stationId": "4", "name": "Shuffle", "isShuffle": true })).unwrap();
    assert!(!shuffle.allow_rename && !shuffle.allow_delete);
}

#[test]
fn station_details_read_seeds_thumbs_and_dates() {
    let v = json!({
        "seeds": [
            { "musicId": "R1", "artist": { "artistName": "Tee Grizzley" } },
            { "musicId": "S1", "song": { "songName": "Midnight City", "artistSummary": "M83" } },
            { "musicId": "G1", "genre": { "stationName": "Chill Out" } },
            { "musicId": "?", "mystery": {} }
        ],
        "positiveFeedbackCount": 12, "negativeFeedbackCount": 3,
        "genre": ["Rap / Hip-Hop", ""], "dateCreated": "2017-12-04T16:26:05.041-08:00",
        "lastPlayed": "2017-12-05T10:00:00.000-08:00", "creatorWebname": "toby"
    });
    let d = parse_station_details(&v);
    let seeds: Vec<(&str, &str)> = d.seeds.iter().map(|s| (s.kind, s.name.as_str())).collect();
    assert_eq!(seeds, [("Artist", "Tee Grizzley"), ("Song", "Midnight City by M83"), ("Genre", "Chill Out")]);
    assert_eq!((d.thumbs_up, d.thumbs_down), (Some(12), Some(3)));
    assert_eq!(d.genres, ["Rap / Hip-Hop"]);
    assert_eq!((d.created.as_str(), d.creator.as_str()), ("2017-12-04T16:26:05.041-08:00", "toby"));
    // An answer with nothing in it shows nothing, rather than made-up zeros.
    assert_eq!(parse_station_details(&json!({})), StationDetails::default());
}

#[test]
fn renaming_removing_and_details_send_what_the_site_expects() {
    let (base, seen) = fake_pandora(Box::new(|r| match r.path.as_str() {
        "/api/v1/auth/login" => login_ok("tok"),
        "/api/v1/station/updateStation" => (200, json!({ "stationId": "9", "name": "Jazz Radio" }).to_string()),
        // The site may say nothing at all when a station is gone.
        "/api/v1/station/removeStation" => (200, String::new()),
        "/api/v1/station/getStationDetails" => {
            (200, json!({ "seeds": [], "positiveFeedbackCount": 4, "negativeFeedbackCount": 1 }).to_string())
        }
        _ => (200, "{}".into()),
    }));
    let mut c = Client::with_base(&base, "");
    c.login("me", "pw").unwrap();
    assert_eq!(c.rename_station("9", "Jazz").unwrap(), "Jazz Radio", "the name the site now has");
    c.remove_station("9").unwrap();
    let d = c.station_details("9", true).unwrap();
    assert_eq!((d.thumbs_up, d.thumbs_down), (Some(4), Some(1)));
    let seen = seen.lock().unwrap();
    let body = |end: &str| seen.iter().find(|s| s.path.ends_with(end)).unwrap().body.clone();
    assert_eq!(body("updateStation"), json!({ "stationId": "9", "name": "Jazz" }));
    assert_eq!(body("removeStation"), json!({ "stationId": "9" }));
    assert_eq!(body("getStationDetails"), json!({ "stationId": "9", "isCurrentStation": true }));
}

fn search_answer() -> Value {
    json!({
        "results": ["AR:1", "TR:2", "AL:3", "CO:4", "PL:5", "AR:gone", "TR:6"],
        "annotations": {
            "AR:1": { "name": " Tee Grizzley ", "pandoraId": "AR:1", "trackCount": 9, "icon": {} },
            "TR:2": { "name": "Slow Hands", "artistName": "Niall Horan", "pandoraId": "TR:2" },
            "AL:3": { "name": "An album", "pandoraId": "AL:3" },
            "CO:4": { "name": "A Composer", "pandoraId": "CO:4" },
            "PL:5": { "name": "A playlist", "pandoraId": "PL:5" },
            "TR:6": { "name": "", "pandoraId": "TR:6" }
        }
    })
}

#[test]
fn search_keeps_artists_and_songs_in_order_and_ignores_the_rest() {
    let hits = parse_search(&search_answer());
    let got: Vec<(HitKind, &str, &str, &str)> =
        hits.iter().map(|h| (h.kind, h.id.as_str(), h.name.as_str(), h.by.as_str())).collect();
    assert_eq!(
        got,
        vec![
            (HitKind::Artist, "AR:1", "Tee Grizzley", ""),
            (HitKind::Song, "TR:2", "Slow Hands", "Niall Horan"),
            (HitKind::Artist, "CO:4", "A Composer", ""),
        ]
    );
    assert!(parse_search(&json!({})).is_empty());
    assert!(parse_search(&json!({ "results": ["AR:1"] })).is_empty(), "no annotations");
    assert!(parse_search(&json!({ "results": "nope", "annotations": 4 })).is_empty());
}

#[test]
fn genres_parse_tolerantly() {
    let cats = parse_genre_categories(&json!({ "categories": [
        { "name": " Rock ", "token": "rock", "art": [] }, { "name": "No token" }, { "token": "x" } ] }));
    assert_eq!(cats, vec![("Rock".to_owned(), "rock".to_owned())]);
    let genres = parse_genres(&json!({ "genres": [
        { "name": "Classic Rock ", "token": "classic-rock", "musicId": "G1", "pandoraId": "GE:1",
          "description": " Big guitars. ", "listenerCount": 5, "unknown": [1] },
        { "name": "No id" }, { "pandoraId": "GE:9" } ] }));
    assert_eq!(
        genres,
        vec![Genre { id: "GE:1".into(), name: "Classic Rock".into(), description: "Big guitars.".into() }]
    );
    assert!(parse_genres(&json!({ "genres": 3 })).is_empty());
}

#[test]
fn searching_creating_and_genres_send_what_the_site_expects() {
    let (base, seen) = fake_pandora(Box::new(|r| match r.path.as_str() {
        "/api/v1/auth/login" => login_ok("tok"),
        "/api/v3/sod/search" => (200, search_answer().to_string()),
        "/api/v1/station/createStation" => (
            200,
            json!({ "stationId": "3815", "pandoraId": "ST:0:3815", "name": "Tee Grizzley Radio",
                    "allowRename": true, "allowDelete": true, "isShuffle": false })
            .to_string(),
        ),
        "/api/v1/music/genrecategories" => (
            200,
            json!({ "categories": [ { "name": "Rock", "token": "rock" }, { "name": "Empty", "token": "none" },
                                    { "name": "Broken", "token": "bad" } ] })
            .to_string(),
        ),
        "/api/v1/music/genres" => match r.body["categoryToken"].as_str() {
            Some("rock") => (200, json!({ "genres": [ { "name": "Classic Rock", "pandoraId": "GE:1" } ] }).to_string()),
            Some("bad") => (500, json!({ "errorCode": 0, "errorString": "INTERNAL" }).to_string()),
            _ => (200, json!({ "genres": [] }).to_string()),
        },
        _ => (200, "{}".into()),
    }));
    let mut c = Client::with_base(&base, "");
    c.login("me", "pw").unwrap();
    assert_eq!(c.search("tee").unwrap().len(), 3);
    let made = c.create_station("AR:1").unwrap();
    assert_eq!((made.id.as_str(), made.name.as_str()), ("3815", "Tee Grizzley Radio"));
    let cats = c.genres().unwrap();
    assert_eq!(cats.len(), 1, "empty and failing categories are left out");
    assert_eq!((cats[0].name.as_str(), cats[0].genres[0].id.as_str()), ("Rock", "GE:1"));
    let seen = seen.lock().unwrap();
    let body = |end: &str| seen.iter().find(|s| s.path.ends_with(end)).unwrap().body.clone();
    assert_eq!(
        body("sod/search"),
        json!({ "query": "tee", "start": 0, "count": 20, "annotate": true, "types": ["AR", "CO", "TR"] })
    );
    assert_eq!(body("createStation"), json!({ "pandoraId": "AR:1", "stationName": "" }));
    assert_eq!(body("genrecategories"), json!({}));
}

#[test]
fn creating_a_station_needs_an_answer_that_names_it_and_all_genres_failing_is_an_error() {
    let (base, _) = fake_pandora(Box::new(|r| match r.path.as_str() {
        "/api/v1/auth/login" => login_ok("tok"),
        "/api/v1/music/genrecategories" => {
            (200, json!({ "categories": [ { "name": "A", "token": "a" } ] }).to_string())
        }
        "/api/v1/music/genres" => (500, json!({ "errorCode": 0, "errorString": "INTERNAL" }).to_string()),
        _ => (200, "{}".into()),
    }));
    let mut c = Client::with_base(&base, "");
    c.login("me", "pw").unwrap();
    assert_eq!(c.create_station("AR:1"), Err(Error::InvalidResponse));
    assert!(c.genres().is_err());
}

/// `cargo test live_site -- --ignored`: the real www.pandora.com hands out a
/// csrftoken to boxvibe's HEAD (no login, nothing else sent).
#[test]
#[ignore]
fn live_site_hands_out_a_csrftoken() {
    let token = Client::new("").site_csrf().expect("no csrftoken cookie from www.pandora.com");
    assert!(token.len() >= 8 && token.chars().all(|ch| ch.is_ascii_alphanumeric()), "{token}");
}

// --- Seeds and thumbs history ---

#[test]
fn seeds_keep_their_ids_and_flags_say_whether_they_can_be_edited() {
    let v = json!({ "seeds": [
        { "pandoraId": "AR:1", "musicId": "R1", "artist": { "artistName": "M83" } },
        { "pandoraId": "TR:2", "musicId": "S2", "song": { "songName": "Kids", "artistSummary": "MGMT" } },
    ] });
    let seeds = parse_seeds(&v);
    assert_eq!((seeds[0].id.as_str(), seeds[0].music_id.as_str()), ("AR:1", "R1"));
    assert_eq!(seeds[1].name, "Kids by MGMT");
    // allowAddSeed says it; without it, a station you can rename can be edited too.
    let station = |extra: Value| {
        let mut v = json!({ "stationId": "1", "name": "X", "allowRename": true, "allowDelete": true });
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        parse_station(&v).unwrap().allow_edit_seeds
    };
    assert!(station(json!({})));
    assert!(!station(json!({ "allowAddSeed": false })));
    assert!(station(json!({ "allowAddSeed": true, "allowRename": false })));
    assert!(!station(json!({ "allowRename": false })), "genre stations are locked");
}

#[test]
fn seed_suggestions_and_feedback_pages_parse_tolerantly() {
    let hits = parse_seed_suggestions(&json!([{ "name": "MGMT", "pandoraId": "AR:9" }, { "name": "No id" }]));
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].id.as_str(), hits[0].kind), ("AR:9", HitKind::Artist));
    let page = parse_feedback(&json!({ "total": 3, "feedback": [
        { "feedbackId": "f1", "isPositive": true, "songTitle": " Kids ", "artistName": "MGMT",
          "albumTitle": "Oracular", "feedbackDateCreated": "2026-10-01T10:00:00.000-04:00" },
        { "songTitle": "No id: can't be deleted" },
    ] }));
    assert_eq!(page.total, 3);
    assert_eq!(page.items.len(), 1);
    assert_eq!((page.items[0].title.as_str(), page.items[0].id.as_str()), ("Kids", "f1"));
    assert_eq!(parse_feedback(&json!({})), FeedbackPage::default());
}

#[test]
fn seed_and_thumb_calls_send_what_the_site_expects() {
    let (base, seen) = fake_pandora(Box::new(|r| {
        match r.path.as_str() {
        "/api/v1/auth/login" => login_ok("tok"),
        "/api/v1/station/getSeeds" => {
            (200, json!({ "seeds": [{ "pandoraId": "AR:1", "artist": { "artistName": "M83" } }] }).to_string())
        }
        "/api/v1/search/getSeedSuggestions" => (200, json!([{ "name": "MGMT", "pandoraId": "AR:9" }]).to_string()),
        // Pandora's answer to a path it doesn't have (what the old suggestions path got).
        "/api/v1/station/getSeedSuggestions" => {
            (404, json!({ "errorCode": 0, "errorString": "NOT_FOUND", "message": "Service method not found" }).to_string())
        }
        // The first graphql path is unknown to this fake: the second works.
        "/api/v1/graphql" => (404, json!({ "errorCode": 0, "errorString": "NOT_FOUND" }).to_string()),
        "/api/v1/graphql/graphql" => {
            let field = if r.body["operationName"] == "AddSeedToStationWeb" { "addStationSeed" } else { "removeStationSeed" };
            (200, json!({ "data": { field: { "status": "OK" } } }).to_string())
        }
        "/api/v1/station/getStationFeedback" => (
            200,
            json!({ "total": 31, "feedback": [{ "feedbackId": "f1", "isPositive": r.body["positive"], "songTitle": "Kids" }] })
                .to_string(),
        ),
        _ => (200, "{}".into()),
    }
    }));
    let mut c = Client::with_base(&base, "");
    c.login("me", "pw").unwrap();
    assert_eq!(c.seeds("9").unwrap().len(), 1);
    assert_eq!(c.seed_suggestions("9", "R1").unwrap()[0].name, "MGMT");
    c.add_seed("9", "AR:9").unwrap();
    c.remove_seed("9", "AR:1").unwrap();
    let up = c.station_feedback("9", true, 0).unwrap();
    let down = c.station_feedback("9", false, 30).unwrap();
    assert_eq!((up.total, up.items[0].positive, down.items[0].positive), (31, true, false));
    c.delete_feedback("f1", true).unwrap();
    let seen = seen.lock().unwrap();
    let all = |end: &str| seen.iter().filter(|s| s.path.ends_with(end)).map(|s| s.body.clone()).collect::<Vec<_>>();
    assert_eq!(all("getSeeds")[0], json!({ "stationId": "9" }));
    assert_eq!(all("getSeedSuggestions")[0]["stationId"], "9");
    // Each change tried the first graphql path, then the second (the site had two).
    let graphql: Vec<&str> = seen.iter().filter(|s| s.path.contains("graphql")).map(|s| s.path.as_str()).collect();
    assert_eq!(graphql, ["/api/v1/graphql", "/api/v1/graphql/graphql", "/api/v1/graphql", "/api/v1/graphql/graphql"]);
    let add = &all("graphql/graphql")[0];
    assert_eq!(add["operationName"], "AddSeedToStationWeb");
    assert!(add["query"].as_str().unwrap().contains("addStationSeed"));
    // The site wants the variables as a string, and the station as ST:0:<id>.
    let vars: Value = serde_json::from_str(add["variables"].as_str().unwrap()).unwrap();
    assert_eq!(vars, json!({ "stationId": "ST:0:9", "seedId": "AR:9" }));
    let remove = &all("graphql/graphql")[1];
    assert_eq!(remove["operationName"], "DeleteSeedFromStationWeb");
    let vars: Value = serde_json::from_str(remove["variables"].as_str().unwrap()).unwrap();
    assert_eq!(vars, json!({ "stationId": "ST:0:9", "seedId": "AR:1" }));
    assert_eq!(
        all("getStationFeedback"),
        [
            json!({ "pageSize": 30, "startIndex": 0, "stationId": "9", "positive": true }),
            json!({ "pageSize": 30, "startIndex": 30, "stationId": "9", "positive": false }),
        ]
    );
    assert_eq!(all("deleteFeedback")[0], json!({ "feedbackId": "f1", "isPositive": true }));
}

#[test]
fn a_refused_seed_change_is_an_error_not_a_success() {
    let answers = Arc::new(Mutex::new(vec![
        json!({ "errors": [{ "message": "Seed is already in the station" }] }),
        json!({ "data": { "addStationSeed": { "status": "FAILED" } } }),
        json!({ "data": null }),
    ]));
    let (base, _) = fake_pandora(Box::new(move |r| match r.path.as_str() {
        "/api/v1/graphql" => (200, answers.lock().unwrap().remove(0).to_string()),
        "/api/v1/auth/login" => login_ok("tok"),
        _ => (200, "{}".into()),
    }));
    let mut c = Client::with_base(&base, "");
    c.login("me", "pw").unwrap();
    let first = c.add_seed("9", "AR:1").unwrap_err().to_string();
    assert!(first.contains("already in the station"), "{first}");
    let second = c.add_seed("9", "AR:1").unwrap_err().to_string();
    assert!(second.contains("FAILED"), "{second}");
    assert!(c.add_seed("9", "AR:1").is_err(), "an answer with no status is not OK");
}

/// A two-ad pod whose report links point back at the fake server (`host`).
fn fake_pod(host: &str) -> Value {
    let ad = |n: u32, length: f64| {
        let url = |what: &str, query: &str| format!("http://{host}/haymaker/{what}/{n}{query}");
        json!({
            "adToken": format!("pod:1/2/{n}"), "adRequestMetaData": format!("meta-{n}"),
            "adType": "AUDIO_AD", "adLength": length,
            "audioUrlMap": { "mediumQuality": format!("http://{host}/audio/{n}.mp4") },
            "adVerificationData": [{ "vendor": "pandora-monocle", "parameters": format!("m={n}") }],
            "vastTrackingEvents": [
                { "eventType": "IMPRESSION", "urls": [url("imp", "?ts=[TIMESTAMP]")] },
                { "eventType": "AUDIO_START", "urls": [url("start", "?ps=[PLAYERSTATE]&ph=[ADPLAYHEAD]")] },
                { "eventType": "AUDIO_FIRST_QUARTILE", "urls": [url("q1", "?ph=[ADPLAYHEAD]&uri=[ASSETURI]")] },
                { "eventType": "AUDIO_MIDPOINT", "urls": [url("mid", "?ph=[ADPLAYHEAD]")] },
                { "eventType": "AUDIO_THIRD_QUARTILE", "urls": [url("q3", "?ph=[ADPLAYHEAD]")] },
                { "eventType": "AUDIO_COMPLETE", "urls": [url("done", "?ph=[ADPLAYHEAD]")] },
            ],
        })
    };
    json!({ "ads": [ad(1, 8.0), ad(2, 4.0)] })
}

#[test]
fn a_two_ad_pod_is_asked_played_and_reported_in_the_websites_order() {
    use crate::ads::{Action, Break, Tracker, parse_ad_list};
    use std::time::{Duration, SystemTime};
    let (base, seen) = fake_pandora(Box::new(|r| match r.path.as_str() {
        "/api/v1/auth/login" => login_ok("tok"),
        "/api/v1/ad/getAdList" => (200, fake_pod(r.header("Host").unwrap()).to_string()),
        _ => (200, "{}".into()),
    }));
    let mut c = Client::with_base(&base, "");
    c.login("me", "pw").unwrap();
    let ask = AdAsk { station_id: "st9".into(), track_token: "trk".into(), seconds_played: 31, user_initiated: true };
    let answer = c.ad_list(&ask).unwrap();
    let ads = parse_ad_list(&answer, false);
    assert_eq!(ads.len(), 2);

    // Play both ads the way the app does, sending every report in order.
    let tracker = Tracker::new(true, false, "boxvibe-test");
    let mut brk = Break::new(ads, "st9");
    let now = SystemTime::now();
    let send = |c: &mut Client, actions: Vec<Action>| {
        for action in actions {
            match action {
                Action::Track { urls, .. } => tracker.fire(urls).unwrap().join().unwrap(),
                Action::Register { token, request_meta, station_id } => {
                    c.register_audio_impression(&token, &request_meta, &station_id).unwrap()
                }
            }
        }
    };
    while !brk.done() {
        send(&mut c, brk.start(now));
        let length = brk.ad().unwrap().length;
        for part in [0.25, 0.5, 0.75] {
            send(&mut c, brk.progress(Duration::from_secs_f64(length * part), now));
        }
        send(&mut c, brk.finish(now));
    }

    let seen = seen.lock().unwrap();
    let calls: Vec<String> = seen
        .iter()
        .map(|s| s.path.clone())
        .filter(|p| p.starts_with("/api/v1/ad") || p.starts_with("/haymaker"))
        .collect();
    let q = |p: &str| calls.iter().position(|c| c.starts_with(p)).unwrap_or_else(|| panic!("{p} missing in {calls:?}"));
    assert_eq!(calls[0], "/api/v1/ad/getAdList");
    let order = [
        "/haymaker/imp/1?ts=",
        "/haymaker/te/other_ad_interaction?m=1",
        "/haymaker/start/1?ps=foreground&ph=00:00:00.000",
        "/api/v1/ad/registerAudioProgrammaticImpression",
        "/haymaker/q1/1?ph=00:00:02.000&uri=http%3A%2F%2F127.0.0.1%3A",
        "/haymaker/mid/1?ph=00:00:04.000",
        "/haymaker/q3/1?ph=00:00:06.000",
        "/haymaker/done/1?ph=00:00:08.000",
        "/haymaker/imp/2?ts=",
        "/haymaker/te/other_ad_interaction?m=2",
        "/haymaker/start/2",
        "/api/v1/ad/registerAudioProgrammaticImpression",
        "/haymaker/q1/2?ph=00:00:01.000",
        "/haymaker/mid/2?ph=00:00:02.000",
        "/haymaker/q3/2?ph=00:00:03.000",
        "/haymaker/done/2?ph=00:00:04.000",
    ];
    let mut at = 0;
    for step in order {
        let found = calls[at..]
            .iter()
            .position(|c| c.starts_with(step))
            .unwrap_or_else(|| panic!("{step} not after #{at} in {calls:?}"));
        at += found + 1;
    }
    assert!(q("/haymaker/imp/1") < q("/haymaker/imp/2"));
    assert_eq!(calls.len(), 1 + order.len(), "{calls:?}");
    // The timestamp is a filled-in ISO date, no macro left behind.
    assert!(calls.iter().all(|c| !c.contains('[') && !c.contains("%5B")), "{calls:?}");
    let imp = &calls[q("/haymaker/imp/1")];
    assert!(imp.contains("ts=20") && imp.ends_with('Z'), "{imp}");
    // What the site's player sent for the ask and the registration.
    let ask_body = &seen.iter().find(|s| s.path.ends_with("getAdList")).unwrap().body;
    assert_eq!(ask_body["currentTrackStationId"], "st9");
    assert_eq!(ask_body["currentTrackToken"], "trk");
    assert_eq!(ask_body["currentTrackSecondsPlayed"], 31);
    assert_eq!(ask_body["userInitiatedTrackEnd"], true);
    assert!(ask_body.get("uid").is_none());
    let reg = &seen.iter().find(|s| s.path.ends_with("registerAudioProgrammaticImpression")).unwrap();
    assert_eq!(reg.method, "POST");
    assert_eq!(reg.header("X-AuthToken"), Some("tok"));
    assert_eq!(
        reg.body,
        json!({ "adRequestMetaData": "meta-1", "adToken": "pod:1/2/1", "stationId": "st9", "httpReferrer": "" })
    );
    // Reports to the tracking hosts carry no Pandora login.
    let imp_request = seen.iter().find(|s| s.path.starts_with("/haymaker/imp/1")).unwrap();
    assert_eq!(imp_request.header("X-AuthToken"), None);
    assert_eq!(imp_request.header("User-Agent"), Some("boxvibe-test"));
}

#[test]
fn an_empty_or_odd_ad_answer_means_no_ads_and_a_refusal_is_an_error() {
    use crate::ads::parse_ad_list;
    let mut answers = vec![json!({ "ads": [] }).to_string(), "{}".into(), "[]".into()].into_iter();
    let (base, _) = fake_pandora(Box::new(move |r| match r.path.as_str() {
        "/api/v1/auth/login" => login_ok("tok"),
        "/api/v1/ad/getAdList" => match answers.next() {
            Some(a) => (200, a),
            None => (500, r#"{"errorCode":0,"errorString":"INTERNAL"}"#.into()),
        },
        _ => (200, "{}".into()),
    }));
    let mut c = Client::with_base(&base, "");
    c.login("me", "pw").unwrap();
    let ask = AdAsk { station_id: "s".into(), track_token: "t".into(), seconds_played: 0, user_initiated: false };
    for _ in 0..3 {
        assert!(parse_ad_list(&c.ad_list(&ask).unwrap(), true).is_empty());
    }
    assert!(c.ad_list(&ask).is_err());
}

#[test]
fn artist_pages_and_modes_parse_tolerantly() {
    let a = parse_artist(&json!({
        "name": "M83", "detailUrl": "https://www.pandora.com/artist/m83/AR1",
        "bio": { "text": "<p>French &amp; electronic.</p>  <b>Formed 2001</b>" },
        "similar": [ { "name": "MGMT", "pandoraId": "AR:9" }, { "name": "Only music id", "musicId": "R7" }, { "name": "No id" } ]
    }))
    .unwrap();
    assert_eq!(a.bio, "French & electronic. Formed 2001");
    assert_eq!(a.similar.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), ["AR:9", "AR:7"]);
    let plain = parse_artist(&json!({ "name": "X", "biography": "Short." })).unwrap();
    assert_eq!((plain.bio.as_str(), plain.similar.len()), ("Short.", 0));
    assert_eq!(parse_artist(&json!({})), None);

    let modes = parse_modes(&json!({ "result": { "currentModeId": 3, "availableModes": [
        { "modeId": 0, "modeName": "My Station", "isModeAvailable": true },
        { "modeId": 5, "modeButtonText": "Artist Only", "isModeAvailable": false, "isPremiumOnly": true },
        { "modeName": "no id" }
    ] } }));
    assert_eq!(modes.current, Some(3));
    assert_eq!(modes.modes.len(), 2);
    assert_eq!(
        (modes.modes[1].name.as_str(), modes.modes[1].available, modes.modes[1].premium_only),
        ("Artist Only", false, true)
    );
    assert_eq!(parse_modes(&json!({})), StationModes::default());
}

#[test]
fn artist_shuffle_mode_and_suggestion_calls() {
    let (base, seen) = fake_pandora(Box::new(|r| {
        match r.path.as_str() {
        "/api/v1/auth/login" => login_ok("tok"),
        "/api/v1/music/artist" => (
            200,
            json!({ "name": "M83", "similar": [{ "name": "MGMT", "pandoraId": "AR:9" }] }).to_string(),
        ),
        "/api/v1/search" => (200, json!({ "items": [] }).to_string()),
        // No suggestions path: the artist's similar artists stand in.
        "/api/v1/search/getSeedSuggestions" => (404, json!({ "errorString": "NOT_FOUND" }).to_string()),
        "/api/v1/station/shuffle" => (200, json!({ "stationId": "77", "name": "Shuffle", "isShuffle": true }).to_string()),
        // The first modes guess is unknown here, the second answers.
        "/api/v1/interactiveradio/getAvailableModesSimple" | "/api/v1/interactiveradio/setAndGetAvailableModes" => {
            (404, json!({ "errorString": "NOT_FOUND" }).to_string())
        }
        "/api/v1/station/getAvailableModes" => (
            200,
            json!({ "currentModeId": 0, "availableModes": [{ "modeId": 0, "modeName": "My Station" }, { "modeId": 3, "modeName": "Deep Cuts" }] })
                .to_string(),
        ),
        "/api/v1/station/setAndGetAvailableModes" => (
            200,
            json!({ "currentModeId": r.body["modeId"], "availableModes": [{ "modeId": 0, "modeName": "My Station" }, { "modeId": 3, "modeName": "Deep Cuts" }] })
                .to_string(),
        ),
        _ => (200, "{}".into()),
    }
    }));
    let mut c = Client::with_base(&base, "");
    c.login("me", "pw").unwrap();
    assert_eq!(c.artist("AR:83").unwrap().similar[0].name, "MGMT");
    assert_eq!(c.artist_by_name("R83", "M83").unwrap().name, "M83");
    assert!(c.artist_by_name("", "Nobody").is_err(), "no music id and no search hit");
    assert_eq!(c.seed_suggestions("9", "R83").unwrap()[0].name, "MGMT");
    assert!(c.shuffle().unwrap().is_shuffle);
    assert_eq!(c.station_modes("9").unwrap().modes.len(), 2);
    assert_eq!(c.set_station_mode("9", 3).unwrap().current, Some(3));
    let seen = seen.lock().unwrap();
    let artist: Vec<&Value> = seen.iter().filter(|s| s.path == "/api/v1/music/artist").map(|s| &s.body).collect();
    assert_eq!(artist[0], &json!({ "token": "R83" }), "AR:83 asked as its musicId");
    let set = seen.iter().find(|s| s.path == "/api/v1/station/setAndGetAvailableModes").unwrap();
    assert_eq!(set.body, json!({ "stationId": "9", "modeId": 3 }));
}

#[test]
fn every_modes_guess_missing_is_a_plain_error() {
    let (base, _) = fake_pandora(Box::new(|r| match r.path.as_str() {
        "/api/v1/auth/login" => login_ok("tok"),
        _ => (404, json!({ "errorString": "NOT_FOUND", "message": "Service method not found" }).to_string()),
    }));
    let mut c = Client::with_base(&base, "");
    c.login("me", "pw").unwrap();
    let e = c.station_modes("9").unwrap_err();
    assert!(e.to_string().contains("station modes") || e.to_string().contains("not found"), "{e}");
}
