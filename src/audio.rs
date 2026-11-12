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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::log::log;

/// A stream that has not moved for this long has lost its device (unplugged,
/// or PipeWire restarted): no samples are being pulled any more.
const STALL_AFTER: Duration = Duration::from_secs(3);

/// Spots a stream that stopped moving.
///
/// A dead stream does not report anything through [`Audio::finished`]: its
/// queue still holds the song, so it never looks empty, and its position stops
/// advancing because nothing is consuming samples. Without this the player sat
/// on a frozen progress bar for ever.
#[derive(Default)]
struct Stall {
    seen: Option<(Duration, Instant)>,
}

impl Stall {
    /// Whether `pos` has not moved since it was last seen moving.
    fn stuck(&mut self, pos: Duration, now: Instant) -> bool {
        match self.seen {
            Some((last, at)) if pos == last => now.duration_since(at) >= STALL_AFTER,
            _ => {
                self.seen = Some((pos, now));
                false
            }
        }
    }

    fn reset(&mut self) {
        self.seen = None;
    }
}

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
    /// The last error the sound device reported, if any (see [`Audio::take_device_error`]).
    device_error: Arc<Mutex<Option<String>>>,
    /// Spots a stream that stopped moving (see [`Stall`]).
    stall: Stall,
}

impl Audio {
    /// Opens the default sound device; `silent` (demo) skips it.
    pub fn open(silent: bool, volume: u8) -> Self {
        let device_error: Arc<Mutex<Option<String>>> = Arc::default();
        let (device, problem) = if silent {
            (None, None)
        } else {
            match open_device(Arc::clone(&device_error)) {
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
            device_error,
            stall: Stall::default(),
        }
    }

    pub fn has_device(&self) -> bool {
        self.device.is_some()
    }

    /// The sound device's last error, once (the stream died under us).
    ///
    /// rodio's default callback prints to stderr, which the interface owns:
    /// the message would land in the middle of the screen and be wiped by the
    /// next frame. Taking it here puts it in the log and the status line.
    pub fn take_device_error(&mut self) -> Option<String> {
        self.device_error.lock().ok()?.take()
    }

    /// Whether the stream has stopped moving (the device went away).
    pub fn stalled(&mut self) -> bool {
        if self.player.is_none() || self.paused {
            return false;
        }
        let pos = self.position();
        self.stall.stuck(pos, Instant::now())
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
        self.stall.reset();
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
        self.stall.reset();
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
///
/// The error callback is ours rather than rodio's default, which prints to
/// stderr: the interface owns the screen, so that message would appear over
/// the UI and be wiped by the next frame. It is kept for [`Audio::take_device_error`].
fn open_device(device_error: Arc<Mutex<Option<String>>>) -> Result<rodio::MixerDeviceSink, rodio::DeviceSinkError> {
    let on_error = move |e: rodio::cpal::Error| {
        if let Ok(mut slot) = device_error.lock() {
            *slot = Some(e.to_string());
        }
    };
    rodio::DeviceSinkBuilder::from_default_device()
        .map(|b| b.with_error_callback(on_error.clone()))
        .and_then(|b| b.with_buffer_size(rodio::cpal::BufferSize::Fixed(BUFFER_FRAMES)).open_stream())
        .or_else(|_| {
            rodio::DeviceSinkBuilder::from_default_device()
                .map(|b| b.with_error_callback(on_error))
                .and_then(|b| b.open_stream())
        })
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
mod tests;

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
