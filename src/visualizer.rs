//! The spectrum bars Now Playing shows when there is no cover: the decoded
//! audio is tapped on its way to the speakers ([`Tap`]), and each frame the
//! newest samples go through an FFT into log-spaced bands.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::Source;
use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

/// Samples kept for the FFT (mono).
pub const WINDOW: usize = 2048;

/// The newest decoded samples, mixed to mono, shared with the audio thread.
#[derive(Clone, Default)]
pub struct Samples {
    inner: Arc<Mutex<(VecDeque<f32>, u32)>>,
}

impl Samples {
    /// The newest [`WINDOW`] samples and the sample rate.
    pub fn snapshot(&self) -> (Vec<f32>, u32) {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        (g.0.iter().copied().collect(), g.1)
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).0.clear();
    }

    fn push(&self, frame: &[f32], rate: u32) {
        let mono = frame.iter().sum::<f32>() / frame.len().max(1) as f32;
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.1 = rate;
        if g.0.len() >= WINDOW {
            g.0.pop_front();
        }
        g.0.push_back(mono);
    }
}

/// A rodio source that passes its samples on unchanged and copies them to [`Samples`].
pub struct Tap<S> {
    source: S,
    out: Samples,
    frame: Vec<f32>,
}

impl<S: Source> Tap<S> {
    pub fn new(source: S, out: Samples) -> Self {
        Self { source, out, frame: Vec::with_capacity(2) }
    }
}

impl<S: Source> Iterator for Tap<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let s = self.source.next()?;
        self.frame.push(s);
        if self.frame.len() >= usize::from(self.source.channels().get()) {
            self.out.push(&self.frame, self.source.sample_rate().get());
            self.frame.clear();
        }
        Some(s)
    }
}

impl<S: Source> Source for Tap<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.source.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.source.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.source.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.source.total_duration()
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.frame.clear();
        self.source.try_seek(pos)
    }
}

/// Band levels 0.0–1.0 for `n` log-spaced bands from 40 Hz to 16 kHz.
pub fn bands(samples: &[f32], rate: u32, n: usize) -> Vec<f32> {
    if n == 0 {
        return Vec::new();
    }
    let len = samples.len().next_power_of_two().min(WINDOW);
    if samples.len() < 64 || rate == 0 {
        return vec![0.0; n];
    }
    let start = samples.len().saturating_sub(len);
    let tail = &samples[start..];
    let len = tail.len();
    // Hann window.
    let mut buf: Vec<Complex<f32>> = tail
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (len - 1) as f32).cos();
            Complex::new(s * w, 0.0)
        })
        .collect();
    FftPlanner::new().plan_fft_forward(len).process(&mut buf);
    let hz_per_bin = rate as f32 / len as f32;
    let (lo, hi) = (40f32, 16_000f32.min(rate as f32 / 2.0));
    (0..n)
        .map(|b| {
            let f0 = lo * (hi / lo).powf(b as f32 / n as f32);
            let f1 = lo * (hi / lo).powf((b + 1) as f32 / n as f32);
            let i0 = ((f0 / hz_per_bin) as usize).max(1);
            let i1 = ((f1 / hz_per_bin).ceil() as usize).clamp(i0 + 1, len / 2);
            let peak = buf[i0.min(len / 2 - 1)..i1].iter().map(|c| c.norm()).fold(0.0, f32::max);
            // Magnitude to dB, -60…0 dB as 0…1 (a full-scale sine is about len/4).
            let db = 20.0 * (peak / (len as f32 / 4.0)).max(1e-6).log10();
            ((db + 60.0) / 60.0).clamp(0.0, 1.0)
        })
        .collect()
}

/// Bars that rise at once and fall smoothly, with peak caps that linger.
#[derive(Default, Clone)]
pub struct Bars {
    pub levels: Vec<f32>,
    pub peaks: Vec<f32>,
}

impl Bars {
    pub fn update(&mut self, target: &[f32]) {
        if self.levels.len() != target.len() {
            self.levels = vec![0.0; target.len()];
            self.peaks = vec![0.0; target.len()];
        }
        for (i, t) in target.iter().enumerate() {
            let l = &mut self.levels[i];
            *l = if *t > *l { *t } else { (*l * 0.8 + t * 0.2).max(*l - 0.08) };
            let p = &mut self.peaks[i];
            *p = if *l >= *p { *l } else { (*p - 0.02).max(0.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, rate: u32) -> Vec<f32> {
        (0..WINDOW).map(|i| (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin()).collect()
    }

    #[test]
    fn a_tone_lights_its_band() {
        for hz in [100.0, 1000.0, 8000.0] {
            let b = bands(&sine(hz, 44_100), 44_100, 16);
            let loudest = b.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
            let want = (16.0 * (hz / 40.0).ln() / (16_000f32 / 40.0).ln()) as usize;
            assert!(loudest.abs_diff(want) <= 1, "{hz} Hz in band {loudest}, want {want}: {b:?}");
            assert!(b[loudest] > 0.8, "full scale is near the top: {b:?}");
        }
    }

    #[test]
    fn silence_and_too_little_are_flat() {
        assert!(bands(&[0.0; WINDOW], 44_100, 12).iter().all(|l| *l == 0.0));
        assert_eq!(bands(&[0.5; 10], 44_100, 5), vec![0.0; 5]);
        assert!(bands(&sine(440.0, 48_000), 48_000, 0).is_empty());
    }

    #[test]
    fn bars_fall_slowly_and_peaks_linger() {
        let mut bars = Bars::default();
        bars.update(&[1.0, 0.0]);
        assert_eq!(bars.levels, vec![1.0, 0.0]);
        bars.update(&[0.0, 0.0]);
        assert!(bars.levels[0] < 1.0 && bars.levels[0] > 0.8);
        assert!(bars.peaks[0] > bars.levels[0]);
    }

    #[test]
    fn the_tap_passes_samples_through_and_keeps_mono() {
        let rate = std::num::NonZero::new(8000).unwrap();
        let ch = std::num::NonZero::new(2).unwrap();
        let src = rodio::buffer::SamplesBuffer::new(ch, rate, vec![1.0, 0.0, 0.5, 0.5]);
        let out = Samples::default();
        let got: Vec<f32> = Tap::new(src, out.clone()).collect();
        assert_eq!(got, vec![1.0, 0.0, 0.5, 0.5]);
        assert_eq!(out.snapshot(), (vec![0.5, 0.5], 8000));
    }
}
