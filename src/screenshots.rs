//! The README screenshots: `cargo test --release screenshots -- --ignored`
//! draws each demo screen with ratatui's `TestBackend` and rasterizes the cell
//! grid straight to `docs/screenshots/*.png`, so the images need no browser and
//! no Python. DejaVu Sans Mono draws the text; the Nerd Font symbols draw the
//! icons DejaVu lacks (boxvibe's default icon set).

use std::path::{Path, PathBuf};

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use image::{ImageEncoder, Rgb, RgbImage};
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

use crate::theme::Palette;

/// A 16px DejaVu Sans Mono in a 10x20 cell, 16px of padding around the frame.
const SIZE: f32 = 16.0;
const CW: u32 = 10;
const CH: u32 = 20;
const PAD: u32 = 16;

/// Where the fonts usually are: Debian and Arch disagree, and the Nerd Font
/// symbols come from a package or a hand-installed file. A path starting with
/// `~/` is looked up under `$HOME`.
const REGULAR: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu-sans-mono-fonts/DejaVuSansMono.ttf",
    "~/.local/share/fonts/DejaVuSansMono.ttf",
    "~/.fonts/DejaVuSansMono.ttf",
];
const BOLD: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono-Bold.ttf",
    "/usr/share/fonts/dejavu-sans-mono-fonts/DejaVuSansMono-Bold.ttf",
    "~/.local/share/fonts/DejaVuSansMono-Bold.ttf",
    "~/.fonts/DejaVuSansMono-Bold.ttf",
];
const NERD: &[&str] = &[
    "/usr/share/fonts/TTF/SymbolsNerdFontMono-Regular.ttf",
    "/usr/share/fonts/nerd-fonts/SymbolsNerdFontMono-Regular.ttf",
    "/usr/share/fonts/truetype/nerd-fonts/SymbolsNerdFontMono-Regular.ttf",
    "~/.local/share/fonts/nerd/SymbolsNerdFontMono-Regular.ttf",
    "~/.local/share/fonts/SymbolsNerdFontMono-Regular.ttf",
    "~/.fonts/SymbolsNerdFontMono-Regular.ttf",
    // `ttf-dejavu-nerd` patches DejaVu itself, so its icons live in this face.
    "/usr/share/fonts/TTF/DejaVuSansMNerdFontMono-Regular.ttf",
];

/// The three faces a frame is drawn with.
struct Fonts {
    regular: FontVec,
    bold: FontVec,
    /// `None` when the symbols font is missing: icons then draw as DejaVu's
    /// `.notdef` box, which is what the old script did too.
    nerd: Option<FontVec>,
}

impl Fonts {
    /// Loads the fonts, or says which one is missing and where it looked.
    fn load() -> anyhow::Result<Self> {
        Ok(Self {
            regular: load_one("BOXVIBE_SCREENSHOT_FONT", REGULAR, "DejaVu Sans Mono")?,
            bold: load_one("BOXVIBE_SCREENSHOT_FONT_BOLD", BOLD, "DejaVu Sans Mono Bold")?,
            nerd: find("BOXVIBE_SCREENSHOT_FONT_NERD", NERD)
                .and_then(|p| FontVec::try_from_vec(std::fs::read(p).ok()?).ok()),
        })
    }

    /// The face that has `ch`: DejaVu, or the symbols font for the icons.
    fn face(&self, ch: char, bold: bool) -> &FontVec {
        let main = if bold { &self.bold } else { &self.regular };
        match &self.nerd {
            Some(nerd) if main.glyph_id(ch).0 == 0 && nerd.glyph_id(ch).0 != 0 => nerd,
            _ => main,
        }
    }
}

/// The first of `candidates` that exists, or `$env` when it is set.
fn find(env: &str, candidates: &[&str]) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(env) {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    candidates
        .iter()
        .filter_map(|c| match c.strip_prefix("~/") {
            Some(rest) => home.as_ref().map(|h| h.join(rest)),
            None => Some(PathBuf::from(c)),
        })
        .find(|p| p.is_file())
}

fn load_one(env: &str, candidates: &[&str], what: &str) -> anyhow::Result<FontVec> {
    let path = find(env, candidates).ok_or_else(|| {
        anyhow::anyhow!("{what} not found; install it or set {env} to the .ttf (looked in {candidates:?})")
    })?;
    let bytes = std::fs::read(&path).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    FontVec::try_from_vec(bytes).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
}

/// Draws `buffer` to `out` as a PNG, `PAD` pixels around the frame.
pub fn write(buffer: &Buffer, palette: &Palette, out: &Path) -> anyhow::Result<()> {
    let fonts = Fonts::load()?;
    let area = buffer.area;
    let base = rgb(buffer[(0, 0)].bg, palette_rgb(palette.background));
    let mut img = RgbImage::from_pixel(area.width as u32 * CW + 2 * PAD, area.height as u32 * CH + 2 * PAD, base);
    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buffer[(x, y)];
            let (fg, bg) = if cell.modifier.contains(Modifier::REVERSED) {
                (rgb(cell.bg, base), rgb(cell.fg, palette_rgb(palette.fg)))
            } else {
                (rgb(cell.fg, palette_rgb(palette.fg)), rgb(cell.bg, base))
            };
            let (px, py) = (PAD + x as u32 * CW, PAD + y as u32 * CH);
            fill(&mut img, px, py, CW, CH, bg);
            let symbol = cell.symbol();
            if symbol.trim().is_empty() {
                continue;
            }
            let bold = cell.modifier.contains(Modifier::BOLD);
            draw_cell(&mut img, &fonts, symbol, bold, px, py, fg);
        }
    }
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    save(&img, out)?;
    Ok(())
}

/// Writes `img` as a PNG at the best compression the encoder offers. The
/// default is fast but leaves the files about twice the size: these frames are
/// mostly flat colour, so the adaptive filter and the slowest deflate pay off.
fn save(img: &RgbImage, out: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::create(out).map_err(|e| anyhow::anyhow!("{}: {e}", out.display()))?;
    let mut writer = std::io::BufWriter::new(file);
    image::codecs::png::PngEncoder::new_with_quality(
        &mut writer,
        image::codecs::png::CompressionType::Best,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgb8)
    .map_err(|e| anyhow::anyhow!("{}: {e}", out.display()))?;
    Ok(())
}

/// One cell's symbol: blocks and box lines as shapes so they meet with no
/// gaps, everything else as text.
fn draw_cell(img: &mut RgbImage, fonts: &Fonts, symbol: &str, bold: bool, x: u32, y: u32, fg: Rgb<u8>) {
    let (cx, cy) = (x + CW / 2, y + CH / 2);
    match symbol {
        "▀" => fill(img, x, y, CW, CH / 2, fg),
        "▄" => fill(img, x, y + CH / 2, CW, CH / 2, fg),
        "█" => fill(img, x, y, CW, CH, fg),
        "─" | "━" => hline(img, x, cx, cy, if symbol == "─" { 1 } else { 3 }, fg),
        "│" | "┃" => vline(img, cx, y, y + CH, if symbol == "│" { 1 } else { 3 }, fg),
        "╭" | "╮" | "╰" | "╯" | "┌" | "┐" | "└" | "┘" => {
            let right = matches!(symbol, "╭" | "┌" | "╰" | "└");
            let down = matches!(symbol, "╭" | "┌" | "╮" | "┐");
            hline(img, if right { cx } else { x }, if right { x + CW } else { cx }, cy, 1, fg);
            vline(img, cx, if down { cy } else { y }, if down { y + CH } else { cy }, 1, fg);
        }
        _ => {
            // A wide symbol (an emoji) is drawn once, in its first cell.
            let font = fonts.face(symbol.chars().next().unwrap_or(' '), bold);
            let scaled = font.as_scaled(PxScale::from(SIZE));
            let baseline = y as f32 + scaled.ascent();
            for (i, ch) in symbol.chars().enumerate() {
                let mut glyph = scaled.scaled_glyph(ch);
                glyph.position = ab_glyph::point(x as f32 + i as f32 * CW as f32, baseline);
                let Some(outline) = font.outline_glyph(glyph) else { continue };
                let bounds = outline.px_bounds();
                outline.draw(|gx, gy, coverage| {
                    blend(img, bounds.min.x as i32 + gx as i32, bounds.min.y as i32 + gy as i32, fg, coverage);
                });
            }
        }
    }
}

fn fill(img: &mut RgbImage, x: u32, y: u32, w: u32, h: u32, color: Rgb<u8>) {
    for py in y..(y + h).min(img.height()) {
        for px in x..(x + w).min(img.width()) {
            img.put_pixel(px, py, color);
        }
    }
}

/// A horizontal line of `width` pixels centred on `y`, from `x0` to `x1`.
fn hline(img: &mut RgbImage, x0: u32, x1: u32, y: u32, width: u32, color: Rgb<u8>) {
    let top = y.saturating_sub(width / 2);
    fill(img, x0, top, x1.saturating_sub(x0), width, color);
}

/// A vertical line of `width` pixels centred on `x`, from `y0` to `y1`.
fn vline(img: &mut RgbImage, x: u32, y0: u32, y1: u32, width: u32, color: Rgb<u8>) {
    let left = x.saturating_sub(width / 2);
    fill(img, left, y0, width, y1.saturating_sub(y0), color);
}

/// Puts `color` over the pixel at `coverage` (0..=1) of the way.
fn blend(img: &mut RgbImage, x: i32, y: i32, color: Rgb<u8>, coverage: f32) {
    if x < 0 || y < 0 || x >= img.width() as i32 || y >= img.height() as i32 {
        return;
    }
    let a = coverage.clamp(0.0, 1.0);
    let p = img.get_pixel_mut(x as u32, y as u32);
    for i in 0..3 {
        p.0[i] = (p.0[i] as f32 * (1.0 - a) + color.0[i] as f32 * a).round() as u8;
    }
}

/// A ratatui colour as RGB, `Reset` taking `fallback`.
fn rgb(color: Color, fallback: Rgb<u8>) -> Rgb<u8> {
    match if color == Color::Reset { Color::Rgb(fallback.0[0], fallback.0[1], fallback.0[2]) } else { color } {
        Color::Rgb(r, g, b) => Rgb([r, g, b]),
        Color::Black => Rgb([0, 0, 0]),
        Color::Red => Rgb([128, 0, 0]),
        Color::Green => Rgb([0, 128, 0]),
        Color::Yellow => Rgb([128, 128, 0]),
        Color::Blue => Rgb([0, 0, 128]),
        Color::Magenta => Rgb([128, 0, 128]),
        Color::Cyan => Rgb([0, 128, 128]),
        Color::Gray => Rgb([192, 192, 192]),
        Color::DarkGray => Rgb([128, 128, 128]),
        Color::LightRed => Rgb([255, 0, 0]),
        Color::LightGreen => Rgb([0, 255, 0]),
        Color::LightYellow => Rgb([255, 255, 0]),
        Color::LightBlue => Rgb([0, 0, 255]),
        Color::LightMagenta => Rgb([255, 0, 255]),
        Color::LightCyan => Rgb([0, 255, 255]),
        Color::White => Rgb([255, 255, 255]),
        _ => fallback,
    }
}

/// A palette colour as RGB, with `Reset` falling back to a mid grey.
fn palette_rgb(color: Color) -> Rgb<u8> {
    rgb(color, Rgb([0xcc, 0xcc, 0xcc]))
}

#[cfg(test)]
mod tests {
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
}
