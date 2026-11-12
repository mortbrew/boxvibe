use super::*;
use crate::config::Config;
use crate::pandora::FEEDBACK_PAGE;
use ratatui::crossterm::event::KeyModifiers;
use std::time::Duration;

const WAIT: Duration = Duration::from_millis(250);

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    app.settle(WAIT);
}

fn chr(app: &mut App, c: char) {
    press(app, KeyCode::Char(c));
}

/// The demo, on the Stations page, with the editor open on the first
/// station that has one (Thumbprint Radio: 2 seeds, 37 thumbs up).
fn editing() -> App {
    let mut app = App::new(Config::default(), None, &[], true);
    app.demo_warm_up(2, Duration::ZERO);
    app.settle(Duration::from_millis(300));
    app.on_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT));
    for _ in 0..8 {
        chr(&mut app, 'e');
        if app.editor.is_some() {
            break;
        }
        chr(&mut app, 'j');
    }
    assert!(app.editor.is_some(), "e opened the editor on a station that allows it");
    app
}

fn seed_names(app: &App) -> Vec<String> {
    app.editor.as_ref().unwrap().seed_list().iter().map(|s| s.name.clone()).collect()
}

#[test]
fn e_opens_the_editor_only_on_stations_that_allow_it() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.demo_warm_up(2, Duration::ZERO);
    app.settle(Duration::from_millis(300));
    app.on_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT));
    // Shuffle and the genre stations: Pandora does not let their seeds be edited.
    for locked in [true, false] {
        for _ in 0..12 {
            app.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        }
        let wanted =
            |app: &App| app.station_in_reach().is_some_and(|(s, _)| s.is_shuffle == locked && !s.allow_edit_seeds);
        for _ in 0..12 {
            if wanted(&app) {
                break;
            }
            app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        }
        assert!(wanted(&app), "a station without seeds to edit exists");
        let name = app.station_in_reach().unwrap().0.name;
        chr(&mut app, 'e');
        if locked {
            assert!(app.editor.is_none(), "{name}");
            assert!(app.status.contains("doesn't let you edit") && app.status.contains(&name), "{}", app.status);
            continue;
        }
        // A genre station opens on its thumbs only: no Seeds tab, no seed keys.
        app.settle(Duration::from_millis(300));
        let ed = app.editor.as_ref().expect("genre stations open on their thumbs");
        assert!(ed.thumbs_only, "{name}");
        assert_eq!(ed.tab, EditorTab::ThumbsUp);
        assert!(app.status.contains("genre station"), "{}", app.status);
        for _ in 0..3 {
            press(&mut app, KeyCode::Tab);
            assert_ne!(app.editor.as_ref().unwrap().tab, EditorTab::Seeds);
            press(&mut app, KeyCode::Left);
            assert_ne!(app.editor.as_ref().unwrap().tab, EditorTab::Seeds);
        }
        chr(&mut app, 'a');
        assert!(app.editor.as_ref().unwrap().add.is_none(), "no seeds to add on a genre station");
        press(&mut app, KeyCode::Esc);
        assert!(app.editor.is_none());
        break;
    }
    let app = editing();
    let ed = app.editor.as_ref().unwrap();
    assert_eq!(ed.tab, EditorTab::Seeds);
    assert_eq!(seed_names(&app), ["M83", "Midnight City by M83"]);
    assert_eq!(ed.thumbs_up, Some(37), "the counts come from the station details");
}

#[test]
fn esc_closes_and_tab_and_arrows_switch_the_three_tabs_round() {
    let mut app = editing();
    let tab = |app: &App| app.editor.as_ref().unwrap().tab;
    // Tab: Seeds → Thumbs Up → Thumbs Down → Seeds.
    press(&mut app, KeyCode::Tab);
    assert_eq!(tab(&app), EditorTab::ThumbsUp);
    press(&mut app, KeyCode::Tab);
    assert_eq!(tab(&app), EditorTab::ThumbsDown);
    press(&mut app, KeyCode::Tab);
    assert_eq!(tab(&app), EditorTab::Seeds);
    // Shift+Tab goes the other way round.
    press(&mut app, KeyCode::BackTab);
    assert_eq!(tab(&app), EditorTab::ThumbsDown);
    press(&mut app, KeyCode::BackTab);
    assert_eq!(tab(&app), EditorTab::ThumbsUp);
    press(&mut app, KeyCode::BackTab);
    assert_eq!(tab(&app), EditorTab::Seeds);
    // → is Tab and ← is Shift+Tab.
    press(&mut app, KeyCode::Right);
    assert_eq!(tab(&app), EditorTab::ThumbsUp);
    press(&mut app, KeyCode::Right);
    assert_eq!(tab(&app), EditorTab::ThumbsDown);
    press(&mut app, KeyCode::Left);
    assert_eq!(tab(&app), EditorTab::ThumbsUp);
    press(&mut app, KeyCode::Left);
    assert_eq!(tab(&app), EditorTab::Seeds);
    // The keys of the page behind it do nothing while it is open.
    chr(&mut app, 'N');
    assert!(app.editor.is_some());
    press(&mut app, KeyCode::Esc);
    assert!(app.editor.is_none());
}

#[test]
fn a_adds_a_suggestion_with_enter_and_the_list_refreshes() {
    let mut app = editing();
    chr(&mut app, 'a');
    let add = app.editor.as_ref().unwrap().add.as_ref().expect("the add box");
    assert!(add.suggested && !add.search.hits.is_empty(), "suggestions show while the box is empty");
    let pick = add.search.hits[0].clone();
    press(&mut app, KeyCode::Enter);
    app.settle(WAIT);
    assert!(app.editor.as_ref().unwrap().add.is_none());
    assert_eq!(seed_names(&app)[0], pick.name, "the new seed is listed from Pandora's answer");
    assert_eq!(seed_names(&app).len(), 3);
    assert!(app.status.contains("Added"), "{}", app.status);
}

#[test]
fn typing_replaces_the_suggestions_with_a_search_and_emptying_brings_them_back() {
    let mut app = editing();
    chr(&mut app, 'a');
    for c in "tame".chars() {
        chr(&mut app, c);
    }
    assert!(!app.editor.as_ref().unwrap().add.as_ref().unwrap().suggested);
    assert!(app.editor.as_ref().unwrap().add.as_ref().unwrap().search.hits.is_empty());
    press(&mut app, KeyCode::Enter);
    let search = &app.editor.as_ref().unwrap().add.as_ref().unwrap().search;
    assert_eq!(search.mode, SearchMode::Results);
    assert_eq!(search.hits[0].name, "Tame Impala");
    press(&mut app, KeyCode::Enter);
    assert_eq!(seed_names(&app)[0], "Tame Impala");
    // Back in a new box, typing and then deleting it all shows them again.
    chr(&mut app, 'a');
    chr(&mut app, 'x');
    assert!(!app.editor.as_ref().unwrap().add.as_ref().unwrap().suggested);
    press(&mut app, KeyCode::Backspace);
    assert!(app.editor.as_ref().unwrap().add.as_ref().unwrap().suggested);
    // Esc closes just the box; the editor stays.
    press(&mut app, KeyCode::Esc);
    assert!(app.editor.as_ref().is_some_and(|e| e.add.is_none()));
}

#[test]
fn a_seed_that_is_already_there_is_not_added_twice() {
    let mut app = editing();
    chr(&mut app, 'a');
    for c in "m83".chars() {
        chr(&mut app, c);
    }
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    assert!(app.status.contains("already a seed"), "{}", app.status);
    assert_eq!(seed_names(&app).len(), 2);
}

#[test]
fn x_asks_what_will_happen_then_removes_and_the_last_seed_stays() {
    let mut app = editing();
    chr(&mut app, 'x');
    let station = app.editor.as_ref().unwrap().station.name.clone();
    let question = app.editor_confirm().expect("asks first").question(&station);
    assert!(question.contains("Remove the artist seed “M83”"), "{question}");
    assert!(question.contains("stops picking songs"), "{question}");
    // n / Esc says no; stray keys leave the question open.
    chr(&mut app, 'x');
    assert!(app.editor_confirm().is_some());
    chr(&mut app, 'n');
    assert!(app.editor_confirm().is_none());
    assert_eq!(seed_names(&app).len(), 2);
    chr(&mut app, 'x');
    chr(&mut app, 'y');
    app.settle(WAIT);
    assert_eq!(seed_names(&app), ["Midnight City by M83"]);
    assert!(app.status.contains("Removed"), "{}", app.status);
    // The only seed left cannot go: no question, a message.
    chr(&mut app, 'x');
    assert!(app.editor_confirm().is_none());
    assert!(app.status.contains("at least one seed"), "{}", app.status);
    assert_eq!(seed_names(&app).len(), 1);
}

#[test]
fn each_thumbs_tab_loads_its_own_list_when_first_opened_and_pages_through_it() {
    let mut app = editing();
    press(&mut app, KeyCode::Tab);
    let ed = app.editor.as_ref().unwrap();
    assert_eq!((ed.up.items.len(), ed.up.total), (FEEDBACK_PAGE, 37));
    assert!(ed.tab == EditorTab::ThumbsUp && ed.down.state == FeedState::Idle, "the other list waits until asked for");
    // Walking down past the end of the loaded page fetches the next one.
    for _ in 0..FEEDBACK_PAGE + 2 {
        chr(&mut app, 'j');
    }
    let ed = app.editor.as_ref().unwrap();
    assert_eq!(ed.up.items.len(), 37, "the rest was appended");
    assert_eq!(ed.up.cursor, FEEDBACK_PAGE + 2);
    // At the very end it stops (no wrap-around while more could come, and nothing to load).
    for _ in 0..10 {
        chr(&mut app, 'j');
    }
    assert_eq!(app.editor.as_ref().unwrap().up.items.len(), 37);
    // The next tab is the thumbs down list, loaded now, not before.
    press(&mut app, KeyCode::Tab);
    app.settle(WAIT);
    let ed = app.editor.as_ref().unwrap();
    assert_eq!(ed.tab, EditorTab::ThumbsDown);
    assert_eq!(ed.down.total, ed.down.items.len());
    assert!(!ed.down.items.is_empty() && ed.down.items.iter().all(|f| !f.positive));
    // Back on the thumbs up tab, the list is kept (not asked for again).
    press(&mut app, KeyCode::BackTab);
    let ed = app.editor.as_ref().unwrap();
    assert_eq!((ed.tab, ed.up.items.len()), (EditorTab::ThumbsUp, 37));
}

#[test]
fn t_no_longer_flips_anything() {
    let mut app = editing();
    chr(&mut app, 't');
    assert_eq!(app.editor.as_ref().unwrap().tab, EditorTab::Seeds);
    press(&mut app, KeyCode::Tab);
    chr(&mut app, 't');
    let ed = app.editor.as_ref().unwrap();
    assert_eq!(ed.tab, EditorTab::ThumbsUp);
    assert_eq!(ed.down.state, FeedState::Idle, "t did not open the thumbs down list");
    press(&mut app, KeyCode::Tab);
    chr(&mut app, 't');
    assert_eq!(app.editor.as_ref().unwrap().tab, EditorTab::ThumbsDown);
}

#[test]
fn x_on_a_thumb_asks_then_deletes_and_refreshes_the_list_and_the_counts() {
    let mut app = editing();
    press(&mut app, KeyCode::Tab);
    chr(&mut app, 'j');
    let gone = app.editor.as_ref().unwrap().up.items[1].clone();
    chr(&mut app, 'x');
    let station = app.editor.as_ref().unwrap().station.name.clone();
    let question = app.editor_confirm().expect("asks first").question(&station);
    assert!(question.contains("Take back your thumbs up") && question.contains(&gone.title), "{question}");
    chr(&mut app, 'n');
    assert_eq!(app.editor.as_ref().unwrap().up.total, 37, "no: nothing changed");
    chr(&mut app, 'x');
    chr(&mut app, 'y');
    app.settle(WAIT);
    let ed = app.editor.as_ref().unwrap();
    assert_eq!(ed.up.total, 36);
    assert!(!ed.up.items.iter().any(|f| f.id == gone.id));
    assert_eq!(ed.thumbs_up, Some(36), "the count in the tab comes from the refreshed details");
    // On the thumbs down tab the question says the song may play again; y takes it back.
    press(&mut app, KeyCode::Tab);
    app.settle(WAIT);
    let before = app.editor.as_ref().unwrap().down.total;
    assert!(before > 0);
    let gone = app.editor.as_ref().unwrap().down.items[0].clone();
    chr(&mut app, 'x');
    let station = app.editor.as_ref().unwrap().station.name.clone();
    let question = app.editor_confirm().unwrap().question(&station);
    assert!(question.contains("thumbs down") && question.contains("can play again"), "{question}");
    chr(&mut app, 'y');
    app.settle(WAIT);
    let ed = app.editor.as_ref().unwrap();
    assert_eq!(ed.down.total, before - 1);
    assert!(!ed.down.items.iter().any(|f| f.id == gone.id));
    assert_eq!(ed.up.total, 36, "the other list is untouched");
}

#[test]
fn failures_go_to_the_status_line_and_the_list_says_so() {
    let mut app = editing();
    app.on_reply(Reply::Failed { what: Failed::ThumbsHistory, message: "HTTP 500".into() });
    assert!(app.status.contains("thumbs history") && app.status.contains("HTTP 500"), "{}", app.status);
    app.on_reply(Reply::Failed { what: Failed::Seeds, message: "offline".into() });
    assert!(matches!(app.editor.as_ref().unwrap().seeds, Loaded::Failed(_)));
    assert!(app.status.contains("seeds"), "{}", app.status);
    // A failed thumbs page marks only the list that was loading.
    press(&mut app, KeyCode::Tab);
    app.editor.as_mut().unwrap().up.state = FeedState::Loading;
    app.on_reply(Reply::Failed { what: Failed::ThumbsHistory, message: "HTTP 500".into() });
    assert!(matches!(app.editor.as_ref().unwrap().up.state, FeedState::Failed(_)));
    // No suggestions is not an error worth the status line.
    chr(&mut app, 'a');
    press(&mut app, KeyCode::BackTab);
    chr(&mut app, 'a');
    app.status.clear();
    app.on_reply(Reply::Failed { what: Failed::Suggestions, message: "Service method not found (HTTP 404)".into() });
    assert!(app.status.is_empty(), "{}", app.status);
}

#[test]
fn failed_suggestions_leave_the_add_box_quiet_and_a_plain_search_box() {
    let mut app = editing();
    chr(&mut app, 'a');
    app.editor.as_mut().unwrap().add.as_mut().unwrap().suggesting = true;
    app.status.clear();
    app.on_reply(Reply::Failed { what: Failed::Suggestions, message: "Service method not found (HTTP 404)".into() });
    assert!(app.status.is_empty(), "{}", app.status);
    let add = app.editor.as_ref().unwrap().add.as_ref().unwrap();
    assert!(add.search.error.is_none() && !add.suggesting);
    // Typing still searches afterwards.
    chr(&mut app, 'm');
    assert!(app.editor.as_ref().unwrap().add.is_some());
}

#[test]
fn answers_for_another_station_or_a_closed_editor_are_ignored() {
    let mut app = editing();
    let before = seed_names(&app);
    app.on_reply(Reply::Seeds { station_id: "someone-else".into(), seeds: Vec::new() });
    assert_eq!(seed_names(&app), before);
    press(&mut app, KeyCode::Esc);
    app.on_reply(Reply::Seeds { station_id: "1".into(), seeds: Vec::new() });
    app.on_reply(Reply::SeedAdded { station_id: "1".into(), name: "x".into() });
    assert!(app.editor.is_none());
}
