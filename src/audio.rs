//! Playback: rodio (cpal → ALSA/PipeWire) with symphonia decoding the MP3
//! and AAC/MP4 files Pandora serves, no ffmpeg. Without a sound device
//! (or in `--demo`) a silent clock stands in, so the interface still works.
//!
//! What reaches the speakers: the decoded song, resampled to the sound
//! device's rate with a windowed-sinc filter ([`Resampled`]; rodio's own
//! converter interpolates in straight lines, which adds a gritty edge to
//! Pandora's 22 kHz AAC), kept inside ±1.0, then scaled by [`level`]: the
//! volume curve times Pandora's per-song `fileGain`, never above 1.0.

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
    /// Pandora's loudness correction for the song playing, in dB.
    gain_db: f32,
    /// Silent whatever the volume (an ad under Good Vibes Only: Muted).
    muted: bool,
    /// 1.0, or less while the sleep timer fades out.
    fade: f32,
    /// What the sound device plays at (the songs are resampled to it).
    rate: Option<u32>,
    /// Why there is no sound, when there is none.
    pub problem: Option<String>,
}

impl Audio {
    /// Opens the default sound device; `silent` (demo) skips it.
    pub fn open(silent: bool, volume: u8) -> Self {
        let (device, problem) = if silent {
            (None, None)
        } else {
            match open_device() {
                Ok(mut device) => {
                    device.log_on_drop(false);
                    (Some(device), None)
                }
                Err(e) => (None, Some(format!("no sound device ({e}): playing silently"))),
            }
        };
        let rate = device.as_ref().map(|d| d.config().sample_rate().get());
        Self {
            device,
            player: None,
            clock: Clock::default(),
            paused: false,
            active: false,
            volume,
            gain_db: 0.0,
            muted: false,
            fade: 1.0,
            rate,
            problem,
        }
    }

    pub fn has_device(&self) -> bool {
        self.device.is_some()
    }

    /// Starts a song from its downloaded bytes (`None`: the silent clock
    /// only), `gain_db` louder (Pandora's `fileGain`), or silent when `muted`.
    pub fn start(&mut self, data: Option<Vec<u8>>, gain_db: f32, muted: bool) -> Result<(), String> {
        self.stop();
        self.gain_db = if gain_db.is_finite() { gain_db } else { 0.0 };
        self.muted = muted;
        self.paused = false;
        self.active = true;
        self.clock = Clock::default();
        self.clock.resume();
        let (Some(device), Some(data)) = (&self.device, data) else { return Ok(()) };
        let decoder = decoder(data).map_err(|e| format!("can't decode this song: {e}"))?;
        let player = rodio::Player::connect_new(device.mixer());
        player.set_volume(self.level());
        match self.rate {
            Some(rate) if rate != rodio::Source::sample_rate(&decoder).get() => {
                player.append(Resampled::new(decoder, rate))
            }
            _ => player.append(Clamped(decoder)),
        }
        player.play();
        self.player = Some(player);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(p) = self.player.take() {
            p.stop();
        }
        self.active = false;
        self.muted = false;
        self.gain_db = 0.0;
        self.clock = Clock::default();
    }

    /// Mutes or unmutes what is playing; the volume stays what it was.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.apply();
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    /// The amplitude the player is set to now.
    pub fn level(&self) -> f32 {
        if self.muted { 0.0 } else { level(self.volume, self.gain_db) * self.fade }
    }

    /// The sleep timer's fade: 1.0 is the volume as set, 0.0 silence.
    pub fn set_fade(&mut self, fade: f32) {
        self.fade = fade.clamp(0.0, 1.0);
        self.apply();
    }

    fn apply(&self) {
        if let Some(p) = &self.player {
            p.set_volume(self.level());
        }
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
        self.apply();
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

/// The volume and Pandora's `fileGain` together, as the website does it
/// (amplitude = 10^(dB/20)), capped at 1.0 so a song Pandora turns up can't
/// clip: at full volume a positive gain does nothing, below it it helps.
pub fn level(volume: u8, gain_db: f32) -> f32 {
    let db = if gain_db.is_finite() { gain_db.clamp(-30.0, 30.0) } else { 0.0 };
    (gain(volume) * 10f32.powf(db / 20.0)).min(1.0)
}

/// The output buffer, in frames: about 85 ms at 48 kHz. rodio's default is
/// 50 ms, which Bluetooth (AirPods through PipeWire) can run dry, and that
/// is heard as clicks. The volume reacts this much later, which nobody notices.
const BUFFER_FRAMES: u32 = 4096;

/// The default sound device with a roomier buffer; rodio's own choice when
/// the device won't take it.
fn open_device() -> Result<rodio::MixerDeviceSink, rodio::DeviceSinkError> {
    rodio::DeviceSinkBuilder::from_default_device()
        .and_then(|b| b.with_buffer_size(rodio::cpal::BufferSize::Fixed(BUFFER_FRAMES)).open_stream())
        .or_else(|_| rodio::DeviceSinkBuilder::open_default_sink())
}

/// A source with every sample kept inside ±1.0.
struct Clamped<S>(S);

impl<S: rodio::Source> Iterator for Clamped<S> {
    type Item = rodio::Sample;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|s| s.clamp(-1.0, 1.0))
    }
}

impl<S: rodio::Source> rodio::Source for Clamped<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.0.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.0.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.0.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.0.total_duration()
    }
}

/// Taps on each side of the sinc filter.
const TAPS: usize = 16;
/// Filter phases between two input samples (the nearest one is used).
const PHASES: usize = 512;

/// A source resampled to `rate` with a Blackman-windowed sinc filter, cut
/// off a little below the lower of the two Nyquist frequencies, and clamped
/// to ±1.0 (the filter can overshoot a little on a song mastered to the top).
pub struct Resampled<S> {
    input: S,
    channels: usize,
    from: u32,
    to: u32,
    /// `PHASES + 1` rows of `2 * TAPS` weights.
    table: Vec<f32>,
    /// Input frames from `base` on, interleaved.
    frames: std::collections::VecDeque<f32>,
    /// Input frame index of `frames`' first frame.
    base: i64,
    /// Input frames read so far (the zeros at the end not counted).
    read: i64,
    done: bool,
    /// Output frame being made, as a fraction `step` of input frames.
    out_frame: u64,
    /// The output frame being handed out, one channel at a time.
    pending: Vec<f32>,
    next_channel: usize,
}

impl<S: rodio::Source> Resampled<S> {
    pub fn new(input: S, to: u32) -> Self {
        let channels = usize::from(input.channels().get());
        let from = input.sample_rate().get();
        let cutoff = 0.92 * (f64::from(to) / f64::from(from)).min(1.0) * 0.5;
        let n = 2 * TAPS;
        let mut table = Vec::with_capacity((PHASES + 1) * n);
        for phase in 0..=PHASES {
            let frac = phase as f64 / PHASES as f64;
            let row: Vec<f64> = (0..n)
                .map(|k| {
                    // Distance from the output point to input sample k.
                    let x = k as f64 - (TAPS as f64 - 1.0) - frac;
                    let sinc = if x.abs() < 1e-9 {
                        2.0 * cutoff
                    } else {
                        (2.0 * std::f64::consts::PI * cutoff * x).sin() / (std::f64::consts::PI * x)
                    };
                    let w = (x + TAPS as f64) / (2.0 * TAPS as f64);
                    let blackman = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * w).cos()
                        + 0.08 * (4.0 * std::f64::consts::PI * w).cos();
                    sinc * blackman.max(0.0)
                })
                .collect();
            // Each row adds up to 1, so a steady level stays that level.
            let sum: f64 = row.iter().sum();
            table.extend(row.iter().map(|w| (w / sum) as f32));
        }
        Self {
            input,
            channels,
            from,
            to,
            table,
            frames: std::collections::VecDeque::new(),
            base: 0,
            read: 0,
            done: false,
            out_frame: 0,
            pending: Vec::new(),
            next_channel: 0,
        }
    }

    /// Input frame `i`: zero before the start and after the end.
    fn frame_sample(&self, i: i64, c: usize) -> f32 {
        if i < self.base {
            return 0.0;
        }
        let at = (i - self.base) as usize * self.channels + c;
        self.frames.get(at).copied().unwrap_or(0.0)
    }

    /// Reads input until frame `upto` is there (or the input ends).
    fn fill(&mut self, upto: i64) {
        while !self.done && self.read <= upto {
            let mut frame = Vec::with_capacity(self.channels);
            for _ in 0..self.channels {
                match self.input.next() {
                    Some(s) => frame.push(s),
                    None => break,
                }
            }
            if frame.len() < self.channels {
                self.done = true;
                break;
            }
            self.frames.extend(frame);
            self.read += 1;
        }
    }

    fn make_frame(&mut self) -> bool {
        // Where this output frame falls in the input, in input frames.
        let pos = self.out_frame as f64 * f64::from(self.from) / f64::from(self.to);
        let i = pos.floor() as i64;
        if self.done && i >= self.read {
            return false;
        }
        let first = i - (TAPS as i64 - 1);
        self.fill(i + TAPS as i64);
        if self.done && i >= self.read {
            return false;
        }
        // Frames before the filter's reach are no longer needed.
        while self.base < first && !self.frames.is_empty() {
            for _ in 0..self.channels {
                self.frames.pop_front();
            }
            self.base += 1;
        }
        let phase = (((pos - i as f64) * PHASES as f64).round() as usize).min(PHASES);
        let n = 2 * TAPS;
        let row = &self.table[phase * n..phase * n + n];
        self.pending.clear();
        for c in 0..self.channels {
            let mut acc = 0.0f32;
            for (k, w) in row.iter().enumerate() {
                acc += w * self.frame_sample(first + k as i64, c);
            }
            self.pending.push(acc.clamp(-1.0, 1.0));
        }
        self.out_frame += 1;
        self.next_channel = 0;
        true
    }
}

impl<S: rodio::Source> Iterator for Resampled<S> {
    type Item = rodio::Sample;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_channel >= self.pending.len() && !self.make_frame() {
            return None;
        }
        let s = self.pending[self.next_channel];
        self.next_channel += 1;
        Some(s)
    }
}

impl<S: rodio::Source> rodio::Source for Resampled<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        rodio::ChannelCount::new(self.channels as u16).unwrap_or(rodio::nz!(2))
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        rodio::SampleRate::new(self.to).unwrap_or(rodio::nz!(48000))
    }
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junk_is_not_a_song() {
        assert!(decoder(b"not audio at all, just text".to_vec()).is_err());
    }

    #[test]
    fn file_gain_scales_the_volume_like_the_website_and_never_clips() {
        // -6 dB is half the amplitude; 0 dB changes nothing.
        assert!((level(100, -6.0206) - 0.5).abs() < 1e-3);
        assert_eq!(level(70, 0.0), gain(70));
        assert!((level(50, -3.0) - gain(50) * 0.70795).abs() < 1e-4);
        // Turned up: louder below full volume, never above 1.0.
        assert!(level(50, 3.0) > gain(50));
        assert_eq!(level(100, 6.0), 1.0);
        assert_eq!(level(100, 40.0), 1.0);
        assert!(level(90, 2.0) <= 1.0);
        // Nonsense from the server is ignored.
        assert_eq!(level(80, f32::NAN), gain(80));
        assert_eq!(level(0, 6.0), 0.0);
    }

    #[test]
    fn muted_is_silent_and_unmuting_brings_back_the_volume() {
        let mut a = Audio::open(true, 60);
        a.start(None, -2.0, true).unwrap();
        assert_eq!(a.level(), 0.0);
        a.set_volume(80);
        assert_eq!(a.level(), 0.0, "a volume change while muted is kept, not heard");
        a.set_muted(false);
        assert_eq!(a.level(), level(80, -2.0));
        a.start(None, 0.0, true).unwrap();
        a.stop();
        assert!(!a.is_muted(), "stopping (an ad failing, quitting) unmutes");
    }

    /// A tone as a rodio source.
    fn tone(freq: f32, rate: u32, secs: f32, amp: f32) -> rodio::buffer::SamplesBuffer {
        let n = (rate as f32 * secs) as usize;
        let samples: Vec<f32> = (0..n)
            .flat_map(|i| {
                let v = amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin();
                [v, v]
            })
            .collect();
        rodio::buffer::SamplesBuffer::new(rodio::nz!(2), rodio::SampleRate::new(rate).unwrap(), samples)
    }

    #[test]
    fn resampling_keeps_the_length_and_the_tone_and_stays_inside_full_scale() {
        use rodio::Source;
        let r = Resampled::new(tone(1000.0, 22050, 1.0, 0.99), 48000);
        assert_eq!(r.sample_rate().get(), 48000);
        assert_eq!(r.channels().get(), 2);
        let out: Vec<f32> = r.collect();
        let frames = out.len() / 2;
        assert!((47_900..=48_100).contains(&frames), "{frames} frames for a second");
        assert!(out.iter().all(|s| s.abs() <= 1.0));
        // Away from the ends: the same 0.99 peak, and both channels alike.
        let middle = &out[2000..out.len() - 2000];
        let peak = middle.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!((0.97..=1.0).contains(&peak), "peak {peak}");
        assert!(middle.chunks(2).all(|f| (f[0] - f[1]).abs() < 1e-6));
        // A 1 kHz tone crosses zero 2000 times a second.
        let left: Vec<f32> = middle.iter().step_by(2).copied().collect();
        let crossings = left.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count() as f32;
        let secs = left.len() as f32 / 48000.0;
        assert!((crossings / secs - 1000.0).abs() < 5.0, "{} Hz", crossings / secs);
    }

    #[test]
    fn resampling_is_smoother_than_straight_lines() {
        // A 9 kHz tone at 22.05 kHz, taken to 48 kHz: what is left over after
        // removing the tone itself is the noise added. The sinc filter's must be
        // far below what linear interpolation (rodio's converter) leaves.
        let (freq, from, to) = (9000.0f64, 22050u32, 48000u32);
        let input: Vec<f32> = tone(freq as f32, from, 0.5, 0.5).step_by(2).collect();
        let ours: Vec<f32> = Resampled::new(tone(freq as f32, from, 0.5, 0.5), to).step_by(2).collect();
        let ideal = |j: usize| 0.5 * (2.0 * std::f64::consts::PI * freq * j as f64 / f64::from(to)).sin();
        let linear = |j: usize| {
            let pos = j as f64 * f64::from(from) / f64::from(to);
            let (i, t) = (pos.floor() as usize, pos.fract());
            f64::from(input[i]) * (1.0 - t) + f64::from(*input.get(i + 1).unwrap_or(&0.0)) * t
        };
        let range = 1000..ours.len() - 1000;
        let err = |f: &dyn Fn(usize) -> f64| {
            (range.clone().map(|j| (f(j) - ideal(j)).powi(2)).sum::<f64>() / range.len() as f64).sqrt()
        };
        let (sinc, lin) = (err(&|j| f64::from(ours[j])), err(&linear));
        assert!(sinc * 10.0 < lin, "sinc {sinc:.5} vs linear {lin:.5}");
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
            let (rate, channels) = (d.sample_rate(), d.channels());
            let (mut n, mut peak, mut over) = (0usize, 0f32, 0usize);
            for s in d {
                n += 1;
                peak = peak.max(s.abs());
                over += usize::from(s.abs() > 1.0);
            }
            let secs = n as f32 / f32::from(channels.get()) / rate.get() as f32;
            eprintln!("{file}: {rate:?} Hz, {channels:?} ch, {secs:.1} s, peak {peak:.3}, {over} samples over 1.0");
            assert!(n > 0, "{file} decodes to samples");
        }
    }
}
