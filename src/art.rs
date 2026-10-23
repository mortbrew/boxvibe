//! Album covers for Now Playing. The terminal's graphics protocol (sixel,
//! kitty or iTerm2) is asked once at startup; anything else, and tmux or
//! Zellij, draws Unicode half blocks. Covers are downloaded on a backend
//! thread, kept in memory (and in `~/.cache/boxvibe/art`, capped), and
//! encoded for the terminal only when shown or resized.

use std::cell::{RefCell, RefMut};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use image::imageops::FilterType;
use image::{DynamicImage, Rgb, RgbImage};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;

use crate::config::{AlbumArt, ArtTint, Pixelation};
use crate::theme::Palette;
use crate::visualizer::Bars;

/// Covers kept in memory.
const MEMORY: usize = 24;
/// The disk cache is trimmed (oldest first) to this many bytes.
pub const DISK_CAP: u64 = 50 * 1024 * 1024;
/// Covers are shrunk to at most this many pixels across once downloaded.
const MAX_PIXELS: u32 = 640;

/// Inside tmux or Zellij, graphics are unreliable: half blocks.
pub fn in_multiplexer() -> bool {
    ["TMUX", "ZELLIJ"].iter().any(|v| std::env::var_os(v).is_some_and(|s| !s.is_empty()))
}

/// Asks the terminal what it can draw (call once, on the alternate screen,
/// before reading keys). `None`: half blocks only.
pub fn detect() -> Option<Picker> {
    if in_multiplexer() {
        return None;
    }
    Picker::from_query_stdio().ok().filter(|p| p.protocol_type() != ProtocolType::Halfblocks)
}

/// What the art box shows when there is no picture.
#[derive(Debug, PartialEq, Eq)]
pub enum Cover {
    /// Still downloading.
    Loading,
    /// None for this song, it failed, or art is off.
    None,
}

/// How a cover is drawn: the settings and colours it was built for.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub mode: AlbumArt,
    pub pixelation: Pixelation,
    pub tint: ArtTint,
    pub palette: Palette,
}

/// Two looks draw the same cover. The theme's colours only matter when the
/// cover is tinted, so a new theme doesn't rebuild (and resend) an untinted one.
impl PartialEq for Look {
    fn eq(&self, other: &Self) -> bool {
        self.mode == other.mode
            && self.pixelation == other.pixelation
            && self.tint == other.tint
            && (self.tint == ArtTint::Off || self.palette == other.palette)
    }
}

impl Eq for Look {}

impl Look {
    pub fn of(app_config: &crate::config::UiConfig, palette: Palette) -> Self {
        Self { mode: app_config.album_art, pixelation: app_config.art_pixelation, tint: app_config.art_tint, palette }
    }
}

struct Shown {
    url: String,
    look: Look,
    protocol: StatefulProtocol,
}

#[derive(Default)]
pub struct Art {
    /// The terminal's graphics, when it has any (Picture).
    graphics: Option<Picker>,
    images: HashMap<String, Arc<DynamicImage>>,
    order: VecDeque<String>,
    requested: HashSet<String>,
    failed: HashSet<String>,
    /// Covers the terminal's graphics couldn't encode: drawn in half blocks.
    unencodable: RefCell<HashSet<String>>,
    shown: RefCell<Option<Shown>>,
    /// A picture protocol was drawn and then replaced: clear the screen once.
    pub needs_clear: std::cell::Cell<bool>,
    pub bars: Bars,
    /// A popup (Help, the quit box) is open: a terminal picture would draw
    /// over it, so none is shown until it closes.
    suspended: std::cell::Cell<bool>,
    /// Where the art box was last drawn (the cover or visualizer fills it).
    pub last_area: std::cell::Cell<ratatui::layout::Rect>,
}

impl Art {
    pub fn set_graphics(&mut self, picker: Option<Picker>) {
        self.graphics = picker;
        self.forget_shown();
    }

    /// Call before each frame with whether a popup covers the screen.
    /// A terminal picture (sixel, kitty, iTerm2) would draw over a popup, so
    /// while one is open the same cover is drawn in half blocks instead.
    /// Opening or closing one clears the screen once, so no remnant stays
    /// behind, and the picture is encoded and sent again, whole, after it.
    pub fn set_overlay(&mut self, open: bool) {
        if self.suspended.get() == open {
            return;
        }
        self.suspended.set(open);
        if self.graphics.is_some() {
            self.forget_shown();
            self.needs_clear.set(true);
        }
    }

    /// The protocol Picture uses ("halfblocks" without graphics).
    pub fn protocol_name(&self) -> &'static str {
        match self.graphics.as_ref().map(Picker::protocol_type) {
            Some(ProtocolType::Sixel) => "sixel",
            Some(ProtocolType::Kitty) => "kitty",
            Some(ProtocolType::Iterm2) => "iTerm2",
            _ => "half blocks",
        }
    }

    /// Whether `url` should be asked for (marks it asked).
    pub fn want(&mut self, url: &str) -> bool {
        !url.is_empty()
            && !self.images.contains_key(url)
            && !self.failed.contains(url)
            && self.requested.insert(url.into())
    }

    pub fn arrived(&mut self, url: String, image: Option<DynamicImage>) {
        self.requested.remove(&url);
        let Some(image) = image else {
            self.failed.insert(url);
            return;
        };
        if self.order.len() >= MEMORY
            && let Some(old) = self.order.pop_front()
        {
            self.images.remove(&old);
        }
        self.order.push_back(url.clone());
        self.images.insert(url, Arc::new(image));
    }

    pub fn status(&self, url: &str) -> Cover {
        if self.requested.contains(url) { Cover::Loading } else { Cover::None }
    }

    /// Settings changed: build the protocol again on the next frame.
    pub fn forget_shown(&mut self) {
        if self.shown.get_mut().take().is_some_and(|s| !is_blocks(&s.protocol)) {
            self.needs_clear.set(true);
        }
    }

    /// The encoded cover for `url`, built when the song or a setting changed.
    /// A new theme rebuilds it too (the tint uses its colours).
    pub fn protocol(&self, url: &str, look: Look) -> Option<RefMut<'_, StatefulProtocol>> {
        let mode = look.mode;
        let mut shown = self.shown.borrow_mut();
        // Half blocks for Blocks, without graphics, under a popup, or when
        // the terminal couldn't encode this cover.
        let picture = match (mode, &self.graphics) {
            (AlbumArt::Picture, Some(p)) if !self.suspended.get() && !self.unencodable.borrow().contains(url) => {
                Some(p)
            }
            _ => None,
        };
        let current = shown
            .as_ref()
            .is_some_and(|s| s.url == url && s.look == look && is_blocks(&s.protocol) == picture.is_none());
        if !current {
            if shown.take().is_some_and(|s| !is_blocks(&s.protocol)) {
                self.needs_clear.set(true);
            }
            let image = match (mode, self.images.get(url)) {
                (AlbumArt::Off, _) | (_, None) => return None,
                (_, Some(image)) => image,
            };
            let picker = picture.cloned().unwrap_or_else(Picker::halfblocks);
            let protocol =
                picker.new_resize_protocol(tint(&pixelate(image, look.pixelation), look.tint, &look.palette));
            *shown = Some(Shown { url: url.into(), look, protocol });
        }
        Some(RefMut::map(shown, |s| &mut s.as_mut().expect("just set").protocol))
    }

    /// Call after drawing the cover: when the terminal's graphics couldn't
    /// encode it, it is drawn in half blocks from the next frame on rather
    /// than leaving the box empty.
    pub fn check_encoding(&self, url: &str) {
        let mut shown = self.shown.borrow_mut();
        let failed = shown.as_mut().is_some_and(|s| {
            s.url == url && !is_blocks(&s.protocol) && matches!(s.protocol.last_encoding_result(), Some(Err(_)))
        });
        if failed {
            self.unencodable.borrow_mut().insert(url.into());
            shown.take();
            self.needs_clear.set(true);
        }
    }

    /// Nothing to show (no song, no cover): a picture left on screen goes.
    pub fn hide(&self) {
        if self.shown.borrow_mut().take().is_some_and(|s| !is_blocks(&s.protocol)) {
            self.needs_clear.set(true);
        }
    }
}

fn is_blocks(p: &StatefulProtocol) -> bool {
    matches!(p.protocol_type(), ratatui_image::protocol::StatefulProtocolType::Halfblocks(_))
}

/// The cover shrunk to `pixelation`'s grid and blown back up, nearest
/// neighbour both ways, so every protocol shows the same big pixels.
pub fn pixelate(image: &DynamicImage, pixelation: Pixelation) -> DynamicImage {
    let Some(grid) = pixelation.grid() else { return image.clone() };
    let (w, h) = (image.width().max(1), image.height().max(1));
    let (gw, gh) = if w >= h { (grid, (grid * h / w).max(1)) } else { ((grid * w / h).max(1), grid) };
    if gw >= w && gh >= h {
        return image.clone();
    }
    image.resize_exact(gw, gh, FilterType::Nearest).resize_exact(w, h, FilterType::Nearest)
}

fn rgb_of(c: ratatui::style::Color) -> Option<[u8; 3]> {
    match c {
        ratatui::style::Color::Rgb(r, g, b) => Some([r, g, b]),
        _ => None,
    }
}

/// The cover recoloured with the theme's colour scheme. Duotone maps each
/// pixel's brightness from the theme's background through its muted grey
/// to its yellow; Palette snaps each pixel to the nearest theme colour.
/// Themes without RGB colours leave the cover as is.
pub fn tint(image: &DynamicImage, tint: ArtTint, p: &Palette) -> DynamicImage {
    let colors: Vec<[u8; 3]> = match tint {
        ArtTint::Off => return image.clone(),
        ArtTint::Duotone => [p.background, p.yellow].iter().filter_map(|c| rgb_of(*c)).collect(),
        ArtTint::Palette => [
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
        .iter()
        .filter_map(|c| rgb_of(*c))
        .collect(),
    };
    if colors.len() < 2 {
        return image.clone();
    }
    let mut out = image.to_rgb8();
    for px in out.pixels_mut() {
        let [r, g, b] = px.0.map(f32::from);
        px.0 = match tint {
            ArtTint::Duotone => {
                let t = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
                let (lo, hi) = (colors[0], colors[1]);
                std::array::from_fn(|i| (f32::from(lo[i]) + (f32::from(hi[i]) - f32::from(lo[i])) * t).round() as u8)
            }
            _ => *colors
                .iter()
                .min_by_key(|c| {
                    let d = |i: usize| (f32::from(c[i]) - [r, g, b][i]).powi(2);
                    (2.0 * d(0) + 4.0 * d(1) + 3.0 * d(2)) as u32
                })
                .expect("two or more"),
        };
    }
    DynamicImage::ImageRgb8(out)
}

/// A downloaded file as a cover, shrunk to [`MAX_PIXELS`].
pub fn decode(bytes: &[u8]) -> Option<DynamicImage> {
    let image = image::load_from_memory(bytes).ok()?;
    Some(if image.width() > MAX_PIXELS || image.height() > MAX_PIXELS {
        image.resize(MAX_PIXELS, MAX_PIXELS, FilterType::Triangle)
    } else {
        image
    })
}

/// `--demo` and tests: a made-up cover (a sunset with stripes), the same for the same URL.
pub fn demo_cover(url: &str) -> DynamicImage {
    let seed = hash(url);
    let hue = (seed % 6) as u8;
    let pick = |t: u32| -> [u8; 3] {
        let a = [[251, 73, 52], [250, 189, 47], [184, 187, 38], [131, 165, 152], [211, 134, 155], [254, 128, 25]];
        a[(usize::from(hue) + t as usize) % a.len()]
    };
    let size = 256;
    DynamicImage::ImageRgb8(RgbImage::from_fn(size, size, |x, y| {
        let band = (y * 4 / size).min(3);
        let [r, g, b] = pick(band);
        let stripe = ((x + y + (seed as u32 % 32)) / 16).is_multiple_of(2);
        let k = if stripe { 1.0 } else { 0.75 } * (1.0 - 0.4 * x as f32 / size as f32);
        Rgb([(f32::from(r) * k) as u8, (f32::from(g) * k) as u8, (f32::from(b) * k) as u8])
    }))
}

/// FNV-1a: a file name for the disk cache that stays the same between versions.
fn hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3))
}

/// `~/.cache/boxvibe/art` (XDG).
pub fn cache_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "boxvibe").map(|d| d.cache_dir().join("art"))
}

pub fn cache_path(dir: &std::path::Path, url: &str) -> PathBuf {
    dir.join(format!("{:016x}.img", hash(url)))
}

/// Deletes the oldest files in `dir` until it holds at most `cap` bytes.
pub fn trim_cache(dir: &std::path::Path, cap: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            m.is_file().then(|| (m.modified().ok(), m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.0);
    for (_, len, path) in files {
        if total <= cap {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    fn look(mode: AlbumArt, pixelation: Pixelation) -> Look {
        Look { mode, pixelation, tint: ArtTint::Off, palette: crate::theme::BUILT_IN[0].2 }
    }

    #[test]
    fn pixelation_makes_big_square_pixels_at_the_same_size() {
        let cover = demo_cover("x");
        assert_eq!(pixelate(&cover, Pixelation::Off).dimensions(), (256, 256));
        let blocky = pixelate(&cover, Pixelation::Max);
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
        let mut look = Look { mode: AlbumArt::Blocks, pixelation: Pixelation::Off, tint: ArtTint::Duotone, palette: p };
        let first = art.protocol("u", look).map(|_| ()).is_some();
        assert!(first);
        let ptr = |art: &Art| art.shown.borrow().as_ref().map(|s| s.look);
        assert_eq!(ptr(&art), Some(look));
        look.palette = crate::theme::BUILT_IN[1].2;
        assert!(art.protocol("u", look).is_some());
        assert_eq!(ptr(&art), Some(look), "rebuilt for the new theme");
    }

    #[test]
    fn a_popup_shows_the_cover_in_half_blocks_and_clears_around_it() {
        let mut art = Art::default();
        art.arrived("u".into(), Some(demo_cover("u")));
        let pic = look(AlbumArt::Picture, Pixelation::Off);
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
        let tinted = Look { tint: ArtTint::Duotone, ..look(AlbumArt::Picture, Pixelation::Max) };
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
        assert!(art.protocol("u", look(AlbumArt::Picture, Pixelation::Off)).is_none());
        art.arrived("u".into(), Some(demo_cover("u")));
        assert!(!art.want("u"));
        assert!(art.protocol("u", look(AlbumArt::Blocks, Pixelation::Off)).is_some());
        assert!(art.protocol("u", look(AlbumArt::Off, Pixelation::Off)).is_none(), "off: the visualizer");
        art.arrived("bad".into(), None);
        assert!(!art.want("bad") && matches!(art.status("bad"), Cover::None), "a failure isn't asked again");
        assert_eq!(art.protocol_name(), "half blocks", "no terminal query in tests");
    }

    #[test]
    fn the_disk_cache_is_trimmed_oldest_first() {
        let dir = std::env::temp_dir().join(format!("boxvibe-art-{}", std::process::id()));
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
            for pixelation in [Pixelation::Off, Pixelation::Max] {
                let look = Look { tint, ..look(AlbumArt::Picture, pixelation) };
                assert!(draw_sixel(&art, look).is_some(), "{look:?} shows the picture");
            }
        }
    }

    #[test]
    fn a_new_theme_only_rebuilds_a_tinted_cover() {
        let [a, b] = [crate::theme::BUILT_IN[0].2, crate::theme::BUILT_IN[1].2];
        let off = look(AlbumArt::Picture, Pixelation::Off);
        assert_eq!(Look { palette: a, ..off }, Look { palette: b, ..off });
        let duo = Look { tint: ArtTint::Duotone, ..off };
        assert_ne!(Look { palette: a, ..duo }, Look { palette: b, ..duo });
    }

    #[test]
    fn a_cover_the_terminal_cant_encode_falls_back_to_half_blocks() {
        let mut art = Art::default();
        art.set_graphics(Some(sixel()));
        art.arrived("u".into(), Some(demo_cover("u")));
        let l = look(AlbumArt::Picture, Pixelation::Off);
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
}
