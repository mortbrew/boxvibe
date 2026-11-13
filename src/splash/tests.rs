use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn show(cols: u16, rows: u16, progress: f32) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(cols, rows)).unwrap();
    let p = crate::theme::TOKYO_NIGHT;
    terminal.draw(|f| draw(f, &p, progress, "v9.9.9", "Pandora in your terminal")).unwrap();
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
fn the_whole_screen_shows_the_box_the_word_the_tagline_and_no_hint_about_keys() {
    let all = text(&show(80, 24, 1.0));
    assert!(all.contains("Pandora in your terminal"), "{all}");
    assert!(all.contains("v9.9.9"), "{all}");
    assert!(
        !all.to_lowercase().contains("any key") && !all.contains("continue"),
        "no on-screen text about keys: {all}"
    );
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
