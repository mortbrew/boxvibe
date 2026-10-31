//! MPRIS: media keys, the desktop's player widget and `playerctl` see boxvibe
//! on the session bus as `org.mpris.MediaPlayer2.boxvibe`.
//!
//! The bus runs on its own thread. The app hands it a [`Snapshot`] when the
//! song or play state changes and reads [`Control`]s back each frame, so a slow
//! or missing D-Bus never stalls the screen. With no session bus (a console,
//! ssh, a build machine) the thread just ends and nothing else changes.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const BUS_NAME: &str = "org.mpris.MediaPlayer2.boxvibe";
const PATH: &str = "/org/mpris/MediaPlayer2";

/// What the desktop asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    PlayPause,
    Play,
    Pause,
    Next,
}

/// What the desktop is shown.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
    pub url: String,
    /// Pandora's track token, made into an object path.
    pub track: String,
    /// Seconds; 0 = unknown.
    pub length: u32,
    /// `None`: nothing playing.
    pub playing: Option<bool>,
}

impl Snapshot {
    pub fn status(&self) -> &'static str {
        match self.playing {
            None => "Stopped",
            Some(true) => "Playing",
            Some(false) => "Paused",
        }
    }

    /// The `mpris:trackid`: an object path, so only `[A-Za-z0-9_]` from the token.
    pub fn track_id(&self) -> String {
        if self.playing.is_none() || self.track.is_empty() {
            return "/org/mpris/MediaPlayer2/TrackList/NoTrack".into();
        }
        let safe: String = self.track.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        format!("/org/boxvibe/track/t{safe}")
    }

    pub fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut m = HashMap::new();
        let mut put = |k: &str, v: Value<'_>| {
            if let Ok(v) = OwnedValue::try_from(v) {
                m.insert(k.to_string(), v);
            }
        };
        if let Ok(p) = ObjectPath::try_from(self.track_id()) {
            put("mpris:trackid", Value::from(p));
        }
        if self.playing.is_none() {
            return m;
        }
        put("xesam:title", Value::from(self.title.clone()));
        put("xesam:artist", Value::from(vec![self.artist.clone()]));
        put("xesam:album", Value::from(self.album.clone()));
        if !self.art_url.is_empty() {
            put("mpris:artUrl", Value::from(self.art_url.clone()));
        }
        if !self.url.is_empty() {
            put("xesam:url", Value::from(self.url.clone()));
        }
        if self.length > 0 {
            put("mpris:length", Value::from(i64::from(self.length) * 1_000_000));
        }
        m
    }
}

/// The app's end of the bus thread.
pub struct Mpris {
    pub controls: Receiver<Control>,
    shown: Arc<Mutex<Shared>>,
    changed: Sender<()>,
    last: Option<Snapshot>,
}

#[derive(Default)]
struct Shared {
    now: Snapshot,
    /// Microseconds into the song.
    position: i64,
}

impl Mpris {
    /// Starts the bus thread. Never fails: without a session bus the
    /// controls simply never come.
    pub fn start() -> Self {
        let (mpris, control_tx, changed_rx) = Self::detached();
        let shown = Arc::clone(&mpris.shown);
        let _ = std::thread::Builder::new().name("mpris".into()).spawn(move || serve(shown, control_tx, changed_rx));
        mpris
    }

    /// No bus: the tests drive the other ends.
    pub fn detached() -> (Self, Sender<Control>, Receiver<()>) {
        let (control_tx, controls) = mpsc::channel();
        let (changed, changed_rx) = mpsc::channel();
        let mpris = Self { controls, shown: Arc::default(), changed, last: None };
        (mpris, control_tx, changed_rx)
    }

    /// Called every frame: the position always, the rest only when it changed.
    pub fn show(&mut self, now: Snapshot, position_secs: f64) {
        let Ok(mut shared) = self.shown.lock() else { return };
        shared.position = (position_secs * 1_000_000.0) as i64;
        if self.last.as_ref() != Some(&now) {
            shared.now = now.clone();
            self.last = Some(now);
            let _ = self.changed.send(());
        }
    }

    #[cfg(test)]
    pub fn shown(&self) -> Snapshot {
        self.shown.lock().unwrap().now.clone()
    }
}

fn serve(shown: Arc<Mutex<Shared>>, controls: Sender<Control>, changed: Receiver<()>) {
    let player = Player { shown: Arc::clone(&shown), controls };
    let conn = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.name(BUS_NAME))
        .and_then(|b| b.serve_at(PATH, Root))
        .and_then(|b| b.serve_at(PATH, player))
        .and_then(|b| b.build());
    let Ok(conn) = conn else { return };
    let Ok(iface) = conn.object_server().interface::<_, Player>(PATH) else { return };
    while changed.recv().is_ok() {
        while changed.try_recv().is_ok() {}
        let p = iface.get();
        let e = iface.signal_emitter();
        zbus::block_on(async {
            let _ = p.playback_status_changed(e).await;
            let _ = p.metadata_changed(e).await;
            let _ = p.can_play_changed(e).await;
            let _ = p.can_go_next_changed(e).await;
        });
    }
}

struct Root;

#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {}
    fn quit(&self) {}
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> String {
        "boxvibe".into()
    }
    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "boxvibe".into()
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Player {
    shown: Arc<Mutex<Shared>>,
    controls: Sender<Control>,
}

impl Player {
    fn now(&self) -> Snapshot {
        self.shown.lock().map(|s| s.now.clone()).unwrap_or_default()
    }
    fn send(&self, c: Control) {
        let _ = self.controls.send(c);
    }
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        self.send(Control::Next);
    }
    /// Pandora can't go back.
    fn previous(&self) {}
    fn pause(&self) {
        self.send(Control::Pause);
    }
    fn play_pause(&self) {
        self.send(Control::PlayPause);
    }
    /// Pandora has no stop: pause.
    fn stop(&self) {
        self.send(Control::Pause);
    }
    fn play(&self) {
        self.send(Control::Play);
    }
    fn seek(&self, _offset: i64) {}
    fn set_position(&self, _track: ObjectPath<'_>, _position: i64) {}
    fn open_uri(&self, _uri: &str) {}

    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.now().status().into()
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        self.now().metadata()
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        1.0
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.shown.lock().map(|s| s.position).unwrap_or(0)
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.now().playing.is_some()
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.now().playing.is_some()
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.now().playing.is_some()
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        false
    }
    #[zbus(property(emits_changed_signal = "const"))]
    fn can_control(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
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
}
