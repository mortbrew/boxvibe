use super::*;

#[test]
fn a_colour_is_its_rgb_and_reset_takes_the_fallback() {
    assert_eq!(rgb(Color::Rgb(1, 2, 3), Rgb([9, 9, 9])), Rgb([1, 2, 3]));
    assert_eq!(rgb(Color::Reset, Rgb([9, 9, 9])), Rgb([9, 9, 9]));
    assert_eq!(rgb(Color::White, Rgb([9, 9, 9])), Rgb([255, 255, 255]));
}

#[test]
fn a_missing_font_says_where_it_looked() {
    let err = load_one("BOXVIBE_NO_SUCH_FONT_VAR", &["/nonexistent/DejaVuSansMono.ttf"], "DejaVu Sans Mono")
        .unwrap_err()
        .to_string();
    assert!(err.contains("DejaVu Sans Mono not found"), "{err}");
    assert!(err.contains("/nonexistent/DejaVuSansMono.ttf"), "{err}");
}

#[test]
fn a_tilde_path_is_looked_up_under_home() {
    // `$HOME` is set in every test run, so this resolves to a real path
    // that does not exist rather than to the literal `~/...`.
    let found = find("BOXVIBE_NO_SUCH_FONT_VAR", &["~/definitely-not-a-font.ttf"]);
    assert_eq!(found, None);
}

#[test]
fn a_block_fills_its_cell_and_a_line_is_one_pixel_wide() {
    let mut img = RgbImage::from_pixel(20, 20, Rgb([0, 0, 0]));
    fill(&mut img, 0, 0, 10, 10, Rgb([255, 0, 0]));
    assert_eq!(*img.get_pixel(9, 9), Rgb([255, 0, 0]));
    assert_eq!(*img.get_pixel(10, 10), Rgb([0, 0, 0]));

    let mut img = RgbImage::from_pixel(20, 20, Rgb([0, 0, 0]));
    hline(&mut img, 0, 20, 10, 1, Rgb([0, 255, 0]));
    assert_eq!(*img.get_pixel(5, 10), Rgb([0, 255, 0]));
    assert_eq!(*img.get_pixel(5, 11), Rgb([0, 0, 0]));
}

#[test]
fn a_glyph_edge_blends_and_an_off_image_pixel_is_ignored() {
    let mut img = RgbImage::from_pixel(4, 4, Rgb([0, 0, 0]));
    blend(&mut img, 1, 1, Rgb([255, 255, 255]), 0.5);
    assert_eq!(*img.get_pixel(1, 1), Rgb([128, 128, 128]));
    // Out of bounds is a no-op, not a panic: glyphs overhang their cell.
    blend(&mut img, -1, 1, Rgb([255, 255, 255]), 1.0);
    blend(&mut img, 4, 1, Rgb([255, 255, 255]), 1.0);
}

#[test]
fn a_saved_png_round_trips_and_is_smaller_than_the_raw_pixels() {
    let dir = crate::config::scratch_dir("screenshots-save");
    let out = dir.join("frame.png");
    // Flat colour, like a frame: the encoder should beat the raw bytes.
    let img = RgbImage::from_pixel(64, 64, Rgb([0x1a, 0x1b, 0x26]));
    save(&img, &out).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "a PNG");
    assert!(bytes.len() < 64 * 64 * 3, "{} bytes for {} raw", bytes.len(), 64 * 64 * 3);
    let back = image::open(&out).unwrap().to_rgb8();
    assert_eq!(back.dimensions(), (64, 64));
    assert_eq!(*back.get_pixel(10, 10), Rgb([0x1a, 0x1b, 0x26]));
    let _ = std::fs::remove_dir_all(&dir);
}
