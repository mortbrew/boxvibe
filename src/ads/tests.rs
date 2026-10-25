use super::*;
use serde_json::json;
use std::time::{Duration, SystemTime};

/// A made-up pod in the shape of Pandora's answer (no real values).
pub fn pod() -> Value {
    let ad = |n: u32, length: f64| {
        json!({
            "success": true,
            "adToken": format!("pod:1/2/{n}"),
            "adRequestMetaData": format!("meta-{n}"),
            "adType": "AUDIO_AD",
            "adLength": length,
            "audioUrlMap": { "mediumQuality": format!("https://ads.example.com/audio/{n}.mp4") },
            "adVerificationData": [
                { "vendor": "other", "parameters": "ignore=me" },
                { "vendor": "pandora-monocle", "parameters": format!("a=1&b={n}") }
            ],
            "vastTrackingEvents": [
                { "eventType": "IMPRESSION", "urls": [format!("https://track.example.com/haymaker/imp/{n}?t=[TIMESTAMP]")] },
                { "eventType": "AUDIO_START", "urls": [format!("https://track.example.com/haymaker/start/{n}?s=[PLAYERSTATE]&h=[ADPLAYHEAD]")] },
                { "eventType": "AUDIO_FIRST_QUARTILE", "urls": [format!("https://track.example.com/haymaker/q1/{n}?h=[ADPLAYHEAD]&a=[ASSETURI]")] },
                { "eventType": "AUDIO_MIDPOINT", "urls": [format!("https://track.example.com/haymaker/mid/{n}")] },
                { "eventType": "AUDIO_THIRD_QUARTILE", "urls": [format!("https://track.example.com/haymaker/q3/{n}")] },
                { "eventType": "AUDIO_COMPLETE", "urls": [format!("https://track.example.com/haymaker/done/{n}"), "http://plain.example.com/x".to_owned()] }
            ]
        })
    };
    json!({ "ads": [ad(1, 8.0), ad(2, 4.0)] })
}

fn names(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .map(|a| match a {
            Action::Track { event, .. } => (*event).to_owned(),
            Action::Register { .. } => "register".to_owned(),
        })
        .collect()
}

#[test]
fn a_pod_is_parsed_and_anything_unsafe_is_dropped() {
    let ads = parse_ad_list(&pod(), true);
    assert_eq!(ads.len(), 2);
    assert_eq!((ads[0].token.as_str(), ads[0].request_meta.as_str(), ads[0].length), ("pod:1/2/1", "meta-1", 8.0));
    assert_eq!(ads[0].monocle, "a=1&b=1");
    assert_eq!(ads[0].seconds(), 8);
    // The plain http report URL is not kept when https is required, but is for the test server.
    let complete = |ad: &Ad| ad.events.iter().find(|(n, _)| n == "AUDIO_COMPLETE").unwrap().1.len();
    assert_eq!(complete(&ads[0]), 1);
    assert_eq!(complete(&parse_ad_list(&pod(), false)[0]), 2);
}

#[test]
fn nothing_playable_means_no_ads() {
    assert!(parse_ad_list(&json!({ "ads": [] }), true).is_empty(), "the usual answer");
    assert!(parse_ad_list(&json!({}), true).is_empty());
    assert!(parse_ad_list(&json!("nope"), true).is_empty());
    let one = |change: &dyn Fn(&mut Value)| {
        let mut v = pod();
        let ad = &mut v["ads"][0];
        change(ad);
        v["ads"] = json!([ad.clone()]);
        parse_ad_list(&v, true).len()
    };
    assert_eq!(one(&|_| {}), 1);
    assert_eq!(one(&|a| a["adType"] = json!("DISPLAY_AD")), 0);
    assert_eq!(one(&|a| a["adLength"] = json!(0.0)), 0);
    assert_eq!(one(&|a| a["adLength"] = json!(3600.0)), 0);
    assert_eq!(one(&|a| a["audioUrlMap"] = json!({})), 0);
    assert_eq!(one(&|a| a["audioUrlMap"]["mediumQuality"] = json!("http://ads.example.com/a.mp4")), 0);
    assert_eq!(one(&|a| a["audioUrlMap"]["mediumQuality"] = json!("https://127.0.0.1/a.mp4")), 0);
    assert_eq!(one(&|a| a["audioUrlMap"]["mediumQuality"] = json!("https://localhost/a.mp4")), 0);
    assert_eq!(one(&|a| a["adLength"] = json!("12.5")), 1, "a length may come as text");
}

#[test]
fn a_pod_is_cut_to_four_ads() {
    let ad = pod()["ads"][0].clone();
    let v = json!({ "ads": [ad, ad, ad, ad, ad, ad] });
    assert_eq!(parse_ad_list(&v, true).len(), 4);
}

#[test]
fn only_plain_https_hosts_with_a_name_are_allowed() {
    for ok in ["https://track.example.com/a?b=c", "https://a.b.example.org:8443/x"] {
        assert!(allowed(ok, true), "{ok}");
    }
    for bad in [
        "http://track.example.com/a",
        "ftp://track.example.com/a",
        "https://127.0.0.1/a",
        "https://10.0.0.5:8080/a",
        "https://[::1]/a",
        "https://localhost/a",
        "https://x.localhost/a",
        "https://user:pw@track.example.com/a",
        "https://intranet/a",
        "https://track.example.com/a b",
        "track.example.com/a",
        "",
    ] {
        assert!(!allowed(bad, true), "{bad}");
    }
    assert!(allowed("http://127.0.0.1:9/a", false), "tests may use a local server");
    assert!(!allowed("file:///etc/passwd", false));
}

#[test]
fn the_website_macros_are_filled_in() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_millis(1_793_000_000_123);
    let url = fill_macros(
        "https://t.example.com/x?p=[PLAYERSTATE]&h=[ADPLAYHEAD]&u=[ASSETURI]&t=[TIMESTAMP]&k=[OTHER]",
        Duration::from_millis(3_723_456),
        "https://ads.example.com/a b.mp4?x=1",
        now,
    );
    assert_eq!(
        url,
        "https://t.example.com/x?p=foreground&h=01:02:03.456&u=https%3A%2F%2Fads.example.com%2Fa%20b.mp4%3Fx%3D1&t=2026-10-26T07:33:20.123Z&k=[OTHER]"
    );
}

#[test]
fn dates_are_written_in_utc() {
    let at = |s: u64, ms: u64| SystemTime::UNIX_EPOCH + Duration::from_millis(s * 1000 + ms);
    assert_eq!(iso_utc(at(0, 0)), "1970-01-01T00:00:00.000Z");
    assert_eq!(iso_utc(at(951_782_400, 7)), "2000-02-29T00:00:00.007Z");
    assert_eq!(iso_utc(at(1_709_251_199, 999)), "2024-02-29T23:59:59.999Z");
    assert_eq!(clock(Duration::from_millis(59_999)), "00:00:59.999");
    assert_eq!(percent_encode("aZ09-._~ /é"), "aZ09-._~%20%2F%C3%A9");
}

#[test]
fn an_ad_reports_in_the_order_the_website_does() {
    let ads = parse_ad_list(&pod(), true);
    let mut brk = Break::new(ads, "station-1");
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_793_000_000);
    assert!(brk.progress(Duration::from_secs(5), now).is_empty(), "nothing before the start");
    assert!(brk.finish(now).is_empty());

    let start = brk.start(now);
    assert_eq!(names(&start), ["IMPRESSION", "other_ad_interaction", "AUDIO_START", "register"]);
    assert!(brk.start(now).is_empty(), "the start is reported once");
    let Action::Track { urls, .. } = &start[1] else { panic!() };
    assert_eq!(urls, &["https://track.example.com/haymaker/te/other_ad_interaction?a=1&b=1"]);
    let Action::Track { urls, .. } = &start[2] else { panic!() };
    assert_eq!(urls, &["https://track.example.com/haymaker/start/1?s=foreground&h=00:00:00.000"]);
    let Action::Register { token, request_meta, station_id } = &start[3] else { panic!() };
    assert_eq!((token.as_str(), request_meta.as_str(), station_id.as_str()), ("pod:1/2/1", "meta-1", "station-1"));

    // 8 s long: marks at 2, 4 and 6 s.
    assert!(brk.progress(Duration::from_millis(1999), now).is_empty());
    let q1 = brk.progress(Duration::from_millis(2100), now);
    assert_eq!(names(&q1), ["AUDIO_FIRST_QUARTILE"]);
    let Action::Track { urls, .. } = &q1[0] else { panic!() };
    assert_eq!(
        urls,
        &["https://track.example.com/haymaker/q1/1?h=00:00:02.100&a=https%3A%2F%2Fads.example.com%2Faudio%2F1.mp4"]
    );
    assert!(brk.progress(Duration::from_millis(2200), now).is_empty(), "a mark is sent once");
    assert_eq!(names(&brk.progress(Duration::from_secs(7), now)), ["AUDIO_MIDPOINT", "AUDIO_THIRD_QUARTILE"]);

    let end = brk.finish(now);
    assert_eq!(names(&end), ["AUDIO_COMPLETE"]);
    assert_eq!((brk.index(), brk.done(), brk.count()), (1, false, 2));

    // The second ad: skipping straight to the end reports every mark in order.
    let start = brk.start(now);
    assert_eq!(names(&start), ["IMPRESSION", "other_ad_interaction", "AUDIO_START", "register"]);
    assert_eq!(
        names(&brk.finish(now)),
        ["AUDIO_FIRST_QUARTILE", "AUDIO_MIDPOINT", "AUDIO_THIRD_QUARTILE", "AUDIO_COMPLETE"]
    );
    assert!(brk.done() && brk.ad().is_none());
}

#[test]
fn an_ad_without_reports_still_plays_and_registers() {
    let mut v = pod();
    v["ads"][0]["vastTrackingEvents"] = json!([]);
    v["ads"][0]["adVerificationData"] = json!([]);
    let mut brk = Break::new(parse_ad_list(&v, true), "s");
    assert_eq!(names(&brk.start(SystemTime::UNIX_EPOCH)), ["register"]);
    assert!(brk.finish(SystemTime::UNIX_EPOCH).is_empty());
}

#[test]
fn a_disabled_tracker_sends_nothing_and_a_blocked_url_is_never_fetched() {
    assert!(Tracker::new(false, true, "ua").fire(vec!["https://t.example.com/a".into()]).is_none());
    assert!(
        Tracker::new(true, true, "ua").fire(vec!["http://127.0.0.1:9/a".into(), "https://localhost/".into()]).is_none()
    );
}
