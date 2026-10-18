//! Playback: rodio (cpal → ALSA/PipeWire) with symphonia decoding the MP3
//! and AAC/MP4 files Pandora serves, no ffmpeg. Without a sound device
//! (or in `--demo`) a silent clock stands in, so the interface still works.

use std::io::Cursor;
use std::time::{Duration, Instant};

/// A clock that can pause: where a song is when nothing makes a sound.
#[derive(Default)]
struct Clock {
    started: Option<Instant>,
    before: Duration,
}

impl Clock {
    fn position(&self) -> Duration {
        self.before + self.started.map_or(Duration::ZERO, |s| s.elapsed())
    }
    fn pause(&mut self) {
        self.before = self.position();
        self.started = None;
    }
    fn resume(&mut self) {
        if self.started.is_none() {
            self.started = Some(Instant::now());
        }
    }
}

pub struct Audio {
    device: Option<rodio::MixerDeviceSink>,
    player: Option<rodio::Player>,
    clock: Clock,
    paused: bool,
    active: bool,
    volume: u8,
    /// Why there is no sound, when there is none.
    pub problem: Option<String>,
}

impl Audio {
    /// Opens the default sound device; `silent` (demo) skips it.
    pub fn open(silent: bool, volume: u8) -> Self {
        let (device, problem) = if silent {
            (None, None)
        } else {
            match rodio::DeviceSinkBuilder::open_default_sink() {
                Ok(mut device) => {
                    device.log_on_drop(false);
                    (Some(device), None)
                }
                Err(e) => (None, Some(format!("no sound device ({e}): playing silently"))),
            }
        };
        Self { device, player: None, clock: Clock::default(), paused: false, active: false, volume, problem }
    }

    pub fn has_device(&self) -> bool {
        self.device.is_some()
    }

    /// Starts a song from its downloaded bytes (`None`: the silent clock only).
    pub fn start(&mut self, data: Option<Vec<u8>>) -> Result<(), String> {
        self.stop();
        self.paused = false;
        self.active = true;
        self.clock = Clock::default();
        self.clock.resume();
        let (Some(device), Some(data)) = (&self.device, data) else { return Ok(()) };
        let decoder = decoder(data).map_err(|e| format!("can't decode this song: {e}"))?;
        let player = rodio::Player::connect_new(device.mixer());
        player.set_volume(gain(self.volume));
        player.append(decoder);
        player.play();
        self.player = Some(player);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(p) = self.player.take() {
            p.stop();
        }
        self.active = false;
        self.clock = Clock::default();
    }

    pub fn toggle_pause(&mut self) {
        if !self.active {
            return;
        }
        self.paused = !self.paused;
        if self.paused {
            self.clock.pause();
            if let Some(p) = &self.player {
                p.pause();
            }
        } else {
            self.clock.resume();
            if let Some(p) = &self.player {
                p.play();
            }
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn position(&self) -> Duration {
        match &self.player {
            Some(p) => p.get_pos(),
            None => self.clock.position(),
        }
    }

    /// The song played to its end (`length`: what Pandora says it lasts).
    pub fn finished(&self, length: u32) -> bool {
        if !self.active || self.paused {
            return false;
        }
        match &self.player {
            Some(p) => p.empty(),
            None => length > 0 && self.clock.position() >= Duration::from_secs(u64::from(length)),
        }
    }

    pub fn set_volume(&mut self, volume: u8) {
        self.volume = volume.min(100);
        if let Some(p) = &self.player {
            p.set_volume(gain(self.volume));
        }
    }

    /// For screenshots: put the silent clock at `at`.
    pub fn seek_silent(&mut self, at: Duration) {
        if self.player.is_none() {
            self.clock.before = at;
        }
    }
}

/// A decoder for a whole downloaded song. symphonia only recognises MP4
/// (Pandora's `aacplus`) when told the length and that it can seek, so the
/// container is named from the file's first bytes.
pub fn decoder(data: Vec<u8>) -> Result<rodio::Decoder<Cursor<Vec<u8>>>, rodio::decoder::DecoderError> {
    let len = data.len() as u64;
    let mime = match data.get(..8) {
        Some([_, _, _, _, b'f', b't', b'y', b'p']) => "audio/mp4",
        Some([0xff, b, ..]) if b & 0xf6 == 0xf0 => "audio/aac",
        _ => "audio/mpeg",
    };
    rodio::Decoder::builder()
        .with_data(Cursor::new(data))
        .with_byte_len(len)
        .with_seekable(true)
        .with_mime_type(mime)
        .build()
}

/// 0–100 to an amplitude, on a curve so the low steps are still audible.
fn gain(volume: u8) -> f32 {
    (f32::from(volume) / 100.0).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junk_is_not_a_song() {
        assert!(decoder(b"not audio at all, just text".to_vec()).is_err());
    }

    #[test]
    fn volume_curve() {
        assert_eq!(gain(0), 0.0);
        assert_eq!(gain(100), 1.0);
        assert!(gain(50) < 0.5);
    }

    /// `BOXVIBE_DECODE=a.mp3:b.m4a cargo test decodes_real_files -- --ignored`
    #[test]
    #[ignore]
    fn decodes_real_files() {
        use rodio::Source;
        for file in std::env::var("BOXVIBE_DECODE").unwrap_or_default().split(':').filter(|f| !f.is_empty()) {
            let d = decoder(std::fs::read(file).unwrap()).unwrap_or_else(|e| panic!("{file}: {e}"));
            let rate = d.sample_rate();
            assert!(d.count() > 0, "{file} decodes to samples ({rate:?} Hz)");
        }
    }
}
