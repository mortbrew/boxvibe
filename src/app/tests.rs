use super::*;
use ratatui::crossterm::event::KeyEvent;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn settings_start_with_account_then_boxvibe_playback_look() {
    let mut groups: Vec<&str> = Setting::ALL.iter().map(|(g, _)| *g).collect();
    groups.dedup();
    assert_eq!(groups, ["ACCOUNT", "BOXVIBE", "PLAYBACK", "LOOK", "BROWSE"]);
    // Every Browse section has a Settings row: `Setting::ALL` lists them by
    // hand, so a new section would otherwise get a menu pin and no row.
    let rows = Setting::ALL.iter().filter(|(_, s)| matches!(s, Setting::BrowseSection(_))).count();
    assert_eq!(rows, config::BROWSE_SECTIONS.len(), "one BROWSE row per section in config::BROWSE_SECTIONS");
    for (i, (key, title)) in config::BROWSE_SECTIONS.iter().enumerate() {
        let setting = Setting::BrowseSection(i as u8);
        assert_eq!(setting.label(), *title);
        assert_eq!(setting.saved_as(), Some(("browse", *key)));
    }
    assert_eq!(Setting::ALL[0].1, Setting::Email, "the account is on top");
    assert!(Setting::at(Setting::Splash) > Setting::at(Setting::ThemeAutoMode), "the boot screen is under LOOK");
    assert_eq!(App::new(Config::default(), None, &[], true).setting(), Setting::Email, "the cursor starts on it");
    let emoji = crate::icons::icons(crate::config::IconSet::Emoji);
    assert_eq!(emoji.setting[Setting::at(Setting::CheckUpdates)], "🆕", "icons move with their settings");
    assert_eq!(emoji.setting[Setting::at(Setting::Email)], "👤");
    let mut app = App::new(Config::default(), None, &[], true);
    app.open_prompt(Prompt::Password);
    assert_eq!(app.setting(), Setting::Password, "the cursor follows a setting, not a row number");
}

#[test]
fn enter_cycles_album_art_and_retro_mode() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.view = View::Settings;
    app.focus = Focus::Content;
    for (setting, first, second) in [(Setting::AlbumArt, "Off", "Picture"), (Setting::RetroMode, "64-bit", "32-bit")] {
        app.settings_cursor = Setting::ALL.iter().position(|(_, s)| *s == setting).unwrap();
        assert_eq!(Setting::ALL[app.settings_cursor].0, "LOOK");
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.setting_value(setting), first);
        app.on_key(key(KeyCode::Enter));
        assert!(app.setting_value(setting).starts_with(second), "{}", app.setting_value(setting));
    }
    assert_eq!(app.config.ui.album_art, AlbumArt::Picture);
    assert_eq!(app.config.ui.retro_mode, crate::config::RetroMode::Bit32);
    assert_eq!(app.status, "Retro mode: 32-bit");
    assert!(!app.status.contains('🎮'), "no controller icon: {}", app.status);
}

#[test]
fn enter_on_a_favorite_plays_it_and_shows_now_playing() {
    for playing_first in [false, true] {
        let mut config = Config::default();
        config.ui.autoplay_last_station = false;
        let mut app = App::new(config, None, &[], true);
        app.settle(Duration::from_millis(150));
        if playing_first {
            app.tune(0);
        }
        let fav = app.stations[1].clone();
        app.config.ui.favorites.push(crate::config::Favorite { id: fav.id.clone(), name: fav.name.clone() });
        app.view = View::Settings;
        app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Favorite(0)).unwrap();
        app.focus = Focus::Menu;
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.current_station.as_deref(), Some(fav.id.as_str()));
        assert_eq!(
            (app.view, app.focus, app.pane, app.menu_selected()),
            (View::Player, Focus::Content, Pane::NowPlaying, MenuItem::NowPlaying)
        );
    }
}

#[test]
fn enter_on_a_station_plays_it_and_shows_now_playing() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.settle(Duration::from_millis(150));
    app.on_key(shift('S'));
    app.on_key(key(KeyCode::Char('j')));
    let want = app.stations[app.stations_index()].id.clone();
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.current_station.as_deref(), Some(want.as_str()));
    assert_eq!(
        (app.view, app.focus, app.pane, app.menu_selected()),
        (View::Player, Focus::Content, Pane::NowPlaying, MenuItem::NowPlaying)
    );
}

#[test]
fn startup_stays_on_an_idle_now_playing() {
    let mut config = Config::default();
    config.ui.autoplay_last_station = false;
    let mut app = App::new(config, None, &[], true);
    app.settle(Duration::from_millis(150));
    assert_eq!(app.view, View::Player);
    assert!(app.now.is_none() && app.current_station.is_none());
    app.on_key(shift('S'));
    assert_eq!(app.view, View::Stations, "Shift+S opens it by hand");
}

#[test]
fn only_shift_q_quits() {
    let mut app = demo();
    app.on_key(key(KeyCode::Char('q')));
    assert!(!app.quit && !app.quit_asked());
    app.on_key(KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT));
    assert!(app.quit_asked() && !app.quit, "Q asks first");
    app.on_key(key(KeyCode::Char('y')));
    assert!(app.quit);
}

#[test]
fn quit_box_answers_only_on_y_n_enter_esc_and_a_second_q() {
    let shift_q = KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT);
    // No: n, Esc, and Enter (the box starts on No).
    for no in [KeyCode::Char('n'), KeyCode::Enter, KeyCode::Esc] {
        let mut app = demo();
        app.on_key(shift_q);
        app.on_key(key(no));
        assert!(!app.quit && !app.quit_asked(), "{no:?}");
    }
    // Yes: y, a second Q, and Enter once moved to Quit.
    for yes in [vec![key(KeyCode::Char('y'))], vec![shift_q], vec![key(KeyCode::Left), key(KeyCode::Enter)]] {
        let mut app = demo();
        app.on_key(shift_q);
        for k in yes {
            app.on_key(k);
        }
        assert!(app.quit, "yes");
    }
    // Anything else is ignored: the question stays, nothing quits.
    let mut app = demo();
    app.on_key(shift_q);
    for other in [KeyCode::Char('q'), KeyCode::Char('x'), KeyCode::Char(' '), KeyCode::Char('?'), KeyCode::Down] {
        app.on_key(key(other));
        assert!(app.quit_asked() && !app.quit, "{other:?}");
    }
}

#[test]
fn unpin_and_delete_questions_ignore_stray_keys() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.settle(Duration::from_millis(150));
    app.on_key(shift('S'));
    let station = app.stations.iter().position(|s| s.allow_delete).unwrap();
    app.stations_cursor = station;
    app.on_key(key(KeyCode::Char('x')));
    for other in ['q', 'j', 'x', ' '] {
        app.on_key(key(KeyCode::Char(other)));
        assert!(app.confirm_delete().is_some(), "{other} leaves the question open");
    }
    app.on_key(key(KeyCode::Esc));
    assert!(app.confirm_delete().is_none() && app.status == "Kept the station", "{}", app.status);
}

#[test]
fn help_closes_only_with_question_mark_or_esc() {
    for close in [KeyCode::Char('?'), KeyCode::Esc] {
        let mut app = demo();
        app.on_key(key(KeyCode::Char('?')));
        for other in ['q', 'x', 'j', ' '] {
            app.on_key(key(KeyCode::Char(other)));
            assert!(app.help_open(), "{other} leaves Help open");
        }
        app.on_key(key(close));
        assert!(!app.help_open() && !app.quit, "{close:?} closes it");
    }
}

#[test]
fn space_pauses_from_anywhere_and_lowercase_p_does_not() {
    let mut app = demo();
    assert!(app.now.is_some() && !app.audio.is_paused());
    app.focus = Focus::Menu;
    app.on_key(key(KeyCode::Char('p')));
    assert!(!app.audio.is_paused(), "p is unbound");
    app.on_key(key(KeyCode::Char(' ')));
    assert!(app.audio.is_paused(), "space pauses from the menu");
    app.on_key(key(KeyCode::Char(' ')));
    assert!(!app.audio.is_paused());
}

#[test]
fn the_recently_played_row_joins_the_collection_once_pandora_answers() {
    let app = demo();
    // The demo worker answers with three of its stations.
    assert!(app.recent_row(), "the demo has a history: {:#?}", app.recent);
    let items = app.menu_items();
    let pinned = items.iter().position(|m| *m == MenuItem::Pinned).expect("Pinned");
    let recent = items.iter().position(|m| *m == MenuItem::RecentlyPlayed).expect("Recently Played");
    assert_eq!(recent + 1, pinned, "above Pinned, inside MY COLLECTION: {items:?}");
    // Nothing asked for yet, or an empty history: no row at all, the way
    // FAVORITES hides.
    let fresh = App::new(Config::default(), None, &[], true);
    assert!(!fresh.recent_row());
    assert!(!fresh.menu_items().contains(&MenuItem::RecentlyPlayed), "{:?}", fresh.menu_items());
}

#[test]
fn enter_on_a_recent_station_plays_it_and_a_track_says_it_cannot() {
    let mut app = demo();
    let at = app.menu_items().iter().position(|m| *m == MenuItem::RecentlyPlayed).unwrap();
    app.focus = Focus::Menu;
    app.menu_cursor = at;
    app.on_key(key(KeyCode::Enter));
    // No status text in the message: it can carry the account, and CodeQL
    // counts a panic message as a log sink (cleartext-logging).
    assert_eq!((app.view, app.focus), (View::Recent, Focus::Content));
    let first = app.recent[0].clone();
    assert_eq!(first.kind, "ST", "{first:#?}");
    app.recent_cursor = 0;
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.current_station.as_deref(), Some(first.id.as_str()));
    assert_eq!(app.view, View::Player, "tuning lands on Now Playing, like Shift+P");
    // A track cannot be played from here: Pandora's answer names no station
    // to play it from, so the row says so rather than guessing one.
    app.view = View::Recent;
    app.recent = vec![RecentSource { id: "TR:1".into(), kind: "TR".into(), name: "Kids".into(), by: "MGMT".into() }];
    app.recent_cursor = 0;
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.view, View::Recent, "still here");
    assert!(app.status.contains("Kids") && app.status.contains("not a station"));
}

#[test]
fn a_recent_entry_is_titled_by_its_station_then_its_own_fields() {
    let app = demo();
    let station = app.stations[1].clone();
    let joined = RecentSource { id: station.id.clone(), kind: "ST".into(), ..Default::default() };
    assert_eq!(app.recent_title(&joined), station.name, "a station's name comes from the station list");
    let gone = RecentSource { id: "ST:0:99".into(), kind: "ST".into(), ..Default::default() };
    assert_eq!(app.recent_title(&gone), "ST:0:99", "nowhere to get a name: the id, never a blank row");
    let track = RecentSource { id: "TR:1".into(), kind: "TR".into(), name: "Kids".into(), by: "MGMT".into() };
    assert_eq!(app.recent_title(&track), "Kids");
    // ARTIST_TOP_SONGS answers an ArtistPlay: artist.name, no top-level name.
    let artist = RecentSource { id: "AP:2".into(), kind: "AP".into(), name: String::new(), by: "MGMT".into() };
    assert_eq!(app.recent_title(&artist), "MGMT");
}

#[test]
fn a_graphql_station_id_joins_and_plays_the_rest_station_without_moving_the_menu() {
    let mut app = demo();
    app.recent.clear();
    app.recent_loading = true;
    app.recent_generation = 7;
    let at = app.menu_items().iter().position(|m| *m == MenuItem::Settings).unwrap();
    app.focus = Focus::Menu;
    app.menu_cursor = at;
    let station = app.stations[2].clone();
    app.on_reply(Reply::Recent {
        generation: 7,
        sources: vec![RecentSource { id: format!("ST:0:{}", station.id), kind: "ST".into(), ..Default::default() }],
    });
    assert!(app.recent_row());
    assert_eq!(app.menu_selected(), MenuItem::Settings, "the new row must not steal the cursor");
    assert_eq!(app.recent_title(&app.recent[0]), station.name);
    app.view = View::Recent;
    app.focus = Focus::Content;
    app.recent_cursor = 0;
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.current_station.as_deref(), Some(station.id.as_str()));
    assert_eq!(app.view, View::Player);
}

#[test]
fn a_stale_recently_played_answer_is_ignored_and_logout_asks_again() {
    let mut app = demo();
    let before = app.recent.clone();
    app.recent_generation = 4;
    app.recent_loading = true;
    app.on_reply(Reply::Recent { generation: 3, sources: vec![] });
    assert_eq!(app.recent, before);
    assert!(app.recent_loading, "an older success must not clear the newer request");
    app.on_reply(Reply::RecentFailed { generation: 3, message: "old".into() });
    assert!(app.recent_loading, "an older failure must not clear the newer request");
    assert!(!app.status.contains("old"));
    let generation = app.recent_generation;
    app.log_out();
    assert!(app.recent.is_empty());
    assert!(!app.recent_asked && !app.recent_loading);
    assert_ne!(app.recent_generation, generation);
    app.on_reply(Reply::Recent {
        generation,
        sources: vec![RecentSource { id: "ST:0:1".into(), kind: "ST".into(), ..Default::default() }],
    });
    assert!(app.recent.is_empty(), "a late answer from the old login is dropped");
    app.session = Session::LoggedIn;
    app.on_reply(Reply::Stations(vec![Station { id: "3815".into(), name: "Tee".into(), ..Default::default() }]));
    assert!(app.recent_loading && app.recent_asked, "the next login asks once");
}

#[test]
fn favorites_wait_for_the_station_list_and_stale_ones_are_pruned() {
    let mut app = demo();
    let (keep, gone) = (app.stations[1].clone(), app.stations[2].clone());
    app.config.ui.favorites = vec![
        crate::config::Favorite { id: keep.id.clone(), name: "an old name".into() },
        crate::config::Favorite { id: gone.id.clone(), name: gone.name.clone() },
    ];
    // Logged out: no account, so the section is not there at all -- but the
    // stars are kept for when one signs in.
    app.log_out();
    assert!(app.stations.is_empty());
    assert!(!app.favorites_in_menu(), "hidden while logged out");
    assert!(!app.favorites_loading());
    assert_eq!(app.config.ui.favorites.len(), 2, "kept while logged out");
    // Signed in, before the list arrives: the section is back with a
    // loading row instead of rows that cannot say what kind of station it is.
    app.session = Session::LoggedIn;
    assert!(app.favorites_in_menu() && app.favorites_loading(), "a loading row stands in");
    // The list arrives without `gone`: that favorite goes, and the one that
    // stayed takes the name Pandora uses now.
    app.on_reply(Reply::Stations(vec![keep.clone()]));
    assert!(app.favorites_in_menu() && !app.favorites_loading());
    let ids: Vec<&str> = app.config.ui.favorites.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, vec![keep.id.as_str()], "the stale favorite is pruned");
    assert_eq!(app.config.ui.favorites[0].name, keep.name, "renamed to Pandora's");
    assert!(app.status.contains("removed"), "{}", app.status);
}

#[test]
fn an_empty_or_logged_out_list_never_prunes_favorites() {
    let mut app = demo();
    let starred = app.stations[1].clone();
    app.config.ui.favorites = vec![crate::config::Favorite { id: starred.id.clone(), name: starred.name.clone() }];
    // Logged out: the list is cleared, and the star must survive it.
    app.log_out();
    assert!(app.stations.is_empty());
    assert_eq!(app.config.ui.favorites.len(), 1, "logging out keeps the stars");
    // A logged-in account with no stations at all: still no pruning.
    app.session = Session::LoggedIn;
    app.on_reply(Reply::Stations(Vec::new()));
    assert_eq!(app.config.ui.favorites.len(), 1, "an empty list is not proof a station is gone");
}

#[test]
fn a_failed_recently_played_call_stops_the_spinner_and_says_why() {
    let mut app = demo();
    app.recent_loading = true;
    app.on_reply(Reply::Failed {
        what: Failed::RecentlyPlayed,
        message: "Variable \"$types\" got invalid value \"ALBUM\" (HTTP 400)".into(),
    });
    assert!(!app.recent_loading, "the pending flag clears");
    assert!(app.status.contains("Recently Played") && app.status.contains("invalid value"));
}

#[test]
fn playing_or_opening_recently_played_refreshes_once_and_keeps_the_selection() {
    let mut app = demo();
    let before = app.recent.clone();
    let generation = app.recent_generation;
    app.tune(2);
    assert!(app.recent_loading);
    assert_eq!(app.recent_generation, generation.wrapping_add(1));
    assert_eq!(app.recent, before, "playback does not invent a row before Pandora answers");
    app.tune(1);
    assert_eq!(app.recent_generation, generation.wrapping_add(1), "a second start waits for the one in flight");
    assert!(app.recent_dirty);
    app.settle(Duration::from_millis(500));
    assert!(!app.recent_loading && !app.recent_dirty);
    assert!(app.recent_generation > generation.wrapping_add(1), "the waiting start sends once");
    app.recent_cursor = app.recent.len() - 1;
    let id = app.recent[app.recent_cursor].id.clone();
    app.show_recently_played();
    assert_eq!(app.view, View::Recent);
    assert!(app.recent_loading);
    app.settle(Duration::from_millis(400));
    assert_eq!(app.recent[app.recent_cursor].id, id, "the open page keeps the selected source");
}

#[test]
fn a_failed_recently_played_fetch_can_be_asked_again_by_opening_the_page() {
    let mut app = demo();
    app.recent_loading = true;
    app.recent_generation = 9;
    app.on_reply(Reply::RecentFailed { generation: 9, message: "down".into() });
    assert!(!app.recent_loading);
    assert!(app.status.contains("down"));
    let generation = app.recent_generation;
    app.show_recently_played();
    assert!(app.recent_loading && app.recent_generation == generation.wrapping_add(1));
}

#[test]
fn x_removes_a_recently_played_row_here_only() {
    let mut app = demo();
    app.view = View::Recent;
    app.focus = Focus::Content;
    let before = app.recent.len();
    app.recent_cursor = 0;
    app.on_key(key(KeyCode::Char('x')));
    app.on_key(key(KeyCode::Char('y')));
    assert_eq!(app.recent.len(), before - 1);
    assert!(app.status.contains("device only"), "{}", app.status);
    app.refresh_recently_played();
    app.settle(Duration::from_millis(400));
    assert_eq!(app.recent.len(), before - 1, "hidden rows stay filtered after refresh");
}

#[test]
fn a_recent_row_appearing_above_pinned_keeps_the_pinned_cursor() {
    let mut app = demo();
    app.recent.clear();
    let pinned = app.menu_items().iter().position(|m| *m == MenuItem::Pinned).unwrap();
    app.focus = Focus::Menu;
    app.menu_cursor = pinned;
    app.recent_loading = true;
    app.recent_generation = 2;
    app.on_reply(Reply::Recent {
        generation: 2,
        sources: vec![RecentSource { id: "ST:0:1".into(), kind: "ST".into(), ..Default::default() }],
    });
    assert_eq!(app.menu_selected(), MenuItem::Pinned);
    let items = app.menu_items();
    let recent = items.iter().position(|m| *m == MenuItem::RecentlyPlayed).unwrap();
    let pinned = items.iter().position(|m| *m == MenuItem::Pinned).unwrap();
    assert!(recent < pinned);
}

#[test]
fn browse_menu_loads_directory_and_enter_starts_a_station() {
    let mut app = demo();
    let at = app.menu_items().iter().position(|m| *m == MenuItem::BrowsePin(0)).unwrap();
    app.focus = Focus::Menu;
    app.menu_cursor = at;
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.view, View::Browse);
    app.settle(Duration::from_millis(150));
    assert!(matches!(app.browse, BrowseState::Loaded(_)), "{:?}", app.browse);
    assert!(app.browse_rows().iter().any(|r| matches!(r, BrowseRow::Item(_))));
    // Land on M83 (first playable) and start it.
    while !matches!(app.browse_rows().get(app.browse_index()), Some(BrowseRow::Item(i)) if i.name == "M83") {
        app.on_key(key(KeyCode::Char('j')));
    }
    let before = app.stations.len();
    app.on_key(key(KeyCode::Enter));
    app.settle(Duration::from_millis(150));
    assert!(app.stations.len() > before || app.now.is_some(), "{}", app.status);
}

#[test]
fn browse_section_toggle_pins_on_the_main_menu_and_is_saved() {
    let dir = config::temp_dir("browse-section");
    let path = dir.join("config.toml");
    let mut app = App::new(Config::default(), Some(path.clone()), &[], true);
    app.demo = false;
    app.config_path = Some(path.clone());
    assert!(app.browse_menu_pins().iter().any(|(_, t)| t == "Artists Picked For You"), "section pins default On");
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.settings_cursor = Setting::at(Setting::BrowseSection(0));
    app.on_key(key(KeyCode::Enter));
    assert!(!app.config.browse.pinned_on_menu("Artists Picked For You"));
    assert!(!app.browse_menu_pins().iter().any(|(_, t)| t == "Artists Picked For You"));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("artists_picked_for_you = false"), "{text}");
    app.on_key(key(KeyCode::Enter));
    assert!(app.config.browse.pinned_on_menu("Artists Picked For You"));
    assert!(std::fs::read_to_string(&path).unwrap().contains("artists_picked_for_you = true"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn browse_lists_every_section_even_when_pins_are_off() {
    let mut app = demo();
    app.show_browse();
    app.settle(Duration::from_millis(150));
    let titles: Vec<_> = app
        .browse_rows()
        .iter()
        .filter_map(|r| match r {
            BrowseRow::Header(t) => Some(*t),
            _ => None,
        })
        .collect();
    assert!(titles.contains(&"Artists Picked For You"));
    assert!(titles.contains(&"Browse Genres"));
    assert!(titles.len() >= 10, "{titles:?}");
}

#[test]
fn g_in_a_list_is_not_end_so_shift_g_stays_genres() {
    let mut app = demo();
    app.focus = Focus::Content;
    app.pane = Pane::NowPlaying;
    app.on_key(key(KeyCode::Char('.')));
    assert!(app.song_menu.is_some());
    app.on_key(key(KeyCode::Char('G')));
    assert!(app.song_menu.is_some(), "G does not jump to the end of the song menu");
    assert_eq!(app.song_menu.as_ref().unwrap().cursor, 0);
    app.on_key(key(KeyCode::Esc));
    app.on_key(key(KeyCode::Char('G')));
    assert_eq!(app.view, View::Browse, "G opens Browse Genres");
    assert_eq!(app.browse_filter.as_deref(), Some("Browse Genres"));
}

#[test]
fn lowercase_q_closes_neither_the_song_menu_nor_a_panel() {
    let mut app = demo();
    app.focus = Focus::Content;
    app.pane = Pane::NowPlaying;
    app.on_key(key(KeyCode::Char('.')));
    assert!(app.song_menu.is_some(), "the song menu opens");
    app.on_key(key(KeyCode::Char('q')));
    assert!(app.song_menu.is_some() && !app.quit_asked(), "q does nothing");
    app.on_key(key(KeyCode::Esc));
    assert!(app.song_menu.is_none(), "Esc closes it");
    app.on_key(key(KeyCode::Char('m')));
    app.settle(Duration::from_millis(150));
    assert!(app.panel.is_some(), "the modes panel opens");
    app.on_key(key(KeyCode::Char('q')));
    assert!(app.panel.is_some(), "q does nothing");
    app.on_key(key(KeyCode::Esc));
    assert!(app.panel.is_none(), "Esc closes it");
}

#[test]
fn ctrl_c_quits_at_once_even_in_the_quit_box() {
    let mut app = demo();
    app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(app.quit);
    let mut app = demo();
    app.on_key(KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT));
    app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(app.quit);
}

fn demo() -> App {
    // Ads have their own tests: here a skip goes straight to the next song.
    let mut config = Config::default();
    config.player.good_vibes_only = GoodVibes::Off;
    let mut app = App::new(config, None, &[], true);
    app.demo_warm_up(2, Duration::ZERO);
    app
}

/// A demo app that asks for ads (every third answer is a pod of two).
fn ad_demo() -> App {
    let mut app = App::new(Config::default(), None, &[], true);
    app.settle(Duration::from_millis(300));
    assert!(app.now.is_some() && app.config.player.good_vibes_only == GoodVibes::On);
    app
}

#[test]
fn b_steps_the_sleep_timer_and_it_fades_out_then_pauses() {
    let mut app = ad_demo();
    let mut seen = Vec::new();
    for _ in 0..6 {
        app.on_key(shift('B'));
        seen.push(match app.sleep {
            Some(Sleep::Minutes(m, _)) => m.to_string(),
            Some(Sleep::EndOfSong) => "end".into(),
            None => "off".into(),
        });
    }
    assert_eq!(seen, ["15", "30", "60", "90", "end", "off"]);
    assert!(app.status.contains("off"), "{}", app.status);
    // 5 seconds left: half way down the fade.
    let full = app.audio.level();
    app.sleep = Some(Sleep::Minutes(15, Instant::now() - Duration::from_secs(15 * 60 - 5)));
    app.tick();
    let half = app.audio.level() / full;
    assert!((0.4..=0.55).contains(&half), "{half}");
    assert!(!app.audio.is_paused());
    // Time's up: paused, the volume as it was, the timer gone.
    app.sleep = Some(Sleep::Minutes(15, Instant::now() - Duration::from_secs(15 * 60 + 1)));
    app.tick();
    assert!(app.audio.is_paused() && app.sleep.is_none());
    assert_eq!(app.audio.level(), full);
    app.on_key(key(KeyCode::Char(' ')));
    assert!(!app.audio.is_paused(), "space plays on");
    assert_eq!(clock_text(Duration::from_secs(14 * 60 + 59)), "14:59");
    assert_eq!(clock_text(Duration::from_secs(90 * 60)), "1:30:00");
}

#[test]
fn sleep_at_the_end_of_the_song_pauses_the_next_one_at_once() {
    let mut app = App::new(
        Config {
            player: crate::config::PlayerConfig { good_vibes_only: GoodVibes::Off, ..Default::default() },
            ..Default::default()
        },
        None,
        &[],
        true,
    );
    app.settle(Duration::from_millis(300));
    let first = app.now.as_ref().unwrap().track_token.clone();
    let length = app.now.as_ref().unwrap().length;
    app.sleep = Some(Sleep::EndOfSong);
    app.audio.seek_silent(Duration::from_secs(u64::from(length) - 3));
    app.tick();
    assert!(app.audio.level() < 0.5 * level_at(&app), "fading in the last seconds");
    app.audio.seek_silent(Duration::from_secs(u64::from(length) + 1));
    app.tick();
    app.settle(Duration::from_millis(300));
    app.tick();
    assert!(app.now.as_ref().is_some_and(|s| s.track_token != first), "the next song is there");
    assert!(app.audio.is_paused() && app.sleep.is_none(), "{}", app.status);
    assert_eq!(app.audio.level(), level_at(&app));
}

fn level_at(app: &App) -> f32 {
    crate::audio::level(app.config.player.volume, app.now.as_ref().map_or(0.0, |s| s.gain))
}

#[test]
fn shift_arrows_change_the_volume_and_plain_arrows_do_not() {
    let mut app = demo();
    app.config.player.volume = 50;
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
    assert_eq!(app.config.player.volume, 55);
    app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
    app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
    assert_eq!(app.config.player.volume, 45);
    for code in [KeyCode::Left, KeyCode::Right, KeyCode::Char('['), KeyCode::Char(']')] {
        app.on_key(key(code));
    }
    assert_eq!(app.config.player.volume, 45, "only Shift+arrows touch the volume");
}

#[test]
fn up_and_down_wrap_around_the_ends() {
    assert_eq!((step(0, 5, -1), step(4, 5, 1), step(2, 5, 1)), (4, 0, 3));
    assert_eq!((step(3, 5, -1000), step(1, 5, 1000), step(0, 0, -1)), (0, 4, 0), "g / G stop at the ends");
    let mut app = demo();
    app.focus = Focus::Menu;
    let last = app.menu_items().len() - 1;
    app.menu_cursor = 0;
    app.on_key(key(KeyCode::Up));
    assert_eq!(app.menu_cursor, last, "menu: up from the top is the bottom");
    app.on_key(key(KeyCode::Down));
    assert_eq!(app.menu_cursor, 0, "and down from the bottom is the top");
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.settings_cursor = 0;
    app.on_key(key(KeyCode::Char('k')));
    assert_eq!(app.settings_cursor, Setting::ALL.len() - 1, "settings wrap too");
    app.view = View::Stations;
    app.stations_cursor = app.stations.len() - 1;
    app.on_key(key(KeyCode::Down));
    assert!(app.on_add_row, "stations: the last one is followed by the Add station row");
    app.on_key(key(KeyCode::Down));
    assert_eq!((app.on_add_row, app.stations_index()), (false, 0), "and then the list wraps");
    app.on_key(key(KeyCode::Up));
    assert!(app.on_add_row, "up from the first station is the Add station row");
    app.on_key(key(KeyCode::Up));
    assert_eq!(app.stations_index(), app.stations.len() - 1, "and up from that wraps to the last");
}

#[test]
fn menu_has_stations_then_settings_about_quit() {
    let app = demo();
    let items = app.menu_items();
    assert_eq!(items[0], MenuItem::NowPlaying, "a station plays: NOW PLAYING on top");
    assert_eq!(items[1], MenuItem::Stations);
    assert_eq!(items[2], MenuItem::RecentlyPlayed, "Recently Played takes Pinned's old place");
    assert_eq!(items[3], MenuItem::Pinned);
    let pins = config::BROWSE_SECTIONS.len();
    assert_eq!(&items[4..4 + pins], &(0..pins).map(MenuItem::BrowsePin).collect::<Vec<_>>()[..]);
    assert_eq!(
        items.len(),
        2 + 1 + 1 + pins + 4,
        "NOW PLAYING, My Stations, Pinned, Recently Played, all section pins, no favorites"
    );
    assert_eq!(
        &items[items.len() - 4..],
        [MenuItem::Settings, MenuItem::Notifications, MenuItem::About, MenuItem::Quit]
    );
}

#[test]
fn enter_on_settings_and_about_opens_them() {
    let mut app = demo();
    app.focus = Focus::Menu;
    app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Settings).unwrap();
    app.on_key(key(KeyCode::Enter));
    assert_eq!((app.view, app.focus), (View::Settings, Focus::Content));
    app.on_key(key(KeyCode::Esc));
    assert_eq!(app.menu_selected(), MenuItem::Settings);
    app.on_key(key(KeyCode::Char('j')));
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.view, View::Notifications);
    app.on_key(key(KeyCode::Esc));
    app.on_key(key(KeyCode::Char('j')));
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.view, View::About);
    app.on_key(shift('S'));
    assert_eq!((app.view, app.focus), (View::Stations, Focus::Content));
    app.on_key(key(KeyCode::Esc));
    assert_eq!(app.menu_selected(), MenuItem::Stations);
}

#[test]
fn now_playing_from_the_menu_and_with_shift_p() {
    let mut app = demo();
    app.view = View::About;
    app.focus = Focus::Content;
    app.on_key(shift('P'));
    assert_eq!((app.view, app.focus, app.menu_selected()), (View::Player, Focus::Content, MenuItem::NowPlaying));
    assert_eq!(app.pane, Pane::NowPlaying);
    let title = app.now.as_ref().map(|s| s.title.clone());
    app.view = View::Settings;
    app.focus = Focus::Menu;
    app.menu_cursor = 0;
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.view, View::Player);
    assert_eq!(app.now.as_ref().map(|s| s.title.clone()), title, "only shows it: the song plays on");
    // Shift+N skips.
    app.on_key(shift('N'));
    assert_ne!(app.now.as_ref().map(|s| s.title.clone()), title);
}

#[test]
fn no_now_playing_entry_before_a_station_plays() {
    let mut config = Config::default();
    config.ui.autoplay_last_station = false;
    let mut app = App::new(config, None, &[], true);
    app.settle(Duration::from_millis(150));
    assert_eq!(app.view, View::Player, "startup never opens Stations by itself");
    assert!(!app.menu_items().contains(&MenuItem::NowPlaying));
    app.on_key(shift('P'));
    assert!(app.status.starts_with("Nothing is playing"), "{}", app.status);
    assert_eq!(app.view, View::Player, "Shift+P with nothing playing stays put");
    app.on_key(shift('S'));
    assert_eq!((app.view, app.focus), (View::Stations, Focus::Content), "Shift+S opens the list");
    // Enter on the second station: it plays and NOW PLAYING appears, selected.
    app.on_key(key(KeyCode::Char('j')));
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.current_station.as_deref(), Some(app.stations[1].id.as_str()));
    assert_eq!(
        (app.view, app.focus, app.pane, app.menu_selected()),
        (View::Player, Focus::Content, Pane::NowPlaying, MenuItem::NowPlaying)
    );
}

#[test]
fn f_stars_stations_into_a_favorites_section_that_plays_them() {
    let path = crate::config::temp_dir("favorites").join("config.toml");
    let mut config = Config::default();
    config.ui.autoplay_last_station = false;
    let mut app = App::new(config, None, &[], true);
    app.settle(Duration::from_millis(150));
    // The demo never saves: point it at a file to see what would be.
    app.demo = false;
    app.config_path = Some(path.clone());
    app.on_key(shift('S'));
    app.on_key(key(KeyCode::Char('j')));
    app.on_key(key(KeyCode::Char('j')));
    app.on_key(key(KeyCode::Char('f')));
    let third = app.stations[2].clone();
    assert!(app.is_favorite(&third.id));
    // The demo's recently played answer lands during `settle`, above Pinned.
    let mut want = vec![MenuItem::Stations, MenuItem::RecentlyPlayed, MenuItem::Pinned];
    want.extend((0..config::BROWSE_SECTIONS.len()).map(MenuItem::BrowsePin));
    want.extend([MenuItem::Favorite(0), MenuItem::Settings, MenuItem::Notifications, MenuItem::About, MenuItem::Quit]);
    assert_eq!(app.menu_items(), want);
    let saved = Config::load(&path).0.ui.favorites;
    assert_eq!(
        (saved.len(), saved[0].id.as_str(), saved[0].name.as_str()),
        (1, third.id.as_str(), third.name.as_str())
    );
    // Enter on the favorite in the menu plays it.
    app.on_key(key(KeyCode::Esc));
    let fav_at = app.menu_items().iter().position(|m| *m == MenuItem::Favorite(0)).unwrap();
    app.menu_cursor = fav_at;
    assert_eq!(app.menu_selected(), MenuItem::Favorite(0));
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.current_station.as_deref(), Some(third.id.as_str()));
    assert_eq!((app.view, app.menu_selected()), (View::Player, MenuItem::NowPlaying), "on to Now Playing");
    app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Favorite(0)).unwrap();
    // f on it in the menu unstars it: the section goes.
    app.on_key(key(KeyCode::Char('f')));
    assert!(app.config.ui.favorites.is_empty() && !app.menu_items().iter().any(|m| matches!(m, MenuItem::Favorite(_))));
    assert!(Config::load(&path).0.ui.favorites.is_empty());
    assert!(app.menu_cursor < app.menu_items().len());
}

#[test]
fn first_run_opens_settings_typing_the_email() {
    let dir = crate::config::temp_dir("first-run");
    let path = dir.join("config.toml");
    let mut config = Config::default();
    config.ui.check_updates = false;
    let mut app = App::new(config, Some(path.clone()), &[], false);
    assert_eq!((app.view, app.prompt), (View::Settings, Some(Prompt::Email)));
    for c in "me@example.com".chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.prompt, Some(Prompt::Password), "then the password");
    assert_eq!(app.config.pandora.username, "me@example.com");
    assert!(std::fs::read_to_string(&path).unwrap().contains("me@example.com"), "email saved");
    // Esc leaves the password empty: no login is tried.
    app.on_key(key(KeyCode::Esc));
    assert_eq!((app.prompt, &app.session), (None, &Session::LoggedOut));
}

#[test]
fn tab_goes_menu_now_playing_up_next_history_and_back() {
    let mut app = demo();
    assert_eq!((app.focus, app.pane), (Focus::Content, Pane::NowPlaying), "autoplay lands like N");
    app.focus = Focus::Menu;
    app.on_key(key(KeyCode::Tab));
    assert_eq!((app.focus, app.pane), (Focus::Content, Pane::NowPlaying));
    app.on_key(key(KeyCode::Tab));
    assert_eq!((app.focus, app.pane), (Focus::Content, Pane::UpNext));
    app.on_key(key(KeyCode::Tab));
    assert_eq!((app.focus, app.pane), (Focus::Content, Pane::History));
    app.on_key(key(KeyCode::Tab));
    assert_eq!(app.focus, Focus::Menu);
    app.on_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!((app.focus, app.pane), (Focus::Content, Pane::History));
}

fn shift(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)
}

#[test]
fn shift_u_d_rate_the_playing_song_from_any_pane() {
    let mut app = demo();
    for pane in [Pane::History, Pane::UpNext] {
        app.focus = Focus::Content;
        app.pane = pane;
        let before = app.history[0].rating;
        let playing = app.now.as_ref().unwrap().track_token.clone();
        app.on_key(shift('U'));
        assert_eq!(app.now.as_ref().unwrap().rating, Rating::Up, "{pane:?}: the playing song");
        assert_eq!(app.history[0].rating, before, "{pane:?}: the History row is untouched");
        assert_eq!(app.now.as_ref().unwrap().track_token, playing);
    }
    // From the menu too, and from another page.
    app.focus = Focus::Menu;
    app.view = View::Settings;
    let playing = app.now.as_ref().unwrap().track_token.clone();
    app.on_key(shift('D'));
    assert!(app.history.iter().any(|s| s.track_token == playing && s.rating == Rating::Down));
    assert_ne!(app.now.as_ref().unwrap().track_token, playing, "thumbs down skips");
}

#[test]
fn the_old_thumb_keys_do_nothing() {
    let mut app = demo();
    app.focus = Focus::Content;
    app.pane = Pane::NowPlaying;
    let playing = app.now.as_ref().unwrap().track_token.clone();
    for c in ['+', '-', '=', 'u', 'd', 'n', 's', 't'] {
        let (view, theme) = (app.view, app.config.ui.theme().to_owned());
        app.on_key(key(KeyCode::Char(c)));
        assert_eq!(app.now.as_ref().unwrap().rating, Rating::None, "{c} rates nothing");
        assert_eq!(app.now.as_ref().unwrap().track_token, playing, "{c} skips nothing");
        assert_eq!((app.view, &app.config.ui.theme()), (view, &theme.as_str()), "{c} goes nowhere");
    }
}

#[test]
fn notifications_keep_what_the_bottom_bar_said_newest_first() {
    let mut app = demo();
    for text in ["one", "one", "", "two"] {
        app.status = text.into();
        app.tick();
    }
    let said: Vec<&str> = app.notes.iter().map(|(_, s)| s.as_str()).collect();
    assert_eq!(&said[..2], ["two", "one"], "newest first, repeats once");
    for i in 0..NOTES_KEPT + 5 {
        app.status = format!("n{i}");
        app.note_status();
    }
    assert_eq!(app.notes.len(), NOTES_KEPT);
}

#[test]
fn dot_opens_the_song_menu_and_start_station_fills_the_search() {
    let mut app = demo();
    app.focus = Focus::Content;
    app.pane = Pane::NowPlaying;
    app.on_key(key(KeyCode::Char('.')));
    assert!(app.song_menu.is_some());
    assert!(app.overlay_open());
    app.on_key(key(KeyCode::Esc));
    assert!(app.song_menu.is_none());
    let artist = app.now.as_ref().unwrap().artist.clone();
    app.on_key(key(KeyCode::Char('.')));
    app.on_key(key(KeyCode::Enter));
    let search = app.add_station.as_ref().expect("the add-station box opens");
    assert_eq!(search.text.text(), artist.trim());
    assert_eq!(search.prefer, Some(crate::pandora::HitKind::Artist));
}

fn menu_pick(app: &mut App, action: SongAction) {
    app.on_key(key(KeyCode::Char('.')));
    let at = SongAction::ALL.iter().position(|a| *a == action).unwrap();
    for _ in 0..at {
        app.on_key(key(KeyCode::Char('j')));
    }
    app.on_key(key(KeyCode::Enter));
}

#[test]
fn the_artist_panel_shows_a_bio_and_starts_a_similar_station() {
    let mut app = demo();
    app.focus = Focus::Content;
    app.pane = Pane::NowPlaying;
    let artist = app.now.as_ref().unwrap().artist.clone();
    menu_pick(&mut app, SongAction::Artist);
    let panel = app.panel.as_ref().expect("the artist panel opens");
    assert!(panel.loading && app.overlay_open());
    app.settle(Duration::from_millis(150));
    let panel = app.panel.as_ref().unwrap();
    assert!(!panel.loading);
    assert!(panel.intro.contains(&artist), "{}", panel.intro);
    assert!(!panel.items.is_empty() && panel.items.iter().all(|i| !i.label.eq_ignore_ascii_case(&artist)));
    let similar = panel.items[0].label.clone();
    let stations = app.stations.len();
    app.on_key(key(KeyCode::Enter));
    assert!(app.panel.is_none());
    app.settle(Duration::from_millis(150));
    assert!(app.stations.len() >= stations);
    let playing = app.current_station.clone().unwrap();
    let name = &app.stations.iter().find(|s| s.id == playing).unwrap().name;
    assert!(name.contains(&similar), "{name} from {similar}");
}

#[test]
fn a_failed_artist_page_says_so_in_the_panel() {
    let mut app = demo();
    app.focus = Focus::Content;
    app.pane = Pane::NowPlaying;
    menu_pick(&mut app, SongAction::Artist);
    app.on_reply(Reply::Failed { what: Failed::Artist, message: "Pandora is down".into() });
    let panel = app.panel.as_ref().unwrap();
    assert!(panel.intro.contains("Pandora is down") && panel.items.is_empty());
    app.on_key(key(KeyCode::Enter));
    assert!(app.panel.is_some(), "Enter on nothing does nothing");
    app.on_key(key(KeyCode::Esc));
    assert!(app.panel.is_none());
}

#[test]
fn m_picks_a_station_mode_and_premium_ones_stay_locked() {
    let mut app = demo();
    app.focus = Focus::Content;
    app.pane = Pane::NowPlaying;
    let playing = app.current_station.clone().unwrap();
    app.on_key(key(KeyCode::Char('m')));
    app.settle(Duration::from_millis(150));
    let panel = app.panel.as_ref().expect("m opens the modes");
    assert_eq!(panel.items.len(), 6);
    assert!(panel.items[0].current && panel.cursor == 0);
    // Down to Deep Cuts (the fourth) and switch.
    for _ in 0..3 {
        app.on_key(key(KeyCode::Char('j')));
    }
    app.on_key(key(KeyCode::Enter));
    app.settle(Duration::from_millis(150));
    let panel = app.panel.as_ref().unwrap();
    assert!(panel.items[3].current && !panel.items[0].current);
    assert!(app.status.contains("Deep Cuts"), "{}", app.status);
    assert_eq!(app.current_station.as_deref(), Some(playing.as_str()), "the same station plays on");
    // Artist Only is Premium in the demo: locked, nothing sent.
    app.on_key(key(KeyCode::End));
    app.on_key(key(KeyCode::Enter));
    assert!(app.status.contains("isn't available"), "{}", app.status);
    app.on_key(key(KeyCode::Esc));
    // Reopened from the song menu, it remembers.
    menu_pick(&mut app, SongAction::Mode);
    app.settle(Duration::from_millis(150));
    assert_eq!(app.panel.as_ref().unwrap().cursor, 3);
}

#[test]
fn no_modes_from_pandora_fails_gracefully() {
    let mut app = demo();
    app.focus = Focus::Content;
    app.pane = Pane::NowPlaying;
    app.on_key(key(KeyCode::Char('m')));
    app.on_reply(Reply::Failed { what: Failed::Mode, message: "Pandora doesn't offer station modes here".into() });
    let panel = app.panel.as_ref().unwrap();
    assert!(panel.intro.starts_with("No station modes") && !panel.loading);
    assert_eq!(crate::ui::key_caps(&app), [("Esc", "Close")]);
}

#[test]
fn shift_x_plays_shuffle() {
    let mut app = demo();
    let shuffle = app.stations.iter().find(|s| s.is_shuffle).unwrap().id.clone();
    if app.current_station.as_deref() == Some(shuffle.as_str()) {
        let other = app.stations.iter().position(|s| !s.is_shuffle).unwrap();
        app.tune(other);
    }
    app.on_key(shift('X'));
    assert_eq!(app.current_station.as_deref(), Some(shuffle.as_str()));
    app.on_key(shift('X'));
    assert_eq!(app.status, "Already playing Shuffle");
    // An account without one asks Pandora, which makes it.
    app.stations.retain(|s| !s.is_shuffle);
    app.current_station = None;
    app.on_key(shift('X'));
    app.settle(Duration::from_millis(150));
    assert!(app.status.starts_with("⚠ No Shuffle") || app.status.starts_with("Tuning"), "{}", app.status);
    app.on_reply(Reply::Shuffle(Station {
        id: "sh".into(),
        name: "Shuffle".into(),
        is_shuffle: true,
        ..Default::default()
    }));
    assert_eq!((app.current_station.as_deref(), app.stations[0].is_shuffle), (Some("sh"), true));
}

#[test]
fn tired_from_the_menu_drops_an_up_next_song() {
    let mut app = demo();
    app.focus = Focus::Content;
    app.pane = Pane::UpNext;
    let first = app.queue[0].track_token.clone();
    app.on_key(key(KeyCode::Char('.')));
    let at = SongAction::ALL.iter().position(|a| *a == SongAction::Tired).unwrap();
    for _ in 0..at {
        app.on_key(key(KeyCode::Char('j')));
    }
    app.on_key(key(KeyCode::Enter));
    assert!(app.queue.iter().all(|s| s.track_token != first));
    assert!(app.status.starts_with("Tired of"), "{}", app.status);
}

#[test]
fn share_links_and_base64() {
    let mut s = Song { title: "A B".into(), artist: "C&D".into(), ..Song::default() };
    assert_eq!(share_link(&s), "https://www.pandora.com/search/A%20B%20C%26D/all");
    s.detail_url = "/artist/x".into();
    assert_eq!(share_link(&s), "https://www.pandora.com/artist/x");
    assert_eq!(base64(b"Man"), "TWFu");
    assert_eq!(base64(b"Ma"), "TWE=");
    assert_eq!(base64(b"M"), "TQ==");
}

#[test]
fn the_same_thumb_again_takes_it_back() {
    let mut app = demo();
    app.focus = Focus::Content;
    app.pane = Pane::NowPlaying;
    app.on_key(key(KeyCode::Char('l')));
    app.settle(Duration::from_millis(150));
    assert_eq!(app.now.as_ref().unwrap().rating, Rating::Up);
    assert!(!app.now.as_ref().unwrap().feedback_id.is_empty(), "Pandora's feedbackId is kept");
    app.on_key(key(KeyCode::Char('l')));
    assert_eq!(app.now.as_ref().unwrap().rating, Rating::None);
    app.settle(Duration::from_millis(150));
    assert!(app.status.starts_with("Thumb taken back"), "{}", app.status);
    // Without a feedbackId it says why and changes nothing.
    app.now.as_mut().unwrap().rating = Rating::Up;
    app.on_key(key(KeyCode::Char('l')));
    assert_eq!(app.now.as_ref().unwrap().rating, Rating::Up);
    assert!(app.status.contains("didn't say which thumb"), "{}", app.status);
}

#[test]
fn a_download_that_never_answers_is_given_up_on() {
    // A reply for a token nobody waits for is dropped, so without this the
    // player would sit on "buffering…" for the rest of the session.
    let mut app = demo();
    let playing = app.now.as_ref().unwrap().track_token.clone();
    app.buffering = Some((playing.clone(), Instant::now()));
    app.buffer_tick();
    assert!(app.buffering.is_some(), "a download still in time is left alone");

    app.buffering = Some((playing.clone(), Instant::now() - BUFFER_TIMEOUT - Duration::from_secs(1)));
    app.buffer_tick();
    assert!(app.buffering.is_none(), "the wait is over");
    assert!(app.status.contains("took too long"), "{}", app.status);
    assert_ne!(app.now.as_ref().unwrap().track_token, playing, "the next song starts");
}

#[test]
fn a_run_of_held_dislikes_is_walked_in_one_call() {
    // `next_song` used to reach the next song by calling `advance`, which called
    // `next_song` again, so a run of songs to skip was a recursion. It is a loop
    // now: every held dislike in the queue is cleared in this one call.
    let mut app = demo();
    let queued = app.queue.len();
    assert!(queued >= 2, "the demo queue has songs to skip through");
    let tokens: Vec<String> = app.queue.iter().map(|s| s.track_token.clone()).collect();
    for token in &tokens {
        app.held_dislikes.insert(token.clone());
    }
    app.next_song();
    assert!(app.held_dislikes.is_empty(), "every held dislike was sent and cleared");
    assert!(app.queue.is_empty(), "the whole run was walked");
    assert!(app.history.len() >= tokens.len(), "each skipped song went to History");
}

#[test]
fn an_up_next_dislike_waits_for_its_turn_and_b_again_cancels_it() {
    let mut app = demo();
    app.focus = Focus::Content;
    app.pane = Pane::UpNext;
    let first = app.queue[0].track_token.clone();
    app.on_key(key(KeyCode::Char('b')));
    app.on_key(key(KeyCode::Char('b')));
    assert_eq!(app.queue[0].rating, Rating::None, "b again cancels");
    assert!(app.held_dislikes.is_empty());
    app.on_key(key(KeyCode::Char('b')));
    let second = app.queue[1].track_token.clone();
    app.on_key(shift('N'));
    app.settle(Duration::from_millis(150));
    assert_eq!(app.now.as_ref().unwrap().track_token, second, "the held song is skipped at its turn");
    assert!(app.history.iter().any(|s| s.track_token == first && s.rating == Rating::Down));
    assert!(app.held_dislikes.is_empty());
}

#[test]
fn a_held_dislike_does_not_outlive_its_station_or_its_login() {
    // A dislike held on an Up Next row is for a song of this station: tuning
    // away or logging out must not leave it to skip a song later.
    let mut app = demo();
    app.focus = Focus::Content;
    app.pane = Pane::UpNext;
    app.on_key(key(KeyCode::Char('b')));
    assert_eq!(app.held_dislikes.len(), 1);
    app.tune(1);
    assert!(app.held_dislikes.is_empty(), "tuning away drops the held dislike");

    app.settle(Duration::from_millis(150));
    app.focus = Focus::Content;
    app.pane = Pane::UpNext;
    app.on_key(key(KeyCode::Char('b')));
    assert_eq!(app.held_dislikes.len(), 1);
    app.log_out();
    assert!(app.held_dislikes.is_empty(), "logging out drops the held dislike");
}

#[test]
fn l_and_b_rate_what_the_pane_has() {
    let mut app = demo();
    app.focus = Focus::Content;
    // History: the highlighted row, never the playing song.
    app.pane = Pane::History;
    app.on_key(key(KeyCode::Char('j')));
    let playing = app.now.as_ref().unwrap().track_token.clone();
    app.on_key(key(KeyCode::Char('b')));
    assert_eq!(app.history[1].rating, Rating::Down);
    assert_eq!(app.now.as_ref().unwrap().rating, Rating::None);
    assert_eq!(app.now.as_ref().unwrap().track_token, playing, "rating a row doesn't skip");
    // Up Next: l likes the highlighted song in place; b holds a dislike for its turn.
    app.pane = Pane::UpNext;
    let token = app.queue[0].track_token.clone();
    app.on_key(key(KeyCode::Char('l')));
    assert_eq!((app.queue[0].track_token.as_str(), app.queue[0].rating), (token.as_str(), Rating::Up));
    let n = app.queue.len();
    app.on_key(key(KeyCode::Char('b')));
    assert_eq!((app.queue.len(), app.queue[0].rating), (n, Rating::Down));
    assert!(app.status.contains("b again cancels"), "{}", app.status);
    // Now Playing focused: the current song.
    app.pane = Pane::NowPlaying;
    app.on_key(key(KeyCode::Char('l')));
    assert_eq!(app.now.as_ref().unwrap().rating, Rating::Up);
    // From the menu there is no song pane: it says so and rates nothing.
    app.focus = Focus::Menu;
    let (up, down) = (app.history[0].rating, app.history[1].rating);
    app.on_key(key(KeyCode::Char('b')));
    assert_eq!((app.history[0].rating, app.history[1].rating), (up, down));
    assert!(app.status.contains("open Up Next, History or Now Playing"), "{}", app.status);
    // An empty pane says so and does nothing.
    app.focus = Focus::Content;
    app.pane = Pane::History;
    app.history.clear();
    app.on_key(key(KeyCode::Char('l')));
    assert!(app.status.starts_with("History is empty"), "{}", app.status);
}

#[test]
fn shift_n_skips_and_shift_p_goes_to_now_playing() {
    let mut app = demo();
    app.focus = Focus::Content;
    app.pane = Pane::History;
    app.view = View::Settings;
    let playing = app.now.as_ref().unwrap().track_token.clone();
    // Shift+P: where Shift+N used to go.
    app.on_key(shift('P'));
    assert_eq!((app.view, app.focus, app.pane), (View::Player, Focus::Content, Pane::NowPlaying));
    assert_eq!(app.menu_selected(), MenuItem::NowPlaying);
    assert_eq!(app.now.as_ref().unwrap().track_token, playing, "going there skips nothing");
    app.on_key(key(KeyCode::Tab));
    assert_eq!(app.pane, Pane::UpNext);
    // Shift+N skips, from any pane and any page.
    app.view = View::About;
    app.focus = Focus::Menu;
    app.on_key(shift('N'));
    assert_ne!(app.now.as_ref().unwrap().track_token, playing);
    assert_eq!((app.view, app.focus), (View::About, Focus::Menu), "skipping doesn't move you");
    // Lowercase n no longer skips.
    let now = app.now.as_ref().unwrap().track_token.clone();
    app.on_key(key(KeyCode::Char('n')));
    assert_eq!(app.now.as_ref().unwrap().track_token, now);
}

#[test]
fn the_notifications_toggle_is_saved() {
    let path = crate::config::temp_dir("notifications").join("config.toml");
    std::fs::write(&path, "[ui]\nnotify = true\n").unwrap();
    let mut config = Config::default();
    config.ui.check_updates = false;
    let mut app = App::new(config, Some(path.clone()), &[], false);
    app.prompt = None;
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.settings_cursor = Setting::ALL.iter().position(|(_, s)| *s == Setting::Notify).unwrap();
    assert_eq!(app.setting_value(Setting::Notify), "on");
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.setting_value(Setting::Notify), "off");
    let (saved, _) = Config::load(&path);
    assert!(!saved.ui.notifications);
    assert!(!std::fs::read_to_string(&path).unwrap().contains("notify ="), "the 0.1 name is gone");
}

/// A demo app on the Stations page with the cursor on `pick`'s first match.
fn on_station(config: Config, path: Option<std::path::PathBuf>, pick: fn(&Station) -> bool) -> App {
    let mut app = App::new(config, path, &[], true);
    app.settle(Duration::from_millis(150));
    app.prompt = None;
    app.on_key(shift('S'));
    app.stations_cursor = app.stations.iter().position(pick).expect("a station like that");
    app
}

fn genre(s: &Station) -> bool {
    s.is_locked() && !s.is_shuffle
}

#[test]
fn a_genre_station_that_cant_be_changed_gets_a_popup_and_the_bar_message() {
    {
        let (key_pressed, what, bar) = ('r', "renamed", "doesn't let you rename");
        let mut app = on_station(Config::default(), None, genre);
        assert!(app.config.ui.genre_edit_popup, "on by default");
        app.on_key(key(KeyCode::Char(key_pressed)));
        let notice = app.genre_notice.clone().expect("the popup");
        assert_eq!((notice.what, notice.dont_show), (what, false));
        assert!(app.overlay_open() && app.rename.is_none() && app.editor.is_none());
        assert!(app.status.contains(bar), "the bar says it too: {}", app.status);
        // Other keys do nothing; Esc closes it and the bar keeps the message.
        app.on_key(key(KeyCode::Char('q')));
        assert!(app.genre_notice.is_some() && !app.quit_asked(), "the popup eats other keys");
        app.on_key(key(KeyCode::Esc));
        assert!(app.genre_notice.is_none() && app.status.contains(bar), "{}", app.status);
    }
    // `e` opens its thumbs instead (no popup).
    let mut app = on_station(Config::default(), None, genre);
    app.on_key(key(KeyCode::Char('e')));
    assert!(app.genre_notice.is_none() && app.editor.as_ref().is_some_and(|e| e.thumbs_only));
    // Deleting a genre station Pandora does allow deleting asks as before.
    let mut app = on_station(Config::default(), None, genre);
    app.on_key(key(KeyCode::Char('x')));
    assert!(app.confirm_delete().is_some() && app.genre_notice.is_none());
}

#[test]
fn only_genre_stations_get_the_popup() {
    // Shuffle: the bar only. Your own station: it just opens.
    let mut app = on_station(Config::default(), None, |s| s.is_shuffle);
    app.on_key(key(KeyCode::Char('r')));
    assert!(app.genre_notice.is_none() && app.status.contains("doesn't let you rename"), "{}", app.status);
    let mut app = on_station(Config::default(), None, |s| s.allow_rename);
    app.on_key(key(KeyCode::Char('r')));
    assert!(app.genre_notice.is_none() && app.rename.is_some());
    // A favorite that is a genre station too.
    let mut app = on_station(Config::default(), None, genre);
    let st = app.stations[app.stations_cursor].clone();
    app.config.ui.favorites.push(config::Favorite { id: st.id, name: st.name });
    app.focus = Focus::Menu;
    app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Favorite(0)).unwrap();
    app.on_key(key(KeyCode::Char('r')));
    assert!(app.genre_notice.is_some(), "{}", app.status);
}

#[test]
fn with_the_popup_off_only_the_bar_says_it() {
    let mut config = Config::default();
    config.ui.genre_edit_popup = false;
    {
        let key_pressed = 'r';
        let mut app = on_station(config.clone(), None, genre);
        app.on_key(key(KeyCode::Char(key_pressed)));
        assert!(app.genre_notice.is_none() && !app.overlay_open());
        assert!(app.status.contains("doesn't let you"), "{}", app.status);
    }
}

#[test]
fn the_checkbox_turns_the_popup_off_for_good_only_with_enter() {
    let path = crate::config::temp_dir("genre-popup").join("config.toml");
    let mut config = Config::default();
    config.ui.check_updates = false;
    let mut app = on_station(config, Some(path.clone()), genre);
    app.demo = false;
    // Ticked, then Esc: nothing saved, the popup comes back next time.
    app.on_key(key(KeyCode::Char('r')));
    app.on_key(key(KeyCode::Char(' ')));
    assert!(app.genre_notice.as_ref().unwrap().dont_show);
    app.on_key(key(KeyCode::Char('x')));
    assert!(!app.genre_notice.as_ref().unwrap().dont_show, "x toggles too");
    app.on_key(key(KeyCode::Char(' ')));
    app.on_key(key(KeyCode::Esc));
    assert!(app.config.ui.genre_edit_popup, "Esc keeps the setting");
    assert!(!std::fs::read_to_string(&path).unwrap_or_default().contains("genre_edit_popup"));
    app.on_key(key(KeyCode::Char('r')));
    assert!(app.genre_notice.as_ref().is_some_and(|n| !n.dont_show), "back, with the box empty again");
    // Enter without the tick: just closes.
    app.on_key(key(KeyCode::Enter));
    assert!(app.genre_notice.is_none() && app.config.ui.genre_edit_popup);
    // Ticked, then Enter: saved as off, and the bar still has the message.
    app.on_key(key(KeyCode::Char('r')));
    app.on_key(key(KeyCode::Char(' ')));
    app.on_key(key(KeyCode::Enter));
    assert!(app.genre_notice.is_none() && !app.config.ui.genre_edit_popup);
    assert!(app.status.contains("doesn't let you rename"), "{}", app.status);
    assert_eq!(app.setting_value(Setting::GenrePopup), "off");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("[boxvibe]") && text.contains("genre_edit_popup = false"), "{text}");
    assert!(!Config::load(&path).0.ui.genre_edit_popup);
    // And now it stays quiet.
    app.on_key(key(KeyCode::Char('r')));
    assert!(app.genre_notice.is_none());
}

#[test]
fn the_settings_row_toggles_the_popup_and_is_saved() {
    assert!(Config::default().ui.genre_edit_popup);
    let path = crate::config::temp_dir("genre-popup-row").join("config.toml");
    let mut config = Config::default();
    config.ui.check_updates = false;
    let mut app = App::new(config, Some(path.clone()), &[], false);
    app.prompt = None;
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.settings_cursor = Setting::at(Setting::GenrePopup);
    assert_eq!(Setting::GenrePopup.label(), "Genre station edit popup");
    assert_eq!(Setting::ALL[Setting::at(Setting::GenrePopup)].0, "BOXVIBE");
    assert_eq!(app.setting_value(Setting::GenrePopup), "on");
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.setting_value(Setting::GenrePopup), "off");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("[boxvibe]") && text.contains("genre_edit_popup = false"), "{text}");
    assert!(!Config::load(&path).0.ui.genre_edit_popup);
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.setting_value(Setting::GenrePopup), "on");
    assert!(Config::load(&path).0.ui.genre_edit_popup);
    let about = Setting::GenrePopup.about();
    assert!(about.contains("genre_edit_popup") && about.contains("Saved"), "{about}");
}

#[test]
fn the_genre_popup_setting_loads_with_a_default_and_tolerates_junk() {
    let (c, w) = Config::parse("").unwrap();
    assert!(c.ui.genre_edit_popup && w.is_empty(), "missing: on");
    let (c, w) = Config::parse("[boxvibe]\ncheck_updates = false\n").unwrap();
    assert!(c.ui.genre_edit_popup && !c.ui.check_updates && w.is_empty(), "an older file");
    let (c, _) = Config::parse("[boxvibe]\ngenre_edit_popup = false\n").unwrap();
    assert!(!c.ui.genre_edit_popup);
    let (c, w) = Config::parse("[boxvibe]\ngenre_edit_popup = \"maybe\"\ncheck_updates = false\n").unwrap();
    assert!(c.ui.genre_edit_popup && !c.ui.check_updates, "a bad value keeps the default, the rest loads");
    assert!(!w.is_empty());
}

#[test]
fn good_vibes_only_steps_on_muted_off_asking_first_and_is_saved() {
    assert_eq!(Config::default().player.good_vibes_only, GoodVibes::On);
    let path = crate::config::temp_dir("good-guy").join("config.toml");
    let mut config = Config::default();
    config.ui.check_updates = false;
    let mut app = App::new(config, Some(path.clone()), &[], false);
    app.prompt = None;
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.settings_cursor = Setting::at(Setting::GoodVibesOnly);
    assert_eq!(Setting::GoodVibesOnly.label(), "Good Vibes Only");
    assert_eq!(app.setting_value(Setting::GoodVibesOnly), "on");
    // Muting asks first; nothing changes or is saved until Enter.
    app.on_key(key(KeyCode::Enter));
    assert!(app.confirm_good_vibes == Some(GoodVibes::Muted) && app.overlay_open());
    assert_eq!(app.setting_value(Setting::GoodVibesOnly), "on");
    assert!(!std::fs::read_to_string(&path).unwrap_or_default().contains("good_vibes_only"));
    app.on_key(key(KeyCode::Enter));
    assert!(app.confirm_good_vibes.is_none());
    assert_eq!(app.setting_value(Setting::GoodVibesOnly), "muted");
    assert!(std::fs::read_to_string(&path).unwrap().contains("good_vibes_only = \"muted\""));
    assert_eq!(Config::load(&path).0.player.good_vibes_only, GoodVibes::Muted);
    // Off asks too.
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.confirm_good_vibes, Some(GoodVibes::Off));
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.setting_value(Setting::GoodVibesOnly), "off");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("[playback]") && text.contains("good_vibes_only = false"), "{text}");
    assert_eq!(Config::load(&path).0.player.good_vibes_only, GoodVibes::Off);
    assert!(
        Setting::GoodVibesOnly.about().contains("good_vibes_only")
            && Setting::GoodVibesOnly.about().contains("Saved")
            && Setting::GoodVibesOnly.about().contains("Muted")
    );
    // Turning it back on needs no question and is saved at once.
    app.on_key(key(KeyCode::Enter));
    assert!(app.confirm_good_vibes.is_none(), "no popup for turning it on");
    assert_eq!(app.setting_value(Setting::GoodVibesOnly), "on");
    assert!(std::fs::read_to_string(&path).unwrap().contains("good_vibes_only = true"));
    assert_eq!(Config::load(&path).0.player.good_vibes_only, GoodVibes::On);
}

#[test]
fn esc_in_the_good_vibes_popup_keeps_it_on_and_other_keys_do_nothing() {
    let path = crate::config::temp_dir("good-vibes-esc").join("config.toml");
    let mut config = Config::default();
    config.ui.check_updates = false;
    let mut app = App::new(config, Some(path.clone()), &[], false);
    app.prompt = None;
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.settings_cursor = Setting::at(Setting::GoodVibesOnly);
    app.on_key(key(KeyCode::Enter));
    assert!(app.confirm_good_vibes.is_some());
    for code in [KeyCode::Char('y'), KeyCode::Char(' '), KeyCode::Down, KeyCode::Char('q')] {
        app.on_key(key(code));
        assert!(app.confirm_good_vibes.is_some() && app.config.player.good_vibes_only == GoodVibes::On, "{code:?}");
    }
    app.on_key(key(KeyCode::Esc));
    assert!(app.confirm_good_vibes.is_none() && app.config.player.good_vibes_only == GoodVibes::On);
    assert_eq!(app.setting_value(Setting::GoodVibesOnly), "on");
    assert!(!std::fs::read_to_string(&path).unwrap_or_default().contains("good_vibes_only"), "nothing saved");
}

/// Skips until the demo's third answer, a pod of two ads.
fn skip_into_an_ad(app: &mut App) {
    for _ in 0..3 {
        app.on_key(shift('N'));
        app.settle(Duration::from_millis(300));
    }
}

#[test]
fn muted_ads_play_silently_and_the_volume_comes_back_even_when_changed_meanwhile() {
    let mut app = ad_demo();
    app.config.player.good_vibes_only = GoodVibes::Muted;
    let before = app.audio.level();
    assert!(before > 0.0);
    skip_into_an_ad(&mut app);
    assert!(app.ad_view().is_some(), "Muted still plays the ad");
    assert!(app.audio.is_muted() && app.audio.level() == 0.0);
    // A volume change during the ad is what comes back.
    app.change_volume(-20);
    assert!(app.audio.level() == 0.0 && app.config.player.volume == 50);
    app.audio.seek_silent(Duration::from_secs(7));
    app.tick();
    assert!(app.ad_view().is_some() && app.audio.is_muted(), "the second ad is muted too");
    app.audio.seek_silent(Duration::from_secs(5));
    app.tick();
    app.settle(Duration::from_millis(300));
    assert!(app.now.is_some() && !app.audio.is_muted());
    assert!(app.audio.level() > 0.0 && app.audio.level() < before);
}

#[test]
fn an_ad_pod_plays_in_now_playing_without_touching_history_and_the_music_comes_back() {
    let mut app = ad_demo();
    let songs = app.history.len();
    app.on_key(shift('N'));
    app.settle(Duration::from_millis(300));
    assert!(app.now.is_some() && !app.ad_break(), "no ad is due the first time");
    app.on_key(shift('N'));
    app.settle(Duration::from_millis(300));
    app.on_key(shift('N'));
    assert!(app.now.is_none() && app.ad_break(), "asking: the next song waits");
    app.settle(Duration::from_millis(300));
    let view = app.ad_view().expect("an ad is playing");
    assert_eq!((view.number, view.count, view.length), (1, 2, 6));
    assert_eq!(app.history.len(), songs + 3, "the three skipped songs, no ad");
    // No skip, no thumbs; pause works.
    let history = app.history.len();
    app.on_key(shift('N'));
    assert!(app.status.contains("can't be skipped") && app.ad_view().unwrap().number == 1, "{}", app.status);
    app.on_key(shift('U'));
    assert!(app.status.contains("can't be rated"), "{}", app.status);
    app.on_key(shift('D'));
    assert!(app.ad_break() && app.history.len() == history);
    app.on_key(key(KeyCode::Char(' ')));
    assert!(app.ad_view().unwrap().paused);
    app.on_key(key(KeyCode::Char(' ')));
    assert!(!app.ad_view().unwrap().paused);
    // The first ad ends, then the second, then the music.
    app.audio.seek_silent(Duration::from_secs(7));
    app.tick();
    let view = app.ad_view().expect("the second ad");
    assert_eq!((view.number, view.length), (2, 4));
    app.audio.seek_silent(Duration::from_secs(5));
    app.tick();
    assert!(!app.ad_break() && app.ad_view().is_none());
    app.settle(Duration::from_millis(300));
    assert!(app.now.is_some(), "the next song plays after the ads");
    assert_eq!(app.history.len(), history, "ads are not added to History");
}

#[test]
fn with_good_vibes_only_off_no_ad_is_asked_for_and_the_next_song_starts_at_once() {
    let mut config = Config::default();
    config.player.good_vibes_only = GoodVibes::Off;
    let mut app = App::new(config, None, &[], true);
    app.settle(Duration::from_millis(300));
    let first = app.now.as_ref().unwrap().track_token.clone();
    for _ in 0..4 {
        app.on_key(shift('N'));
        assert!(!app.ad_break(), "no waiting for an answer");
        app.settle(Duration::from_millis(300));
        assert!(app.now.is_some());
    }
    assert_ne!(app.now.as_ref().unwrap().track_token, first);
    app.settle(Duration::from_millis(300));
    assert!(!app.ad_break(), "no pod ever came");
}

#[test]
fn an_ad_question_that_fails_or_never_answers_goes_on_to_the_next_song() {
    let mut app = ad_demo();
    app.end_song(End::Skipped);
    assert!(matches!(app.ad, AdPhase::Asking(_)) && app.now.is_none());
    app.on_reply(Reply::Failed { what: Failed::Ads, message: "boom".into() });
    assert!(!app.ad_break() && app.now.is_some(), "{}", app.status);
    assert!(app.status.starts_with("Ads:") && app.status.contains("playing on"), "{}", app.status);
    // No answer at all.
    app.end_song(End::Played);
    app.ad = AdPhase::Asking(Instant::now() - Duration::from_secs(9));
    app.tick();
    assert!(!app.ad_break() && app.now.is_some());
    // The audio of an ad can't be fetched.
    app.demo = false;
    app.ad_list_arrived(&app.current_station.clone().unwrap(), crate::demo::ads(3));
    // (not asking, so ignored)
    assert!(!app.ad_break());
    app.ad = AdPhase::Asking(Instant::now());
    app.ad_list_arrived(&app.current_station.clone().unwrap(), crate::demo::ads(3));
    assert!(app.ad_view().is_some_and(|v| v.loading));
    app.on_reply(Reply::AudioFailed { track_token: format!("ad:{}:0", app.ad_serial), message: "gone".into() });
    assert!(!app.ad_break() && app.status.contains("playing on"), "{}", app.status);
}

#[test]
fn songs_arriving_during_an_ad_wait_and_changing_station_ends_the_ad() {
    let mut app = ad_demo();
    skip_into_an_ad(&mut app);
    assert!(app.ad_view().is_some());
    let station = app.current_station.clone().unwrap();
    app.on_reply(Reply::Playlist { station_id: station, songs: crate::demo::playlist("x", 9) });
    assert!(app.now.is_none() && app.ad_view().is_some(), "the music waits for the ad");
    app.tune(1);
    assert!(!app.ad_break());
}

#[test]
fn border_style_and_menu_width_are_saved() {
    let path = crate::config::temp_dir("borders").join("config.toml");
    let mut config = Config::default();
    config.ui.check_updates = false;
    let mut app = App::new(config, Some(path.clone()), &[], false);
    app.prompt = None;
    app.view = View::Settings;
    app.focus = Focus::Content;
    let at = |s: Setting| Setting::ALL.iter().position(|(_, x)| *x == s).unwrap();
    app.settings_cursor = at(Setting::BorderStyle);
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.setting_value(Setting::BorderStyle), "Rounded");
    app.settings_cursor = at(Setting::MenuWidth);
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.setting_value(Setting::MenuWidth), "32 columns");
    let (saved, w) = Config::load(&path);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!((saved.ui.border_style, saved.ui.menu_width), (crate::config::BorderStyle::Rounded, 32));
}

#[test]
fn braces_resize_the_menu_and_save_it() {
    let path = crate::config::temp_dir("resize").join("config.toml");
    let mut config = Config::default();
    config.ui.check_updates = false;
    let mut app = App::new(config, Some(path.clone()), &[], false);
    app.prompt = None;
    app.on_key(key(KeyCode::Char('}')));
    assert_eq!(app.config.ui.menu_width, 32);
    for _ in 0..20 {
        app.on_key(key(KeyCode::Char('{')));
    }
    assert_eq!(app.config.ui.menu_width, 20, "stops at the narrowest");
    assert_eq!(Config::load(&path).0.ui.menu_width, 20);
}

#[test]
fn the_autoplay_toggle_is_saved() {
    let path = crate::config::temp_dir("autoplay").join("config.toml");
    std::fs::write(&path, "[player]\nautoplay = true\nvolume = 50\n").unwrap();
    let mut config = Config::default();
    config.ui.check_updates = false;
    let mut app = App::new(config, Some(path.clone()), &[], false);
    app.prompt = None;
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.settings_cursor = Setting::ALL.iter().position(|(_, s)| *s == Setting::Autoplay).unwrap();
    assert_eq!(app.setting_value(Setting::Autoplay), "on");
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.setting_value(Setting::Autoplay), "off");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("autoplay_last_station = false") && !text.contains("\nautoplay ="), "{text}");
    assert!(!Config::load(&path).0.ui.autoplay_last_station);
}

#[test]
fn autoplay_at_startup_lands_like_shift_n() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.settle(Duration::from_millis(150));
    assert!(app.current_station.is_some());
    assert_eq!(
        (app.view, app.focus, app.pane, app.menu_selected()),
        (View::Player, Focus::Content, Pane::NowPlaying, MenuItem::NowPlaying)
    );
}

#[test]
fn autoplay_off_waits_for_a_station() {
    let mut config = Config::default();
    config.ui.autoplay_last_station = false;
    let mut app = App::new(config, None, &[], true);
    app.settle(Duration::from_millis(150));
    assert!(!app.stations.is_empty() && app.current_station.is_none() && app.now.is_none());
}

#[test]
fn update_now_says_why_it_cannot() {
    let mut app = demo();
    app.update = Some(Update { version: "9.9.9".into(), how: "cargo build".into(), replace: Replace::FromSource });
    assert!(!app.update_button_ready());
    app.view = View::About;
    app.focus = Focus::Content;
    app.on_key(key(KeyCode::Enter));
    assert!(app.status.starts_with("Built from source"), "{}", app.status);
    assert_eq!(app.self_update, Progress::Idle);
}

#[test]
fn restart_now_after_the_update_is_in() {
    let mut app = demo();
    let ready = Replace::Ready("https://example.com/boxvibe-x86_64-linux.tar.gz".into());
    app.update = Some(Update { version: "9.9.9".into(), how: String::new(), replace: ready });
    app.view = View::About;
    app.focus = Focus::Content;
    app.self_update = Progress::Downloading(40);
    app.on_key(key(KeyCode::Enter));
    assert!(!app.quit, "pressing again while it downloads does nothing");
    app.on_reply(Reply::SelfUpdated(Ok("9.9.9".into())));
    assert!(app.status.contains("9.9.9 installed · Restart Now"), "{}", app.status);
    assert!(app.update_button_ready() && app.restart_ready());
    app.on_key(key(KeyCode::Enter));
    assert!(app.restart && app.quit, "Enter on Restart Now quits so main starts the new one");
}

#[test]
fn shift_keys_open_stations_settings_about_and_favorites() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.on_key(shift('S'));
    assert_eq!((app.view, app.focus), (View::Stations, Focus::Content), "S is Stations");
    app.on_key(shift('O'));
    assert_eq!((app.view, app.focus, app.menu_selected()), (View::Settings, Focus::Content, MenuItem::Settings));
    app.on_key(shift('A'));
    assert_eq!((app.view, app.focus, app.menu_selected()), (View::About, Focus::Content, MenuItem::About));
    app.config.ui.favorites.clear();
    app.on_key(shift('F'));
    assert!(app.status.contains("No favorites"), "{}", app.status);
    assert_eq!(app.focus, Focus::Content);
    // Starred but logged out: the section is not there, and F says why.
    app.config.ui.favorites.push(crate::config::Favorite { id: "1".into(), name: "One".into() });
    app.session = Session::LoggedOut;
    app.on_key(shift('F'));
    assert!(app.status.contains("Log in"), "{}", app.status);
    assert_eq!(app.focus, Focus::Content);
    // Signed in, before the list arrives: the section is back with a
    // loading row, and F lands on it.
    app.session = Session::LoggedIn;
    app.on_key(shift('F'));
    assert_eq!((app.focus, app.menu_selected()), (Focus::Menu, MenuItem::Favorite(0)));
    // With the list in, F still lands on the first favorite. A favorite
    // whose station Pandora no longer has is pruned as the list arrives, so
    // this one names a station the demo really has.
    app.demo_warm_up(0, Duration::ZERO);
    let real = app.stations.first().map(|s| (s.id.clone(), s.name.clone())).unwrap();
    app.config.ui.favorites = vec![crate::config::Favorite { id: real.0, name: real.1 }];
    app.on_key(shift('F'));
    assert_eq!((app.focus, app.menu_selected()), (Focus::Menu, MenuItem::Favorite(0)));
    let theme = app.config.ui.theme().to_owned();
    app.on_key(key(KeyCode::Char('t')));
    assert_eq!(app.config.ui.theme(), theme, "t no longer switches the theme: only Settings does");
}

#[test]
fn shift_o_opens_settings_and_shift_t_is_free() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.on_key(shift('O'));
    assert_eq!((app.view, app.focus, app.menu_selected()), (View::Settings, Focus::Content, MenuItem::Settings));
    app.on_key(key(KeyCode::Esc));
    app.on_key(shift('P'));
    let before = (app.view, app.focus);
    app.on_key(shift('T'));
    assert_eq!((app.view, app.focus), before, "Shift+T is unbound now");
    assert!(!KEYMAP.iter().any(|b| b.code == KeyCode::Char('T')));
}

#[test]
fn shift_i_pins_the_playing_or_selected_song_and_x_unpins_after_asking() {
    let mut app = demo();
    let dir = crate::config::temp_dir("pins-app");
    app.pins_path = Some(dir.join("pinned.toml"));
    let playing = app.now.clone().unwrap();
    app.on_key(shift('I'));
    assert_eq!(app.pins.len(), 1);
    assert_eq!(
        (app.pins[0].title.as_str(), app.pins[0].artist.as_str()),
        (playing.title.as_str(), playing.artist.as_str())
    );
    assert_eq!(app.pins[0].link, playing.detail_url);
    assert_eq!(app.pins[0].pinned.len(), 10);
    app.on_key(shift('I'));
    assert_eq!(app.pins.len(), 1, "already pinned: {}", app.status);
    // The selected Up Next row, when Up Next has the keys.
    app.view = View::Player;
    app.focus = Focus::Content;
    app.pane = Pane::UpNext;
    let next = app.queue[app.next_index()].clone();
    app.on_key(shift('I'));
    assert_eq!(app.pins[0].title, next.title, "newest first");
    let on_disk = crate::pinned::load(app.pins_path.as_ref().unwrap());
    assert_eq!(on_disk, app.pins, "kept on disk, not in the config");
    // The menu's Pinned entry opens the page; x asks, n keeps, y unpins.
    app.focus = Focus::Menu;
    app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Pinned).unwrap();
    app.on_key(key(KeyCode::Enter));
    assert_eq!((app.view, app.focus), (View::Pinned, Focus::Content));
    app.on_key(key(KeyCode::Char('x')));
    assert_eq!(app.confirm_unpin(), Some(0));
    app.on_key(key(KeyCode::Char('n')));
    assert_eq!(app.pins.len(), 2);
    app.on_key(key(KeyCode::Char('j')));
    app.on_key(key(KeyCode::Char('x')));
    app.on_key(key(KeyCode::Char('y')));
    assert_eq!(app.pins.len(), 1);
    assert_eq!(app.pins[0].title, next.title);
    assert_eq!(crate::pinned::load(app.pins_path.as_ref().unwrap()).len(), 1);
    app.on_key(key(KeyCode::Enter));
    assert!(app.status.contains("no page"), "the demo songs have no page: {}", app.status);
    app.pins[0].link = "https://www.pandora.com/artist/x".into();
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.status, "Opening https://www.pandora.com/artist/x", "the demo opens nothing");
}

#[test]
fn the_settings_hide_favorites_and_pinned_from_the_menu() {
    let mut app = saving_app("menu-entries");
    // The section needs the stations in: a favorite is only an id and a
    // name until the list says what kind of station it is.
    app.stations = crate::demo::stations();
    app.config.ui.favorites.push(crate::config::Favorite { id: "x".into(), name: "X".into() });
    assert!(app.menu_items().contains(&MenuItem::Pinned) && app.menu_items().contains(&MenuItem::Favorite(0)));
    app.view = View::Settings;
    app.focus = Focus::Content;
    for (setting, key_name) in
        [(Setting::FavoritesInMenu, "favorites_in_menu"), (Setting::PinnedInMenu, "pinned_in_menu")]
    {
        app.settings_cursor = Setting::at(setting);
        assert_eq!(app.setting_value(setting), "on");
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.setting_value(setting), "off");
        assert!(saved(&app).contains(&format!("{key_name} = false")), "{}", saved(&app));
    }
    let items = app.menu_items();
    assert!(!items.contains(&MenuItem::Pinned) && !items.iter().any(|m| matches!(m, MenuItem::Favorite(_))));
    assert_eq!(app.config.ui.favorites.len(), 1, "hidden, not dropped");
    app.on_key(shift('F'));
    assert!(app.status.contains("hidden"), "{}", app.status);
}

fn saving_app(name: &str) -> App {
    let mut app = App::new(Config::default(), None, &[], true);
    app.demo = false;
    app.config_path = Some(crate::config::temp_dir(name).join("config.toml"));
    app
}

fn saved(app: &App) -> String {
    std::fs::read_to_string(app.config_path.as_ref().unwrap()).unwrap_or_default()
}

// --- The Pandora password lives in the keychain ---

use crate::keychain::MemoryKeychain;

const MAIL: &str = "me@example.com";
const WAIT: Duration = Duration::from_millis(300);

/// An app as `App::new` makes it for a real account (its backend answers like the
/// demo, so no network), started from `config.toml` text and a fake keychain.
fn account_app(name: &str, file: &str, keychain: &Arc<MemoryKeychain>) -> App {
    let path = crate::config::temp_dir(name).join("config.toml");
    std::fs::write(&path, file).unwrap();
    let (config, _) = Config::load(&path);
    let mut app = App::with_keychain(config, Some(path), &[], true, Arc::clone(keychain) as Arc<dyn Keychain>);
    app.settle(WAIT);
    app.demo = false;
    app.session = Session::LoggedOut;
    app.status.clear();
    app.start_account();
    app.settle(WAIT);
    app
}

fn enter_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
    app.on_key(key(KeyCode::Enter));
    app.settle(WAIT);
}

#[test]
fn a_password_from_an_older_config_moves_to_the_keychain_and_leaves_the_file() {
    let k = MemoryKeychain::new();
    let app = account_app(
        "kc-legacy",
        "# mine\n[account]\nusername = \"me@example.com\" # who\npassword = \"hunter2\"\n[look]\nicons = \"emoji\"\n",
        &k,
    );
    assert_eq!(app.session, Session::LoggedIn, "it logged in with the password from the file");
    assert_eq!(k.saved(MAIL).as_deref(), Some("hunter2"));
    let text = saved(&app);
    assert!(!text.contains("hunter2") && !text.contains("password"), "{text}");
    assert!(text.contains("# mine") && text.contains("# who") && text.contains("emoji"), "{text}");
    assert!(text.contains("email_or_username = \"me@example.com\" # who") && !text.contains("\nusername ="), "{text}");
    assert!(app.status.contains("moved from config.toml to the keychain"), "{}", app.status);
    assert_eq!(app.keychain_state, KeychainState::Saved);
    assert_eq!(app.setting_value(Setting::Password), "•••••••• in your keychain");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(app.config_path.as_ref().unwrap()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn a_wrong_old_password_is_not_moved_and_leaves_the_file_too() {
    let k = MemoryKeychain::new();
    let mut app = account_app("kc-legacy-wrong", "[pandora]\nusername = \"me@example.com\"\npassword = \"old\"\n", &k);
    // (the first login above was the demo's yes; now Pandora says no)
    k.set(MAIL, "old").unwrap();
    app.on_reply(Reply::LoginFailed { message: "Wrong password".into(), wrong_password: true });
    app.settle(WAIT);
    assert_eq!(k.saved(MAIL), None, "a wrong password is not kept");
    assert!(app.config.pandora.password.is_empty());
    assert!(!saved(&app).contains("password"), "{}", saved(&app));
    assert_eq!(app.setting(), Setting::LogIn, "the cursor waits on Log in, which asks again");
}

#[test]
fn with_no_keychain_the_old_password_stays_in_the_file_and_in_memory() {
    let k = MemoryKeychain::missing();
    let file = "[account]\nemail_or_username = \"me@example.com\"\npassword = \"hunter2\"\n";
    let app = account_app("kc-legacy-missing", file, &k);
    assert_eq!(app.session, Session::LoggedIn);
    assert_eq!(app.status, NO_KEYCHAIN);
    assert!(saved(&app).contains("hunter2"), "not lost: the keychain never had it");
    assert_eq!(app.keychain_state, KeychainState::Unavailable);
}

#[test]
fn the_usual_start_logs_in_with_the_keychain_and_the_file_never_has_a_password() {
    let k = MemoryKeychain::new();
    k.set(MAIL, "hunter2").unwrap();
    let file = "[account]\nemail_or_username = \"me@example.com\"\n";
    let app = account_app("kc-usual", file, &k);
    assert_eq!(app.session, Session::LoggedIn);
    assert_eq!(app.config.pandora.password.expose(), "hunter2");
    assert_eq!(saved(&app), file, "nothing was written");
    assert!(app.status.is_empty(), "{}", app.status);
    assert!(!format!("{:?}", app.config).contains("hunter2"));
}

#[test]
fn no_saved_password_asks_for_it_then_keeps_it_in_the_keychain_after_pandora_accepts() {
    let k = MemoryKeychain::new();
    let mut app = account_app("kc-ask", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
    assert_eq!(app.prompt, Some(Prompt::Password), "asked, like a login form");
    assert_eq!(app.keychain_state, KeychainState::NotSaved);
    assert_eq!(app.session, Session::LoggedOut);
    enter_text(&mut app, "hunter2");
    assert_eq!(app.session, Session::LoggedIn);
    assert_eq!(k.saved(MAIL).as_deref(), Some("hunter2"));
    assert!(!saved(&app).contains("password") && !saved(&app).contains("hunter2"), "{}", saved(&app));
    assert!(saved(&app).contains(MAIL));
    assert_eq!(app.setting_value(Setting::Password), "•••••••• in your keychain");
}

#[test]
fn a_password_pandora_never_accepted_is_never_saved() {
    let k = MemoryKeychain::new();
    let mut app = account_app("kc-never", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
    app.on_key(key(KeyCode::Char('x')));
    app.on_key(key(KeyCode::Enter));
    // Typed, but the answer is a no before the demo worker's yes arrives.
    assert!(app.password_unsaved);
    app.on_reply(Reply::LoginFailed { message: "no".into(), wrong_password: true });
    app.settle(WAIT);
    assert_eq!(k.saved(MAIL), None);
    assert!(!app.password_unsaved && app.config.pandora.password.is_empty());
}

#[test]
fn no_keychain_means_a_session_password_a_clear_message_and_nothing_on_disk() {
    let k = MemoryKeychain::missing();
    let file = "[account]\nemail_or_username = \"me@example.com\"\n";
    let mut app = account_app("kc-missing", file, &k);
    assert_eq!(app.prompt, Some(Prompt::Password));
    assert_eq!(app.status, NO_KEYCHAIN, "told at start");
    assert_eq!(app.keychain_state, KeychainState::Unavailable);
    enter_text(&mut app, "hunter2");
    assert_eq!(app.session, Session::LoggedIn, "it works this session");
    assert_eq!(app.config.pandora.password.expose(), "hunter2", "kept in memory");
    assert_eq!(app.status, NO_KEYCHAIN);
    assert_eq!(saved(&app), file, "and never written");
    assert_eq!(app.setting_value(Setting::Password), "no keychain: asked each start");
    // Next start: asked again.
    let again = account_app("kc-missing", file, &k);
    assert!(again.config.pandora.password.is_empty() && again.prompt == Some(Prompt::Password));
}

#[test]
fn forgetting_the_saved_password_empties_the_keychain_and_says_so() {
    let k = MemoryKeychain::new();
    k.set(MAIL, "hunter2").unwrap();
    let mut app = account_app("kc-forget", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.settings_cursor = Setting::at(Setting::Password);
    app.on_key(key(KeyCode::Enter));
    app.settle(WAIT);
    assert_eq!(k.saved(MAIL), None);
    assert_eq!(app.status, "Forgot the saved password: you'll be asked for it next start");
    assert_eq!(app.setting_value(Setting::Password), "not saved");
    assert!(app.prompt.is_none(), "Enter forgets, it doesn't ask");
    // The next start asks.
    let again = account_app("kc-forget", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
    assert_eq!(again.prompt, Some(Prompt::Password));
}

#[test]
fn forgetting_also_removes_a_password_still_in_the_file() {
    let k = MemoryKeychain::missing();
    let mut app = account_app(
        "kc-forget-file",
        "[account]\nemail_or_username = \"me@example.com\"\npassword = \"hunter2\"\n",
        &k,
    );
    app.settings_cursor = Setting::at(Setting::Password);
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.on_key(key(KeyCode::Enter));
    assert!(!saved(&app).contains("hunter2"), "{}", saved(&app));
}

#[test]
fn logging_out_removes_the_password_from_the_keychain() {
    let k = MemoryKeychain::new();
    k.set(MAIL, "hunter2").unwrap();
    let mut app = account_app("kc-logout", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
    assert_eq!(app.session, Session::LoggedIn);
    app.log_out();
    app.settle(WAIT);
    assert_eq!(k.saved(MAIL), None);
    assert!(app.config.pandora.password.is_empty());
    assert_eq!(app.status, "Logged out: the saved password was removed from your keychain");
    // Log in now asks.
    app.log_in();
    assert_eq!(app.prompt, Some(Prompt::Password));
}

#[test]
fn the_status_line_never_names_the_account() {
    // `status` is kept for the Notifications page and is the one string that
    // could reach a log, so it stays free of anything personal. The account is
    // on the Settings row and in the status bar's "Logged In As", which is
    // where it belongs; repeating it here is what put it in a taint chain.
    let k = MemoryKeychain::new();
    k.set(MAIL, "hunter2").unwrap();
    let mut app = account_app("status-private", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
    assert_eq!(app.session, Session::LoggedIn);
    app.log_out();
    app.settle(WAIT);
    assert!(!app.status.contains(MAIL), "logged out: {}", app.status);

    // Asking for the password names no one.
    app.log_in();
    assert_eq!(app.prompt, Some(Prompt::Password));
    assert!(!app.status.contains(MAIL), "asking for the password: {}", app.status);

    // Signing in names no one either.
    enter_text(&mut app, "hunter2");
    assert!(!app.status.contains(MAIL), "signing in: {}", app.status);

    // Nor does anything the Notifications page kept.
    for (_, note) in &app.notes {
        assert!(!note.contains(MAIL), "a note names the account: {note}");
    }
}

#[test]
fn another_email_does_not_borrow_the_old_password() {
    let k = MemoryKeychain::new();
    k.set(MAIL, "hunter2").unwrap();
    let mut app = account_app("kc-switch", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
    app.view = View::Settings;
    app.open_prompt(Prompt::Email);
    enter_text(&mut app, "other@example.com");
    assert!(app.config.pandora.password.is_empty());
    assert_eq!(app.prompt, Some(Prompt::Password), "asks for the other account's password");
}

#[test]
fn editing_config_toml_keeps_the_password_in_use() {
    let k = MemoryKeychain::new();
    k.set(MAIL, "hunter2").unwrap();
    let mut app = account_app("kc-reload", "[account]\nemail_or_username = \"me@example.com\"\n", &k);
    std::fs::write(
        app.config_path.as_ref().unwrap(),
        "[account]\nemail_or_username = \"me@example.com\"\n[look]\nicons = \"emoji\"\n",
    )
    .unwrap();
    app.reload_config();
    app.settle(WAIT);
    assert_eq!(app.config.pandora.password.expose(), "hunter2");
    assert_eq!(app.session, Session::LoggedIn, "no new login for an unrelated edit");
}

#[test]
fn the_demo_never_touches_the_keychain() {
    let k = MemoryKeychain::new();
    let mut app = App::with_keychain(Config::default(), None, &[], true, Arc::clone(&k) as Arc<dyn Keychain>);
    app.settle(WAIT);
    app.settings_cursor = Setting::at(Setting::Password);
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.on_key(key(KeyCode::Enter));
    app.settle(WAIT);
    assert_eq!(app.setting_value(Setting::Password), "not used in the demo");
    assert_eq!(app.status, "The demo has no password");
    assert!(k.saved("demo").is_none());
}

#[test]
fn every_setting_that_is_saved_says_where_and_the_others_do_not() {
    for (_, setting) in Setting::ALL {
        let about = setting.about();
        match setting.saved_as() {
            Some((table, key)) => {
                assert!(
                    about.contains(&format!("Saved as [{table}] {key}.")),
                    "{setting:?} does not say where it is saved: {about}"
                );
                assert!(config::LAYOUT.iter().any(|(t, k, _)| *t == table && *k == key), "{table}.{key}");
            }
            None => assert!(!about.contains("Saved as"), "{setting:?} is not a saved choice: {about}"),
        }
    }
    // The claim matches the tables the Settings groups are named after.
    for (group, setting) in Setting::ALL {
        if let Some((table, _)) = setting.saved_as() {
            assert_eq!(table, group.to_lowercase(), "{setting:?} is in {group}");
        }
    }
}

#[test]
fn the_stream_label_follows_each_song_as_it_starts() {
    let mut app = demo();
    assert_eq!(app.stream, Some((crate::pandora::AudioFormat::Mp3, 192)), "the first demo song");
    // A song in another format changes it, without touching the setting.
    let mut next = app.queue.front().cloned().expect("a song is queued");
    next.audio_format = crate::pandora::AudioFormat::Aac;
    next.bitrate = 64;
    app.queue.clear();
    app.queue.push_back(next);
    app.advance();
    assert_eq!(app.stream, Some((crate::pandora::AudioFormat::Aac, 64)), "the song now playing");
    assert_eq!(app.config.pandora.audio_quality, "high", "the setting is not what is shown");
}

/// Tests make non-demo apps that play songs. Without the guard in
/// `notify`, each song change put a real notification on the screen.
#[test]
fn a_test_never_puts_a_notification_on_the_screen() {
    let mut app = saving_app("no-notify");
    app.config.ui.notifications = true;
    assert!(!app.demo, "the case that used to notify");
    let song = demo().now.clone().expect("a demo song");
    // The guard is what stops it; the setting alone would not.
    app.notify(&song);
}

#[test]
fn the_quality_row_says_what_is_really_playing() {
    let mut app = demo();
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.settings_cursor = Setting::at(Setting::Quality);

    // A demo song is playing, so the row already says what it is.
    assert!(app.stream.is_some(), "a song is playing");
    assert!(
        app.setting_value(Setting::Quality).ends_with("· playing MP3 192 kbps"),
        "{}",
        app.setting_value(Setting::Quality)
    );

    app.stream = None;

    // Before a login, the choice alone.
    app.hifi = None;
    assert_eq!(app.setting_value(Setting::Quality), "high (MP3 192 kbps if your plan has it)");

    // A free plan has no choice: the row is greyed out and says what
    // Pandora sends, whatever the setting says.
    app.hifi = Some(false);
    assert!(app.quality_greyed());
    assert_eq!(app.setting_value(Setting::Quality), "your plan: 64 kbps HE-AAC");
    app.config.pandora.audio_quality = "standard".into();
    assert_eq!(app.setting_value(Setting::Quality), "your plan: 64 kbps HE-AAC", "the setting is not shown");
    // Once a song arrives, its real bitrate.
    app.stream = Some((crate::pandora::AudioFormat::Aac, 64));
    assert_eq!(app.setting_value(Setting::Quality), "your plan: AAC 64 kbps");

    // Plus / Premium: the choice, and what is playing.
    app.hifi = Some(true);
    assert!(!app.quality_greyed());
    app.config.pandora.audio_quality = "high".into();
    assert_eq!(app.setting_value(Setting::Quality), "high (MP3 192 kbps) · playing AAC 64 kbps");
    app.config.pandora.audio_quality = "standard".into();
    assert_eq!(app.setting_value(Setting::Quality), "standard (MP3 128 kbps) · playing AAC 64 kbps");
    app.stream = Some((crate::pandora::AudioFormat::Mp3, 192));
    assert_eq!(app.setting_value(Setting::Quality), "standard (MP3 128 kbps) · playing MP3 192 kbps");
    // A stream that did not say its bitrate adds nothing.
    app.stream = Some((crate::pandora::AudioFormat::Mp3, 0));
    assert_eq!(app.setting_value(Setting::Quality), "standard (MP3 128 kbps)");
}

#[test]
fn a_free_plan_cannot_toggle_the_quality_and_says_why() {
    let mut app = saving_app("quality-greyed");
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.settings_cursor = Setting::at(Setting::Quality);
    // Not logged in yet: the plan is unknown, so the toggle works.
    app.hifi = None;
    assert!(!app.quality_greyed());
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.config.pandora.audio_quality, "standard", "it toggled");
    // A free plan: Enter explains instead of toggling.
    app.hifi = Some(false);
    assert!(app.quality_greyed());
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.config.pandora.audio_quality, "standard", "nothing changed");
    assert_eq!(app.status, "Audio quality is Pandora's on a free plan: 64 kbps HE-AAC");
    // Plus / Premium: it toggles again.
    app.hifi = Some(true);
    assert!(!app.quality_greyed());
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.config.pandora.audio_quality, "high");
    // Logging out forgets the plan, so the next account is not greyed.
    app.log_out();
    assert_eq!(app.hifi, None);
    assert!(!app.quality_greyed());
    assert_eq!(app.stream, None);
}

#[test]
fn the_details_popup_names_the_bitrate_it_was_told() {
    let song = demo().now.clone().unwrap();
    let details = Details {
        station_name: "Chill".into(),
        station_id: song.station_id.clone(),
        of: DetailsOf::Song(Box::new(crate::pandora::Song {
            bitrate: 64,
            audio_format: crate::pandora::AudioFormat::Aac,
            ..song
        })),
        from_pandora: None,
        why: false,
    };
    let rows = details.rows();
    assert!(rows.iter().any(|(k, v)| k == "Audio" && v == "AAC 64 kbps"), "{rows:?}");
}

#[test]
fn what_a_setting_says_about_where_it_is_saved_is_true() {
    for (_, setting) in Setting::ALL {
        // The account and the volume have their own flows (below).
        let Some((table, name)) = setting.saved_as() else { continue };
        // Theme Select opens the picker; its saving is tested with the picker.
        if matches!(setting, Setting::Email | Setting::Password | Setting::Volume | Setting::ThemeSelect) {
            continue;
        }
        let mut app = saving_app(&format!("saved-{setting:?}"));
        app.view = View::Settings;
        app.focus = Focus::Content;
        app.settings_cursor = Setting::at(setting);
        app.on_key(key(KeyCode::Enter));
        if setting == Setting::GoodVibesOnly {
            // Muting asks first; Enter says yes.
            app.on_key(key(KeyCode::Enter));
        }
        let doc: toml_edit::DocumentMut = saved(&app).parse().unwrap();
        assert!(doc.get(table).and_then(|t| t.get(name)).is_some(), "{setting:?} → [{table}] {name}:\n{doc}");
    }
    // Email: when the prompt is accepted.
    let mut app = saving_app("saved-email");
    app.view = View::Settings;
    app.open_prompt(Prompt::Email);
    for c in "me@example.com".chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
    app.on_key(key(KeyCode::Enter));
    assert!(saved(&app).contains("[account]") && saved(&app).contains("email_or_username = \"me@example.com\""));
    // Volume: Shift+→ anywhere.
    let mut app = saving_app("saved-volume");
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
    assert!(saved(&app).contains("[playback]") && saved(&app).contains("volume ="), "{}", saved(&app));
}

#[test]
fn the_boot_screen_is_two_seconds_long() {
    assert_eq!(SPLASH, Duration::from_secs(2));
}

#[test]
fn the_defaults_are_nerd_icons_and_a_theme_that_exists() {
    let c = Config::default();
    assert_eq!(c.ui.icons, crate::config::IconSet::Nerd);
    assert!(theme::names().any(|n| n == c.ui.theme()));
}

#[test]
fn media_keys_pause_play_and_skip_and_the_desktop_sees_the_song() {
    let mut app = demo();
    let (m, tx, _changed) = crate::mpris::Mpris::detached();
    app.mpris = Some(m);
    app.tick();
    let first = app.now.as_ref().unwrap().clone();
    let shown = app.mpris.as_ref().unwrap().shown();
    assert_eq!((shown.title.as_str(), shown.status()), (first.title.as_str(), "Playing"));
    tx.send(crate::mpris::Control::Play).unwrap();
    app.tick();
    assert!(!app.audio.is_paused(), "Play while playing does nothing");
    tx.send(crate::mpris::Control::PlayPause).unwrap();
    app.tick();
    assert!(app.audio.is_paused());
    assert_eq!(app.mpris.as_ref().unwrap().shown().status(), "Paused");
    tx.send(crate::mpris::Control::Pause).unwrap();
    app.tick();
    assert!(app.audio.is_paused(), "Pause while paused does nothing");
    tx.send(crate::mpris::Control::Play).unwrap();
    tx.send(crate::mpris::Control::Next).unwrap();
    app.tick();
    app.settle(Duration::from_millis(150));
    app.tick();
    assert_ne!(app.now.as_ref().unwrap().track_token, first.track_token);
    assert_eq!(app.mpris.as_ref().unwrap().shown().track, app.now.as_ref().unwrap().track_token);
}

fn wait_for_themes(app: &mut App) {
    for _ in 0..100 {
        app.tick();
        if app.theme_job.is_none() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the theme job never answered");
}

fn type_in(app: &mut App, text: &str) {
    for c in text.chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
}

fn hit_names(app: &App) -> Vec<String> {
    let p = app.theme_picker.as_ref().unwrap();
    p.hits.iter().map(|i| p.catalog[*i].name.clone()).collect()
}

#[test]
fn theme_select_actions_reach_the_app() {
    let mut app = demo();
    app.themes_dir = Some(crate::config::temp_dir("theme-actions"));
    // A concrete theme turns Auto Mode off, so Theme Select is not greyed
    // out on a machine that looks like Omarchy (where the default follows
    // the system theme and Enter would only explain that).
    app.config.ui.set_theme("kanagawa");
    app.focus = Focus::Content;
    app.view = View::Settings;
    app.settings_cursor = Setting::at(Setting::ThemeSelect);
    app.on_key(key(KeyCode::Enter));
    wait_for_themes(&mut app);
    type_in(&mut app, "dracula");
    app.on_key(key(KeyCode::Enter));
    assert!(app.status.starts_with("Downloading Dracula"), "{}", app.status);
    assert!(app.theme_job.is_some());
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.status, "Still downloading the last one…");
    app.on_key(key(KeyCode::Esc));
    assert!(app.theme_picker.is_none(), "Esc closes");
}

#[test]
fn theme_select_searches_like_fzf_downloads_installs_and_deletes_without_closing() {
    let mut app = demo();
    let dir = crate::config::temp_dir("theme-picker");
    app.themes_dir = Some(dir.clone());
    app.config.ui.set_theme("kanagawa");
    app.focus = Focus::Content;
    app.view = View::Settings;
    app.settings_cursor = Setting::at(Setting::ThemeSelect);
    assert!(app.setting_value(Setting::ThemeSelect).ends_with("0 saved ~ Browse for more…"));
    app.on_key(key(KeyCode::Enter));
    let p = app.theme_picker.as_ref().unwrap();
    assert!(p.loading && p.tab == vibekit::picker::ThemeTab::Search);
    wait_for_themes(&mut app);
    // The 3 demo iTerm2 schemes plus the built-in themes; no Omarchy ones
    // (the demo lists two, nord and rose-pine): those come with Auto Mode.
    let built_in = crate::theme::names().filter(|n| *n != "omarchy").count();
    assert_eq!(hit_names(&app).len(), 3 + built_in);
    let p = app.theme_picker.as_ref().unwrap();
    assert!(p.catalog.iter().all(|e| e.source != crate::theme_store::Source::Omarchy));
    assert!(!hit_names(&app).iter().any(|n| n == "Omarchy"));
    // Typing filters at once, no `/`; j and k are letters here.
    type_in(&mut app, "drc");
    assert_eq!(hit_names(&app)[0], "Dracula", "best match first");
    assert!(!hit_names(&app).contains(&"rose-pine".to_string()));
    app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(app.theme_picker.as_ref().unwrap().query, "");
    type_in(&mut app, "jk");
    assert_eq!(app.theme_picker.as_ref().unwrap().query, "jk");
    app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    // A built-in theme applies at once: no download, nothing saved.
    type_in(&mut app, "gruvbox");
    assert_eq!(hit_names(&app)[0], crate::theme::label("gruvbox"));
    app.on_key(key(KeyCode::Enter));
    assert!(app.theme_job.is_none(), "no download");
    assert_eq!(app.config.ui.theme(), "gruvbox");
    assert!(app.user_themes.is_empty() && app.theme_picker.is_some());
    app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    type_in(&mut app, "dracula");
    // Enter downloads and uses it; the popup stays open.
    app.on_key(key(KeyCode::Enter));
    wait_for_themes(&mut app);
    assert_eq!(app.config.ui.theme(), "dracula");
    assert!(dir.join("dracula.toml").is_file());
    assert!(app.theme_picker.is_some(), "still open");
    assert!(!hit_names(&app).contains(&"Dracula".to_string()), "installed ones leave Search");
    assert!(app.setting_value(Setting::ThemeSelect).contains("1 saved ~ Browse for more…"));
    // A file that isn't a theme is refused and nothing changes.
    app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    type_in(&mut app, "broken");
    app.on_key(key(KeyCode::Enter));
    wait_for_themes(&mut app);
    assert!(app.status.contains("Broken"), "{}", app.status);
    assert_eq!(app.config.ui.theme(), "dracula");
    app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    type_in(&mut app, "solarized");
    app.on_key(key(KeyCode::Enter));
    wait_for_themes(&mut app);
    assert_eq!(app.config.ui.theme(), "solarized-dark");
    // Installed: only the downloaded ones; Enter uses one and stays open.
    app.on_key(key(KeyCode::Tab));
    assert_eq!(app.theme_picker.as_ref().unwrap().tab, vibekit::picker::ThemeTab::Installed);
    assert_eq!(app.installed_themes(), ["dracula", "solarized-dark"]);
    app.on_key(key(KeyCode::Home));
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.config.ui.theme(), "dracula");
    app.on_key(key(KeyCode::Char('j')));
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.config.ui.theme(), "solarized-dark");
    assert!(app.theme_picker.is_some());
    // x asks; n keeps it, y deletes and falls back.
    app.on_key(key(KeyCode::Char('k')));
    app.on_key(key(KeyCode::Enter));
    app.on_key(key(KeyCode::Char('x')));
    assert_eq!(app.theme_delete_asked(), Some("dracula"));
    assert!(app.theme_picker.as_ref().unwrap().confirm_delete.is_none());
    app.on_key(key(KeyCode::Char('n')));
    assert!(dir.join("dracula.toml").is_file());
    assert!(app.theme_delete_asked().is_none());
    app.on_key(key(KeyCode::Delete));
    assert_eq!(app.theme_delete_asked(), Some("dracula"));
    app.on_key(key(KeyCode::Char('y')));
    assert!(!dir.join("dracula.toml").exists());
    assert_eq!(app.installed_themes(), ["solarized-dark"]);
    assert_eq!(app.config.ui.theme(), crate::theme::FALLBACK);
    // Deleted, it is back in Search.
    app.on_key(key(KeyCode::Tab));
    app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    type_in(&mut app, "dracula");
    assert_eq!(hit_names(&app)[0], "Dracula");
    // Esc closes.
    app.on_key(key(KeyCode::Esc));
    assert!(app.theme_picker.is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn theme_auto_mode_greys_theme_select_and_both_save() {
    let mut app = saving_app("theme-auto");
    app.config.ui.set_theme("kanagawa");
    app.focus = Focus::Content;
    app.view = View::Settings;
    app.settings_cursor = Setting::at(Setting::ThemeAutoMode);
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.config.ui.theme_auto_mode, crate::config::ThemeAutoMode::Omarchy);
    assert!(app.theme_select_greyed());
    let text = saved(&app);
    assert!(text.contains("theme_auto_mode = \"omarchy\""), "{text}");
    app.settings_cursor = Setting::at(Setting::ThemeSelect);
    app.on_key(key(KeyCode::Enter));
    assert!(app.theme_picker.is_none(), "greyed: no picker");
    app.settings_cursor = Setting::at(Setting::ThemeAutoMode);
    app.on_key(key(KeyCode::Enter));
    assert!(!app.theme_select_greyed());
    assert_eq!(app.config.ui.theme(), "kanagawa", "back to the selected theme");
    assert!(saved(&app).contains("theme_select = \"kanagawa\""));
}

#[test]
fn rank_is_fuzzy_and_best_first() {
    let demo = crate::theme_store::demo_catalog();
    let names = |q: &str| vibekit::picker::rank(&demo, q).into_iter().map(|i| demo[i].name.clone()).collect::<Vec<_>>();
    assert_eq!(names("").len(), demo.len());
    assert_eq!(names("sdk"), ["Solarized Dark"]);
    assert!(names("zzzq").is_empty());
}

#[test]
fn no_key_is_bound_twice_and_the_removed_ones_are_gone() {
    for (i, a) in KEYMAP.iter().enumerate() {
        for b in &KEYMAP[i + 1..] {
            assert!(a.code != b.code, "{:?} is bound twice", a.code);
        }
    }
    for gone in ['+', '-', '=', 'u', 'd', 'n', 's', 't'] {
        let bound = command_for(key(KeyCode::Char(gone)));
        assert!(bound.is_none(), "{gone} should be unbound, is {bound:?}");
    }
    assert_eq!(command_for(shift('U')), Some(Cmd::ThumbUp));
    assert_eq!(command_for(shift('N')), Some(Cmd::Skip));
    assert_eq!(command_for(key(KeyCode::Char('l'))), Some(Cmd::LikePane));
}

#[test]
fn shift_c_r_and_h_cycle_the_cover_art_and_save_it() {
    let mut app = saving_app("cover-keys");
    assert_eq!(app.config.ui.album_art, AlbumArt::Picture);
    app.on_key(shift('C'));
    assert_eq!(app.config.ui.album_art, AlbumArt::Off);
    app.on_key(shift('C'));
    assert_eq!(app.config.ui.album_art, AlbumArt::Picture, "Picture and Off, nothing else");
    let retro = app.config.ui.retro_mode;
    app.on_key(shift('R'));
    assert_eq!(app.config.ui.retro_mode, retro.next());
    let tint = app.config.ui.art_tint;
    app.on_key(shift('H'));
    assert_eq!(app.config.ui.art_tint, tint.next());
    let file = saved(&app);
    assert!(
        file.contains("album_art") && file.contains("retro_mode") && file.contains("tint_art_with_theme_colors"),
        "{file}"
    );
    // The retro levels go all the way round, 64-bit included.
    let mut seen = vec![app.config.ui.retro_mode];
    for _ in 0..crate::config::RetroMode::ALL.len() {
        app.on_key(shift('R'));
        seen.push(app.config.ui.retro_mode);
    }
    assert!(seen.contains(&crate::config::RetroMode::Bit64));
    assert_eq!(seen.first(), seen.last());
}

#[test]
fn shift_z_is_tired_of_this_song_and_lowercase_z_is_nothing() {
    assert_eq!(command_for(key(KeyCode::Char('z'))), None);
    assert_eq!(command_for(shift('Z')), Some(Cmd::Tired));
    // From any pane, on the playing song: it goes to History and the next one starts.
    let mut app = App::new(Config::default(), None, &[], true);
    app.begin_splash();
    app.on_key(key(KeyCode::Char(' ')));
    app.settle(std::time::Duration::from_millis(400));
    let Some(playing) = app.now.clone() else { panic!("demo plays a song") };
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.on_key(key(KeyCode::Char('z')));
    assert_eq!(app.now.as_ref().map(|s| &s.track_token), Some(&playing.track_token), "z does nothing");
    app.on_key(shift('Z'));
    assert_ne!(app.now.as_ref().map(|s| &s.track_token), Some(&playing.track_token), "Z moves on");
    assert!(app.history.iter().any(|s| s.track_token == playing.track_token));
}

#[test]
fn i_g_and_home_end_are_not_keys_any_more() {
    let mut app = saving_app("no-icon-key");
    let icons = app.config.ui.icons;
    for code in [KeyCode::Char('i'), KeyCode::Char('g'), KeyCode::Home, KeyCode::End] {
        assert_eq!(command_for(key(code)), None, "{code:?}");
    }
    app.on_key(key(KeyCode::Char('i')));
    assert_eq!(app.config.ui.icons, icons, "i no longer cycles the icons");
    assert!(saved(&app).is_empty());
    // The icon set is still a setting.
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.settings_cursor = Setting::at(Setting::Icons);
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.config.ui.icons, icons.next());
    assert!(saved(&app).contains("icons"));
}

#[test]
fn any_key_skips_the_splash_and_it_is_used_up() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.begin_splash();
    assert!(app.splash.is_some(), "on by default");
    let view = app.view;
    app.on_key(key(KeyCode::Char('i')));
    assert!(app.splash.is_none());
    assert_eq!(app.view, view);
    let mut config = Config::default();
    config.ui.splash = false;
    let mut off = App::new(config, None, &[], true);
    off.begin_splash();
    assert!(off.splash.is_none(), "the toggle turns it off");
}

#[test]
fn r_renames_a_station_and_the_name_is_asked_for_first() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.settle(Duration::from_millis(150));
    app.on_key(shift('S'));
    let id = app.stations[app.stations_index()].id.clone();
    let old = app.stations[app.stations_index()].name.clone();
    app.on_key(key(KeyCode::Char('r')));
    let rename = app.rename.clone().expect("the rename box opens");
    assert_eq!((rename.station.id.as_str(), rename.text.text()), (id.as_str(), old.as_str()));
    for _ in 0..old.chars().count() {
        app.on_key(key(KeyCode::Backspace));
    }
    for c in "Mine".chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
    app.on_key(key(KeyCode::Enter));
    assert!(app.rename.is_none());
    app.settle(Duration::from_millis(150));
    assert_eq!(app.stations.iter().find(|s| s.id == id).unwrap().name, "Mine");
    // Esc cancels, and nothing is sent.
    app.on_key(key(KeyCode::Char('r')));
    app.on_key(key(KeyCode::Char('X')));
    app.on_key(key(KeyCode::Esc));
    assert!(app.rename.is_none());
    assert_eq!(app.stations.iter().find(|s| s.id == id).unwrap().name, "Mine");
}

#[test]
fn x_asks_then_deletes_only_on_y_and_cleans_favorites() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.settle(Duration::from_millis(150));
    app.on_key(shift('S'));
    let station = app.stations[app.stations_index()].clone();
    app.config.ui.favorites.push(crate::config::Favorite { id: station.id.clone(), name: station.name.clone() });
    let count = app.stations.len();
    app.on_key(key(KeyCode::Char('x')));
    assert_eq!(app.confirm_delete().map(|s| s.id.as_str()), Some(station.id.as_str()));
    app.on_key(key(KeyCode::Char('n')));
    assert!(app.confirm_delete().is_none());
    app.settle(Duration::from_millis(100));
    assert_eq!(app.stations.len(), count, "any key but y keeps it");
    app.on_key(key(KeyCode::Char('x')));
    app.on_key(key(KeyCode::Char('y')));
    app.settle(Duration::from_millis(150));
    assert_eq!(app.stations.len(), count - 1);
    assert!(!app.stations.iter().any(|s| s.id == station.id));
    assert!(!app.is_favorite(&station.id), "its favorite goes too");
}

#[test]
fn r_x_and_v_in_favorites_act_on_that_favorites_station_and_f_removes_the_favorite() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.settle(Duration::from_millis(150));
    let station = app.stations.iter().find(|s| s.allow_rename && s.allow_delete).unwrap().clone();
    app.config.ui.favorites.push(crate::config::Favorite { id: station.id.clone(), name: station.name.clone() });
    app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Favorite(0)).unwrap();
    app.focus = Focus::Menu;
    app.on_key(key(KeyCode::Char('r')));
    let rename = app.rename.take().expect("rename box");
    assert!(rename.station.favorite && rename.station.id == station.id);
    app.on_key(key(KeyCode::Char('x')));
    let delete = app.confirm_delete().cloned().expect("delete box");
    app.on_key(key(KeyCode::Esc));
    assert!(delete.favorite && delete.id == station.id);
    app.on_key(key(KeyCode::Char('v')));
    assert_eq!(app.details().map(|d| d.station_id.as_str()), Some(station.id.as_str()));
    app.on_key(key(KeyCode::Char('q')));
    app.on_key(key(KeyCode::Char('f')));
    assert!(app.details().is_some(), "q and other keys leave details open");
    assert!(app.is_favorite(&station.id), "and do nothing behind it");
    app.on_key(key(KeyCode::Esc));
    assert!(app.details().is_none(), "Esc closes details");
    app.on_key(key(KeyCode::Char('f')));
    assert!(!app.is_favorite(&station.id), "f still removes the favorite");
    assert!(app.stations.iter().any(|s| s.id == station.id), "and leaves the station alone");
}

#[test]
fn a_station_pandora_will_not_delete_or_rename_is_left_alone() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.settle(Duration::from_millis(150));
    app.on_key(shift('S'));
    let i = app.stations.iter().position(|s| !s.allow_delete).expect("the demo has a shuffle station");
    app.stations_cursor = i;
    app.on_key(key(KeyCode::Char('x')));
    assert!(app.confirm_delete().is_none());
    assert!(app.status.contains("doesn't let you delete"), "{}", app.status);
    app.on_key(key(KeyCode::Char('r')));
    assert!(app.rename.is_none());
}

#[test]
fn v_shows_real_station_details_for_a_station_and_for_the_playing_song() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.settle(Duration::from_millis(150));
    app.tune(0);
    app.settle(Duration::from_millis(150));
    app.on_key(key(KeyCode::Char('v')));
    app.settle(Duration::from_millis(150));
    let d = app.details().cloned().expect("details open");
    assert!(matches!(d.of, DetailsOf::Song(_)));
    assert!(matches!(d.from_pandora, Some(Ok(ref s)) if !s.seeds.is_empty()), "{d:?}");
    app.on_key(key(KeyCode::Esc));
    assert!(app.details().is_none());
    app.on_key(shift('S'));
    app.on_key(key(KeyCode::Char('v')));
    app.settle(Duration::from_millis(150));
    assert!(matches!(app.details().unwrap().of, DetailsOf::Station(_)));
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
}

fn open_stations_page() -> App {
    let mut app = demo();
    app.settle(Duration::from_millis(300));
    app.on_key(shift('S'));
    app
}

#[test]
fn a_searches_then_enter_makes_the_station_and_plays_it() {
    let mut app = open_stations_page();
    let before = app.stations.len();
    app.on_key(key(KeyCode::Char('a')));
    assert!(app.add_station.is_some(), "a opens the search box");
    // Letters go into the box, not to j/k/q/a keys.
    type_text(&mut app, "mid");
    assert_eq!(app.add_station.as_ref().unwrap().text.text(), "mid");
    app.on_key(key(KeyCode::Enter));
    assert!(app.add_station.as_ref().unwrap().waiting);
    app.settle(Duration::from_millis(300));
    let search = app.add_station.as_ref().unwrap();
    assert_eq!(search.hits.len(), 1, "{:?}", search.hits);
    assert_eq!(search.hits[0].name, "Midnight City");
    assert_eq!(search.mode, SearchMode::Results, "the answer is ready to pick from");
    app.on_key(key(KeyCode::Enter));
    assert!(app.add_station.is_none());
    app.settle(Duration::from_millis(400));
    assert_eq!(app.stations.len(), before + 1);
    let at = usize::from(app.stations[0].is_shuffle);
    assert_eq!(app.stations[at].name, "Midnight City Radio", "new stations go after Shuffle");
    assert_eq!(app.current_station.as_deref(), Some(app.stations[at].id.as_str()), "and it plays");
    assert_eq!(app.view, View::Player);
    // Making it again does not add a second copy.
    let again = app.stations[at].clone();
    app.station_created(again);
    assert_eq!(app.stations.len(), before + 1);
}

#[test]
fn the_results_move_with_j_k_and_arrows_and_esc_cancels() {
    let mut app = open_stations_page();
    app.on_key(key(KeyCode::Char('a')));
    type_text(&mut app, "m");
    app.on_key(key(KeyCode::Enter));
    app.settle(Duration::from_millis(300));
    let n = app.add_station.as_ref().unwrap().hits.len();
    assert!(n >= 3, "artists and songs both match 'm': {n}");
    use crate::pandora::HitKind;
    let hits = &app.add_station.as_ref().unwrap().hits;
    assert!(
        hits.iter().any(|h| h.kind == HitKind::Artist) && hits.iter().any(|h| h.kind == HitKind::Song),
        "both artists and songs are listed"
    );
    let cursor = |app: &App| app.add_station.as_ref().unwrap().cursor;
    app.on_key(key(KeyCode::Char('j')));
    assert_eq!(cursor(&app), 1);
    app.on_key(key(KeyCode::Down));
    assert_eq!(cursor(&app), 2);
    app.on_key(key(KeyCode::Char('k')));
    app.on_key(key(KeyCode::Up));
    assert_eq!(cursor(&app), 0);
    app.on_key(key(KeyCode::Up));
    assert_eq!(cursor(&app), n - 1, "wraps");
    // / goes back to typing; j is then just a letter.
    app.on_key(key(KeyCode::Char('/')));
    app.on_key(key(KeyCode::Char('j')));
    assert_eq!(app.add_station.as_ref().unwrap().text.text(), "mj");
    let before = app.stations.len();
    app.on_key(key(KeyCode::Esc));
    assert!(app.add_station.is_none());
    app.settle(Duration::from_millis(100));
    assert_eq!(app.stations.len(), before, "Esc adds nothing");
}

#[test]
fn the_add_station_row_does_the_same_as_a() {
    let mut app = open_stations_page();
    app.stations_cursor = 0;
    app.on_key(key(KeyCode::Up));
    assert!(app.on_add_row, "the row sits above the first station");
    app.on_key(key(KeyCode::Char('f')));
    assert!(app.config.ui.favorites.is_empty(), "f does nothing to the row");
    app.on_key(key(KeyCode::Enter));
    assert!(app.add_station.is_some());
    assert!(app.current_station.is_some(), "Enter did not play a station");
}

#[test]
fn a_late_answer_to_an_older_search_is_not_shown_and_errors_are() {
    let mut app = open_stations_page();
    app.on_key(key(KeyCode::Char('a')));
    type_text(&mut app, "kids");
    app.on_key(key(KeyCode::Enter));
    // Change the text before the answer comes.
    app.on_key(key(KeyCode::Backspace));
    app.settle(Duration::from_millis(300));
    let search = app.add_station.as_ref().unwrap();
    assert_eq!(search.mode, SearchMode::Typing, "the text moved on: keep typing");
    // Enter searches the new text.
    app.on_key(key(KeyCode::Enter));
    app.settle(Duration::from_millis(300));
    assert!(!app.add_station.as_ref().unwrap().hits.is_empty());
    app.on_reply(Reply::Failed { what: Failed::Search, message: "Pandora is down".into() });
    let search = app.add_station.as_ref().unwrap();
    assert_eq!((search.waiting, search.error.as_deref()), (false, Some("Pandora is down")));
}

#[test]
fn a_outside_the_station_list_says_where_and_logged_out_asks_to_log_in() {
    let mut app = open_stations_page();
    app.on_key(shift('A'));
    app.on_key(key(KeyCode::Char('a')));
    assert!(app.add_station.is_none() && app.status.contains("Shift+S"), "{}", app.status);
    let mut out = App::new(Config::default(), None, &[], false);
    out.prompt = None;
    out.view = View::Stations;
    out.focus = Focus::Content;
    out.on_key(key(KeyCode::Char('a')));
    assert!(out.add_station.is_none() && out.status.contains("Log in"), "{}", out.status);
}

#[test]
fn a_second_create_is_ignored_while_one_is_on_its_way() {
    let mut app = open_stations_page();
    app.create_station("AR:demo-m83", "M83");
    app.create_station("AR:demo-mgmt", "MGMT");
    assert!(app.status.contains("Still"), "{}", app.status);
    app.settle(Duration::from_millis(300));
    assert!(!app.creating);
    assert_eq!(app.stations.iter().filter(|s| s.name == "M83 Radio").count(), 1);
    assert!(!app.stations.iter().any(|s| s.name == "MGMT Radio"), "the second was dropped");
    app.on_reply(Reply::Failed { what: Failed::CreateStation, message: "no".into() });
    assert!(!app.creating);
}

fn browse_headers(app: &App) -> Vec<&str> {
    app.browse_rows()
        .iter()
        .filter_map(|r| match r {
            BrowseRow::Header(t) => Some(*t),
            _ => None,
        })
        .collect()
}

fn browse_item_count(app: &App) -> usize {
    app.browse_rows().iter().filter(|r| matches!(r, BrowseRow::Item(_))).count()
}

#[test]
fn shift_g_opens_genres_and_a_shelf_reaches_a_station() {
    let mut app = demo();
    app.settle(Duration::from_millis(300));
    app.on_key(shift('G'));
    assert_eq!((app.view, app.focus), (View::Browse, Focus::Content));
    assert_eq!(app.browse_filter.as_deref(), Some("Browse Genres"));
    assert_eq!(app.browse, BrowseState::Loading);
    app.settle(Duration::from_millis(400));
    assert!(matches!(app.browse, BrowseState::Loaded(_)));
    assert_eq!(app.browse_path.as_deref(), Some("/browse/music/genres"));
    assert_eq!(browse_headers(&app), ["Browse Genres"]);
    let shelves = app
        .browse_rows()
        .iter()
        .filter(|r| matches!(r, BrowseRow::Item(i) if i.pandora_id.starts_with("BR:50:")))
        .count();
    assert_eq!(shelves, 4, "named shelves plus one with no page");
    app.browse_cursor = 0;
    app.on_key(key(KeyCode::Enter));
    assert!(app.status.contains("heading"), "{}", app.status);
    move_browse_to(&mut app, "Pop");
    let cursor = app.browse_cursor;
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.browse, BrowseState::Loading);
    app.settle(Duration::from_millis(400));
    assert_eq!(app.browse_path.as_deref(), Some("/browse/music/genres/pop"));
    assert!(app.browse_rows().iter().any(|r| matches!(r, BrowseRow::Item(i) if i.name == "Today's Hits")));
    app.on_key(key(KeyCode::Esc));
    assert_eq!(app.browse_path.as_deref(), Some("/browse/music/genres"));
    assert_eq!(app.browse_cursor, cursor, "Esc returns to the shelf that opened");
    assert_eq!(app.focus, Focus::Content);
    app.on_key(key(KeyCode::Enter));
    app.settle(Duration::from_millis(400));
    let before = app.stations.len();
    move_browse_to(&mut app, "Indie Pop");
    app.on_key(key(KeyCode::Enter));
    app.settle(Duration::from_millis(400));
    assert_eq!(app.stations.len(), before + 1);
    app.on_key(shift('G'));
    app.settle(Duration::from_millis(200));
    move_browse_to(&mut app, "No page");
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.stations.len(), before + 1);
    assert!(app.status.contains("can't start a station"), "{}", app.status);
    app.on_key(shift('S'));
    let loaded = app.browse.clone();
    app.on_key(shift('G'));
    assert_eq!(app.browse, loaded);
    assert!(app.backend.replies.recv_timeout(Duration::from_millis(100)).is_err(), "view-all is cached");
}

#[test]
fn featured_playlists_and_podcasts_drill_down_in_browse() {
    let mut app = demo();
    for (section, shelf, list_path, detail_path, leaf) in [
        (
            "Featured Playlists",
            "The Drop",
            "/browse/music/featured-playlists",
            "/browse/music/featured-playlists/the-drop",
            "Midnight City",
        ),
        ("Recommended Podcasts", "Crime Junkie", "/browse/podcasts", "/browse/podcasts/crime-junkie", "Latest episode"),
    ] {
        app.show_browse_section(section);
        app.settle(Duration::from_millis(400));
        assert_eq!(app.browse_path.as_deref(), Some(list_path));
        move_browse_to(&mut app, shelf);
        app.on_key(key(KeyCode::Enter));
        app.settle(Duration::from_millis(400));
        assert_eq!(app.browse_path.as_deref(), Some(detail_path));
        move_browse_to(&mut app, leaf);
        app.on_key(key(KeyCode::Enter));
        assert!(app.status.contains("browse-only") || app.status.contains("can't"), "{section}: {}", app.status);
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.browse_path.as_deref(), Some(list_path));
    }
}

#[test]
fn decades_and_moods_open_the_same_way_and_a_failed_page_returns() {
    let mut app = demo();
    for (section, shelf, path, station) in [
        ("Decades", "2010s", "/browse/music/decades/2010s", "Classic Rock"),
        ("Moods and activities", "Happy", "/browse/music/activities/happy", "Indie Pop"),
    ] {
        app.show_browse_section(section);
        app.settle(Duration::from_millis(400));
        assert_eq!(
            app.browse_path.as_deref(),
            Some(match section {
                "Decades" => "/browse/music/decades",
                _ => "/browse/music/activities",
            })
        );
        move_browse_to(&mut app, shelf);
        app.on_key(key(KeyCode::Enter));
        app.settle(Duration::from_millis(400));
        assert_eq!(app.browse_path.as_deref(), Some(path));
        app.on_key(key(KeyCode::Esc));
        assert!(
            app.browse_path.as_deref().is_some_and(|p| p == "/browse/music/decades" || p == "/browse/music/activities")
        );
        app.on_key(key(KeyCode::Enter));
        app.settle(Duration::from_millis(400));
        move_browse_to(&mut app, station);
        let before = app.stations.len();
        app.on_key(key(KeyCode::Enter));
        app.settle(Duration::from_millis(400));
        assert_eq!(app.stations.len(), before + 1);
        app.show_browse_section(section);
        app.settle(Duration::from_millis(200));
        move_browse_to(&mut app, "No page");
        app.on_key(key(KeyCode::Enter));
        assert!(app.status.contains("can't start a station"), "{}", app.status);
    }
    app.show_browse_section("Browse Genres");
    app.settle(Duration::from_millis(400));
    move_browse_to(&mut app, "Pop");
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.browse, BrowseState::Loading);
    app.on_reply(Reply::BrowseFailed { path: Some("/browse/music/genres/pop".into()), message: "nope".into() });
    assert_eq!(app.browse_path.as_deref(), Some("/browse/music/genres"));
    assert!(app.status.contains("didn't load") && app.status.contains("nope"), "{}", app.status);
    if let BrowseState::Loaded(sections) = &mut app.browse {
        sections[0].items.push(BrowseItem {
            name: "Empty".into(),
            detail: String::new(),
            pandora_id: "BR:50:empty".into(),
            station_factory: String::new(),
            link: Some("/browse/music/genres/empty".into()),
        });
    }
    move_browse_to(&mut app, "Empty");
    app.on_key(key(KeyCode::Enter));
    app.settle(Duration::from_millis(400));
    assert_eq!(app.browse_path.as_deref(), Some("/browse/music/genres/empty"));
    assert_eq!(browse_item_count(&app), 0);
    assert_eq!(browse_headers(&app), ["Empty"]);
}

fn move_browse_to(app: &mut App, name: &str) {
    let rows = app.browse_rows().len();
    for _ in 0..rows {
        if matches!(app.browse_rows().get(app.browse_index()), Some(BrowseRow::Item(i)) if i.name == name) {
            return;
        }
        app.on_key(key(KeyCode::Char('j')));
    }
    panic!("no browse row {name}");
}

#[test]
fn new_music_pin_shows_every_view_all_section_with_no_cap() {
    let mut app = demo();
    let pins = app.browse_menu_pins();
    let i = pins.iter().position(|(_, t)| t == "New Music").unwrap();
    let at = app.menu_items().iter().position(|m| *m == MenuItem::BrowsePin(i)).unwrap();
    app.focus = Focus::Menu;
    app.menu_cursor = at;
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.browse_filter.as_deref(), Some("New Music"));
    app.settle(Duration::from_millis(400));
    assert_eq!(browse_headers(&app), ["New Music Stations", "Recommended new releases", "All New Albums"]);
    let expected: usize = crate::demo::browse_at(Some("/browse/music/new-music")).iter().map(|s| s.items.len()).sum();
    assert!(expected > 8, "the fixture itself is past the teaser size");
    assert_eq!(browse_item_count(&app), expected, "every View-all row is listed");
    let albums =
        app.browse_rows().iter().filter(|r| matches!(r, BrowseRow::Item(i) if i.name.starts_with("New Album"))).count();
    assert_eq!(albums, 10);
    // Coming back does not fetch again.
    app.on_key(shift('S'));
    app.show_browse_section("New Music");
    assert!(matches!(app.browse, BrowseState::Loaded(_)));
    assert_eq!(browse_headers(&app), ["New Music Stations", "Recommended new releases", "All New Albums"]);
    assert!(app.backend.replies.recv_timeout(Duration::from_millis(80)).is_err(), "cached");
}

#[test]
fn other_view_all_pins_use_their_path_and_artists_stay_a_teaser() {
    let mut app = demo();
    for title in ["Trendsetters: Artists on the rise", "Decades", "Featured Playlists", "Top stations"] {
        app.show_browse_section(title);
        app.settle(Duration::from_millis(400));
        let teaser = crate::demo::browse().into_iter().find(|s| s.title == title).unwrap();
        assert_eq!(app.browse_path.as_deref(), teaser.view_all_path.as_deref(), "{title}");
        assert!(browse_item_count(&app) > teaser.items.len(), "{title} should be the View-all list");
        let names: Vec<_> = app
            .browse_rows()
            .iter()
            .filter_map(|r| match r {
                BrowseRow::Item(i) => Some(i.name.as_str()),
                _ => None,
            })
            .collect();
        assert!(names.iter().any(|n| n.contains("extra")), "{title}: {names:?}");
    }
    app.show_browse_section("Artists Picked For You");
    assert_eq!(app.browse_path, None, "no viewAll: stay on the teaser");
    assert!(matches!(app.browse, BrowseState::Loaded(_)), "the teaser is already cached");
    assert_eq!(browse_headers(&app), ["Artists Picked For You"]);
    assert_eq!(browse_item_count(&app), 1);
    assert!(matches!(app.browse_rows().get(app.browse_index()), Some(BrowseRow::Item(i)) if i.name == "M83"));
    assert!(app.backend.replies.recv_timeout(Duration::from_millis(80)).is_err(), "no View-all request");
}

#[test]
fn a_failed_view_all_falls_back_to_the_teaser_and_a_late_page_is_not_shown() {
    let mut app = demo();
    app.show_browse_section("New Music");
    app.on_reply(Reply::Browse { path: None, sections: crate::demo::browse() });
    assert!(matches!(app.browse, BrowseState::Loading));
    assert_eq!(app.browse_path.as_deref(), Some("/browse/music/new-music"));
    app.on_reply(Reply::BrowseFailed { path: Some("/browse/music/new-music".into()), message: "offline".into() });
    assert!(app.status.contains("short list") && app.status.contains("offline"), "{}", app.status);
    assert_eq!(app.browse_path, None);
    assert_eq!(browse_headers(&app), ["New Music"]);
    assert!(app.browse_rows().iter().any(|r| matches!(r, BrowseRow::Item(i) if i.name == "Midnight City")));
    assert!(!browse_headers(&app).contains(&"Recommended new releases"));

    // A late page for a section the user already left is cached, not shown.
    app.show_browse_section("Browse Genres");
    assert_eq!(app.browse_path.as_deref(), Some("/browse/music/genres"));
    assert!(matches!(app.browse, BrowseState::Loading));
    app.on_reply(Reply::Browse {
        path: Some("/browse/music/new-music".into()),
        sections: crate::demo::browse_at(Some("/browse/music/new-music")),
    });
    assert!(matches!(app.browse, BrowseState::Loading), "still waiting on genres");
    assert!(!browse_headers(&app).contains(&"Recommended new releases"));
    app.on_reply(Reply::Browse {
        path: Some("/browse/music/genres".into()),
        sections: crate::demo::browse_at(Some("/browse/music/genres")),
    });
    assert_eq!(browse_headers(&app), ["Browse Genres"]);
    assert!(browse_item_count(&app) > 1);
}

#[test]
fn view_all_pages_are_capped_and_the_page_on_screen_is_never_dropped() {
    let mut app = demo();
    // Fill past the cap with pages that are not on screen.
    for i in 0..VIEW_ALL_KEPT + 5 {
        app.on_reply(Reply::Browse { path: Some(format!("/browse/page-{i}")), sections: Vec::new() });
    }
    assert_eq!(app.view_all.len(), VIEW_ALL_KEPT, "the map stops growing");
    assert_eq!(app.view_all_order.len(), VIEW_ALL_KEPT, "the order stays in step");
    assert!(!app.view_all.contains_key("/browse/page-0"), "the oldest page went");
    assert!(app.view_all.contains_key(&format!("/browse/page-{}", VIEW_ALL_KEPT + 4)), "the newest stayed");

    // A page on screen is kept even when it is the oldest.
    app.browse_path = Some("/browse/page-5".into());
    for i in 0..VIEW_ALL_KEPT {
        app.on_reply(Reply::Browse { path: Some(format!("/browse/fresh-{i}")), sections: Vec::new() });
    }
    assert!(app.view_all.contains_key("/browse/page-5"), "the page on screen is never dropped");
}

#[test]
fn downloaded_songs_are_capped_so_a_long_session_does_not_grow_without_end() {
    let mut app = demo();
    for i in 0..AUDIO_CACHE_KEPT + 4 {
        app.keep_audio(format!("token-{i}"), vec![0u8; 8]);
    }
    assert_eq!(app.audio_cache.len(), AUDIO_CACHE_KEPT, "the cache stops growing");
    assert_eq!(app.audio_order.len(), AUDIO_CACHE_KEPT, "the order stays in step");
    assert!(!app.audio_cache.contains_key("token-0"), "the oldest song went");
    assert!(app.audio_cache.contains_key(&format!("token-{}", AUDIO_CACHE_KEPT + 3)), "the newest stayed");

    // Asking for a song already held moves it to the young end, not a second copy.
    app.keep_audio(format!("token-{}", AUDIO_CACHE_KEPT + 3), vec![1u8; 8]);
    assert_eq!(app.audio_order.len(), AUDIO_CACHE_KEPT, "no duplicate entry");
    assert_eq!(app.audio_cache[&format!("token-{}", AUDIO_CACHE_KEPT + 3)], vec![1u8; 8], "the new bytes win");
}

#[test]
fn a_browse_list_that_failed_is_asked_for_again_and_logged_out_says_so() {
    let mut app = demo();
    app.settle(Duration::from_millis(300));
    app.on_key(shift('G'));
    app.on_reply(Reply::BrowseFailed { path: None, message: "no".into() });
    assert_eq!(app.browse, BrowseState::Failed("no".into()));
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.browse, BrowseState::Loading, "Enter tries again");
    let mut out = App::new(Config::default(), None, &[], false);
    out.prompt = None;
    out.on_key(shift('G'));
    assert!(out.status.contains("Log in") && out.browse == BrowseState::Idle, "{}", out.status);
}

#[test]
fn browse_follows_stations_in_the_menu_and_g_opens_browse_genres() {
    let app = demo();
    let items = app.menu_items();
    let at = items.iter().position(|m| *m == MenuItem::Stations).unwrap();
    assert_eq!(items[at + 1], MenuItem::RecentlyPlayed, "Recently Played sits above Pinned");
    assert_eq!(items[at + 2], MenuItem::Pinned, "Pinned stays above the Browse sections");
    assert_eq!(items[at + 3], MenuItem::BrowsePin(0), "section pins follow MY COLLECTION");
    assert_eq!(
        items.iter().filter(|m| matches!(m, MenuItem::BrowsePin(_))).count(),
        config::BROWSE_SECTIONS.len(),
        "pins default On"
    );
    assert_eq!(command_for(KeyEvent::from(KeyCode::Char('a'))), Some(Cmd::Add));
    assert_eq!(command_for(shift('G')), Some(Cmd::Genres));
}

#[test]
fn animations_off_skips_the_boot_screen_and_is_saved() {
    let mut app = saving_app("animations");
    assert!(app.config.ui.animations, "on by default");
    app.begin_splash();
    assert!(app.splash.is_some());
    app.splash = None;
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.settings_cursor = Setting::at(Setting::Animations);
    assert_eq!(app.setting_value(Setting::Animations), "on");
    app.on_key(key(KeyCode::Enter));
    assert!(!app.config.ui.animations);
    assert_eq!(app.setting_value(Setting::Animations), "off");
    assert!(saved(&app).contains("[look]") && saved(&app).contains("animations = false"), "{}", saved(&app));
    app.begin_splash();
    assert!(app.splash.is_none() && app.splash_progress().is_none(), "no boot screen when off");
    // It loads back.
    let (config, warnings) = Config::load(app.config_path.as_ref().unwrap());
    assert!(!config.ui.animations && warnings.is_empty(), "{warnings:?}");
    // And the other settings are unaffected: the clock and progress still count.
    assert!(config.ui.splash);
    app.on_key(key(KeyCode::Enter));
    assert!(app.config.ui.animations);
    app.begin_splash();
    assert!(app.splash.is_some(), "on again: the boot screen is back");
}

// --- System Check ---

fn facts(font: bool, notify: bool) -> crate::deps::Facts {
    crate::deps::Facts {
        families: Some(if font { "JetBrainsMono Nerd Font\n" } else { "DejaVu Sans\n" }.into()),
        glyph_fonts: Some(String::new()),
        notify_send: notify,
        keychain: Some(true),
    }
}

#[test]
fn the_list_opens_by_itself_only_when_something_needed_is_missing() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.system_checked(&facts(true, false));
    assert!(!app.system_open(), "the font is there: only a line about the extra");
    assert_eq!(app.status, "Optional, not installed: notify-send");
    app.system_checked(&facts(false, true));
    assert!(app.system_open(), "no Nerd Font: the list opens");
    assert!(app.status.contains("Missing: Nerd Font symbols"), "{}", app.status);
}

#[test]
fn nothing_missing_says_nothing() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.status = String::new();
    app.system_checked(&facts(true, true));
    assert!(!app.system_open() && app.status.is_empty(), "{}", app.status);
}

#[test]
fn emoji_icons_need_no_nerd_font_so_nothing_opens() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.config.ui.icons = crate::config::IconSet::Emoji;
    app.system_checked(&facts(false, true));
    assert!(!app.system_open());
    assert!(app.status.contains("Optional"), "the font is listed but optional: {}", app.status);
}

#[test]
fn it_never_opens_over_what_you_are_typing() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.open_prompt(Prompt::Email);
    app.system_checked(&facts(false, true));
    assert!(!app.system_open(), "the email box keeps the keys");
    assert!(app.status.contains("Missing: Nerd Font symbols"), "{}", app.status);
    app.prompt = None;
    app.open_help();
    app.system_checked(&facts(false, true));
    assert!(!app.system_open(), "nor over Help");
}

#[test]
fn enter_looks_again_and_only_esc_closes_the_list() {
    let mut app = App::new(Config::default(), None, &[], true);
    app.system_checked(&facts(false, false));
    assert!(app.system_open() && app.overlay_open());
    app.on_key(key(KeyCode::Enter));
    assert!(app.system_open() && app.system.is_none(), "looking again");
    for other in ['q', 'x', ' '] {
        app.on_key(key(KeyCode::Char(other)));
        assert!(app.system_open() && !app.quit, "{other} does nothing");
    }
    app.on_key(key(KeyCode::Esc));
    assert!(!app.system_open() && !app.quit, "Esc closes it");
}

#[test]
fn the_answer_arrives_through_tick() {
    let mut app = App::new(Config::default(), None, &[], true);
    let (send, receive) = std::sync::mpsc::channel();
    app.system_look = Some(receive);
    app.tick();
    assert!(app.system.is_none(), "not yet");
    send.send(facts(false, true)).unwrap();
    app.tick();
    assert!(app.system.is_some() && app.system_open());
}

#[test]
fn the_boot_screen_toggle_saves_under_look_and_drops_the_old_place() {
    let mut app = saving_app("splash-look");
    std::fs::write(app.config_path.as_ref().unwrap(), "[boxvibe]\nsplash = true\ncheck_updates = false\n").unwrap();
    app.view = View::Settings;
    app.focus = Focus::Content;
    app.settings_cursor = Setting::at(Setting::Splash);
    assert_eq!(Setting::ALL[app.settings_cursor].0, "LOOK");
    app.on_key(key(KeyCode::Enter));
    assert!(!app.config.ui.splash);
    let text = saved(&app);
    assert!(text.contains("[look]") && text.contains("boot_screen = false"), "{text}");
    assert!(!text.contains("splash"), "the old name is gone: {text}");
    assert!(
        text.contains("look_for_new_versions_on_startup = false") && !text.contains("check_updates"),
        "the rest stays, renamed: {text}"
    );
    assert!(Setting::Splash.about().contains("Saved as [look] boot_screen."));
}
