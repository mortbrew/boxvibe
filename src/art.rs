//! Album covers for Now Playing. The terminal's graphics protocol (sixel,
//! kitty or iTerm2) is asked once at startup; anything else, and tmux or
//! Zellij, draws Unicode half blocks. Covers are downloaded on a backend
//! thread, kept in memory (and in `~/.cache/boxvibe/art`, capped), and
//! encoded for the terminal only when shown or resized.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use image::imageops::FilterType;
use image::{DynamicImage, Rgb, RgbImage};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;

use crate::config::{AlbumArt, ArtTint, RetroMode};
use crate::theme::Palette;

/// Covers kept in memory.
const MEMORY: usize = 24;
/// The disk cache is trimmed (oldest first) to this many bytes.
pub const DISK_CAP: u64 = 50 * 1024 * 1024;
/// Covers are shrunk to at most this many pixels across once downloaded.
const MAX_PIXELS: u32 = 640;

/// How long to wait before each new try at a cover that failed to download
/// (so a failed cover is tried [`RETRY_WAITS`]`.len() + 1` times in all).
pub const RETRY_WAITS: [Duration; 3] = [Duration::from_secs(2), Duration::from_secs(6), Duration::from_secs(15)];
/// After those quick tries, a cover still missing is tried again this often
/// (for as long as its song is playing or next), showing "no cover" between tries.
pub const SLOW_RETRY: Duration = Duration::from_secs(60);
/// The art log is started afresh (the old one kept as `art.log.old`) past this size.
const LOG_CAP: u64 = 256 * 1024;

/// A cover that failed: how many times, and when it may be asked for again.
#[derive(Debug, Clone, Copy)]
struct Failure {
    tries: usize,
    retry_at: Instant,
}

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
    pub pixelation: RetroMode,
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
        Self { mode: app_config.album_art, pixelation: app_config.retro_mode, tint: app_config.art_tint, palette }
    }
}

struct Shown {
    url: String,
    look: Look,
    protocol: StatefulProtocol,
}

/// The pixelated and tinted cover for one `(url, look)`, so going back through
/// the retro levels (Shift+R) is a cache hit rather than another encode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LookKey {
    pixelation: RetroMode,
    tint: ArtTint,
    /// Only part of the key when the cover is tinted, matching `Look`'s equality.
    palette: Option<Palette>,
}

/// The retro levels (7), plus the cover of the song playing and the one before.
const CACHED_COVERS: usize = 16;

#[derive(Default)]
pub struct Art {
    /// The terminal's graphics, when it has any (Picture).
    graphics: Option<Picker>,
    images: HashMap<String, Arc<DynamicImage>>,
    order: VecDeque<String>,
    requested: HashSet<String>,
    failed: HashMap<String, Failure>,
    /// Covers the terminal's graphics couldn't encode: drawn in half blocks.
    unencodable: HashSet<String>,
    shown: Option<Shown>,
    /// A picture protocol was drawn and then replaced: clear the screen once.
    pub needs_clear: bool,
    /// A popup (Help, the quit box) is open: a terminal picture would draw
    /// over it, so none is shown until it closes.
    suspended: bool,
    /// Where the art box was last drawn (the cover, or the message that there is none).
    pub last_area: ratatui::layout::Rect,
    /// Where the cover itself was last drawn inside that box.
    pub last_cover: ratatui::layout::Rect,
    /// Prepared covers, newest last: Shift+R cycles back through the levels.
    cache: VecDeque<(String, LookKey, Arc<DynamicImage>)>,
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
        if self.suspended == open {
            return;
        }
        self.suspended = open;
        // The cover is needed again when a popup closes: any that failed get
        // a fresh set of tries.
        if !open {
            self.retry_failed();
        }
        if self.graphics.is_some() {
            self.forget_shown();
            self.needs_clear = true;
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
        self.want_at(url, Instant::now())
    }

    /// [`Art::want`] at a given moment: a cover that failed is asked for again
    /// once its wait is over, until it has used up its tries.
    pub fn want_at(&mut self, url: &str, now: Instant) -> bool {
        if url.is_empty() || self.images.contains_key(url) || self.requested.contains(url) {
            return false;
        }
        if let Some(failure) = self.failed.get(url)
            && now < failure.retry_at
        {
            return false;
        }
        self.requested.insert(url.into())
    }

    /// Gives every cover that failed a fresh set of tries, right away (a
    /// popup closed, or the song changed: the cover is needed again).
    pub fn retry_failed(&mut self) {
        self.failed.clear();
    }

    pub fn arrived(&mut self, url: String, image: Option<DynamicImage>) {
        self.arrived_at(url, image, Instant::now());
    }

    pub fn arrived_at(&mut self, url: String, image: Option<DynamicImage>, now: Instant) {
        self.requested.remove(&url);
        let Some(image) = image else {
            let tries = self.failed.get(&url).map_or(0, |f| f.tries) + 1;
            let wait = RETRY_WAITS.get(tries - 1).copied().unwrap_or(SLOW_RETRY);
            self.failed.insert(url, Failure { tries, retry_at: now + wait });
            return;
        };
        self.failed.remove(&url);
        if self.order.len() >= MEMORY
            && let Some(old) = self.order.pop_front()
        {
            self.images.remove(&old);
        }
        self.order.push_back(url.clone());
        self.images.insert(url, Arc::new(image));
    }

    /// Loading while it is downloading or waiting for another try.
    pub fn status(&self, url: &str) -> Cover {
        let waiting = self.failed.get(url).is_some_and(|f| f.tries <= RETRY_WAITS.len());
        if self.requested.contains(url) || waiting { Cover::Loading } else { Cover::None }
    }

    /// Settings changed: build the protocol again on the next frame.
    pub fn forget_shown(&mut self) {
        if self.shown.take().is_some_and(|s| !is_blocks(&s.protocol)) {
            self.needs_clear = true;
        }
    }

    /// The encoded cover for `url`, built when the song or a setting changed.
    /// A new theme rebuilds it too (the tint uses its colours).
    pub fn protocol(&mut self, url: &str, look: Look) -> Option<&mut StatefulProtocol> {
        let mode = look.mode;
        // Half blocks without graphics, under a popup, or when
        // the terminal couldn't encode this cover.
        let picture = match (mode, &self.graphics) {
            (AlbumArt::Picture, Some(p)) if !self.suspended && !self.unencodable.contains(url) => Some(p.clone()),
            _ => None,
        };
        let current = self
            .shown
            .as_ref()
            .is_some_and(|s| s.url == url && s.look == look && is_blocks(&s.protocol) == picture.is_none());
        if !current {
            let image = match (mode, self.images.get(url).cloned()) {
                (AlbumArt::Off, _) | (_, None) => {
                    if self.shown.take().is_some_and(|s| !is_blocks(&s.protocol)) {
                        self.needs_clear = true;
                    }
                    return None;
                }
                (_, Some(image)) => image,
            };
            let ready = self.prepared(url, &image, look);
            // Changing a setting on the same cover keeps the same kind of
            // protocol, so the cover is swapped in place. Tearing it down
            // instead cleared the screen and re-sent the picture, which is what
            // made Shift+R flash.
            let same_kind =
                self.shown.as_ref().is_some_and(|s| s.url == url && is_blocks(&s.protocol) == picture.is_none());
            if same_kind {
                let picker = picture.unwrap_or_else(|| self.blocks_picker());
                let shown = self.shown.as_mut().expect("just checked");
                shown.look = look;
                shown.protocol = picker.new_resize_protocol(ready);
            } else {
                if self.shown.take().is_some_and(|s| !is_blocks(&s.protocol)) {
                    self.needs_clear = true;
                }
                let picker = picture.unwrap_or_else(|| self.blocks_picker());
                let protocol = picker.new_resize_protocol(ready);
                self.shown = Some(Shown { url: url.into(), look, protocol });
            }
        }
        Some(&mut self.shown.as_mut().expect("just set").protocol)
    }

    /// The cover for `look`, pixelated and tinted: from the cache when this
    /// look has been drawn before, else built once and kept. The cache stores
    /// exactly what the uncached path computes, so it cannot change the pixels.
    fn prepared(&mut self, url: &str, image: &Arc<DynamicImage>, look: Look) -> DynamicImage {
        // The palette only distinguishes looks that are tinted, as in `Look`'s
        // equality: an untinted cover is the same whatever the theme.
        let key = LookKey {
            pixelation: look.pixelation,
            tint: look.tint,
            palette: (look.tint != ArtTint::Off).then_some(look.palette),
        };
        if let Some(at) = self.cache.iter().position(|(u, k, _)| u == url && *k == key) {
            let hit = self.cache.remove(at).expect("just found").2;
            self.cache.push_back((url.into(), key, hit.clone()));
            return (*hit).clone();
        }
        let ready = Arc::new(tint(&pixelate(image, look.pixelation), look.tint, &look.palette));
        // One cover's worth of levels, plus this song and the one before it.
        self.cache.push_back((url.into(), key, ready.clone()));
        while self.cache.len() > CACHED_COVERS {
            self.cache.pop_front();
        }
        (*ready).clone()
    }

    /// Call after drawing the cover: when the terminal's graphics couldn't
    /// encode it, it is drawn in half blocks from the next frame on rather
    /// than leaving the box empty.
    pub fn check_encoding(&mut self, url: &str) {
        let failed = self.shown.as_mut().is_some_and(|s| {
            s.url == url && !is_blocks(&s.protocol) && matches!(s.protocol.last_encoding_result(), Some(Err(_)))
        });
        if failed {
            self.unencodable.insert(url.into());
            self.shown.take();
            self.needs_clear = true;
        }
    }

    /// Nothing to show (no song, no cover): a picture left on screen goes.
    pub fn hide(&mut self) {
        if self.shown.take().is_some_and(|s| !is_blocks(&s.protocol)) {
            self.needs_clear = true;
        }
    }
}

impl Art {
    /// Tests: the cover being shown, and whether it is in half blocks.
    #[cfg(test)]
    pub fn shown_cover(&self) -> Option<(String, bool)> {
        self.shown.as_ref().map(|s| (s.url.clone(), is_blocks(&s.protocol)))
    }

    /// Half blocks sized with the terminal's real font when graphics are on,
    /// so the stand-in under a popup fills exactly the picture's rectangle
    /// (same cells, same place) and nothing beside it moves.
    fn blocks_picker(&self) -> Picker {
        match &self.graphics {
            Some(g) => {
                let mut p = g.clone();
                p.set_protocol_type(ProtocolType::Halfblocks);
                p
            }
            None => Picker::halfblocks(),
        }
    }
}

fn is_blocks(p: &StatefulProtocol) -> bool {
    matches!(p.protocol_type(), ratatui_image::protocol::StatefulProtocolType::Halfblocks(_))
}

/// The cover shrunk to `pixelation`'s grid and blown back up, nearest
/// neighbour both ways, so every protocol shows the same big pixels.
///
/// Nearest neighbour is a plain index remap, so the two `resize_exact` passes
/// it used to take are folded into one loop and one buffer. The sampling index
/// is `floor((x + 0.5) * grid / full)`, which is how `resize_exact` picks its
/// pixel: the half-pixel offset centres the sample, and leaving it out shifts
/// the whole image and smears the rows.
pub fn pixelate(image: &DynamicImage, pixelation: RetroMode) -> DynamicImage {
    let Some(grid) = pixelation.grid() else { return image.clone() };
    let (w, h) = (image.width().max(1), image.height().max(1));
    let (gw, gh) = if w >= h { (grid, (grid * h / w).max(1)) } else { ((grid * w / h).max(1), grid) };
    if gw >= w && gh >= h {
        return image.clone();
    }
    let src = image.to_rgb8();
    let mut out = image::RgbImage::new(w, h);
    // The two `resize_exact` passes, composed: an output pixel picks a grid
    // cell, and the grid cell picks a source pixel. `image` samples at
    // floor((i + 0.5) * small / full) in f32, so this does the same in f32
    // rather than trusting integer division to round identically.
    let pick = |full: u32, small: u32| -> Vec<u32> {
        let step = small as f32 / full as f32;
        (0..full).map(|i| ((i as f32 + 0.5) * step).floor() as u32).collect()
    };
    let to_grid_x = pick(w, gw);
    let to_grid_y = pick(h, gh);
    let from_grid_x = pick(gw, w);
    let from_grid_y = pick(gh, h);
    for (y, gy) in to_grid_y.iter().enumerate() {
        let sy = from_grid_y[*gy as usize];
        for (x, gx) in to_grid_x.iter().enumerate() {
            let sx = from_grid_x[*gx as usize];
            out.put_pixel(x as u32, y as u32, *src.get_pixel(sx, sy));
        }
    }
    DynamicImage::ImageRgb8(out)
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
        px.0 = match tint {
            ArtTint::Duotone => {
                let [r, g, b] = px.0.map(f32::from);
                let t = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
                let (lo, hi) = (colors[0], colors[1]);
                std::array::from_fn(|i| (f32::from(lo[i]) + (f32::from(hi[i]) - f32::from(lo[i])) * t).round() as u8)
            }
            // Nearest theme colour by a weighted squared distance, in integers:
            // the eye is most sensitive to green, least to blue. This runs for
            // every pixel of every cover on the UI thread, so it stays integer
            // arithmetic — the float version was ~5M `powi` calls per cover.
            _ => *colors
                .iter()
                .min_by_key(|c| {
                    let d = |i: usize| {
                        let diff = i32::from(c[i]) - i32::from(px.0[i]);
                        diff * diff
                    };
                    (2 * d(0) + 4 * d(1) + 3 * d(2)) as u32
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

/// Where cover download failures are written (never to the terminal):
/// `~/.cache/boxvibe/art.log` (XDG).
pub fn log_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "boxvibe").map(|d| d.cache_dir().join("art.log"))
}

/// Adds a line to the art log: when (UTC), the cover (without any query string)
/// and why it failed. Past [`LOG_CAP`] the log moves to `.old` and starts again.
/// Errors writing the log are ignored: a cover is not worth a crash or a message.
pub fn log_failure(path: &std::path::Path, url: &str, why: &str) {
    let url = url.split(['?', '#']).next().unwrap_or_default();
    let line = format!("{} {url}: {why}", crate::time::iso_utc(std::time::SystemTime::now()));
    crate::log::append(path, LOG_CAP, &line);
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
mod tests;
