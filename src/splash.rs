//! The boot screen: a small box drawn with half blocks beside "boxvibe" in
//! big letters, the tagline, the version and "any key to go on". It fades in
//! from the theme's background to the theme's colours, left to right, in the
//! colours of the active theme. `app.splash_progress()` says how far along
//! it is (0 to 1); any key ends it (the app does that).
//!
//! The letters are in the style of tubevibe's wordmark: seven rows, five
//! cells a letter.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::icons::LOGO;
use crate::theme::Palette;

/// Seven rows a letter, five cells wide.
const B: [&str; 7] = ["▗▖   ", "▐▌   ", "▐▙█▙ ", "▐▛ ▜▌", "▐▌ ▐▌", "▐█▄█▘", "▝▘▀▘ "];
const O: [&str; 7] = ["     ", "     ", " ▟█▙ ", "▐▛ ▜▌", "▐▌ ▐▌", "▐▙ ▟▌", " ▀▀▀ "];
const X: [&str; 7] = ["     ", "     ", "▜▙ ▟▛", " ▜█▛ ", " ▟█▙ ", "▟▛ ▜▙", "▀   ▀"];
const V: [&str; 7] = ["     ", "     ", "▐▌ ▐▌", "▝█ █▘", " █ █ ", " ▐█▌ ", "  ▀  "];
const I: [&str; 7] = ["  █  ", "  ▀  ", " ██  ", "  █  ", "  █  ", "▗▄█▄▖", "▝▀▀▀▘"];
const E: [&str; 7] = ["     ", "     ", " ▟█▙ ", "▐▙▄▟▌", "▐▛▀▀▘", "▝█▄▄▌", " ▝▀▀ "];

/// "boxvibe": the first three letters are "box".
const WORD: [&[&str; 7]; 7] = [&B, &O, &X, &V, &I, &B, &E];
/// How many cells the letters of "box" take.
const BOX_CELLS: usize = 15;

/// The box: a lid on top, the body under it, a strip of tape down the middle.
/// Width in cells, then one string a row (`l` lid, `b` body, `t` tape).
const BOX_W: usize = 20;
const BOX: [&str; 7] = [
    " ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄ ",
    "▐██████████████████▌",
    " ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀ ",
    "  ████████████████  ",
    "  ████████████████  ",
    "  ████████████████  ",
    "  ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀  ",
];
/// Which colour each box row takes: `true` = the lid's.
const LID_ROWS: usize = 3;
/// The tape: these two columns.
const TAPE: std::ops::Range<usize> = 9..11;

/// Total width of the big picture: the box, a gap, the letters.
pub fn picture_width() -> u16 {
    (BOX_W + 2 + WORD.len() * 5) as u16
}

/// Rows of the whole screen's content: the picture, a gap, the tagline, a
/// gap and the hint.
const CONTENT_ROWS: u16 = 7 + 1 + 1 + 1 + 1;

/// `from` mixed toward `to` by `k` (0 to 1). Only RGB colours can be mixed;
/// any other colour (a named one) shows once `k` is past a third.
fn mix(from: Color, to: Color, k: f32) -> Color {
    match (from, to) {
        (Color::Rgb(a, b, c), Color::Rgb(x, y, z)) => {
            let m = |from: u8, to: u8| (f32::from(from) + (f32::from(to) - f32::from(from)) * k).round() as u8;
            Color::Rgb(m(a, x), m(b, y), m(c, z))
        }
        _ if k > 0.33 => to,
        _ => from,
    }
}

/// How far (0 to 1) the cell at column `x` of `width` has faded in when the
/// whole screen is `progress` done: the left side comes first.
fn local(progress: f32, x: usize, width: usize) -> f32 {
    let lead = x as f32 / width.max(1) as f32;
    (progress * 1.6 - lead * 0.6).clamp(0.0, 1.0)
}

/// A line of text-art where every cell fades in on its own: `colour(x)`
/// is the final colour of column `x`.
fn fading(
    text: &str,
    start: usize,
    width: usize,
    bg: Color,
    progress: f32,
    colour: impl Fn(usize) -> Color,
) -> Vec<Span<'static>> {
    text.chars()
        .enumerate()
        .map(|(i, c)| {
            let x = start + i;
            let color = mix(bg, colour(x), local(progress, x, width));
            Span::styled(c.to_string(), Style::new().fg(color).bold())
        })
        .collect()
}

pub fn draw(frame: &mut Frame, p: &Palette, progress: f32, version: &str, tagline: &str, hint: &str) {
    let area = frame.area();
    let width = picture_width();
    if area.width < width || area.height < CONTENT_ROWS + 1 {
        // A small terminal: just the name.
        let line = Line::from(vec![
            Span::styled(format!(" {LOGO} "), Style::new().fg(p.blue).bold()),
            Span::styled("box", Style::new().fg(p.blue).bold()),
            Span::styled("vibe", Style::new().fg(p.fg).bold()),
        ]);
        let at = Rect::new(area.x, area.y + area.height / 2, area.width, 1.min(area.height));
        frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), at);
        return;
    }
    let w = usize::from(width);
    let mut lines: Vec<Line> = Vec::new();
    for row in 0..7 {
        let lid = row < LID_ROWS;
        let mut spans = fading(BOX[row], 0, w, p.background, progress, |x| {
            if TAPE.contains(&x) {
                p.fg
            } else if lid {
                p.blue
            } else {
                p.orange
            }
        });
        spans.push(Span::raw("  "));
        let mut start = BOX_W + 2;
        for (n, glyph) in WORD.iter().enumerate() {
            let boxy = n * 5 < BOX_CELLS;
            spans.extend(fading(glyph[row], start, w, p.background, progress, |_| if boxy { p.blue } else { p.fg }));
            start += 5;
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    let late = local(progress, w * 3 / 4, w);
    lines.push(
        Line::from(vec![
            Span::styled(tagline.to_owned(), Style::new().fg(mix(p.background, p.blue, late))),
            Span::styled(format!("  {version}"), Style::new().fg(mix(p.background, p.muted, late))),
        ])
        .centered(),
    );
    lines.push(Line::raw(""));
    lines.push(Line::from(hint.to_owned()).fg(mix(p.background, p.muted, late)).centered());
    let at =
        Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - CONTENT_ROWS) / 2, width, CONTENT_ROWS);
    frame.render_widget(Paragraph::new(lines), at);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn show(cols: u16, rows: u16, progress: f32) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(cols, rows)).unwrap();
        let p = crate::theme::TOKYO_NIGHT;
        terminal.draw(|f| draw(f, &p, progress, "v9.9.9", "Pandora in your terminal", "any key to go on")).unwrap();
        terminal.backend().buffer().clone()
    }

    fn text(buf: &ratatui::buffer::Buffer) -> String {
        let a = buf.area;
        (0..a.height)
            .map(|y| (0..a.width).map(|x| buf[(x, y)].symbol().to_owned()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn every_letter_is_seven_rows_of_five_cells() {
        for glyph in WORD {
            assert_eq!(glyph.len(), 7);
            assert!(glyph.iter().all(|row| row.chars().count() == 5), "{glyph:?}");
        }
        assert!(BOX.iter().all(|row| row.chars().count() == BOX_W));
    }

    #[test]
    fn the_whole_screen_shows_the_box_the_word_the_tagline_and_the_hint() {
        let all = text(&show(80, 24, 1.0));
        assert!(all.contains("Pandora in your terminal"), "{all}");
        assert!(all.contains("v9.9.9") && all.contains("any key to go on"), "{all}");
        // The wordmark's rows: "box" then "vibe", from the letters.
        let row = WORD.iter().map(|g| g[2]).collect::<String>();
        assert!(all.contains(&row), "the letters' third row\n{all}");
        assert!(all.contains("▐██████████████████▌"), "the box\n{all}");
    }

    #[test]
    fn it_fades_in_from_the_background_and_uses_the_themes_colours() {
        let p = crate::theme::TOKYO_NIGHT;
        let start = show(80, 24, 0.0);
        let end = show(80, 24, 1.0);
        let fgs = |b: &ratatui::buffer::Buffer| {
            let mut colours = std::collections::HashSet::new();
            for y in 0..24 {
                for x in 0..80 {
                    if b[(x, y)].symbol() != " " {
                        colours.insert(b[(x, y)].fg);
                    }
                }
            }
            colours
        };
        assert!(fgs(&start).iter().all(|c| *c == p.background), "invisible at the start: {:?}", fgs(&start));
        let end = fgs(&end);
        for c in [p.blue, p.fg, p.orange] {
            assert!(end.contains(&c), "the theme's colour {c:?} at the end: {end:?}");
        }
    }

    #[test]
    fn a_small_terminal_gets_a_one_line_logo() {
        let all = text(&show(30, 6, 1.0));
        assert!(all.contains("boxvibe") && all.contains(LOGO), "{all}");
        assert!(!all.contains('█'), "no big picture: {all}");
    }
}
