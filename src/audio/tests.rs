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
