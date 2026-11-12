use super::*;
use crate::backend::Failed;
use crate::config::Config;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

#[test]
fn the_theme_picker_never_cuts_its_tags_or_hints() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.themes_dir = Some(crate::config::temp_dir("picker-draw"));
    app.config.ui.set_theme("gruvbox");
    app.view = crate::app::View::Settings;
    app.focus = crate::app::Focus::Content;
    app.settings_cursor = crate::app::Setting::at(crate::app::Setting::ThemeSelect);
    app.on_key(ratatui::crossterm::event::KeyEvent::from(ratatui::crossterm::event::KeyCode::Enter));
    for cols in [80, 100, 160] {
        let (lines, _) = screen(&mut app, cols, 32);
        let all = lines.join("\n");
        assert!(all.contains(vibekit::picker::SEARCH_HINT), "{cols}: search hint cut\n{all}");
        assert!(all.contains("in use "), "{cols}: tag cut\n{all}");
        assert!(!all.contains("installed"), "{cols}\n{all}");
    }
    app.on_key(ratatui::crossterm::event::KeyEvent::from(ratatui::crossterm::event::KeyCode::Tab));
    for cols in [80, 160] {
        let all = screen(&mut app, cols, 32).0.join("\n");
        assert!(all.contains(vibekit::picker::INSTALLED_HINT), "{cols}: installed hint cut\n{all}");
    }
}

#[test]
fn the_add_station_hint_always_fits() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent};
    let mut app = App::new(Config::default(), None, &[], true);
    app.demo_warm_up(3, std::time::Duration::ZERO);
    for k in [KeyCode::Char('S'), KeyCode::Char('a'), KeyCode::Char('m'), KeyCode::Char('8'), KeyCode::Char('3')] {
        app.on_key(KeyEvent::from(k));
    }
    for cols in [80, 100, 160] {
        let all = screen(&mut app, cols, 40).0.join("\n");
        assert!(all.contains("Enter search · Esc cancel"), "{cols}: typing hint cut\n{all}");
    }
    app.on_key(KeyEvent::from(KeyCode::Enter));
    app.settle(std::time::Duration::from_millis(300));
    for cols in [80, 100, 160] {
        let all = screen(&mut app, cols, 40).0.join("\n");
        assert!(all.contains(ADD_RESULTS_HINT), "{cols}: results hint cut\n{all}");
    }
}

fn screen(app: &mut App, cols: u16, rows: u16) -> (Vec<String>, ratatui::buffer::Buffer) {
    let mut terminal = Terminal::new(TestBackend::new(cols, rows)).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let lines = (0..rows).map(|y| (0..cols).map(|x| buffer[(x, y)].symbol().to_owned()).collect::<String>()).collect();
    (lines, buffer)
}

fn demo() -> App {
    let mut app = App::new(Config::default(), None, &[], true);
    // The tests look for emoji (the default is the Nerd Font set).
    app.config.ui.icons = crate::config::IconSet::Emoji;
    app.demo_warm_up(3, std::time::Duration::ZERO);
    app
}

#[test]
fn the_ad_card_says_advertisement_with_a_countdown_and_no_rating_line() {
    let mut app = demo();
    for _ in 0..3 {
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('N'),
            crossterm::event::KeyModifiers::SHIFT,
        ));
        app.settle(std::time::Duration::from_millis(300));
    }
    assert!(app.ad_view().is_some());
    let text = screen(&mut app, 110, 34).0.join("\n");
    assert!(text.contains("Advertisement") && text.contains("Ad 1 of 2"), "{text}");
    assert!(text.contains("playing") && text.contains("-0:06"), "{text}");
    assert!(text.contains("your music comes right after"), "{text}");
}

/// The Now Playing art box as (top, bottom, left, right), each one
/// cell outside the area the art may use (the old frame's position).
fn art_box(app: &App) -> (usize, usize, usize, usize) {
    let a = app.art.last_area;
    assert!(a.width > 0 && a.height > 0, "art box drawn");
    (a.y as usize - 1, (a.y + a.height) as usize, a.x as usize - 1, (a.x + a.width) as usize)
}

/// Where the terminal's cursor is after drawing `app`, and the screen text.
fn cursor_of(app: &mut App, cols: u16, rows: u16) -> (Position, Vec<String>) {
    let mut terminal = Terminal::new(TestBackend::new(cols, rows)).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    let cursor = terminal.get_cursor_position().unwrap();
    let buffer = terminal.backend().buffer().clone();
    let lines = (0..rows).map(|y| (0..cols).map(|x| buffer[(x, y)].symbol().to_owned()).collect::<String>()).collect();
    (cursor, lines)
}

/// A demo app with the genre station popup open (`r` on Today's Hits).
fn genre_popup_demo(set: crate::config::IconSet) -> App {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = demo();
    app.config.ui.icons = set;
    app.view = View::Stations;
    app.focus = crate::app::Focus::Content;
    app.stations_cursor = app.stations.iter().position(|s| s.name == "Today's Hits").unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert!(app.genre_notice.is_some());
    app
}

/// The cell column where `name` starts on the row that has it, at or after `from`.
fn name_column(buffer: &ratatui::buffer::Buffer, name: &str, from: u16, to: u16) -> Option<u16> {
    (0..buffer.area.height).find_map(|y| {
        (from..to).find_map(|x| {
            let rest: String = (x..to).map(|i| buffer[(i, y)].symbol()).collect();
            rest.trim_start_matches(' ').starts_with(name).then_some(())?;
            // The first cell of the name itself, not the blanks before it.
            (rest.starts_with(name) && buffer[(x, y)].symbol() == &name[..1]).then_some(x)
        })
    })
}

/// A demo app with the station editor open on M83 Radio, on the Seeds tab.
fn editor_demo() -> App {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = demo();
    app.view = View::Stations;
    app.focus = crate::app::Focus::Content;
    app.stations_cursor = app.stations.iter().position(|s| s.name == "M83 Radio").unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    app.settle(std::time::Duration::from_millis(300));
    assert!(app.editor.is_some());
    app
}

/// A demo app on Now Playing, playing the station called `name`, with the
/// keys on the Now Playing pane (`menu` false) or the menu on NOW PLAYING.
#[test]
fn the_notifications_page_lists_messages_under_its_menu_entry() {
    let mut app = demo();
    app.status = "Thumb taken back: Song".into();
    app.note_status();
    app.view = View::Notifications;
    app.focus = Focus::Content;
    let (lines, _) = screen(&mut app, 120, 30);
    let text = lines.join("\n");
    assert!(text.contains("Notifications"), "{text}");
    assert!(text.contains("just now"), "{text}");
    assert!(text.contains("Thumb taken back: Song"), "{text}");
    let settings = lines.iter().position(|l| l.contains("Settings")).unwrap();
    let about = lines.iter().position(|l| l.contains("About")).unwrap();
    assert!(lines[settings + 1].contains("Notifications") && about == settings + 2);
}

fn playing_demo(name: &str, menu: bool) -> App {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(300));
    app.current_station = app.stations.iter().find(|s| s.name == name).map(|s| s.id.clone());
    assert!(app.current_station.is_some(), "{name}");
    app.on_key(KeyEvent::new(KeyCode::Char('P'), KeyModifiers::SHIFT));
    if menu {
        app.focus = crate::app::Focus::Menu;
        app.menu_cursor = 0;
        assert_eq!(app.menu_selected(), MenuItem::NowPlaying);
    }
    app
}

fn press(app: &mut App, key: char) {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    app.on_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
}

#[test]
fn r_and_e_work_on_now_playing_for_your_own_station_and_x_does_not() {
    for menu in [false, true] {
        let mut app = playing_demo("M83 Radio", menu);
        let keys: Vec<&str> = key_caps(&app).iter().map(|(k, _)| *k).collect();
        assert!(keys.contains(&"r") && keys.contains(&"e") && !keys.contains(&"x"), "menu={menu}: {keys:?}");
        press(&mut app, 'r');
        let rename = app.rename.take().expect("the rename box opens");
        assert_eq!(rename.station.name, "M83 Radio");
        press(&mut app, 'e');
        app.settle(std::time::Duration::from_millis(300));
        let ed = app.editor.take().expect("the editor opens");
        assert_eq!(ed.station.name, "M83 Radio");
        // Delete stays refused here, with its old message.
        press(&mut app, 'x');
        assert!(app.confirm_delete().is_none() && app.genre_notice.is_none());
        assert!(app.status.starts_with("x deletes works on a station"), "{}", app.status);
    }
}

#[test]
fn closing_the_editor_or_the_rename_box_leaves_the_keys_on_now_playing() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = playing_demo("M83 Radio", false);
    for key in ['r', 'e'] {
        press(&mut app, key);
        app.settle(std::time::Duration::from_millis(300));
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.rename.is_none() && app.editor.is_none(), "{key}: closed");
        assert_eq!((app.view, app.focus, app.pane), (View::Player, crate::app::Focus::Content, Pane::NowPlaying));
    }
}

#[test]
fn renaming_the_playing_station_changes_its_title_in_now_playing() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = playing_demo("M83 Radio", false);
    assert!(screen(&mut app, 120, 36).0.join("\n").contains("M83 Radio"));
    press(&mut app, 'r');
    app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    for c in "Night Drive".chars() {
        press(&mut app, c);
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.settle(std::time::Duration::from_millis(500));
    assert_eq!(app.station_name(), Some("Night Drive"));
    let text = screen(&mut app, 120, 36).0.join("\n");
    assert!(text.contains("Night Drive") && !text.contains("M83 Radio"), "{text}");
    assert_eq!((app.view, app.focus, app.pane), (View::Player, crate::app::Focus::Content, Pane::NowPlaying));
}

#[test]
fn on_now_playing_a_genre_station_gets_the_popup_and_shuffle_just_the_message() {
    for menu in [false, true] {
        {
            let key = 'r';
            let mut app = playing_demo("Today's Hits", menu);
            let keys: Vec<&str> = key_caps(&app).iter().map(|(k, _)| *k).collect();
            assert!(!keys.contains(&"r"), "not hinted for a genre station: {keys:?}");
            assert!(keys.contains(&"e"), "e shows a genre station's thumbs: {keys:?}");
            press(&mut app, key);
            assert!(app.rename.is_none() && app.editor.is_none());
            let notice = app.genre_notice.as_ref().expect("the genre popup");
            assert_eq!(notice.station, "Today's Hits");
            assert!(app.status.contains("Pandora doesn't let you"), "{}", app.status);
            // "Don't show this again" is honoured here as well.
            let mut app = playing_demo("Today's Hits", menu);
            app.config.ui.genre_edit_popup = false;
            press(&mut app, key);
            assert!(app.genre_notice.is_none() && app.status.contains("Pandora doesn't let you"));

            let mut app = playing_demo("Shuffle", menu);
            press(&mut app, key);
            assert!(app.rename.is_none() && app.editor.is_none() && app.genre_notice.is_none());
            assert!(app.status.contains("Pandora doesn't let you"), "{}", app.status);
        }
    }
}

/// The key bar line of a drawn screen: the last row with text.
fn key_bar_line(app: &mut App, cols: u16) -> String {
    let lines = screen(app, cols, 36).0;
    lines.iter().rev().find(|l| !l.trim().is_empty()).unwrap().trim().to_owned()
}

#[test]
fn the_key_bar_has_no_menu_size_hint_and_shows_e_where_the_editor_opens() {
    use crate::config::Favorite;
    for cols in [110u16, 50] {
        // The playing station, from the Now Playing pane and from the menu's NOW PLAYING entry.
        for menu in [false, true] {
            let mut app = playing_demo("M83 Radio", menu);
            let bar = key_bar_line(&mut app, cols);
            assert!(!bar.contains("Menu Size") && !bar.contains("{/}"), "{cols}: {bar}");
            assert!(bar.contains("r  Rename") && bar.contains("e  Edit"), "{cols} menu={menu}: {bar}");
            assert!(bar.contains("?  Help") && bar.contains("Q  Quit"), "{cols}: {bar}");
        }
        // A favorite in the menu.
        let mut app = playing_demo("M83 Radio", true);
        let id = app.stations.iter().find(|s| s.name == "Lo-Fi Beats").unwrap().id.clone();
        app.config.ui.favorites = vec![Favorite { id, name: "Lo-Fi Beats".into() }];
        app.menu_cursor = app.menu_items().iter().position(|m| matches!(m, MenuItem::Favorite(_))).unwrap();
        let bar = key_bar_line(&mut app, cols);
        assert!(!bar.contains("Menu Size") && bar.contains("?  Help") && bar.contains("Q  Quit"), "{cols}: {bar}");
        assert!(bar.contains("r  Rename") && bar.contains("e  Edit"), "{cols} favorite: {bar}");
        // Never for a station Pandora won't let you change, nor on the plain Stations entry.
        let mut genre = playing_demo("Today's Hits", false);
        assert!(!key_bar_line(&mut genre, cols).contains("Edit"), "genre station");
        // The Stations page: a station row has r and e too.
        let mut app = playing_demo("M83 Radio", true);
        app.view = View::Stations;
        app.focus = crate::app::Focus::Content;
        app.stations_cursor = 2;
        let bar = key_bar_line(&mut app, cols);
        assert!(bar.contains("r  Rename") && bar.contains("e  Edit") && !bar.contains("Menu Size"), "{bar}");
        app.focus = crate::app::Focus::Menu;
        app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Stations).unwrap();
        let bar = key_bar_line(&mut app, cols);
        assert!(!bar.contains("Edit") && !bar.contains("Menu Size"), "{cols} Stations entry: {bar}");
    }
}

#[test]
fn the_key_bar_is_centred_with_as_much_room_on_the_left_as_the_right() {
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(400));
    for cols in [200u16, 140, 120, 100, 80, 60, 40] {
        let lines = screen(&mut app, cols, 36).0;
        let bar = lines.last().expect("the key bar");
        let left = bar.chars().take_while(|c| *c == ' ').count();
        let right = bar.chars().rev().take_while(|c| *c == ' ').count();
        assert!(left > 0, "{cols} cols: the bar starts away from the edge: {bar:?}");
        assert!(left.abs_diff(right) <= 1, "{cols} cols: {left} left, {right} right: {bar:?}");
    }
}

#[test]
fn r_and_e_still_do_nothing_on_up_next_and_history() {
    let mut app = playing_demo("M83 Radio", false);
    for pane in [Pane::UpNext, Pane::History] {
        app.pane = pane;
        for key in ['r', 'e'] {
            press(&mut app, key);
            assert!(app.rename.is_none() && app.editor.is_none(), "{pane:?} {key}");
        }
    }
}

#[test]
fn the_seeds_tab_says_what_seeds_are_and_a_short_screen_goes_without() {
    let mut app = editor_demo();
    let tall = screen(&mut app, 120, 40).0.join("\n");
    for line in EDITOR_SEEDS_INTRO {
        assert!(tall.contains(line), "intro line {line:?}\n{tall}");
    }
    assert!(tall.contains("Seeds (") && tall.contains("1 of "), "the list is still there\n{tall}");
    // The two lines are dim and never longer than the popup is wide.
    assert!(EDITOR_SEEDS_INTRO.iter().all(|l| l.width() <= 74), "fits the 76-cell popup");
    let short = screen(&mut app, 120, 18).0.join("\n");
    assert!(!short.contains("plays music like") && short.contains("Seeds ("), "dropped when short\n{short}");
}

#[test]
fn the_thumbs_tab_has_column_heads_and_dates_that_line_up_under_thumbed_on() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = editor_demo();
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    app.settle(std::time::Duration::from_millis(300));
    let lines = screen(&mut app, 120, 40).0;
    let text = lines.join("\n");
    assert!(text.contains("Songs you gave a thumbs up") && !text.contains("press t"), "{text}");
    let head = lines.iter().find(|l| l.contains(EDITOR_THUMBS_SONG)).expect("the header row");
    assert!(head.contains(EDITOR_THUMBS_DATE), "{head}");
    // The header's last cell is the same column as every date's last cell.
    let end_of = |l: &str, word: &str| {
        let at = l.find(word).unwrap() + word.len();
        l[..at].chars().count()
    };
    let want = end_of(head, EDITOR_THUMBS_DATE);
    let dates: Vec<&String> = lines.iter().filter(|l| l.contains("2026-10-")).collect();
    assert!(dates.len() >= 3, "{text}");
    for l in dates {
        let at = l.find("2026-10-").unwrap();
        assert_eq!(l[..at].chars().count() + 10, want, "date ends under the header: {l}");
    }
    // The thumbs-down side says so, and the header is dropped on a short screen.
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    app.settle(std::time::Duration::from_millis(300));
    let down = screen(&mut app, 120, 40).0.join("\n");
    assert!(down.contains("Songs you gave a thumbs down") && !down.contains("press t"), "{down}");
    assert!(down.contains(EDITOR_THUMBS_SONG) && down.contains(EDITOR_THUMBS_DATE), "{down}");
    let short = screen(&mut app, 120, 16).0.join("\n");
    assert!(!short.contains(EDITOR_THUMBS_DATE) && short.contains("Songs you gave"), "{short}");
}

/// The editor's tab row and its key bar, as drawn.
fn editor_tab_row(app: &mut App, cols: u16, rows: u16) -> String {
    let lines = screen(app, cols, rows).0;
    let at = lines.iter().position(|l| l.contains("Edit station")).expect("the editor");
    lines[at + 2].split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn now_playing_keeps_three_blank_columns_inside_each_side() {
    let mut app = playing_demo("Today's Hits", false);
    let lines = screen(&mut app, 160, 40).0;
    let top = lines.iter().position(|l| l.contains("Now Playing ·")).expect("the card");
    let chars: Vec<char> = lines[top].chars().collect();
    let title_at = lines[top].find("Now Playing").map(|b| lines[top][..b].chars().count()).unwrap();
    let left = (0..title_at).rev().find(|&x| matches!(chars[x], '╭' | '┌' | '┏')).expect("left corner");
    let right = (title_at..chars.len()).find(|&x| matches!(chars[x], '╮' | '┐' | '┓')).expect("right corner");
    let mut rows = 0;
    for line in &lines[top + 1..] {
        let c: Vec<char> = line.chars().collect();
        if matches!(c[left], '╰' | '└' | '┗') {
            break;
        }
        rows += 1;
        assert!(c[left + 1..=left + 3].iter().all(|&ch| ch == ' '), "left pad: {line}");
        assert!(c[right - 3..right].iter().all(|&ch| ch == ' '), "right pad: {line}");
    }
    assert!(rows > 3, "the card has rows");
    assert!(lines.join("\n").contains(app.now.as_ref().unwrap().title.as_str()), "the song still shows");
}

#[test]
fn a_genre_station_editor_shows_only_the_two_thumbs_tabs() {
    let mut app = editor_demo();
    let ed = app.editor.as_mut().unwrap();
    ed.thumbs_only = true;
    ed.tab = EditorTab::ThumbsDown;
    let row = editor_tab_row(&mut app, 120, 40);
    assert!(!row.contains("Seeds") && row.contains("Thumbs Up") && row.contains("Thumbs Down"), "{row}");
    let bar = key_bar_line(&mut app, 120);
    assert!(bar.contains("Tab  Thumbs Up") && !bar.contains("Add Seed"), "{bar}");
    let text = screen(&mut app, 120, 40).0.join("\n");
    assert!(text.contains("x = take back · Tab = Thumbs Up"), "{text}");
}

#[test]
fn the_editor_has_three_tabs_with_the_thumb_icons_and_counts_in_their_titles() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let press = |app: &mut App, code| {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        app.settle(std::time::Duration::from_millis(300));
    };
    let mut app = editor_demo();
    let icons = crate::icons::icons(app.config.ui.icons);
    let (up, down) = (icons.up, icons.down);
    // Wide: names and counts (the counts are in before a tab is opened).
    let ed = app.editor.as_ref().unwrap();
    let (seeds, ups, downs) = (ed.seed_list().len(), ed.count(true).unwrap(), ed.count(false).unwrap());
    let row = editor_tab_row(&mut app, 120, 40);
    assert!(row.contains(&format!("Seeds ({seeds}) {up} Thumbs Up {ups} {down} Thumbs Down {downs}")), "{row}");
    // Narrow: just the icon and the count, still three tabs.
    let row = editor_tab_row(&mut app, 50, 30);
    assert!(
        row.contains(&format!("{up} {ups}")) && row.contains(&format!("{down} {downs}")) && !row.contains("Thumbs"),
        "{row}"
    );
    // The hints name the tab Tab goes to, and `t` is gone from them.
    let bar = key_bar_line(&mut app, 120);
    assert!(bar.contains("Tab  Thumbs Up") && bar.contains("a  Add Seed"), "{bar}");
    let text = screen(&mut app, 120, 40).0.join("\n");
    assert!(text.contains("Tab = Thumbs Up") && !text.contains("t = up/down"), "{text}");
    press(&mut app, KeyCode::Tab);
    let bar = key_bar_line(&mut app, 120);
    assert!(bar.contains("Tab  Thumbs Down") && bar.contains("x  Take Back"), "{bar}");
    assert!(!bar.contains("Show Down") && !bar.contains("t  "), "{bar}");
    let text = screen(&mut app, 120, 40).0.join("\n");
    assert!(text.contains("x = take back · Tab = Thumbs Down") && !text.contains("t = up/down"), "{text}");
    press(&mut app, KeyCode::Tab);
    let bar = key_bar_line(&mut app, 120);
    assert!(bar.contains("Tab  Seeds") && bar.contains("x  Take Back"), "{bar}");
    let text = screen(&mut app, 120, 40).0.join("\n");
    assert!(text.contains("x = take back · Tab = Seeds"), "{text}");
    // Once a list is loaded its total is the count in the title.
    let row = editor_tab_row(&mut app, 120, 40);
    assert!(row.contains(&format!("{down} Thumbs Down {downs}")), "{row}");
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.editor.as_ref().unwrap().tab, crate::app::EditorTab::Seeds);
}

#[test]
fn a_thumb_without_a_date_shows_a_blank_not_junk() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = editor_demo();
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    app.settle(std::time::Duration::from_millis(300));
    let ed = app.editor.as_mut().unwrap();
    let list = &mut ed.up;
    list.items[0].date = String::new();
    list.items[1].date = "not a date".to_owned();
    let text = screen(&mut app, 120, 40).0.join("\n");
    assert!(!text.contains("not a date") && !text.contains("None"), "{text}");
}

#[test]
fn help_explains_seeds_and_thumbs_under_the_editor() {
    for (cols, rows) in [(255u16, 70u16), (160, 40)] {
        let mut app = demo();
        app.open_help();
        let text = screen(&mut app, cols, rows).0.join("\n");
        assert!(text.contains("Seeds: the artists, songs and genres it plays music like"), "{cols}x{rows}\n{text}");
        assert!(
            text.contains("Thumbs Up / Down: songs you rated and the day you thumbed each"),
            "{cols}x{rows}\n{text}"
        );
        assert!(!text.contains("t switches"), "{cols}x{rows}\n{text}");
    }
}

#[test]
fn every_station_row_starts_its_name_in_the_same_column() {
    use crate::config::{Favorite, IconSet};
    for set in [IconSet::Nerd, IconSet::Unicode, IconSet::Emoji] {
        let mut app = demo();
        app.config.ui.icons = set;
        app.view = View::Stations;
        app.focus = crate::app::Focus::Content;
        // Starred and not, genre, yours, shared, Shuffle and the one playing.
        for name in ["M83 Radio", "Today's Hits", "Shuffle", "Synthwave Radio"] {
            let st = app.stations.iter().find(|s| s.name == name).unwrap();
            app.config.ui.favorites.push(Favorite { id: st.id.clone(), name: st.name.clone() });
        }
        app.current_station = app.stations.iter().find(|s| s.name == "Lo-Fi Beats").map(|s| s.id.clone());
        for cursor in [0, 3] {
            app.stations_cursor = cursor; // the highlighted row lines up too
            let buffer = screen(&mut app, 120, 34).1;
            let offsets: Vec<(&str, Option<u16>)> = [
                "Shuffle",          // starred, no kind icon
                "Thumbprint Radio", // yours, no star
                "M83 Radio",        // yours, starred
                "Lo-Fi Beats",      // yours, playing
                "Classic Rock Radio",
                "Synthwave Radio", // shared, starred
                "Today's Hits",    // genre, starred
            ]
            .iter()
            .map(|n| (*n, name_column(&buffer, n, 32, 120)))
            .collect();
            let first = offsets[0].1.expect("Shuffle is listed");
            for (name, at) in &offsets {
                assert_eq!(*at, Some(first), "{set:?} (cursor {cursor}): {name} is off: {offsets:?}");
            }
        }
        // The logo and person sit in one column: same x on every row that has one.
        let ic = icons(set);
        let buffer = screen(&mut app, 120, 34).1;
        let x_of = |glyph: &str, name: &str| {
            (0..buffer.area.height).find_map(|y| {
                let line: String = (32..120).map(|i| buffer[(i, y)].symbol()).collect();
                line.contains(name).then(|| (32..120).find(|&i| buffer[(i, y)].symbol() == glyph))?
            })
        };
        let person = [x_of(ic.own_station, "M83 Radio"), x_of(ic.own_station, "Thumbprint Radio")];
        assert!(person[0].is_some() && person[0] == person[1], "{set:?}: person icons {person:?}");
        let genre = x_of(ic.genre_station, "Today's Hits");
        assert!(genre.is_some() && genre == person[0], "{set:?}: logo {genre:?} vs person {person:?}");
    }
}

#[test]
fn favorites_in_the_menu_line_up_and_the_add_row_sits_over_the_radio_icons() {
    use crate::config::{Favorite, IconSet};
    for set in [IconSet::Nerd, IconSet::Unicode, IconSet::Emoji] {
        let mut app = demo();
        app.config.ui.icons = set;
        app.view = View::Stations;
        // A genre station, one of yours, Shuffle, and one not loaded (no mark yet).
        for name in ["Today's Hits", "M83 Radio", "Shuffle"] {
            let st = app.stations.iter().find(|s| s.name == name).unwrap();
            app.config.ui.favorites.push(Favorite { id: st.id.clone(), name: st.name.clone() });
        }
        app.config.ui.favorites.push(Favorite { id: "gone".into(), name: "Quiet Storm".into() });
        let buffer = screen(&mut app, 120, 34).1;
        let at: Vec<_> = ["Today's Hits", "M83 Radio", "Shuffle", "Quiet Storm"]
            .iter()
            .map(|n| (*n, name_column(&buffer, n, 1, 29)))
            .collect();
        assert!(at[0].1.is_some() && at.iter().all(|(_, x)| *x == at[0].1), "{set:?}: {at:?}");
    }
}

#[test]
fn a_narrow_stations_list_cuts_the_names_and_keeps_the_icons() {
    use crate::config::IconSet;
    for set in [IconSet::Nerd, IconSet::Unicode, IconSet::Emoji] {
        let mut app = demo();
        app.config.ui.icons = set;
        app.view = View::Stations;
        let ic = icons(set);
        let (lines, _) = screen(&mut app, 56, 34);
        let row = lines.iter().find(|l| l.contains(ic.genre_station)).expect("the genre row");
        assert!(row.contains(ic.station), "{set:?}: {row}");
        let rows: Vec<&String> = lines.iter().filter(|l| l.contains(ic.own_station)).collect();
        assert!(!rows.is_empty() && rows.iter().any(|r| r.contains('…')), "{set:?}: names are cut: {rows:?}");
    }
}

#[test]
fn the_genre_popup_explains_the_problem_and_both_icons() {
    use crate::config::IconSet;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    for set in [IconSet::Emoji, IconSet::Nerd, IconSet::Unicode] {
        let ic = icons(set);
        let mut app = genre_popup_demo(set);
        let (lines, _) = screen(&mut app, 110, 34);
        let text = lines.join("\n");
        for want in [
            "Genre station ─",
            "“Today's Hits” can't be renamed.",
            "made by Pandora",
            "Genre station: made by Pandora",
            "Your own station: you made it",
            "[ ] Don't show this again",
            "Space = tick the box · Enter = OK · Esc = close",
            "edit popup.",
        ] {
            assert!(text.contains(want), "{set:?}: {want:?} in:\n{text}");
        }
        // Each meaning has its glyph in front of it.
        let row = |what: &str| lines.iter().find(|l| l.contains(what)).unwrap().clone();
        let (g, o) = (row("Genre station: made"), row("Your own station: you"));
        assert!(g.contains(ic.genre_station) && g[..g.find("Genre station: made").unwrap()].contains(ic.genre_station));
        assert!(o.contains(ic.own_station) && o[..o.find("Your own station").unwrap()].contains(ic.own_station));
        // The bar at the bottom still has the error, and its keys.
        assert!(lines[32].contains("Pandora doesn't let you rename “Today's Hits”"), "{}", lines[32]);
        assert!(
            lines[33].contains("Space") && lines[33].contains("Enter") && lines[33].contains("Esc"),
            "{}",
            lines[33]
        );
        // The box ticks with Space.
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        let text = screen(&mut app, 110, 34).0.join("\n");
        assert!(text.contains("[x] Don't show this again") && !text.contains("[ ] Don't"), "{text}");
    }
    // A small terminal keeps the box, the checkbox and the keys.
    let mut app = genre_popup_demo(IconSet::Unicode);
    for (cols, rows) in [(60u16, 24u16), (60, 18), (50, 16)] {
        let lines = screen(&mut app, cols, rows).0;
        let text = lines.join("\n");
        assert!(text.contains("Genre station ─") && text.contains("[ ] Don't show"), "{cols}x{rows}:\n{text}");
        // The hint line is the first to go on a very short screen; the key bar always has the keys.
        assert!(rows < 18 || text.contains("Esc = close"), "{cols}x{rows}:\n{text}");
        assert!(lines[rows as usize - 1].contains("Esc"), "{cols}x{rows}:\n{text}");
    }
}

#[test]
fn help_explains_the_station_icons_in_the_active_set() {
    use crate::config::IconSet;
    for set in [IconSet::Emoji, IconSet::Nerd, IconSet::Unicode] {
        let ic = icons(set);
        let mut app = demo();
        app.config.ui.icons = set;
        app.open_help();
        // Short screens scroll Help; the icons and footer are at its end.
        app.on_key(ratatui::crossterm::event::KeyEvent::from(ratatui::crossterm::event::KeyCode::Char('G')));
        for (cols, rows) in [(160u16, 40u16), (255, 70)] {
            let lines = screen(&mut app, cols, rows).0;
            let all = lines.join("\n");
            assert!(all.contains("STATION ICONS"), "{set:?} {cols}x{rows}:\n{all}");
            let row = |what: &str| lines.iter().find(|l| l.contains(what)).cloned().unwrap_or_default();
            let g = row("genre station: made by Pandora");
            let o = row("a station you made");
            assert!(g.contains(ic.genre_station), "{set:?} {cols}x{rows}: {g}");
            assert!(g.contains("can't be renamed or edited") && g.contains("can be deleted"), "{g}");
            assert!(o.contains(ic.own_station) && o.contains("rename, edit and delete"), "{set:?} {cols}x{rows}: {o}");
            assert!(all.contains(HELP_FOOTER.trim()), "{cols}x{rows}: the footer is still there");
        }
    }
}

#[test]
fn help_keeps_the_station_icons_its_text_cleaning_would_drop() {
    use crate::config::IconSet;
    for set in [IconSet::Emoji, IconSet::Nerd, IconSet::Unicode] {
        let ic = icons(set);
        let s = help_sections(set);
        let icons = s.iter().find(|s| s.title.starts_with("STATION ICONS")).unwrap();
        assert_eq!(icons.keys[0].0, ic.genre_station, "{set:?}");
        assert_eq!(icons.keys[1].0, ic.own_station, "{set:?}");
    }
}

#[test]
fn the_settings_page_lists_the_genre_popup_row() {
    let mut app = demo();
    app.view = View::Settings;
    app.focus = crate::app::Focus::Content;
    app.settings_cursor = crate::app::Setting::at(crate::app::Setting::GenrePopup);
    let text = screen(&mut app, 120, 40).0.join("\n");
    assert!(text.contains("Genre station edit popup"), "{text}");
    let line = text.lines().find(|l| l.contains("Genre station edit popup")).unwrap();
    assert!(line.contains(" on"), "{line}");
}

#[test]
fn the_password_is_asked_in_a_popup_that_says_where_it_goes() {
    use crate::app::KeychainState;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = demo();
    app.config.pandora.username = "you@example.com".into();
    app.keychain_state = KeychainState::NotSaved;
    app.ask_password_for_snapshot("s3cret");
    assert!(app.overlay_open(), "a popup: the page behind is dimmed and its keys are off");
    let (cursor, lines) = cursor_of(&mut app, 110, 34);
    let text = lines.join("\n");
    for want in [
        "Pandora password",
        "Password for you@example.com",
        "system keychain",
        "Secret Service",
        "never written to config.toml",
        "Enter = log in and save",
        "Esc = cancel",
    ] {
        assert!(text.contains(want), "{want:?} in:\n{text}");
    }
    assert!(!text.contains("session only"), "a keychain is there: {text}");
    assert!(!text.contains("s3cret"), "masked");
    let row = lines.iter().position(|l| l.contains("› ••••••")).expect("six dots");
    assert_eq!(usize::from(cursor.y), row, "the cursor is on the input row");
    assert!(lines[row].chars().nth(usize::from(cursor.x) - 1) == Some('•'), "right after the dots: {}", lines[row]);
    // Without a keychain it says the password lasts this session only.
    app.keychain_state = KeychainState::Unavailable;
    let text = screen(&mut app, 110, 34).0.join("\n");
    assert!(text.contains("No system keychain was found"), "{text}");
    assert!(text.contains("session only"), "{text}");
    assert!(text.contains("never written to config.toml"), "{text}");
    assert!(text.contains("Enter = log in ·") && !text.contains("and save"), "{text}");
    // Keychain not asked yet: it says what it will try.
    app.keychain_state = KeychainState::Unknown;
    let text = screen(&mut app, 110, 34).0.join("\n");
    assert!(text.contains("If there is no keychain it is kept for this session only"), "{text}");
    // The risk notice is in every variant (checked on one line: it wraps).
    for state in [KeychainState::NotSaved, KeychainState::Unavailable, KeychainState::Unknown] {
        app.keychain_state = state;
        let (_, lines) = cursor_of(&mut app, 110, 34);
        let flat: String =
            lines.iter().map(|l| l.chars().skip(22).take(66).collect::<String>()).collect::<Vec<_>>().join(" ");
        let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
        let flat: String = flat.replace("│ │", "").replace("│", "");
        let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains(PASSWORD_DISCLAIMER), "{state:?}: {flat}");
    }
    // Small terminals still fit all of it, with the input and its cursor on screen.
    for (cols, rows) in [(60u16, 20u16), (60, 24), (70, 18), (50, 16)] {
        let (cursor, lines) = cursor_of(&mut app, cols, rows);
        let text = lines.join("\n");
        assert!(text.contains("Pandora password"), "{cols}x{rows}:\n{text}");
        assert!(rows < 18 || text.contains("Esc = cancel"), "{cols}x{rows}:\n{text}");
        assert!(text.contains("responsible"), "{cols}x{rows}:\n{text}");
        assert!(text.contains("› "), "{cols}x{rows}: the input row:\n{text}");
        assert!(lines[usize::from(cursor.y)].contains("› "), "{cols}x{rows}: cursor on the input row");
    }
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.prompt.is_none() && !app.overlay_open(), "Esc closes it");
    let text = screen(&mut app, 110, 34).0.join("\n");
    assert!(!text.contains("Pandora password"), "{text}");
    app.open_prompt(Prompt::Email);
    assert!(app.prompt == Some(Prompt::Email) && !app.overlay_open(), "the email box is not a popup");
    let text = screen(&mut app, 110, 34).0.join("\n");
    assert!(!text.contains("Pandora password"), "the email stays in its own box: {text}");
}

/// The words inside the "Turn off Good Vibes Only?" popup, joined by single spaces.
fn good_vibes_popup_words(app: &mut App, cols: u16, rows: u16) -> Option<String> {
    let lines = screen(app, cols, rows).0;
    let width = usize::from(66.min(cols.saturating_sub(4)).max(cols.min(8)));
    let top = lines.iter().position(|l| l.contains("Turn off Good Vibes"))?;
    let left = lines[top].chars().take_while(|c| *c != 'T').count().saturating_sub(2);
    let words: Vec<String> = lines[top + 1..]
        .iter()
        .map(|l| l.chars().skip(left + 1).take(width - 2).collect::<String>())
        .take_while(|l| !l.contains('─'))
        .collect();
    Some(words.join(" ").split_whitespace().collect::<Vec<_>>().join(" "))
}

#[test]
fn turning_good_vibes_only_off_asks_first_in_a_risk_popup_that_fits_small_terminals() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = demo();
    // Keep the menu short so the centered popup extraction is not clipped by section pins.
    for &(key, _) in crate::config::BROWSE_SECTIONS {
        app.config.browse.sections.insert(key.to_owned(), false);
    }
    app.view = View::Settings;
    app.focus = crate::app::Focus::Content;
    app.settings_cursor = crate::app::Setting::at(crate::app::Setting::GoodVibesOnly);
    // The row and its description use the new name.
    let text = screen(&mut app, 120, 40).0.join("\n");
    assert!(text.contains("Good Vibes Only") && !text.contains("Good Guy"), "{text}");
    assert!(good_vibes_popup_words(&mut app, 120, 40).is_none(), "no popup yet");
    // On → Muted asks first, with its own gray-area notice.
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.confirm_good_vibes, Some(GoodVibes::Muted));
    let text = screen(&mut app, 120, 40).0.join("\n");
    assert!(text.contains("Mute Pandora's ads?") && text.contains("Enter = mute them anyway"), "{text}");
    assert!(text.contains("gray area") && text.contains("Mute Anyway"), "{text}");
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.config.player.good_vibes_only, GoodVibes::Muted);
    // Muted → Off asks too.
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.confirm_good_vibes == Some(GoodVibes::Off) && app.overlay_open());
    assert_eq!(app.config.player.good_vibes_only, GoodVibes::Muted);
    let text = screen(&mut app, 120, 40).0.join("\n");
    assert!(text.contains("Turn off Good Vibes Only?") && text.contains("Enter = turn it off anyway"), "{text}");
    assert!(text.contains("Esc = keep it as it was") && !text.contains("http"), "{text}");
    assert!(text.contains("Turn Off Anyway") && text.contains("Keep As Is"), "the key bar: {text}");
    // The whole risk notice is there, at any size that has room for it.
    for (cols, rows) in [(120u16, 40u16), (80, 24), (60, 20), (50, 16), (40, 16), (40, 14)] {
        let words = good_vibes_popup_words(&mut app, cols, rows).unwrap_or_default();
        assert!(words.contains(GOOD_VIBES_DISCLAIMER), "{cols}x{rows}: {words}");
        assert!(words.contains("Esc = keep"), "{cols}x{rows}: {words}");
    }
    // A tiny terminal still draws a popup inside the screen, from the start of the notice.
    for (cols, rows) in [(30u16, 8u16), (24, 6)] {
        let (lines, _) = screen(&mut app, cols, rows);
        assert!(lines.iter().any(|l| l.contains("Turn off")), "{cols}x{rows}");
        assert!(lines.iter().any(|l| l.contains("Skipping")), "{cols}x{rows}: {lines:?}");
    }
    // Esc leaves it on, closes the popup; Enter in a second round turns it off.
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.confirm_good_vibes.is_none() && app.config.player.good_vibes_only == GoodVibes::Muted);
    assert!(!screen(&mut app, 120, 40).0.join("\n").contains("Turn off Good Vibes Only?"));
}

#[test]
fn the_rename_box_edits_in_the_middle_and_draws_the_cursor_there() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let press = |app: &mut App, code| app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(300));
    app.on_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT));
    press(&mut app, KeyCode::Char('r'));
    let name = app.rename.as_ref().unwrap().text.text().to_owned();
    let (end, lines) = cursor_of(&mut app, 120, 36);
    let row = lines.iter().position(|l| l.contains(" › ")).expect("the input row");
    assert_eq!(usize::from(end.y), row);
    // Home puts the cursor on the first letter; typing goes in before it.
    press(&mut app, KeyCode::Home);
    let (home, _) = cursor_of(&mut app, 120, 36);
    assert_eq!(end.x - home.x, name.width() as u16, "the cursor moved to the start");
    press(&mut app, KeyCode::Char('>'));
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Delete);
    let (moved, lines) = cursor_of(&mut app, 120, 36);
    let shown = lines[row].split(" › ").nth(1).unwrap().trim_end_matches(['│', ' ']).to_owned();
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    chars.next();
    let expected = format!(">{first}{}", chars.collect::<String>());
    assert!(shown.starts_with(&expected), "{shown:?} vs {expected:?}");
    assert_eq!(moved.x - home.x, 2, "after the typed > and one step right");
}

#[test]
fn the_password_prompt_masks_and_keeps_the_cursor_where_it_is() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = demo();
    app.view = View::Settings;
    app.open_prompt(Prompt::Password);
    for c in "secret".chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    let (end, lines) = cursor_of(&mut app, 100, 30);
    assert!(lines.iter().any(|l| l.contains("••••••")), "masked");
    assert!(!lines.iter().any(|l| l.contains("secret")));
    app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    let (back, _) = cursor_of(&mut app, 100, 30);
    assert_eq!(end.x - back.x, 2);
    assert_eq!(end.y, back.y);
}

#[test]
fn the_time_line_ends_as_far_from_the_right_border_as_the_cover_starts_from_the_left() {
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(300));
    for (cols, rows) in [(200u16, 50u16), (160, 40), (120, 36)] {
        let (lines, _) = screen(&mut app, cols, rows);
        let grid: Vec<Vec<char>> = lines.iter().map(|l| l.chars().collect()).collect();
        let y = grid.iter().position(|r| r.contains(&'●')).expect("time line");
        let row = &grid[y];
        let cover_x = grid.iter().filter_map(|r| r.iter().position(|c| matches!(c, '▀' | '▄'))).min().unwrap();
        let left = (0..cover_x).rev().find(|&x| "│┃║".contains(row[x])).expect("left border");
        let dot = row.iter().position(|&c| c == '●').unwrap();
        let right = (dot..row.len()).find(|&x| "│┃║".contains(row[x])).expect("right border");
        let end = (dot..right).rev().find(|&x| row[x] != ' ').unwrap();
        assert!(row[end].is_ascii_digit(), "{cols}x{rows}: ends with the total time: {}", lines[y]);
        assert_eq!(right - end, cover_x - left, "{cols}x{rows}: symmetrical\n{}", lines.join("\n"));
    }
}

#[test]
#[allow(deprecated)]
fn the_half_block_stand_in_fills_the_pictures_rectangle_and_nothing_moves() {
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(300));
    let mut sixel = ratatui_image::picker::Picker::from_fontsize(ratatui_image::FontSize::new(8, 17));
    sixel.set_protocol_type(ratatui_image::picker::ProtocolType::Sixel);
    app.art.set_graphics(Some(sixel));
    for (cols, rows) in [(255u16, 70u16), (160, 40)] {
        let mut seen = Vec::new();
        for help in [false, true, false] {
            app.dialogs.clear();
            if help {
                app.open_help();
            }
            app.art.set_overlay(help);
            let (lines, buf) = screen(&mut app, cols, rows);
            let cover = app.art.last_cover;
            // The time line's total, at its right end (the start may be under Help).
            let time = lines
                .iter()
                .position(|l| {
                    l.contains("──")
                        && l.trim_end_matches([' ', '│']).ends_with(|c: char| c.is_ascii_digit())
                        && l.contains(":")
                })
                .map(|y| (y, lines[y].trim_end_matches([' ', '│']).chars().count()));
            assert!(time.is_some(), "{cols}x{rows}: time line\n{}", lines.join("\n"));
            seen.push((cover, time));
            // At 160 columns Help covers the whole cover; at 255 it shows beside it.
            if help && cols > 200 {
                let inside: String = cover.positions().map(|at| buf[at].symbol().to_owned()).collect();
                assert!(inside.contains('▀') || inside.contains('▄'), "{cols}x{rows}: half blocks under Help");
            }
        }
        assert!(seen.windows(2).all(|w| w[0] == w[1]), "{cols}x{rows}: same cover and info with Help: {seen:?}");
    }
}

#[test]
#[allow(deprecated)]
fn a_song_change_under_help_fetches_and_shows_the_new_cover() {
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(300));
    let mut sixel = ratatui_image::picker::Picker::from_fontsize(ratatui_image::FontSize::new(8, 17));
    sixel.set_protocol_type(ratatui_image::picker::ProtocolType::Sixel);
    app.art.set_graphics(Some(sixel));
    let frame = |app: &mut App| {
        app.tick();
        app.art.set_overlay(app.overlay_open());
        screen(app, 255, 70);
    };
    for _ in 0..10 {
        frame(&mut app);
        app.settle(std::time::Duration::from_millis(50));
    }
    let old = app.now.as_ref().unwrap().cover_art_url.clone();
    assert_eq!(app.art.shown_cover(), Some((old.clone(), false)), "the picture");
    app.open_help();
    frame(&mut app);
    assert_eq!(app.art.shown_cover(), Some((old.clone(), true)), "half blocks under Help");
    app.demo_warm_up(1, std::time::Duration::ZERO);
    let new = app.now.as_ref().unwrap().cover_art_url.clone();
    assert_ne!(new, old);
    for _ in 0..20 {
        frame(&mut app);
        app.settle(std::time::Duration::from_millis(50));
    }
    assert!(app.help_open(), "Help still open");
    assert_eq!(app.art.shown_cover(), Some((new.clone(), true)), "the new cover, in half blocks");
    app.dialogs.clear();
    frame(&mut app);
    assert_eq!(app.art.shown_cover(), Some((new, false)), "the new picture once Help closes");
}

#[test]
fn settings_has_no_column_headers_and_starts_with_its_first_section() {
    let mut app = demo();
    app.view = View::Settings;
    let (lines, _) = screen(&mut app, 160, 40);
    let all = lines.join("\n");
    assert!(!all.contains("SETTING") && !all.contains("VALUE"), "{all}");
    let top = lines.iter().position(|l| l.contains("Settings")).unwrap();
    let first = Setting::ALL[0].0;
    assert!(lines[top + 1].contains(first), "{first} right under the border\n{all}");
}

#[test]
fn colours_behind_a_popup_fade_toward_the_background() {
    let bg = Color::Rgb(20, 20, 30);
    let Color::Rgb(r, g, b) = dim_color(Color::Rgb(250, 200, 0), bg) else { panic!() };
    assert!(r < 130 && g < 110 && b > 20, "darker, toward the background: {r} {g} {b}");
    assert!(r.abs_diff(b) < 70, "less saturated: {r} vs {b}");
    assert_eq!(dim_color(bg, bg), bg, "the background stays");
    assert_eq!(dim_color(Color::Yellow, bg), Color::DarkGray);
    assert_eq!(dim_color(Color::Indexed(208), bg), Color::DarkGray);
    assert_eq!(dim_color(Color::Reset, bg), Color::Reset);
}

#[test]
fn a_popup_dims_everything_behind_it_but_not_itself() {
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(300));
    app.art.set_overlay(true);
    let (_, before) = screen(&mut app, 255, 70);
    app.open_help();
    let (lines, after) = screen(&mut app, 255, 70);
    let top = lines.iter().position(|l| l.contains("boxvibe Help")).unwrap() as u16;
    let left = lines[top as usize].chars().position(|c| c == '┌' || c == '╭').unwrap_or(0) as u16;
    let brightness = |c: Color| match c {
        Color::Rgb(r, g, b) => u32::from(r) + u32::from(g) + u32::from(b),
        _ => 0,
    };
    // Outside the popup: nothing brighter than before, and lots darker
    // (the half-block cover included).
    let mut darker = 0;
    for y in 0..70 {
        for x in 0..255 {
            let (a, b) = (&before[(x, y)], &after[(x, y)]);
            if y >= top && x >= left && x < left + 10 {
                continue;
            }
            if a.symbol() == b.symbol() && brightness(b.fg) + brightness(b.bg) < brightness(a.fg) + brightness(a.bg) {
                darker += 1;
            }
        }
    }
    assert!(darker > 500, "only {darker} cells dimmed");
    let blocks = (0..70).flat_map(|y| (0..255).map(move |x| (x, y))).filter(|&p| after[p].symbol() == "▀");
    assert!(blocks.clone().count() > 0);
    assert!(blocks.into_iter().all(|p| !matches!(after[p].fg, Color::Rgb(r, g, b) if r > 140 || g > 140 || b > 140)));
    // The popup's own text keeps its colour.
    let title = lines[top as usize].find("boxvibe Help").unwrap();
    let x = lines[top as usize][..title].chars().count() as u16;
    assert!(brightness(after[(x, top)].fg) > 400, "the popup stays bright: {:?}", after[(x, top)]);
}

#[test]
fn help_lists_every_bound_key_and_nothing_unbound() {
    use crate::app::{KEYMAP, command_for};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let tokens: Vec<&str> =
        HELP.iter().flat_map(|(_, keys)| keys.iter()).flat_map(|(k, _)| k.split_whitespace()).collect();
    for b in KEYMAP {
        let want = match b.code {
            KeyCode::Char(' ') => "space".to_owned(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Up => "↑".into(),
            KeyCode::Down => "↓".into(),
            KeyCode::Left => "←".into(),
            KeyCode::Right => "→".into(),
            KeyCode::BackTab => "Tab".into(),
            other => format!("{other:?}"),
        };
        assert!(tokens.contains(&want.as_str()), "Help does not list {:?} ({want})", b.code);
    }
    for t in &tokens {
        let mut chars = t.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else { continue };
        if matches!(c, '↑' | '↓' | '←' | '→') {
            continue;
        }
        let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        assert!(command_for(key).is_some(), "Help lists {t:?} but nothing is bound to it");
    }
}

#[test]
fn help_fits_its_longest_line_and_its_keys_line_up() {
    let mut app = demo();
    app.open_help();
    for (cols, rows) in [(255u16, 70u16), (160, 40)] {
        let (lines, _) = screen(&mut app, cols, rows);
        let mut end = demo();
        end.open_help();
        end.on_key(ratatui::crossterm::event::KeyEvent::from(ratatui::crossterm::event::KeyCode::Char('G')));
        let all = lines.join("\n") + "\n" + &screen(&mut end, cols, rows).0.join("\n");
        for whole in [
            "play the station · open Settings / About",
            "Now Playing: the pane gets the keys",
            HELP_FOOTER.trim(),
            "boxvibe Help",
            "? or Esc close",
        ] {
            assert!(all.contains(whole), "{cols}x{rows}: {whole:?} is cut off\n{all}");
        }
        // Every description starts the same distance in from Help's left border.
        let start = |what: &str| {
            let l = lines.iter().find(|l| l.contains(what)).unwrap();
            let at = l.find(what).unwrap();
            l[l[..at].rfind('│').unwrap()..at].chars().count()
        };
        let a = start("volume down / up");
        assert_eq!(a, start("like / bad the selected row"), "{cols}x{rows}\n{all}");
        assert_eq!(a, start("skip to the next song"), "{cols}x{rows}\n{all}");
    }
}

#[test]
fn the_pinned_page_lists_the_pins_and_the_menu_has_its_entry() {
    let mut app = demo();
    let (lines, _) = screen(&mut app, 140, 40);
    assert!(lines.iter().any(|l| l.contains("Pinned")), "the menu entry, shown by default");
    // I still pins, but the bar leaves it to the `.` menu.
    assert!(!key_caps(&app).iter().any(|(k, _)| *k == "I"));
    assert!(key_caps(&app).iter().any(|(k, v)| *k == "." && *v == "Song Menu"));
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('I'),
        crossterm::event::KeyModifiers::SHIFT,
    ));
    let title = app.pins[0].title.clone();
    app.view = View::Pinned;
    app.focus = Focus::Content;
    let (lines, _) = screen(&mut app, 140, 40);
    let all = lines.join("\n");
    assert!(all.contains("Pinned (1)") && all.contains(&title) && all.contains(&app.pins[0].pinned), "{all}");
    assert!(all.contains("device only"), "local-only note: {all}");
    assert!(key_caps(&app).iter().any(|(k, v)| *k == "x" && *v == "Unpin"));
    app.config.ui.pinned_in_menu = false;
    app.view = View::Player;
    app.status.clear();
    let (lines, _) = screen(&mut app, 140, 40);
    assert!(!lines.iter().any(|l| l.contains("Pinned")), "hidden");
}

#[test]
fn the_key_bar_leaves_out_theme_and_icons_and_t_does_nothing() {
    let mut app = demo();
    let caps = key_caps(&app);
    assert!(!caps.iter().any(|(k, _)| matches!(*k, "t" | "i" | "T")), "{caps:?}");
    let theme = app.config.ui.theme().to_owned();
    app.on_key(crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('t')));
    assert_eq!(app.config.ui.theme(), theme, "t no longer switches the theme");
}

/// Every place the keys can be: (focus, view, pane), plus the menu on a favorite.
fn contexts(app: &mut App) -> Vec<(String, crate::app::Focus, View, Pane, bool)> {
    use crate::app::Focus;
    let mut all = Vec::new();
    for view in [View::Player, View::Stations, View::Browse, View::Settings, View::About] {
        for (focus, favorite) in [(Focus::Menu, false), (Focus::Menu, true), (Focus::Content, false)] {
            let panes = if view == View::Player && focus == Focus::Content {
                vec![Pane::NowPlaying, Pane::UpNext, Pane::History]
            } else {
                vec![Pane::UpNext]
            };
            for pane in panes {
                all.push((format!("{view:?}/{focus:?}/{pane:?}/favorite={favorite}"), focus, view, pane, favorite));
            }
        }
    }
    let _ = app;
    all
}

/// Puts the app in one of [`contexts`] with a favorite in the menu.
fn enter(app: &mut App, focus: crate::app::Focus, view: View, pane: Pane, favorite: bool) {
    app.rename = None;
    app.dialogs.clear();
    app.dialogs.clear();
    app.add_station = None;
    app.editor = None;
    app.on_add_row = false;
    app.view = view;
    app.focus = focus;
    app.pane = pane;
    app.menu_cursor = 0;
    if favorite {
        let at = app.menu_items().iter().position(|m| matches!(m, MenuItem::Favorite(_))).expect("a favorite");
        app.menu_cursor = at;
    }
}

#[test]
fn a_pane_key_is_hinted_exactly_where_it_does_something() {
    use crate::app::Focus;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(400));
    app.on_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT));
    // The favorite is a station that allows renaming and deleting.
    let station = app.stations.iter().find(|s| s.allow_rename && s.allow_delete).cloned().expect("a station");
    app.config.ui.favorites = vec![crate::config::Favorite { id: station.id.clone(), name: station.name }];
    assert!(!app.queue.is_empty() && !app.history.is_empty() && app.now.is_some(), "the demo has songs");
    let hinted = |app: &App, key: &str| {
        key_caps(app).iter().any(|(k, _)| k.split('/').any(|part| part == key) && *k != "j/k ↑↓")
    };
    for (name, focus, view, pane, favorite) in contexts(&mut app) {
        if favorite && !matches!(view, View::Player | View::Stations | View::Browse | View::Settings | View::About) {
            continue;
        }
        // r, x and v: what the key does is what the bar says.
        for key in ['r', 'x', 'v', 'a', 'e'] {
            enter(&mut app, focus, view, pane, favorite);
            let shown = hinted(&app, &key.to_string());
            app.on_key(KeyEvent::from(KeyCode::Char(key)));
            let did = match key {
                'r' => app.rename.is_some(),
                'x' => app.confirm_delete().is_some(),
                'a' => app.add_station.is_some(),
                'e' => app.editor.is_some(),
                _ => app.details().is_some(),
            };
            // v on a song is in the `.` menu: it works, unhinted.
            let song_v = key == 'v' && view == View::Player && focus == Focus::Content;
            if song_v {
                assert!(!shown, "v in {name} is left to the . menu");
            } else {
                assert_eq!(shown, did, "{key} in {name}: hinted {shown}, acts {did}");
            }
        }
        // l/b, U/D, I, B: in the `.` menu, so never on the bar; `.` is, with a song.
        enter(&mut app, focus, view, pane, favorite);
        for k in ["l", "b", "U", "D", "I", "B", "Z"] {
            assert!(!hinted(&app, k), "{k} in {name}");
        }
        assert!(hinted(&app, "."), ". in {name}");
        // f: stars a station row, or unstars the favorite the menu is on.
        let stars = (view == View::Stations && focus == Focus::Content) || (focus == Focus::Menu && favorite);
        assert_eq!(hinted(&app, "f"), stars, "f in {name}");
        // Enter changes a setting only on the Settings page.
        let change = key_caps(&app).iter().any(|(_, what)| *what == "Change");
        assert_eq!(change, view == View::Settings && focus == Focus::Content, "Change in {name}");
    }
}

#[test]
fn station_keys_leave_out_what_pandora_does_not_allow_and_what_is_not_loaded() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(400));
    app.on_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT));
    let keys = |app: &App| key_caps(app).iter().map(|(k, _)| *k).collect::<Vec<_>>();
    while app.stations_index() > 0 {
        app.on_key(KeyEvent::from(KeyCode::Char('k')));
    }
    for (i, s) in app.stations.clone().iter().enumerate() {
        while app.stations_index() < i {
            app.on_key(KeyEvent::from(KeyCode::Char('j')));
        }
        let caps = keys(&app);
        assert_eq!(caps.contains(&"r"), s.allow_rename, "r on {}", s.name);
        assert_eq!(caps.contains(&"x"), s.allow_delete, "x on {}", s.name);
        let thumbs = s.is_locked() && !s.is_shuffle;
        assert_eq!(caps.contains(&"e"), s.allow_edit_seeds || thumbs, "e on {}", s.name);
    }
    // No stations yet: nothing to play, favorite, rename, delete or show.
    app.stations.clear();
    let caps = keys(&app);
    assert!(!caps.iter().any(|k| matches!(*k, "r" | "x" | "v" | "f" | "e")), "{caps:?}");
}

#[test]
fn pause_skip_and_the_song_menu_are_hinted_only_with_a_song_to_act_on() {
    let mut app = App::new(Config::default(), None, &[], true);
    let has = |app: &App, key: &str| key_caps(app).iter().any(|(k, _)| *k == key);
    // Nothing playing yet.
    assert!(app.now.is_none());
    for key in ["space", "N", "."] {
        assert!(!has(&app, key), "{key} with nothing playing");
    }
    app.settle(std::time::Duration::from_millis(400));
    app.demo_warm_up(3, std::time::Duration::ZERO);
    app.settle(std::time::Duration::from_millis(400));
    assert!(app.now.is_some());
    for key in ["space", "N", "."] {
        assert!(has(&app, key), "{key} while playing");
    }
    // Thumbs, pin, sleep and mode are in the `.` menu, not the bar.
    for key in ["U/D", "I", "B", "l/b", "m"] {
        assert!(!has(&app, key), "{key} is left to the . menu");
    }
}

#[test]
fn about_and_now_playing_have_no_rows_to_move_over() {
    use crate::app::Focus;
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(400));
    let moves = |app: &App| key_caps(app).iter().any(|(k, _)| *k == "j/k ↑↓");
    enter(&mut app, Focus::Content, View::About, Pane::UpNext, false);
    assert!(!moves(&app), "About");
    enter(&mut app, Focus::Content, View::Player, Pane::NowPlaying, false);
    assert!(!moves(&app), "Now Playing");
    enter(&mut app, Focus::Content, View::Player, Pane::UpNext, false);
    assert!(moves(&app), "Up Next");
    enter(&mut app, Focus::Content, View::Settings, Pane::UpNext, false);
    assert!(moves(&app), "Settings");
}

#[test]
fn the_cover_draws_in_half_blocks_inside_the_art_box() {
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(300));
    let song = app.now.clone().expect("a song");
    assert_eq!(app.art.status(&song.cover_art_url), crate::art::Cover::None, "demo cover arrived");
    assert!(app.art.protocol(&song.cover_art_url, crate::art::Look::of(&app.config.ui, app.palette)).is_some());
    let (lines, _) = screen(&mut app, 120, 36);
    let (top, bottom, left, right) = art_box(&app);
    assert!(bottom > top + 2);
    let inside: String = lines[top + 1..bottom]
        .iter()
        .map(|l| l.chars().skip(left + 1).take(right - left - 1).collect::<String>())
        .collect();
    assert!(inside.contains('▀') || inside.contains('▄'), "halfblocks picture:\n{}", lines.join("\n"));
    assert!(!inside.contains("no cover"));
    // No frame around it, and nothing spilled past the box.
    assert!(!lines[top + 1..bottom].iter().any(|l| l.contains('╭') && l.find('╭') < l.find('▀')));
    for line in &lines[top + 1..bottom] {
        let chars: Vec<char> = line.chars().collect();
        assert!(!['▀', '▄'].contains(&chars[left]) && !['▀', '▄'].contains(&chars[right]), "{line}");
    }
}

/// What the art box holds, row by row (no border: it is the area itself).
fn art_rows(app: &App, lines: &[String]) -> Vec<String> {
    let a = app.art.last_area;
    lines[a.y as usize..(a.y + a.height) as usize]
        .iter()
        .map(|l| l.chars().skip(a.x as usize).take(a.width as usize).collect())
        .collect()
}

/// The art box shows just `message`, centred, and no picture or bars.
fn assert_only_message(app: &mut App, message: &str) {
    let (lines, _) = screen(app, 120, 36);
    let rows = art_rows(&*app, &lines);
    let shown: Vec<(usize, &String)> = rows.iter().enumerate().filter(|(_, r)| !r.trim().is_empty()).collect();
    let [(row, text)] = shown[..] else { panic!("one line of text in the art box:\n{}", rows.join("\n")) };
    assert_eq!(text.trim(), message);
    assert_eq!(row, rows.len() / 2, "in the middle, top to bottom");
    let width = app.art.last_area.width as usize;
    let left = text.chars().take_while(|c| *c == ' ').count();
    let right = width - left - message.width();
    assert!(left.abs_diff(right) <= 1, "centred: {left} left, {right} right");
    for glyph in ['▀', '▄', '█', '▁', '▂', '▃', '▅', '▆', '▇', '▔'] {
        assert!(!rows.iter().any(|r| r.contains(glyph)), "no picture or bars ({glyph}):\n{}", rows.join("\n"));
    }
}

#[test]
fn with_album_art_off_the_space_says_cover_art_off() {
    let mut app = demo();
    app.config.ui.album_art = crate::config::AlbumArt::Off;
    assert_only_message(&mut app, "Cover art off");
    // Playing music changes nothing: there is no visualizer to fall back on.
    assert_only_message(&mut app, "Cover art off");
    // Off by choice wins over a song that has no cover anyway.
    if let Some(song) = app.now.as_mut() {
        song.cover_art_url.clear();
    }
    assert_only_message(&mut app, "Cover art off");
    // Turning it back on says "No cover art" for that song.
    app.config.ui.album_art = crate::config::AlbumArt::Picture;
    assert_only_message(&mut app, "No cover art");
}

#[test]
fn shift_c_flips_between_the_picture_and_cover_art_off() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = demo();
    app.config.ui.album_art = crate::config::AlbumArt::Picture;
    app.on_key(KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT));
    assert_eq!(app.config.ui.album_art, crate::config::AlbumArt::Off);
    assert_only_message(&mut app, "Cover art off");
    app.on_key(KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT));
    assert_eq!(app.config.ui.album_art, crate::config::AlbumArt::Picture);
    assert!(!screen(&mut app, 120, 36).0.join("\n").contains("Cover art off"));
}

#[test]
fn without_a_cover_the_space_says_so_and_not_a_visualizer() {
    let mut app = demo();
    if let Some(song) = app.now.as_mut() {
        song.cover_art_url.clear();
    }
    assert_only_message(&mut app, "No cover art");
}

#[test]
fn a_cover_on_its_way_says_loading_and_a_lost_one_says_no_cover() {
    let mut app = demo();
    app.art = crate::art::Art::default();
    let url = app.now.as_ref().map(|s| s.cover_art_url.clone()).unwrap();
    assert!(app.art.want(&url), "the download starts");
    assert_only_message(&mut app, "Loading cover art…");
    // Art off while one downloads: nothing is coming, and the space says art is off.
    app.config.ui.album_art = crate::config::AlbumArt::Off;
    assert_only_message(&mut app, "Cover art off");
    // A cover that has been given up on (every try failed).
    app.config.ui.album_art = crate::config::AlbumArt::Picture;
    let mut gone = crate::art::Art::default();
    for _ in 0..=crate::art::RETRY_WAITS.len() {
        gone.arrived(url.clone(), None);
    }
    app.art = gone;
    assert_only_message(&mut app, "No cover art");
}

#[test]
fn the_quit_question_is_vibekits_confirm_with_a_blank_line_above_the_question() {
    let mut app = demo();
    app.ask_quit();
    let (lines, _) = screen(&mut app, 120, 36);
    let top = lines.iter().position(|l| l.contains("┌ Quit ")).expect("quit box");
    assert!(lines[top + 1].contains("│ ") && !lines[top + 1].contains("Quit boxvibe?"), "blank line first");
    assert!(lines[top + 2].contains("Quit boxvibe?"), "{}", lines[top + 2]);
}

#[test]
fn a_long_question_wraps_and_cuts_to_fit() {
    assert_eq!(wrap_question("Quit boxvibe?", 36, 3), ["Quit boxvibe?"]);
    let w = wrap_question("one two three four five six seven", 9, 2);
    assert_eq!(w.len(), 2, "{w:?}");
    assert!(w[1].ends_with('…') && w.iter().all(|l| l.width() <= 9), "{w:?}");
}

#[test]
fn shift_q_shows_the_quit_box_and_says_q_again_quits() {
    let mut app = demo();
    app.ask_quit();
    let all = screen(&mut app, 120, 36).0.join("\n");
    for want in ["Quit boxvibe?", "Quit (y)", "No (n)", "Q again to quit · Esc stay"] {
        assert!(all.contains(want), "{want:?}\n{all}");
    }
    assert!(!all.contains("any other key"), "{all}");
    let caps = key_caps(&app);
    assert!(caps.contains(&("Q y", "Yes, Quit")), "{caps:?}");
}

#[test]
fn the_logo_and_version_sit_on_the_menus_top_border() {
    let mut app = demo();
    app.config.ui.icons = crate::config::IconSet::Unicode;
    app.status = "hello status".into();
    let (lines, buffer) = screen(&mut app, 120, 36);
    let top = &lines[0];
    assert!(top.starts_with(&format!("┌ {} boxvibe ─", crate::icons::LOGO)), "{top}");
    let menu_end = top.find('┐').unwrap();
    let version = format!(" {} ┐", env!("CARGO_PKG_VERSION"));
    assert!(top[..menu_end + '┐'.len_utf8()].ends_with(&version), "{top}");
    assert!(top.contains("Now Playing"), "no header row: Now Playing's border is the first row: {top}");
    assert!(!lines.iter().any(|l| l.contains("demo@") || l.contains("not logged in ")));
    assert!(lines[34].starts_with(" hello status"), "status bar above the keys: {}", lines[34]);
    // "box" and its icon blue, "vibe" the text colour.
    let p = app.palette;
    let x = top.chars().position(|c| c == 'b').unwrap() as u16;
    assert_eq!(buffer[(2, 0)].fg, p.blue);
    assert_eq!((buffer[(x, 0)].fg, buffer[(x + 3, 0)].fg), (p.blue, p.fg));
}

#[test]
fn help_is_titled_boxvibe_help_and_says_how_to_close() {
    let mut app = demo();
    app.open_help();
    let all = screen(&mut app, 120, 40).0.join("\n");
    assert!(all.contains("┌ boxvibe Help "), "{all}");
    assert!(all.contains("? or Esc close") && !all.contains("Any Key"), "{all}");
}

#[test]
fn every_border_style_draws_and_the_menu_width_holds() {
    let mut app = demo();
    for (style, corner) in [
        (BorderStyle::Plain, '┌'),
        (BorderStyle::Rounded, '╭'),
        (BorderStyle::Thick, '┏'),
        (BorderStyle::Double, '╔'),
        (BorderStyle::None, ' '),
    ] {
        app.config.ui.border_style = style;
        let (lines, _) = screen(&mut app, 120, 36);
        assert_eq!(lines[0].chars().next(), Some(corner), "{style:?}: {}", lines[0]);
        assert!(lines[0].contains("boxvibe") && lines[0].contains("Now Playing"), "titles stay: {}", lines[0]);
    }
    app.config.ui.border_style = BorderStyle::Plain;
    for (width, cols, menu) in [(20, 120, 20), (40, 120, 40), (40, 60, 30)] {
        app.config.ui.menu_width = width;
        let (lines, _) = screen(&mut app, cols, 30);
        let end = lines[0].chars().position(|c| c == '┐').unwrap();
        assert_eq!(end + 1, menu, "menu_width {width} at {cols} columns");
    }
}

#[test]
fn now_playing_heads_the_menu_with_an_inset_rule() {
    let mut app = demo();
    app.view = View::Settings;
    app.focus = crate::app::Focus::Content;
    let (lines, _) = screen(&mut app, 120, 36);
    assert!(lines[1].starts_with("│ ▶  NOW PLAYING"), "{}", lines[1]);
    let menu: String = lines[2].chars().take(30).collect();
    assert!(menu.starts_with("│  ──") && menu.ends_with("──  │"), "inset rule: {menu:?}");
    // That rule doubles as MY COLLECTION's, so its heading sits right under it.
    assert!(lines[3].starts_with("│ MY COLLECTION"), "{}", lines[3]);
    // Nothing playing: that rule is absent, and MY COLLECTION is the first row.
    app.current_station = None;
    let (idle, _) = screen(&mut app, 120, 36);
    assert!(idle[1].starts_with("│ MY COLLECTION"), "no rule before playback: {}", idle[1]);
    assert!(!idle[1].contains('─'), "{}", idle[1]);
    if std::env::var_os("BOXVIBE_SHOTS").is_some() {
        let _ = std::fs::create_dir_all("/opt/cursor/artifacts");
        let _ = std::fs::write("/opt/cursor/artifacts/menu-playing.txt", lines.join("\n"));
        let _ = std::fs::write("/opt/cursor/artifacts/menu-before.txt", idle.join("\n"));
        let mut genres = demo();
        genres.show_browse_section("Browse Genres");
        genres.settle(std::time::Duration::from_millis(400));
        let (page, _) = screen(&mut genres, 120, 36);
        let _ = std::fs::write("/opt/cursor/artifacts/genres.txt", page.join("\n"));
    }
    assert!(lines[4].contains("My Stations ("), "{}", lines[4]);
    assert!(lines[35].contains(" P  Now Playing"), "key bar: {}", lines[35]);
}

#[test]
fn song_panes_fit_ten_rows_and_now_playing_takes_the_rest() {
    let mut app = demo();
    // Side by side: Up Next and History are SONG_PANE (13) tall, Now Playing the rest.
    for (rows, want) in [(36, 21), (50, 35), (20, 11)] {
        assert_eq!(now_playing_height(rows - 2, false), want, "{rows} rows");
        let (lines, _) = screen(&mut app, 120, rows);
        let up = lines.iter().position(|l| l.contains("Up Next (")).expect("Up Next");
        assert_eq!(up, usize::from(want), "Up Next right under Now Playing");
    }
    // Stacked: two song panes of 13 each.
    assert_eq!(now_playing_height(48, true), 22);
    assert_eq!(now_playing_height(30, true), NOW_PLAYING_MIN, "short: Now Playing keeps its minimum");
    assert_eq!(now_playing_height(5, true), 5, "never more than the screen");
    // A full History (10 songs) fits exactly: last row right above the bottom border.
    for i in 0..12 {
        let mut s = app.now.clone().unwrap();
        s.title = format!("Old song {i}");
        app.history.insert(0, s);
    }
    app.history.truncate(10);
    let (lines, _) = screen(&mut app, 120, 36);
    let hist = lines.iter().position(|l| l.contains("History (")).unwrap();
    assert!(lines[hist + 11].contains("Old song"), "10th row: {}", lines[hist + 11]);
    assert!(lines[hist + 12].contains('└') || lines[hist + 12].contains('╰'), "{}", lines[hist + 12]);
}

#[test]
fn stations_page_and_favorites_section() {
    let mut app = demo();
    let (lines, _) = screen(&mut app, 120, 36);
    assert!(!lines.iter().any(|l| l.contains("FAVORITES")), "hidden while empty");
    let fav = app.stations[1].clone();
    app.config.ui.favorites.push(crate::config::Favorite { id: fav.id.clone(), name: fav.name.clone() });
    // Starred but logged out: the section is not there at all.
    let stations = std::mem::take(&mut app.stations);
    app.session = crate::app::Session::LoggedOut;
    let (lines, _) = screen(&mut app, 120, 36);
    assert!(!lines.iter().any(|l| l.contains("FAVORITES")), "hidden while logged out");
    // Signed in, before the list arrives: the heading stays and a loading
    // row stands in, rather than rows with a blank where the Pandora /
    // your-own mark goes.
    app.session = crate::app::Session::LoggedIn;
    let (lines, _) = screen(&mut app, 120, 36);
    assert!(lines.iter().any(|l| l.starts_with("│ FAVORITES (1)")), "the heading stays");
    assert!(lines.iter().any(|l| l.contains("Loading favorites")), "a loading row stands in");
    assert!(!lines.iter().any(|l| l.contains(&fav.name)), "no row until the list arrives");
    app.stations = stations;
    app.view = View::Stations;
    app.focus = crate::app::Focus::Content;
    let (lines, _) = screen(&mut app, 120, 36);
    assert!(lines.iter().any(|l| l.starts_with("│ FAVORITES (1)")), "the menu's section");
    let side = |l: &String, menu: bool| -> String {
        if menu { l.chars().take(30).collect() } else { l.chars().skip(30).collect() }
    };
    assert!(lines.iter().any(|l| side(l, true).contains(&fav.name)), "in the menu");
    assert!(
        lines.iter().any(|l| side(l, false).contains("⭐") && l.contains(&fav.name)),
        "starred on the page: {lines:#?}"
    );
    assert!(lines[0].contains("My Stations ("), "the page: {}", lines[0]);
    assert!(lines[35].contains(" f  Favorite"), "key bar: {}", lines[35]);
}

#[test]
fn a_station_icon_sits_between_the_star_and_the_name() {
    use crate::config::IconSet;
    let mut app = demo();
    app.view = View::Stations;
    app.focus = crate::app::Focus::Content;
    let genre = app.stations.iter().find(|s| s.name == "Today's Hits").cloned().unwrap();
    assert!(genre.is_locked() && genre.allow_delete, "a genre station");
    let made = app.stations.iter().find(|s| s.allow_rename && s.allow_delete).cloned().unwrap();
    app.config.ui.favorites.push(crate::config::Favorite { id: made.id.clone(), name: made.name.clone() });
    assert!(!made.is_locked());
    let shuffle = app.stations.iter().find(|s| s.is_shuffle).cloned().unwrap();
    // The logo for a genre station, a person for your own, in each icon set.
    let sets = [
        (IconSet::Emoji, "\u{f1b2}", "👤"),
        (IconSet::Nerd, "\u{f1b2}", "\u{f007}"),
        (IconSet::Unicode, "\u{f1b2}", "☺"),
    ];
    for (set, logo, person) in sets {
        let ic = icons(set);
        assert_eq!((ic.genre_station, ic.own_station), (logo, person));
        assert_eq!(ic.genre_station, crate::icons::LOGO, "the same icon as the logo");
        app.config.ui.icons = set;
        let (lines, _) = screen(&mut app, 120, 36);
        let row = |name: &str| {
            let page = lines.iter().map(|l| l.chars().skip(30).collect::<String>());
            page.into_iter().find(|l| l.contains(name)).unwrap()
        };
        let (g, m) = (row(&genre.name), row(&made.name));
        let right_before = |row: &str, icon: &str, name: &str| {
            let (at, name) = (row.find(icon).unwrap(), row.find(name).unwrap());
            at < name && row[at + icon.len()..name].trim().is_empty()
        };
        assert!(right_before(&g, logo, &genre.name), "{set:?}: {g}");
        assert!(!g.contains(person), "{set:?}: a genre station has no person: {g}");
        assert!(right_before(&m, person, &made.name), "{set:?}: {m}");
        assert!(!m.contains(logo), "{set:?}: your own station has no logo: {m}");
        // After the star, before the name.
        let (star, at, name) = (m.find(ic.favorite).unwrap(), m.find(person).unwrap(), m.find(&made.name).unwrap());
        assert!(star < at && at < name, "{set:?}: star, icon, name: {m}");
        // Shuffle is neither.
        let s = row(&shuffle.name);
        assert!(!s.contains(logo) && !s.contains(person), "{set:?}: {s}");
    }
    // Favorites in the menu too, once the station is loaded.
    app.config.ui.icons = IconSet::Unicode;
    app.config.ui.favorites.push(crate::config::Favorite { id: genre.id.clone(), name: genre.name.clone() });
    let (lines, _) = screen(&mut app, 120, 36);
    let menu: Vec<String> = lines.iter().map(|l| l.chars().take(30).collect()).collect();
    assert!(menu.iter().any(|l| l.contains("\u{f1b2}") && l.contains("Today's Hits")), "{menu:#?}");
    // A narrow list cuts the name, never the icon.
    let (lines, _) = screen(&mut app, 52, 36);
    let row = lines.iter().rfind(|l| l.contains("Today")).unwrap();
    assert!(row.find("\u{f1b2}").unwrap() < row.find("Today").unwrap(), "{row}");
}

#[test]
fn the_browse_genres_page_shows_neither_station_icon() {
    use crate::config::IconSet;
    let mut app = demo();
    app.show_browse_section("Browse Genres");
    app.settle(std::time::Duration::from_millis(300));
    for set in [IconSet::Emoji, IconSet::Nerd, IconSet::Unicode] {
        app.config.ui.icons = set;
        let ic = icons(set);
        let (lines, _) = screen(&mut app, 120, 36);
        let page: String = lines.iter().map(|l| l.chars().skip(30).collect::<String>()).collect::<Vec<_>>().join("\n");
        assert!(page.contains("Browse Genres") || page.contains("Synthwave"), "{page}");
        assert!(!page.contains(ic.genre_station) && !page.contains(ic.own_station), "{set:?}: {page}");
    }
}

#[test]
fn a_blank_column_separates_the_menu_from_the_page() {
    let mut app = demo();
    let (lines, _) = screen(&mut app, 120, 36);
    let top: Vec<char> = lines[0].chars().collect();
    let end = top.iter().position(|c| *c == '┐').unwrap();
    assert_eq!(end + 1, usize::from(menu_width(&app, 120)), "the menu keeps its width: {}", lines[0]);
    assert_eq!(top[end + 1], ' ', "a blank column between the frames: {}", lines[0]);
    assert_eq!(top[end + 2], '┌', "the page starts after it: {}", lines[0]);
}

#[test]
fn up_next_and_history_sit_side_by_side() {
    let mut app = demo();
    let (lines, _) = screen(&mut app, 120, 36);
    let row = lines.iter().find(|l| l.contains("Up Next (")).expect("Up Next pane");
    let (next, history) = (row.find("Up Next").unwrap(), row.find("History (").expect("on the same row"));
    assert!(next < history, "{row}");
}

#[test]
fn menu_text_is_left_justified_like_tubevibe() {
    let mut app = demo();
    app.config.ui.icons = crate::config::IconSet::Unicode;
    let (lines, _) = screen(&mut app, 120, 36);
    for word in ["NOW PLAYING", "My Stations", "Settings", "About", "Quit"] {
        let line = lines.iter().find(|l| l.chars().take(30).collect::<String>().contains(word)).expect(word);
        let menu: Vec<char> = line.chars().take(30).collect();
        assert_eq!(menu[1], ' ', "{word}: one space after the border: {line}");
        assert_ne!(menu[2], ' ', "{word}: the icon at column 2: {line}");
    }
}

#[test]
fn up_next_and_history_stack_when_narrow() {
    let mut app = demo();
    assert!(stack_songs(STACK_BELOW - 1) && !stack_songs(STACK_BELOW));
    let (lines, _) = screen(&mut app, 100, 40);
    let next = lines.iter().position(|l| l.contains("Up Next (")).expect("Up Next");
    let history = lines.iter().position(|l| l.contains("History (")).expect("History");
    assert!(history > next, "History under Up Next on a narrow window");
    assert!(!lines[next].contains("History ("), "{}", lines[next]);
}

#[test]
fn long_song_text_is_cut_and_the_rating_stays_visible() {
    let mut app = demo();
    app.config.ui.icons = crate::config::IconSet::Unicode;
    let long = "A Very Long Song Title 🎵🎶 That Goes On ❤️ And On\tWith\nControl Characters Forever".to_owned();
    for song in app.history.iter_mut().chain(app.queue.iter_mut()) {
        song.title = long.clone();
        song.artist = "An Artist Whose Name 名前 Is Also Far Too Long For Any Pane".into();
    }
    app.history[0].rating = Rating::Up;
    for cols in [80u16, 100, 120, 160] {
        let (lines, _) = screen(&mut app, cols, 40);
        let rated = lines.iter().any(|l| l.contains("liked"));
        assert!(rated, "{cols} cols: the rating shows: {lines:#?}");
        assert!(lines.iter().any(|l| l.contains('…')), "{cols} cols: cut with an ellipsis");
        let row = lines.iter().find(|l| l.contains("liked")).unwrap();
        let end = row.trim_end().chars().last();
        assert!(matches!(end, Some('│')), "{cols} cols: the pane's border closes the row: {row}");
        assert!(!row.contains('\u{FE0F}') && !row.contains('\t'), "{cols} cols: cleaned: {row:?}");
    }
}

#[test]
fn the_account_sits_at_the_right_of_the_status_bar() {
    let mut app = demo();
    app.config.ui.icons = crate::config::IconSet::Unicode;
    app.config.pandora.username = "toby@example.com".into();
    app.status = "hello status".into();
    app.session = Session::LoggedIn;
    let (lines, buffer) = screen(&mut app, 120, 36);
    let bar = &lines[34];
    assert!(bar.starts_with(" hello status"), "{bar}");
    assert!(bar.trim_end().ends_with("Logged In As: toby@example.com"), "{bar}");
    assert!(bar.ends_with("Logged In As: toby@example.com "), "one space before the edge: {bar:?}");
    let x = (bar.chars().count() - "Logged In As: toby@example.com ".len()) as u16;
    assert_eq!(buffer[(x, 34)].fg, app.palette.blue, "tubevibe's account colour");
    // Narrow, with a long status: both cut, never overlapping.
    app.status = "a very long status message that goes on and on and on".into();
    let (lines, _) = screen(&mut app, 40, 36);
    let bar = &lines[34];
    assert!(bar.contains('…') && bar.contains('@'), "{bar}");
    assert!(bar.trim_end().ends_with("toby@example.com"), "narrow: just the email, whole: {bar}");
    assert!(!bar.contains("Logged In As"), "{bar}");
    assert!(bar.chars().count() == 40);
    let status_end = bar.find("on").unwrap();
    assert!(status_end < bar.find('@').unwrap());
    // Logged out: nothing on the right.
    app.session = Session::LoggedOut;
    let (lines, _) = screen(&mut app, 120, 36);
    assert!(!lines[34].contains("toby@"), "{}", lines[34]);
}

#[test]
fn the_status_bar_shows_the_look_and_the_sleep_timer_left_of_the_account_and_drops_the_look_first() {
    let mut app = demo();
    app.config.ui.icons = crate::config::IconSet::Unicode;
    app.config.pandora.username = "toby@example.com".into();
    app.config.ui.set_theme("gruvbox");
    app.config.ui.album_art = AlbumArt::Picture;
    app.config.ui.retro_mode = crate::config::RetroMode::Bit8;
    app.config.ui.art_tint = crate::config::ArtTint::Duotone;
    app.session = Session::LoggedIn;
    app.status = "hi".into();
    let bar = screen(&mut app, 200, 36).0[34].clone();
    assert!(
        bar.trim_end()
            .ends_with("Gruvbox · Art Picture · 8-bit · Tint Duotone · MP3 192 kbps · Logged In As: toby@example.com"),
        "{bar}"
    );
    // The look is in the account's colour; the separators stay muted.
    let (lines, buffer) = screen(&mut app, 200, 36);
    let col = |s: &str| lines[34].chars().take(lines[34].find(s).unwrap()).count() as u16;
    for label in ["Gruvbox", "Art Picture", "8-bit", "Tint Duotone", "MP3 192 kbps", "Logged In As"] {
        assert_eq!(buffer[(col(label), 34)].fg, app.palette.blue, "{label}");
    }
    assert_eq!(buffer[(col("Gruvbox") + 8, 34)].fg, app.palette.muted);
    app.sleep = Some(crate::app::Sleep::Minutes(15, std::time::Instant::now()));
    let bar = screen(&mut app, 200, 36).0[34].clone();
    assert!(bar.contains("☾ 15:00 · Gruvbox") || bar.contains("☾ 14:59 · Gruvbox"), "{bar}");
    // Narrower: the tint goes first, then the rest of the look; the timer stays.
    let bar = screen(&mut app, 150, 36).0[34].clone();
    assert!(bar.contains("☾ 1") && bar.contains("Logged In As") && !bar.contains("Tint"), "{bar}");
    let bar = screen(&mut app, 96, 36).0[34].clone();
    assert!(bar.contains("☾ 1") && !bar.contains("Gruvbox") && bar.contains("toby@example.com"), "{bar}");
    app.sleep = Some(crate::app::Sleep::EndOfSong);
    assert!(screen(&mut app, 200, 36).0[34].contains("☾ end of song"));
}

#[test]
fn a_free_plans_quality_row_is_greyed_out_like_a_greyed_theme_select() {
    let mut app = demo();
    app.config.ui.icons = crate::config::IconSet::Unicode;
    app.view = View::Settings;
    app.focus = Focus::Content;
    // Off the row: a selected row is drawn in the selection colours.
    app.settings_cursor = Setting::at(Setting::Volume);
    let row = |app: &mut App| {
        let (lines, buffer) = screen(app, 120, 40);
        let y = lines.iter().position(|l| l.contains("Audio quality")).expect("the row") as u16;
        // Find the label's column by its cells: the icon before it is
        // double-width, so a char index would land in the wrong place.
        let x = (0..120u16)
            .find(|&x| (0..13).map(|i| buffer[(x + i, y)].symbol()).collect::<String>() == "Audio quality")
            .expect("the label");
        (lines[y as usize].clone(), buffer[(x, y)].fg)
    };
    // Plus / Premium: the label is the normal text colour.
    app.hifi = Some(true);
    let (line, fg) = row(&mut app);
    assert_eq!(fg, app.palette.fg, "{line}");
    // A free plan: greyed, like Theme Select under Auto Mode.
    app.hifi = Some(false);
    let (line, fg) = row(&mut app);
    assert_eq!(fg, app.palette.muted, "{line}");
    assert!(line.contains("your plan: 64 kbps HE-AAC"), "{line}");
    // Not logged in: the plan is unknown, so it stays usable.
    app.hifi = None;
    assert_eq!(row(&mut app).1, app.palette.fg);
}

#[test]
fn the_status_bar_names_the_stream_left_of_the_account_and_follows_the_song() {
    let mut app = demo();
    app.config.ui.icons = crate::config::IconSet::Unicode;
    app.config.pandora.username = "toby@example.com".into();
    app.session = Session::LoggedIn;
    app.status = "hi".into();
    // The demo songs are 192 kbps MP3.
    let bar = screen(&mut app, 200, 36).0[34].clone();
    assert!(bar.trim_end().ends_with("MP3 192 kbps · Logged In As: toby@example.com"), "{bar}");
    // A free plan's HE-AAC: the label follows the song, not the setting.
    app.stream = Some((crate::pandora::AudioFormat::Aac, 64));
    let bar = screen(&mut app, 200, 36).0[34].clone();
    assert!(bar.trim_end().ends_with("AAC 64 kbps · Logged In As: toby@example.com"), "{bar}");
    // A stream that did not say its bitrate names the format alone.
    app.stream = Some((crate::pandora::AudioFormat::Mp3, 0));
    assert!(screen(&mut app, 200, 36).0[34].contains("· MP3 · Logged In As"));
    // Nothing known yet, or an unknown format: no label at all.
    app.stream = Some((crate::pandora::AudioFormat::Unknown, 128));
    assert!(!screen(&mut app, 200, 36).0[34].contains("kbps"));
    app.stream = None;
    assert!(!screen(&mut app, 200, 36).0[34].contains("kbps"));
    // Narrow: the look goes before the stream, which goes before the timer.
    app.stream = Some((crate::pandora::AudioFormat::Aac, 64));
    app.sleep = Some(crate::app::Sleep::Minutes(15, std::time::Instant::now()));
    let bar = screen(&mut app, 150, 36).0[34].clone();
    assert!(bar.contains("AAC 64 kbps") && bar.contains("☾ 1") && !bar.contains("Tint"), "{bar}");
    let bar = screen(&mut app, 96, 36).0[34].clone();
    assert!(bar.contains("☾ 1") && bar.contains("toby@example.com") && !bar.contains("kbps"), "{bar}");
}

#[test]
fn following_omarchy_the_status_bar_names_its_theme_in_the_account_colour() {
    let mut app = demo();
    app.config.pandora.username = "toby@example.com".into();
    app.session = Session::LoggedIn;
    app.config.ui.set_theme("omarchy");
    app.omarchy_theme = Some("Tokyo Night".into());
    let (lines, buffer) = screen(&mut app, 200, 36);
    let bar = &lines[34];
    assert!(bar.contains("omarchy: Tokyo Night · Art"), "{bar}");
    let x = bar.chars().take(bar.find("omarchy: Tokyo").unwrap()).count() as u16;
    assert_eq!((buffer[(x, 34)].fg, buffer[(x + 12, 34)].fg), (app.palette.blue, app.palette.blue));
    // Its name not known yet: just "Omarchy"; a picked theme: its own name.
    app.omarchy_theme = None;
    assert!(screen(&mut app, 200, 36).0[34].contains("Omarchy · Art"));
    app.config.ui.set_theme("gruvbox");
    app.omarchy_theme = Some("Tokyo Night".into());
    let bar = screen(&mut app, 200, 36).0[34].clone();
    assert!(bar.contains("Gruvbox · Art") && !bar.contains("omarchy:"), "{bar}");
}

#[test]
fn up_next_shows_each_songs_thumbs_and_updates_after_l() {
    let mut app = demo();
    app.config.ui.icons = crate::config::IconSet::Unicode;
    app.focus = crate::app::Focus::Content;
    app.view = View::Player;
    app.pane = Pane::UpNext;
    let up_next = |app: &mut App| {
        let (lines, _) = screen(app, 160, 40);
        let y0 = lines.iter().position(|l| l.contains("Up Next (")).expect("Up Next");
        let x0 = lines[y0].chars().position(|c| c == 'U').unwrap();
        let rows: Vec<String> = lines[y0 + 1..y0 + 4].iter().map(|l| l.chars().skip(x0).take(70).collect()).collect();
        rows
    };
    let before = up_next(&mut app);
    assert!(before[0].contains("TIME") && before[0].contains("RATED"), "{before:?}");
    assert!(before[1].contains('—') && before[1].contains(':'), "not rated, time kept: {before:?}");
    app.on_key(crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('l')));
    assert_eq!(app.queue[app.next_index()].rating, Rating::Up);
    let after = up_next(&mut app);
    assert!(after.iter().any(|r| r.contains("liked")), "{after:?}");
}

#[test]
fn up_next_keeps_a_blank_cell_before_its_right_border() {
    let mut app = demo();
    app.config.ui.icons = crate::config::IconSet::Unicode;
    app.focus = crate::app::Focus::Content;
    app.view = View::Player;
    app.pane = Pane::UpNext;
    // Side by side (wide) and stacked (narrow).
    for cols in [80u16, 100, 120, 140, 160, 200] {
        let (lines, buffer) = screen(&mut app, cols, 40);
        let y0 = lines.iter().position(|l| l.contains("Up Next (")).expect("Up Next");
        let row: Vec<char> = lines[y0].chars().collect();
        let start = lines[y0].chars().position(|c| c == 'U').unwrap();
        let right = (start..row.len()).find(|&x| row[x] == '┐').expect("right corner") as u16;
        let mut y = y0 as u16 + 1;
        let mut checked = 0;
        while buffer[(right, y)].symbol() != "┘" {
            let cell = &buffer[(right - 1, y)];
            assert_eq!(cell.symbol(), " ", "{cols} cols row {y}: {}", lines[y as usize]);
            assert_eq!(cell.bg, buffer[(right, y)].bg, "{cols} cols row {y}: highlight touches the border");
            y += 1;
            checked += 1;
        }
        assert!(checked >= 3, "{cols} cols: rows checked");
    }
}

#[test]
fn clean_and_truncate_keep_cells_honest() {
    assert_eq!(clean("a\tb\nc ❤️ d 🎵"), "a b c  d 🎵");
    assert_eq!(truncate("abcdef", 4), "ab…");
    assert_eq!(truncate("名前名前", 6), "名前…");
    assert!(truncate("名前名前", 5).width() <= 5);
}

#[test]
fn now_playing_art_fills_the_pane_with_info_time_and_volume_beside_it() {
    let mut app = demo();
    app.config.ui.icons = crate::config::IconSet::Unicode;
    let (lines, _) = screen(&mut app, 120, 36);
    let rows = usize::from(now_playing_height(34, false));
    let menu = |l: &String| l.chars().skip(30).collect::<String>();
    let art = app.art.last_area;
    assert_eq!(
        (art.y, art.y + art.height),
        (2, rows as u16 - 2),
        "art fills the pane's height inside one row of padding"
    );
    assert!(!lines[..rows].iter().any(|l| l.contains('╰') && menu(l).chars().nth(2) == Some('╰')), "no frame");
    let title = app.now.as_ref().unwrap().title.clone();
    let time = lines.iter().position(|l| l.contains('●')).expect("time line");
    let volume = lines.iter().position(|l| l.contains('%') && l.contains('▮')).expect("volume");
    assert!(time < rows && volume == time + 1, "both inside Now Playing, beside the art");
    let picture_right = lines[..rows]
        .iter()
        .filter_map(|l| l.chars().enumerate().filter(|(_, c)| *c == '▀' || *c == '▄').map(|(i, _)| i).last())
        .max()
        .expect("picture");
    assert!(lines[time].chars().position(|c| c == '●').unwrap() > picture_right, "beside the picture");
    // The stack is centred: as many blank rows above it as below.
    let first = lines.iter().position(|l| menu(l).contains(&title)).unwrap();
    let last = lines[..rows].iter().rposition(|l| l.contains('╰')).expect("buttons");
    let (above, below) = (first - 1, rows - 2 - last);
    assert!(above.abs_diff(below) <= 1, "centred: {above} above, {below} below");
    for key in ["-/d", "+/u", "space"] {
        assert!(!lines[..rows].iter().any(|l| l.contains(key)), "no {key} under the buttons");
    }
}

/// Every row of Up Next and History ends in its own column: the rating
/// cell starts with the expected icon, and no text crosses a border.
#[test]
fn song_text_never_reaches_the_rating_column_at_any_width() {
    let titles = [
        "Ünïcödé Àccénts Évérywhere Ça Va Très Bien Merci Beaucoup",
        "東京の夜 名前 世界 音楽 歌 東京の夜 名前 世界 音楽 歌",
        "❤️ Love ☀ Sun ★ Star ♪ Note 🎵🎶 👍👎 📻 \u{f164} Emoji ‍👩‍👩‍👧 Family\u{200B}Zero\tTab\nLine",
    ];
    for set in [crate::config::IconSet::Emoji, crate::config::IconSet::Nerd, crate::config::IconSet::Unicode] {
        for (k, title) in titles.iter().enumerate() {
            let mut app = demo();
            app.config.ui.icons = set;
            let ic = crate::icons::icons(set);
            for song in app.history.iter_mut().chain(app.queue.iter_mut()) {
                song.title = (*title).into();
                song.artist = titles[(k + 1) % 3].into();
                song.rating = Rating::Up;
            }
            for cols in [60u16, 80, 89, 90, 100, 120, 160, 200] {
                let (_, buf) = screen(&mut app, cols, 44);
                let mut found = 0;
                for y in 0..44 {
                    let row: Vec<&str> = (0..cols).map(|x| buf[(x, y)].symbol()).collect();
                    for (x, sym) in row.iter().enumerate() {
                        if *sym != "liked"
                            && sym.starts_with('l')
                            && row[x..].iter().take(5).copied().collect::<String>() == "liked"
                        {
                            found += 1;
                            let icon_start = (0..x).rev().find(|&j| !row[j].is_empty() && row[j] != " ").unwrap();
                            assert_eq!(row[icon_start], ic.up, "{cols} cols {set:?}: {}", row.concat());
                            // Two cells of gap before the rating column hold only spaces.
                            let border = row[x + 5..].iter().position(|s| *s == "│").expect("closing border");
                            assert!(row[x + 5..x + 5 + border].iter().all(|s| *s == " " || s.is_empty()));
                        }
                    }
                    // No song text ever paints a border cell.
                    let line = row.concat();
                    for bad in ["东", "東", "Ü", "Love", "Star"] {
                        if let Some(pos) = line.find(bad) {
                            let before = &line[..pos];
                            assert!(before.matches('│').count() % 2 == 1, "{cols} cols: text outside a pane: {line}");
                        }
                    }
                }
                assert!(found > 0, "{cols} cols {set:?}: a rating shows");
            }
        }
    }
}

#[test]
fn fit_is_conservative_about_risky_widths() {
    for (text, cells) in [("❤ ★ ☀ \u{f164} 🎵 名前 é", 6usize), ("abcdef", 4), ("名前名前", 5), ("🎵🎵🎵", 4)]
    {
        let out = fit(text, cells);
        assert!(safe_width(&out) < cells || cells == 0, "{text:?} -> {out:?}");
        assert!(out.width() < cells, "{text:?} -> {out:?}");
    }
    assert_eq!(clean("a\u{200B}b\u{FE0F}c\u{200D}d\te\u{f164}f❤g"), "abcd efg");
    assert_eq!(safe_char_width('★'), 2);
    assert_eq!(safe_char_width('\u{f164}'), 2);
    assert_eq!(safe_char_width('a'), 1);
    assert_eq!(icon_cell("\u{f164}").width(), 3);
    assert_eq!(icon_cell("👍").width(), 2);
}

fn all_text(lines: &[String]) -> String {
    lines.join("\n")
}

#[test]
fn a_failed_search_in_the_seed_box_says_so_in_plain_words() {
    use crate::backend::Reply;
    use ratatui::crossterm::event::{KeyCode, KeyEvent};
    assert_eq!(
        search_error_text("Service method not found (HTTP 404, NOT_FOUND)"),
        "Couldn't search Pandora right now"
    );
    assert_eq!(search_error_text("offline"), "Search failed: offline");
    let mut app = editor_demo();
    app.on_key(KeyEvent::from(KeyCode::Char('a')));
    // Pandora's raw answer to the old suggestions path, then a search that failed the same way.
    app.on_reply(Reply::Failed { what: Failed::Suggestions, message: "Service method not found (HTTP 404)".into() });
    let text = all_text(&screen(&mut app, 120, 40).0);
    assert!(
        !text.contains("Service method") && !text.contains("no suggestions") && !text.contains("Search failed"),
        "{text}"
    );
    app.on_reply(Reply::Failed { what: Failed::Search, message: "Service method not found (HTTP 404)".into() });
    let text = all_text(&screen(&mut app, 120, 40).0);
    assert!(text.contains("Couldn't search Pandora right now") && !text.contains("Service method"), "{text}");
}

#[test]
fn the_add_station_box_shows_the_text_the_results_and_their_kind() {
    use crate::app::Focus;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(400));
    app.on_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT));
    assert_eq!(app.focus, Focus::Content);
    let (lines, _) = screen(&mut app, 120, 36);
    assert!(lines.iter().any(|l| l.contains("Add station…")), "the first row of the list");
    let caps = key_caps(&app);
    assert!(caps.iter().any(|(k, w)| *k == "a" && *w == "Add Station"), "{caps:?}");
    app.on_key(KeyEvent::from(KeyCode::Char('a')));
    for c in "m83".chars() {
        app.on_key(KeyEvent::from(KeyCode::Char(c)));
    }
    let caps = key_caps(&app);
    assert!(caps.iter().any(|(k, _)| *k == "Enter") && caps.iter().any(|(k, _)| *k == "Esc"), "{caps:?}");
    app.on_key(KeyEvent::from(KeyCode::Enter));
    app.settle(std::time::Duration::from_millis(300));
    let (lines, _) = screen(&mut app, 120, 36);
    let text = all_text(&lines);
    assert!(text.contains("Add station") && text.contains("m83"), "{text}");
    assert!(text.contains("M83") && text.contains("Midnight City"), "artist and song: {text}");
    assert!(text.contains("Artist") && text.contains("Song"), "with their kind: {text}");
    let caps = key_caps(&app);
    assert!(caps.iter().any(|(k, _)| k.contains("j/k")), "results can be moved: {caps:?}");
    assert!(!caps.iter().any(|(k, _)| *k == "a" || *k == "f"), "other keys are not live: {caps:?}");
    // Narrow and short windows do not panic.
    for (c, r) in [(40, 10), (20, 6), (120, 8)] {
        let _ = screen(&mut app, c, r);
    }
}

#[test]
fn the_browse_genres_page_lists_items_and_pins_on_the_menu() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(400));
    app.on_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT));
    app.settle(std::time::Duration::from_millis(300));
    let (lines, _) = screen(&mut app, 120, 36);
    let text = all_text(&lines);
    assert!(text.contains("Browse Genres") && text.contains("Synthwave"), "{text}");
    assert!(text.contains("1.2M Listeners"), "detail: {text}");
    let caps = key_caps(&app);
    assert!(caps.iter().any(|(k, w)| *k == "Enter" && *w == "Start Station"), "{caps:?}");
    assert!(caps.iter().any(|(k, _)| k.contains("j/k")), "{caps:?}");
    // Pin Browse Genres on the main menu.
    app.config.browse.set_pinned("Browse Genres", true);
    let genre_icon = crate::icons::icons(app.config.ui.icons).genre;
    let menu: Vec<String> = screen(&mut app, 120, 36).0.iter().map(|l| l.chars().take(30).collect()).collect();
    assert!(
        menu.iter().any(|l| l.contains("Browse Genres") && l.contains(genre_icon)),
        "pinned in the menu with its icon: {menu:#?}"
    );
    // Loading / failed use the Browse directory state.
    app.browse = crate::app::BrowseState::Loading;
    let (lines, _) = screen(&mut app, 120, 36);
    assert!(all_text(&lines).contains("Loading"), "{}", all_text(&lines));
    assert!(!key_caps(&app).iter().any(|(k, _)| k.contains("j/k")));
    app.browse = crate::app::BrowseState::Failed("offline".into());
    assert!(all_text(&screen(&mut app, 120, 36).0).contains("offline"));
    assert!(key_caps(&app).iter().any(|(_, w)| *w == "Try Again"));
    let _ = screen(&mut app, 30, 8);
}

#[test]
fn browse_sections_sit_under_a_browse_heading() {
    let mut app = demo();
    let (lines, _) = screen(&mut app, 120, 40);
    let menu = |l: &str| -> String { l.chars().take(30).collect() };
    let at = |want: &dyn Fn(&str) -> bool| lines.iter().map(|l| menu(l)).position(|l| want(&l));
    let head = at(&|l| l.starts_with("│ BROWSE (10)")).expect("the BROWSE heading, with the pin count");
    let pinned = at(&|l| l.contains("Pinned")).expect("Pinned");
    let first = at(&|l| l.contains("Artists Picked For You")).expect("the first section");
    assert!(
        pinned < head && head < first,
        "Pinned, then BROWSE (n), then the sections:\n{}",
        lines.iter().map(|l| menu(l)).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn my_collection_heads_the_personal_rows_and_hides_when_empty() {
    let mut app = demo();
    let (lines, _) = screen(&mut app, 120, 40);
    let menu = |l: &str| -> String { l.chars().take(30).collect() };
    let at = |want: &dyn Fn(&str) -> bool| lines.iter().map(|l| menu(l)).position(|l| want(&l));
    let head = at(&|l| l.starts_with("│ MY COLLECTION")).expect("the MY COLLECTION heading");
    let stations = at(&|l| l.contains("My Stations")).expect("My Stations");
    let pinned = at(&|l| l.contains("Pinned")).expect("Pinned");
    let browse = at(&|l| l.starts_with("│ BROWSE (10)")).expect("the BROWSE heading");
    let first = at(&|l| l.contains("Artists Picked For You")).expect("the first section");
    assert!(
        head < stations && stations < pinned && pinned < browse && browse < first,
        "MY COLLECTION, My Stations, Pinned, then BROWSE and its sections:\n{}",
        lines.iter().map(|l| menu(l)).collect::<Vec<_>>().join("\n")
    );
    // An empty collection draws no heading, the way an empty FAVORITES does.
    let mut empty = App::new(Config::default(), None, &[], true);
    empty.config.ui.icons = crate::config::IconSet::Emoji;
    let (lines, _) = screen(&mut empty, 120, 40);
    assert!(
        !lines.iter().any(|l| l.contains("MY COLLECTION")),
        "hidden while empty:\n{}",
        lines.iter().map(|l| menu(l)).collect::<Vec<_>>().join("\n")
    );
    assert!(lines.iter().any(|l| l.contains("My Stations")), "the row itself stays");
    // Short and narrow windows still draw it without panicking.
    for (c, r) in [(40, 10), (20, 6), (120, 8)] {
        let _ = screen(&mut app, c, r);
    }
}

#[test]
fn recently_played_sits_above_pinned_and_hides_while_empty() {
    let mut app = demo();
    let (lines, _) = screen(&mut app, 120, 40);
    let menu = |l: &str| -> String { l.chars().take(30).collect() };
    let at = |want: &dyn Fn(&str) -> bool| lines.iter().map(|l| menu(l)).position(|l| want(&l));
    let stations = at(&|l| l.contains("My Stations")).expect("My Stations");
    let pinned = at(&|l| l.contains("Pinned")).expect("Pinned");
    let recent = at(&|l| l.contains("Recently Played (3)")).expect("the recently played row, with its count");
    let browse = at(&|l| l.starts_with("│ BROWSE (10)")).expect("the BROWSE heading");
    let first = at(&|l| l.contains("Artists Picked For You")).expect("the first section");
    assert!(
        stations < recent && recent < pinned && pinned < browse && browse < first,
        "My Stations, Recently Played, Pinned, then BROWSE:\n{}",
        lines.iter().map(|l| menu(l)).collect::<Vec<_>>().join("\n")
    );
    // Short and narrow windows still draw it without panicking.
    for (c, r) in [(40, 10), (20, 6), (120, 8)] {
        let _ = screen(&mut app, c, r);
    }
}

#[test]
fn recently_played_keeps_the_collection_heading_when_stations_are_gone() {
    let mut app = App::new(crate::config::Config::default(), None, &[], true);
    app.config.ui.icons = crate::config::IconSet::Emoji;
    app.config.ui.pinned_in_menu = false;
    app.stations.clear();
    app.recent = vec![crate::pandora::RecentSource { id: "ST:0:1".into(), kind: "ST".into(), ..Default::default() }];
    let text = all_text(&screen(&mut app, 120, 40).0);
    assert!(text.contains("MY COLLECTION"), "{text}");
    assert!(text.contains("Recently Played (1)"), "{text}");
}

#[test]
fn an_empty_history_draws_no_recently_played_state() {
    let mut fresh = App::new(crate::config::Config::default(), None, &[], true);
    fresh.config.ui.icons = crate::config::IconSet::Emoji;
    let (lines, _) = screen(&mut fresh, 120, 40);
    assert!(!lines.iter().any(|l| l.contains("Recently Played")), "hidden while empty:\n{}", lines.join("\n"));
}

#[test]
fn the_recently_played_page_lists_its_rows_and_offers_play() {
    let mut app = demo();
    app.view = View::Recent;
    app.focus = crate::app::Focus::Content;
    app.recent_cursor = 0;
    let (lines, _) = screen(&mut app, 120, 40);
    assert!(lines[0].contains("Recently Played (3)"), "the page title: {}", lines[0]);
    let all = lines.join("\n");
    for name in app.recent.iter().map(|s| app.recent_title(s)) {
        assert!(all.contains(name.as_str()), "{name} missing from:\n{all}");
    }
    let caps = key_caps(&app);
    assert!(caps.iter().any(|(k, w)| *k == "Enter" && *w == "Play"), "{caps:?}");
}

#[test]
fn the_recently_played_page_says_loading_and_nothing_played() {
    let mut blank = App::new(crate::config::Config::default(), None, &[], true);
    blank.config.ui.icons = crate::config::IconSet::Emoji;
    blank.view = View::Recent;
    blank.focus = crate::app::Focus::Content;
    let text = all_text(&screen(&mut blank, 120, 40).0);
    assert!(text.contains("Nothing played"), "{text}");
    blank.recent_state.begin().expect("a request goes out");
    let text = all_text(&screen(&mut blank, 120, 40).0);
    assert!(text.contains("Loading"), "{text}");
    // One row: no "move" hint, and no Enter: there is nothing else to play.
    let caps = key_caps(&blank);
    assert!(!caps.iter().any(|(k, _)| k.contains("j/k")), "{caps:?}");
}

#[test]
fn only_the_active_browse_pin_is_shaded_in_the_menu() {
    let mut app = demo();
    app.settle(std::time::Duration::from_millis(400));
    for title in ["Artists Picked For You", "New Music", "Browse Genres", "Decades"] {
        app.config.browse.set_pinned(title, true);
    }
    app.show_browse_section("Artists Picked For You");
    app.settle(std::time::Duration::from_millis(300));
    assert_eq!(app.focus, Focus::Content);
    assert_eq!(app.browse_filter.as_deref(), Some("Artists Picked For You"));
    let highlight = app.palette.highlight;
    let (lines, buf) = screen(&mut app, 120, 40);
    let menu_w = 28u16;
    let labels = ["Artists Picked For You", "New Music", "Browse Genres", "Decades", "My Stations", "Pinned"];
    let mut shaded = Vec::new();
    for (y, line) in lines.iter().enumerate() {
        let left: String = line.chars().take(menu_w as usize).collect();
        let Some(label) = labels.iter().find(|l| left.contains(*l)) else { continue };
        let has_hl = (0..menu_w).any(|x| buf[(x, y as u16)].bg == highlight);
        if has_hl {
            shaded.push((*label).to_owned());
        }
    }
    assert_eq!(
        shaded,
        vec!["Artists Picked For You".to_owned()],
        "only the open pin should be shaded when Content has focus: {shaded:?}\n{}",
        lines.iter().map(|l| l.chars().take(40).collect::<String>()).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn every_icon_set_has_a_genre_and_an_animations_icon() {
    use crate::config::IconSet;
    for set in [IconSet::Nerd, IconSet::Emoji, IconSet::Unicode] {
        let icons = crate::icons::icons(set);
        assert!(!icons.genre.is_empty(), "{set:?}");
        assert!(!icons.setting[crate::app::Setting::at(crate::app::Setting::Animations)].is_empty(), "{set:?}");
    }
}

#[test]
fn the_system_check_lists_what_is_missing_and_that_is_all() {
    use crate::deps::{Facts, checks};
    let mut app = App::new(Config::default(), None, &[], true);
    app.system_checked(&Facts {
        families: Some("DejaVu Sans\n".into()),
        glyph_fonts: None,
        notify_send: false,
        keychain: Some(false),
    });
    assert!(app.system_open());
    let all = screen(&mut app, 110, 30).0.join("\n");
    for want in [
        "System Check",
        "✘ Nerd Font symbols",
        "missing · needed",
        "package: ttf-nerd-fonts-symbols (Arch) or any Nerd Font",
        "notify-send",
        "not installed · optional",
        "package: libnotify (Debian: libnotify-bin)",
        "Secret Service keychain",
        "package: a Secret Service keychain, e.g. gnome-keyring or KeePassXC",
        "only sees fonts installed on this computer",
        "Enter checks again · Esc closes",
    ] {
        assert!(all.contains(want), "{want:?} is missing from:\n{all}");
    }
    // Nothing to answer, nothing to install from here.
    for gone in ["Install", "y/n", "pacman", "Don't ask"] {
        assert!(!all.contains(gone), "{gone:?} should not be there:\n{all}");
    }
    assert_eq!(key_caps(&app), [("Enter", "Check Again"), ("Esc", "Close")]);
    // The app is behind it, as with Help.
    assert!(
        all.contains("STATIONS")
            || all.contains("NOW PLAYING")
            || all.contains("My Stations")
            || all.contains("Stations"),
        "{all}"
    );
    // What is there gets a tick and no package line.
    app.system = Some(checks(
        true,
        &Facts { families: Some("X Nerd Font\n".into()), glyph_fonts: None, notify_send: true, keychain: Some(true) },
    ));
    app.system_changed();
    let all = screen(&mut app, 110, 30).0.join("\n");
    assert!(all.contains("✔ Nerd Font symbols") && !all.contains("package:"), "{all}");
    // While it looks again.
    app.system = None;
    app.system_changed();
    assert!(screen(&mut app, 110, 30).0.join("\n").contains("Checking…"));
}

#[test]
fn the_system_check_is_plain_text_and_fits_a_small_terminal() {
    use crate::deps::Facts;
    let mut app = App::new(Config::default(), None, &[], true);
    // The app behind it may use the emoji set; the list itself is plain text.
    app.config.ui.icons = crate::config::IconSet::Emoji;
    app.system_checked(&Facts {
        families: Some("DejaVu Sans\n".into()),
        glyph_fonts: None,
        notify_send: false,
        keychain: Some(false),
    });
    // No glyph from the private use area is added by the list: it reads without a Nerd Font.
    let private = |all: &str| all.chars().filter(|c| ('\u{e000}'..='\u{f8ff}').contains(c) || *c > '\u{f0000}').count();
    for (cols, rows) in [(110, 30), (60, 24), (40, 14)] {
        app.dialogs.clear();
        let without = private(&screen(&mut app, cols, rows).0.join(""));
        app.open_system();
        let with = private(&screen(&mut app, cols, rows).0.join(""));
        assert!(with <= without, "{cols}x{rows}");
    }
}
