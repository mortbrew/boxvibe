use super::*;

fn song() -> Snapshot {
    Snapshot {
        title: "Midnight City".into(),
        artist: "M83".into(),
        album: "Hurry Up".into(),
        art_url: "https://x/a.jpg".into(),
        url: "https://pandora.com/s".into(),
        track: "ab-12:3".into(),
        length: 244,
        playing: Some(true),
    }
}

#[test]
fn metadata_has_what_desktops_show() {
    let m = song().metadata();
    let s = |k: &str| String::try_from(m[k].clone()).unwrap();
    assert_eq!(s("xesam:title"), "Midnight City");
    assert_eq!(Vec::<String>::try_from(m["xesam:artist"].clone()).unwrap(), ["M83"]);
    assert_eq!(s("mpris:artUrl"), "https://x/a.jpg");
    assert_eq!(i64::try_from(m["mpris:length"].clone()).unwrap(), 244_000_000);
    let id = ObjectPath::try_from(m["mpris:trackid"].clone()).unwrap();
    assert_eq!(id.as_str(), "/org/boxvibe/track/tab_12_3");
    assert_eq!(song().status(), "Playing");
}

#[test]
fn nothing_playing_is_stopped_with_no_track() {
    let m = Snapshot::default().metadata();
    assert_eq!(m.len(), 1);
    assert_eq!(Snapshot::default().status(), "Stopped");
    assert_eq!(Snapshot { playing: Some(false), ..song() }.status(), "Paused");
}

#[test]
fn only_changes_are_announced() {
    let (mut m, _tx, changed) = Mpris::detached();
    m.show(song(), 1.0);
    m.show(song(), 2.0);
    assert_eq!(changed.try_iter().count(), 1);
    m.show(Snapshot { playing: Some(false), ..song() }, 2.0);
    assert_eq!(changed.try_iter().count(), 1);
    assert_eq!(m.shown().status(), "Paused");
}

/// Needs a session bus: `dbus-run-session -- cargo test mpris_on_a_real_bus -- --ignored`
/// while something calls PlayPause on `org.mpris.MediaPlayer2.boxvibe`.
#[test]
#[ignore]
fn mpris_on_a_real_bus() {
    let mut m = Mpris::start();
    m.show(song(), 3.0);
    let got = m.controls.recv_timeout(std::time::Duration::from_secs(8));
    assert_eq!(got, Ok(Control::PlayPause));
}
