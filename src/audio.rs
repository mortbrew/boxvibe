//! Playback: rodio (cpal → ALSA/PipeWire), with symphonia decoding MP3 and
//! [`syom`] decoding AAC, no ffmpeg. Without a sound device (or in `--demo`)
//! a silent clock stands in, so the interface still works.
//!
//! AAC goes to syom rather than symphonia because Pandora's free tier serves
//! HE-AAC: a 22.05 kHz AAC-LC core that SBR doubles to 44.1 kHz, with PS for
//! stereo. symphonia decodes only the core, so that stream came out at
//! 22.05 kHz, mono, with no high band — audibly dull next to Pithos, which
//! uses GStreamer (issue #11). syom applies SBR and PS.
//!
//! What reaches the speakers: the decoded song, resampled to the sound
//! device's rate with a windowed-sinc filter ([`Resampled`]; rodio's own
//! converter interpolates in straight lines, which adds a gritty edge), kept
//! inside ±1.0, then scaled by [`level`]: the volume curve times Pandora's
//! per-song `fileGain`, never above 1.0.

use std::io::Cursor;
use std::time::{Duration, Instant};

use crate::log::log;

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
        let decoder = decoder(data)?;
        let player = rodio::Player::connect_new(device.mixer());
        player.set_volume(self.level());
        match self.rate {
            Some(rate) if rate != rodio::Source::sample_rate(&decoder).get() => {
                log!("start: resampling to the device's {rate} Hz");
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

    #[cfg(test)]
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

/// A decoder for a whole downloaded song.
///
/// MP3 goes through symphonia. AAC does not: symphonia decodes only the
/// AAC-LC core of Pandora's `aacplus`, so an HE-AAC stream (a 22.05 kHz core
/// with SBR to 44.1 kHz) comes out at half the rate with no high band, and
/// mono. [`syom`] applies SBR and PS, so the same stream comes out at its
/// real rate and channel count.
pub fn decoder(data: Vec<u8>) -> Result<Box<dyn rodio::Source + Send>, String> {
    // Which decoder gets it is decided by the container, not by trying one
    // and reading its error: syom reports an MP3 behind an ID3 tag as
    // `Malformed(AdtsLayer)`, not `NotAac`, so a fallback on the error alone
    // sent every MP3 to syom and skipped it.
    let tag = id3_len(&data);
    let is_aac = is_aac(&data);
    log!(
        "decode: {} bytes, id3 {tag}, {}, head {}",
        data.len(),
        if is_aac { "aac" } else { "mp3" },
        crate::log::head(&data)
    );
    let decoded = if is_aac {
        // A container that looks like AAC but that syom cannot read is still
        // worth trying as MP3: a wrong guess must not cost a song.
        match aac(&data) {
            Ok(source) => Ok(source),
            Err(e) => {
                log!("decode: syom could not read it ({e}), trying symphonia");
                mp3(data)
            }
        }
    } else {
        mp3(data)
    };
    match &decoded {
        Ok(source) => {
            use rodio::Source;
            log!(
                "decode: ok, {} Hz, {} ch, {:?}",
                source.sample_rate().get(),
                source.channels().get(),
                source.total_duration()
            );
        }
        Err(e) => log!("decode: failed: {e}"),
    }
    decoded
}

/// Whether `data` is AAC (ADTS, LATM/LOAS or MP4) rather than MP3.
///
/// Looks past an ID3v2 tag, which Pandora's MP3s carry and which hides the
/// first frame from a plain header check.
fn is_aac(data: &[u8]) -> bool {
    let body = &data[id3_len(data)..];
    match body {
        // ADTS: 12 sync bits, then a layer that must be 0.
        [0xff, b, ..] if b & 0xf6 == 0xf0 => true,
        // LATM/LOAS: 11 sync bits.
        [0x56, b, ..] if b & 0xe0 == 0xe0 => true,
        _ => is_mp4(body),
    }
}

/// Whether `data` is an MP4/M4A file: a `ftyp` box, which is not always the
/// first one (a `free`, `skip` or `wide` box may come before it).
fn is_mp4(data: &[u8]) -> bool {
    let mut at = 0usize;
    // A handful of boxes is plenty: `ftyp` is at or near the start.
    for _ in 0..8 {
        let Some(header) = data.get(at..at + 8) else { return false };
        let size = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let kind = &header[4..8];
        if kind == b"ftyp" {
            return true;
        }
        // A box that is not one of the leading ones means this is not MP4.
        if !matches!(kind, b"free" | b"skip" | b"wide" | b"mdat" | b"moov") {
            return false;
        }
        // 0 means "to the end of the file", 1 means a 64-bit size follows.
        if size < 8 {
            return false;
        }
        at += size;
    }
    false
}

/// How many bytes an ID3v2 tag at the start of `data` takes up (0 if none).
///
/// The size is four 7-bit bytes after a 10-byte header, and a footer adds 10
/// more when its flag is set.
fn id3_len(data: &[u8]) -> usize {
    let Some([b'I', b'D', b'3', _, _, flags, s0, s1, s2, s3, ..]) = data.get(..10) else { return 0 };
    let size = (u32::from(*s0 & 0x7f) << 21)
        | (u32::from(*s1 & 0x7f) << 14)
        | (u32::from(*s2 & 0x7f) << 7)
        | u32::from(*s3 & 0x7f);
    let footer = if flags & 0x10 != 0 { 10 } else { 0 };
    (10 + size as usize + footer).min(data.len())
}

/// AAC (ADTS or MP4) through syom, which decodes HE-AAC properly.
fn aac(data: &[u8]) -> Result<Box<dyn rodio::Source + Send>, syom::AacError> {
    let decoded = syom::decode_with(data, &syom::DecodeOptions::audio())?;
    let channels = decoded.channels.len();
    if channels == 0 || decoded.sample_rate == 0 {
        return Err(syom::AacError::NotAac);
    }
    // syom hands back one plane per channel; rodio wants them interleaved.
    let frames = decoded.channels[0].len();
    let mut samples = Vec::with_capacity(frames * channels);
    for i in 0..frames {
        for plane in &decoded.channels {
            samples.push(plane.get(i).copied().unwrap_or(0.0));
        }
    }
    let channels = rodio::ChannelCount::new(channels as u16).ok_or(syom::AacError::NotAac)?;
    let rate = rodio::SampleRate::new(decoded.sample_rate).ok_or(syom::AacError::NotAac)?;
    Ok(Box::new(rodio::buffer::SamplesBuffer::new(channels, rate, samples)))
}

/// MP3 through symphonia, which handles it well.
fn mp3(data: Vec<u8>) -> Result<Box<dyn rodio::Source + Send>, String> {
    let len = data.len() as u64;
    rodio::Decoder::builder()
        .with_data(Cursor::new(data))
        .with_byte_len(len)
        .with_seekable(true)
        .with_mime_type("audio/mpeg")
        .build()
        .map(|d| Box::new(d) as Box<dyn rodio::Source + Send>)
        .map_err(|e| format!("can't decode this song: {e}"))
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
        // And it says so in terms of the song, not with syom's "Unsupported
        // audio format" from the AAC attempt that was never going to work.
        let Err(why) = decoder(b"<html>not a song</html>".to_vec()) else {
            panic!("an HTML error page decoded as audio");
        };
        assert!(why.starts_with("can't decode this song: "), "{why}");
        assert!(!why.contains("Unsupported audio format"), "{why}");
    }

    /// An MP3 frame: 128 kbps, 44.1 kHz, stereo, 417 bytes of silence.
    fn mp3_frame() -> Vec<u8> {
        let mut f = vec![0xff, 0xfb, 0x90, 0x64];
        f.resize(417, 0);
        f
    }

    /// An ID3v2.4 tag of `size` bytes, as Pandora's MP3s carry.
    fn id3(size: usize) -> Vec<u8> {
        let mut t = b"ID3\x04\x00\x00".to_vec();
        t.extend([(size >> 21) as u8 & 0x7f, (size >> 14) as u8 & 0x7f, (size >> 7) as u8 & 0x7f, size as u8 & 0x7f]);
        t.resize(10 + size, 0);
        t
    }

    /// The bug behind the skipping: an MP3 behind an ID3 tag is not AAC, but
    /// syom called it `Malformed(AdtsLayer)` rather than `NotAac`, so the
    /// fallback never ran and every MP3 was skipped instead of played.
    #[test]
    fn an_mp3_behind_an_id3_tag_is_not_taken_for_aac() {
        let mut tagged = id3(64);
        tagged.extend(mp3_frame());
        assert!(!super::is_aac(&tagged), "an MP3 with an ID3 tag is not AAC");
        assert!(super::decoder(tagged).is_ok(), "and it decodes as MP3");
    }

    #[test]
    fn the_container_is_recognised_past_an_id3_tag() {
        // ADTS, with and without a tag in front.
        let adts = [0xff, 0xf1, 0x50, 0x80];
        assert!(super::is_aac(&adts));
        let mut tagged = id3(32);
        tagged.extend(adts);
        assert!(super::is_aac(&tagged), "the tag is skipped, not the frame");
        // LATM/LOAS and MP4.
        assert!(super::is_aac(&[0x56, 0xe0, 0x00]));
        assert!(super::is_aac(b"\0\0\0\x20ftypM4A "));
        assert!(!super::is_aac(&mp3_frame()));
        assert!(!super::is_aac(b""));
        assert!(!super::is_aac(b"ID3"));
    }

    /// A `free` box before `ftyp` is legal, and ffmpeg writes one: such a
    /// file was taken for MP3 and skipped before this was handled.
    #[test]
    fn an_mp4_is_recognised_behind_a_leading_free_box() {
        let mut data = Vec::new();
        data.extend(8u32.to_be_bytes());
        data.extend(b"free");
        data.extend(0x20u32.to_be_bytes());
        data.extend(b"ftypM4A ");
        assert!(super::is_aac(&data), "ftyp is not always the first box");
        // A `wide` box too, and a 64-bit `mdat` size is not mistaken for one.
        let mut wide = Vec::new();
        wide.extend(8u32.to_be_bytes());
        wide.extend(b"wide");
        wide.extend(0x20u32.to_be_bytes());
        wide.extend(b"ftypM4A ");
        assert!(super::is_aac(&wide));
        // A box that is not one of the leading ones is not MP4.
        let mut other = Vec::new();
        other.extend(8u32.to_be_bytes());
        other.extend(b"junk");
        other.extend(0x20u32.to_be_bytes());
        other.extend(b"ftypM4A ");
        assert!(!super::is_aac(&other));
    }

    #[test]
    fn an_id3_tag_is_measured_by_its_own_size() {
        assert_eq!(super::id3_len(b"not a tag at all"), 0);
        assert_eq!(super::id3_len(&id3(0)), 10);
        assert_eq!(super::id3_len(&id3(100)), 110);
        // A tag claiming more than the file holds stops at the end.
        let mut short = id3(100);
        short.truncate(40);
        assert_eq!(super::id3_len(&short), 40);
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

#[cfg(test)]
mod he_aac {
    use rodio::Source;

    /// A 64 kbps HE-AAC stream, the shape Pandora's free tier serves: a
    /// 22.05 kHz core that SBR doubles to 44.1 kHz, in stereo.
    fn he_aac() -> Vec<u8> {
        let pcm: Vec<f32> = (0..44_100 * 2).map(|i| 0.3 * (i as f32 * 0.14).sin()).collect();
        let opts = syom::EncodeOptions::adts().with_bitrate_bps(64_000).with_he(true);
        syom::encode_with(&[&pcm[..]], 44_100, &opts).unwrap()
    }

    /// The bug behind issue #11: symphonia decodes only the AAC-LC core, so
    /// this stream came out at 22.05 kHz, mono, with no high band. syom
    /// applies SBR and PS, so it comes out at its real rate and channels.
    #[test]
    fn he_aac_keeps_its_rate_and_channels() {
        let d = super::decoder(he_aac()).unwrap();
        assert_eq!(d.sample_rate().get(), 44_100, "SBR applied, not the 22.05 kHz core");
        assert_eq!(d.channels().get(), 2, "stereo kept, not mixed to mono");
        let frames = d.count() / 2;
        assert!((87_000..=89_000).contains(&frames), "{frames} frames for two seconds");
    }

    /// AAC-LC still decodes, and still through the same entry point.
    #[test]
    fn aac_lc_still_decodes() {
        let pcm: Vec<f32> = (0..44_100).map(|i| 0.3 * (i as f32 * 0.14).sin()).collect();
        let lc = syom::encode_with(&[&pcm[..]], 44_100, &syom::EncodeOptions::adts()).unwrap();
        let d = super::decoder(lc).unwrap();
        assert_eq!(d.sample_rate().get(), 44_100);
        assert!(d.count() > 40_000);
    }
}
