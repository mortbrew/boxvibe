use super::*;
use image::GenericImageView;

fn look(mode: AlbumArt, pixelation: RetroMode) -> Look {
    Look { mode, pixelation, tint: ArtTint::Off, palette: crate::theme::BUILT_IN[0].2 }
}

/// `resize_exact` with Nearest, the two-pass version `pixelate` replaced.
fn resized(image: &DynamicImage, pixelation: RetroMode) -> DynamicImage {
    let Some(grid) = pixelation.grid() else { return image.clone() };
    let (w, h) = (image.width().max(1), image.height().max(1));
    let (gw, gh) = if w >= h { (grid, (grid * h / w).max(1)) } else { ((grid * w / h).max(1), grid) };
    if gw >= w && gh >= h {
        return image.clone();
    }
    image.resize_exact(gw, gh, FilterType::Nearest).resize_exact(w, h, FilterType::Nearest)
}

#[test]
fn pixelation_matches_resize_exact_pixel_for_pixel() {
    // Shapes either side of the grid, wide and tall, and an odd size where
    // the sampling rounds differently from plain integer division.
    for (w, h) in [(256, 256), (320, 200), (200, 320), (640, 360), (37, 91), (8, 8), (512, 512)] {
        let mut cover = image::RgbImage::new(w, h);
        for (x, y, px) in cover.enumerate_pixels_mut() {
            // Asymmetric, so a half-pixel shift anywhere shows up.
            *px = image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x * 7 + y * 13) % 239) as u8]);
        }
        let cover = DynamicImage::ImageRgb8(cover);
        for mode in RetroMode::ALL {
            let want = resized(&cover, mode).to_rgb8();
            let got = pixelate(&cover, mode).to_rgb8();
            assert_eq!(want.dimensions(), got.dimensions(), "{w}x{h} {mode:?}: size");
            let differing = want.pixels().zip(got.pixels()).filter(|(a, b)| a != b).count();
            assert_eq!(differing, 0, "{w}x{h} {mode:?}: {differing} pixels differ");
        }
    }
}

#[test]
fn a_cached_cover_is_the_same_pixels_as_an_uncached_one() {
    let mut art = Art::default();
    art.arrived("u".into(), Some(demo_cover("u")));
    let image = art.images.get("u").cloned().expect("just arrived");
    let palette = crate::theme::BUILT_IN[0].2;
    for pixelation in RetroMode::ALL {
        for art_tint in [ArtTint::Off, ArtTint::Duotone, ArtTint::Palette] {
            let look = Look { mode: AlbumArt::Picture, pixelation, tint: art_tint, palette };
            let want = tint(&pixelate(&image, pixelation), art_tint, &palette).to_rgb8();
            // Twice: the second is the cache hit, and must match the first.
            for pass in 0..2 {
                let got = art.prepared("u", &image, look).to_rgb8();
                let differing = want.pixels().zip(got.pixels()).filter(|(a, b)| a != b).count();
                assert_eq!(differing, 0, "{pixelation:?} {art_tint:?} pass {pass}: {differing} differ");
            }
        }
    }
}

#[test]
fn pixelation_makes_big_square_pixels_at_the_same_size() {
    let cover = demo_cover("x");
    assert_eq!(pixelate(&cover, RetroMode::Hd).dimensions(), (256, 256));
    let blocky = pixelate(&cover, RetroMode::Bit2);
    assert_eq!(blocky.dimensions(), (256, 256));
    // 8 px across: each 32×32 cell is one colour.
    let c = blocky.get_pixel(33, 70);
    assert!((32..64).all(|x| (64..96).all(|y| blocky.get_pixel(x, y) == c)));
    let distinct: HashSet<_> = (0..256).step_by(4).map(|x| blocky.get_pixel(x, 5).0).collect();
    assert!(distinct.len() <= 8, "{}", distinct.len());
}

#[test]
fn tint_uses_only_the_themes_colours() {
    let p = crate::theme::BUILT_IN.iter().find(|t| t.0 == "gruvbox").unwrap().2;
    let cover = demo_cover("x");
    assert_eq!(tint(&cover, ArtTint::Off, &p), cover);
    let rgb = |c| rgb_of(c).unwrap();
    let theme = [
        p.background,
        p.fg,
        p.muted,
        p.highlight,
        p.line,
        p.blue,
        p.cyan,
        p.magenta,
        p.green,
        p.orange,
        p.yellow,
        p.red,
    ]
    .map(rgb);
    let pal = tint(&cover, ArtTint::Palette, &p).to_rgb8();
    assert!(pal.pixels().all(|px| theme.contains(&px.0)), "only theme colours");
    // Duotone: black is the background, white the yellow, grey between.
    let mut bw = RgbImage::new(3, 1);
    bw.put_pixel(0, 0, Rgb([0, 0, 0]));
    bw.put_pixel(1, 0, Rgb([255, 255, 255]));
    bw.put_pixel(2, 0, Rgb([128, 128, 128]));
    let duo = tint(&DynamicImage::ImageRgb8(bw), ArtTint::Duotone, &p).to_rgb8();
    assert_eq!((duo.get_pixel(0, 0).0, duo.get_pixel(1, 0).0), (rgb(p.background), rgb(p.yellow)));
    let mid = duo.get_pixel(2, 0).0;
    assert!((0..3).all(|i| mid[i] >= rgb(p.background)[i].min(rgb(p.yellow)[i])
        && mid[i] <= rgb(p.background)[i].max(rgb(p.yellow)[i])));
}

#[test]
fn a_new_theme_or_tint_rebuilds_the_cover() {
    let mut art = Art::default();
    art.arrived("u".into(), Some(demo_cover("u")));
    let p = crate::theme::BUILT_IN[0].2;
    let mut look = Look { mode: AlbumArt::Picture, pixelation: RetroMode::Hd, tint: ArtTint::Duotone, palette: p };
    let first = art.protocol("u", look).map(|_| ()).is_some();
    assert!(first);
    let ptr = |art: &Art| art.shown.borrow().as_ref().map(|s| s.look);
    assert_eq!(ptr(&art), Some(look));
    look.palette = crate::theme::BUILT_IN[1].2;
    assert!(art.protocol("u", look).is_some());
    assert_eq!(ptr(&art), Some(look), "rebuilt for the new theme");
}

#[test]
fn changing_the_level_swaps_the_cover_without_clearing_the_screen() {
    let mut art = Art::default();
    art.arrived("u".into(), Some(demo_cover("u")));
    let mut sixel = Picker::halfblocks();
    sixel.set_protocol_type(ProtocolType::Sixel);
    art.set_graphics(Some(sixel));
    let pic = look(AlbumArt::Picture, RetroMode::Hd);
    assert!(art.protocol("u", pic).is_some_and(|p| !is_blocks(&p)));
    art.needs_clear.take();
    for level in [RetroMode::Bit32, RetroMode::Bit16, RetroMode::Bit2, RetroMode::Hd] {
        let next = Look { pixelation: level, ..pic };
        assert!(art.protocol("u", next).is_some_and(|p| !is_blocks(&p)), "{level:?} is still a picture");
        assert!(!art.needs_clear.take(), "{level:?} needed no screen clear");
        assert_eq!(art.shown.borrow().as_ref().map(|s| s.look), Some(next), "{level:?} was swapped in");
    }
}

#[test]
fn a_popup_shows_the_cover_in_half_blocks_and_clears_around_it() {
    let mut art = Art::default();
    art.arrived("u".into(), Some(demo_cover("u")));
    let pic = look(AlbumArt::Picture, RetroMode::Hd);
    // Half blocks are plain text: a popup covers them, nothing to do.
    art.set_overlay(true);
    assert!(!art.needs_clear.take());
    art.set_overlay(false);
    let mut sixel = Picker::halfblocks();
    sixel.set_protocol_type(ProtocolType::Sixel);
    art.set_graphics(Some(sixel));
    art.needs_clear.take();
    assert!(art.protocol("u", pic).is_some_and(|p| !is_blocks(&p)), "a sixel");
    art.set_overlay(true);
    assert!(art.needs_clear.take(), "the picture is wiped when the popup opens");
    let tinted = Look { tint: ArtTint::Duotone, ..look(AlbumArt::Picture, RetroMode::Bit2) };
    assert!(art.protocol("u", tinted).is_some_and(|p| is_blocks(&p)), "the cover, in half blocks, over Help");
    assert!(!art.needs_clear.take(), "half blocks need no clearing");
    art.set_overlay(true);
    assert!(!art.needs_clear.take(), "once, not every frame");
    art.set_overlay(false);
    assert!(art.needs_clear.take(), "a clean screen when it closes");
    assert!(art.protocol("u", tinted).is_some_and(|p| !is_blocks(&p)), "the sixel, sent again");
}

#[test]
fn covers_are_kept_and_built_once() {
    let mut art = Art::default();
    assert!(art.want("u") && !art.want("u"), "asked once");
    assert!(matches!(art.status("u"), Cover::Loading));
    assert!(art.protocol("u", look(AlbumArt::Picture, RetroMode::Hd)).is_none());
    art.arrived("u".into(), Some(demo_cover("u")));
    assert!(!art.want("u"));
    assert!(art.protocol("u", look(AlbumArt::Picture, RetroMode::Hd)).is_some(), "half blocks without graphics");
    assert!(art.protocol("u", look(AlbumArt::Off, RetroMode::Hd)).is_none(), "off: no cover");
    art.arrived("bad".into(), None);
    assert!(!art.want("bad"), "a failure waits before it is asked again");
    assert_eq!(art.protocol_name(), "half blocks", "no terminal query in tests");
}

#[test]
fn a_failed_cover_is_retried_with_backoff_and_then_given_up_on() {
    let mut art = Art::default();
    let t0 = Instant::now();
    assert!(art.want_at("u", t0));
    let mut now = t0;
    for (i, wait) in RETRY_WAITS.iter().enumerate() {
        art.arrived_at("u".into(), None, now);
        assert!(matches!(art.status("u"), Cover::Loading), "try {} failed: still hoped for", i + 1);
        assert!(!art.want_at("u", now + *wait - Duration::from_millis(1)), "too soon after try {}", i + 1);
        now += *wait;
        assert!(art.want_at("u", now), "asked again after {wait:?}");
        assert!(!art.want_at("u", now), "once per try");
    }
    art.arrived_at("u".into(), None, now);
    assert!(matches!(art.status("u"), Cover::None), "four tries, then no cover");
    // Then about once a minute, for as long as it is wanted.
    for _ in 0..3 {
        assert!(!art.want_at("u", now + SLOW_RETRY - Duration::from_millis(1)), "not before a minute");
        now += SLOW_RETRY;
        assert!(art.want_at("u", now), "a minute later, again");
        assert!(matches!(art.status("u"), Cover::Loading));
        art.arrived_at("u".into(), None, now);
        assert!(matches!(art.status("u"), Cover::None), "no cover between the slow tries");
    }
    art.arrived_at("u".into(), Some(demo_cover("u")), now);
    assert!(!art.want_at("u", now + SLOW_RETRY), "a cover that came is kept");
}

#[test]
fn cover_failures_go_to_the_log_file_without_query_strings_and_it_rolls_over() {
    let dir = crate::config::temp_dir("art-log");
    let log = dir.join("art.log");
    log_failure(&log, "https://cdn.example/a.jpg?token=abc", "HTTP 404\nnot found");
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("https://cdn.example/a.jpg: HTTP 404 not found") && !text.contains("token"), "{text}");
    assert_eq!(text.lines().count(), 1);
    std::fs::write(&log, vec![b'x'; LOG_CAP as usize + 1]).unwrap();
    log_failure(&log, "https://cdn.example/b.jpg", "timed out");
    assert!(dir.join("art.log.old").exists());
    assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 1, "a fresh log");
}

#[test]
fn a_given_up_cover_is_asked_again_when_a_popup_closes() {
    let mut art = Art::default();
    let t0 = Instant::now();
    for _ in 0..=RETRY_WAITS.len() {
        art.arrived_at("u".into(), None, t0);
    }
    assert!(!art.want_at("u", t0 + Duration::from_secs(30)));
    art.set_overlay(true);
    assert!(!art.want_at("u", t0 + Duration::from_secs(30)), "not while it is open");
    art.set_overlay(false);
    assert!(art.want_at("u", t0), "right away, when it closes");
    art.arrived_at("u".into(), Some(demo_cover("u")), t0);
    assert!(matches!(art.status("u"), Cover::None) && !art.want_at("u", t0), "and a cover that arrived is kept");
}

#[test]
fn the_disk_cache_is_trimmed_oldest_first() {
    let dir = crate::config::scratch_dir(&format!("boxvibe-art-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (i, name) in ["a", "b", "c"].iter().enumerate() {
        let p = cache_path(&dir, name);
        std::fs::write(&p, vec![0u8; 100]).unwrap();
        let t = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000 * (i as u64 + 1));
        std::fs::File::options().write(true).open(&p).unwrap().set_modified(t).unwrap();
    }
    trim_cache(&dir, 250);
    assert!(!cache_path(&dir, "a").exists() && cache_path(&dir, "b").exists() && cache_path(&dir, "c").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[allow(deprecated)] // a fixed font size, no terminal to ask
fn sixel() -> Picker {
    let mut p = Picker::from_fontsize(ratatui_image::FontSize { width: 8, height: 16 });
    p.set_protocol_type(ratatui_image::picker::ProtocolType::Sixel);
    p
}

/// Draws the cover into a 20x10 box and returns the sixel sent, if any.
fn draw_sixel(art: &Art, look: Look) -> Option<String> {
    use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
    let area = Rect::new(0, 0, 20, 10);
    let mut buf = Buffer::empty(area);
    let mut cover = art.protocol("u", look)?;
    ratatui_image::StatefulImage::default().render(area, &mut buf, &mut *cover);
    assert!(matches!(cover.last_encoding_result(), Some(Ok(()))), "{look:?} encodes");
    let sym = buf[(0, 0)].symbol().to_owned();
    sym.contains("\x1bP").then_some(sym)
}

#[test]
fn tinted_covers_without_pixelation_are_sent_as_sixels() {
    let mut art = Art::default();
    art.set_graphics(Some(sixel()));
    art.arrived("u".into(), Some(demo_cover("u")));
    for tint in [ArtTint::Off, ArtTint::Duotone, ArtTint::Palette] {
        for pixelation in [RetroMode::Hd, RetroMode::Bit2] {
            let look = Look { tint, ..look(AlbumArt::Picture, pixelation) };
            assert!(draw_sixel(&art, look).is_some(), "{look:?} shows the picture");
        }
    }
}

#[test]
fn a_new_theme_only_rebuilds_a_tinted_cover() {
    let [a, b] = [crate::theme::BUILT_IN[0].2, crate::theme::BUILT_IN[1].2];
    let off = look(AlbumArt::Picture, RetroMode::Hd);
    assert_eq!(Look { palette: a, ..off }, Look { palette: b, ..off });
    let duo = Look { tint: ArtTint::Duotone, ..off };
    assert_ne!(Look { palette: a, ..duo }, Look { palette: b, ..duo });
}

#[test]
fn a_cover_the_terminal_cant_encode_falls_back_to_half_blocks() {
    let mut art = Art::default();
    art.set_graphics(Some(sixel()));
    art.arrived("u".into(), Some(demo_cover("u")));
    let l = look(AlbumArt::Picture, RetroMode::Hd);
    drop(art.protocol("u", l));
    art.unencodable.borrow_mut().insert("u".into());
    art.forget_shown();
    assert!(art.protocol("u", l).is_some_and(|p| is_blocks(&p)));
}

#[test]
fn real_files_decode_and_shrink() {
    let mut png = Vec::new();
    DynamicImage::ImageRgb8(RgbImage::new(1000, 500))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    assert_eq!(decode(&png).unwrap().dimensions(), (640, 320));
    assert!(decode(b"<html>not an image</html>").is_none());
}
