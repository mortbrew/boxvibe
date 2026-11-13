use super::*;

#[test]
fn defaults_and_empty_file() {
    let (c, w) = Config::parse("").unwrap();
    assert_eq!(c, Config::default());
    assert!(w.is_empty());
    assert_eq!(c.ui.theme(), crate::theme::default_name(), "Omarchy on Omarchy, Tokyo Night elsewhere");
    assert_eq!(c.ui.icons, IconSet::Nerd, "a Nerd Font is the default");
    assert_eq!(c.ui.album_art, AlbumArt::Picture);
    assert_eq!(c.player.volume, 70);
    assert_eq!(c.pandora.audio_quality, "high");
    assert!(!c.has_login());
}

#[test]
fn a_newer_config_still_opens() {
    let text = "version = 2\n[ui]\nicons = \"nerd\"\nfuture_glow = true\n[future]\nlevel = 3\n[player]\nvolume = 40\n";
    let (c, w) = Config::parse(text).unwrap();
    assert_eq!((c.ui.icons, c.player.volume), (IconSet::Nerd, 40));
    assert_eq!(
        w,
        [
            ConfigWarning::Unknown("version".into()),
            ConfigWarning::Unknown("ui.future_glow".into()),
            ConfigWarning::Unknown("future".into()),
        ]
    );
    assert!(ConfigWarning::summary(&w).unwrap().ends_with("(+2 more)"));
}

#[test]
fn wrong_types_fall_back_to_their_default() {
    let (c, w) = Config::parse("[player]\nvolume = \"loud\"\nautoplay = false\n[ui]\nicons = \"fancy\"\n").unwrap();
    assert_eq!((c.player.volume, c.ui.autoplay_last_station, c.ui.icons), (70, false, IconSet::Nerd));
    assert_eq!(w.len(), 2);
    let (c, _) = Config::parse("ui = 5\n").unwrap();
    assert_eq!(c, Config::default());
}

#[test]
fn saved_settings_keep_comments_and_are_private() {
    let path = temp_dir("save").join("sub/config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "# mine\n[ui]\ntheme = \"gruvbox\" # cosy\nfuture = 1\n").unwrap();
    set_values(
        &path,
        &[("account", "email_or_username", "me@example.com".into()), ("look", "icons", "unicode".into())],
    )
    .unwrap();
    set_values(&path, &[("playback", "volume", 55_i64.into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# mine") && text.contains("# cosy") && text.contains("future = 1"), "{text}");
    let (c, _) = Config::load(&path);
    assert_eq!(c.pandora.username, "me@example.com");
    assert!(!c.has_login(), "no password in the file");
    assert!(!text.contains("password"), "{text}");
    assert_eq!((c.ui.theme(), c.ui.icons, c.player.volume), ("gruvbox", IconSet::Unicode, 55));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
    }
}

#[test]
fn clearing_a_secret_leaves_it_empty() {
    let mut secret = Secret::new("hunter2");
    assert!(!secret.is_empty());
    secret.clear();
    assert!(secret.is_empty());
    assert_eq!(format!("{secret:?}"), "<empty>");
}

#[test]
fn two_writers_do_not_share_a_temporary_file() {
    // The temporary name carries the pid, so a second boxvibe writing at the
    // same moment cannot truncate the first one's half-written file.
    let dir = temp_dir("two-writers");
    let path = dir.join("config.toml");
    std::fs::create_dir_all(&dir).unwrap();
    let mine = path.with_extension(format!("toml.{}.tmp", std::process::id()));
    let other = path.with_extension("toml.999999.tmp");
    assert_ne!(mine, other, "each writer gets its own temporary file");
    set_values(&path, &[("playback", "volume", 40_i64.into())]).unwrap();
    assert!(!mine.exists(), "the temporary file is renamed away, not left behind");
    assert!(!other.exists());
    assert_eq!(Config::load(&path).0.player.volume, 40);
}

#[test]
fn good_vibes_only_takes_true_false_or_muted_and_saves_them_back() {
    for (text, want) in [
        ("true", GoodVibes::On),
        ("false", GoodVibes::Off),
        ("\"muted\"", GoodVibes::Muted),
        ("\"Muted\"", GoodVibes::Muted),
    ] {
        let (c, w) = Config::parse(&format!("[playback]\ngood_vibes_only = {text}\n")).unwrap();
        assert_eq!(c.player.good_vibes_only, want, "{text}");
        assert!(w.is_empty(), "{text}: {w:?}");
    }
    assert_eq!(GoodVibes::On.next(), GoodVibes::Muted);
    assert_eq!(GoodVibes::Muted.next(), GoodVibes::Off);
    assert_eq!(GoodVibes::Off.next(), GoodVibes::On);
    assert!(GoodVibes::Muted.plays_ads() && !GoodVibes::Off.plays_ads());
    let path = temp_dir("good-vibes-muted").join("config.toml");
    set_values(&path, &[("playback", "good_vibes_only", GoodVibes::Muted.to_value())]).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("good_vibes_only = \"muted\""));
    assert_eq!(Config::load(&path).0.player.good_vibes_only, GoodVibes::Muted);
}

#[test]
fn good_guy_reads_as_good_vibes_only_and_saving_writes_only_the_new_name() {
    assert_eq!(Config::default().player.good_vibes_only, GoodVibes::On);
    // Every old place it lived in loads, without a warning.
    for old in
        ["[player]\ngood_guy = false\n", "[playback]\ngood_guy = false\n", "[playback]\ngood_vibes_only = false\n"]
    {
        let (c, w) = Config::parse(old).unwrap();
        assert!((c.player.good_vibes_only == GoodVibes::Off) && w.is_empty(), "{old}: {w:?}");
    }
    // The new name beats the old one, wherever the old one is.
    let (c, _) = Config::parse("[playback]\ngood_guy = false\ngood_vibes_only = true\n").unwrap();
    assert_eq!(c.player.good_vibes_only, GoodVibes::On);
    let (c, _) = Config::parse("[player]\ngood_guy = false\n[playback]\ngood_vibes_only = true\n").unwrap();
    assert_eq!(c.player.good_vibes_only, GoodVibes::On);
    // Saving anything moves it: the old key and the old table are gone, no key twice.
    let path = temp_dir("good-guy-migrate").join("config.toml");
    std::fs::write(&path, "[player]\n# keep me\ngood_guy = false # off\nvolume = 40\n").unwrap();
    set_values(&path, &[("look", "icons", "unicode".into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("good_vibes_only = false # off") && !text.contains("good_guy"), "{text}");
    assert!(text.contains("# keep me") && !text.contains("[player]") && text.contains("[playback]"), "{text}");
    assert_eq!(text.matches("good_vibes_only").count(), 1, "{text}");
    let (c, w) = Config::load(&path);
    assert!((c.player.good_vibes_only == GoodVibes::Off) && c.player.volume == 40 && w.is_empty(), "{w:?}");
    // Both names in one file: the new value stays, the old key goes.
    std::fs::write(&path, "[playback]\ngood_guy = false\ngood_vibes_only = true\n").unwrap();
    set_values(&path, &[("playback", "volume", 50_i64.into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("good_vibes_only = true") && !text.contains("good_guy"), "{text}");
}

#[test]
fn an_old_ui_file_with_good_guy_loads_saves_in_the_new_names_and_keeps_unknown_keys() {
    let path = temp_dir("old-ui").join("config.toml");
    let old = "# my settings\n[ui]\ntheme = \"gruvbox\" # cosy\nicons = \"emoji\"\nmystery = 1\n\
                   [player]\ngood_guy = false\nvolume = 35\n[pandora]\nusername = \"me@example.com\"\n[extra]\nkeep = true\n";
    std::fs::write(&path, old).unwrap();
    let (c, w) = Config::load(&path);
    assert_eq!((c.ui.theme(), c.ui.icons, c.player.volume), ("gruvbox", IconSet::Emoji, 35));
    assert!((c.player.good_vibes_only == GoodVibes::Off) && c.pandora.username == "me@example.com");
    assert!(w.iter().any(|x| matches!(x, ConfigWarning::Unknown(k) if k == "ui.mystery")), "{w:?}");
    set_values(&path, &[("look", "animations", false.into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    // `[ui]` stays, holding only the key boxvibe does not know.
    for gone in ["[player]", "[pandora]", "good_guy", "theme = "] {
        assert!(!text.contains(gone), "{gone} left in\n{text}");
    }
    for kept in
        ["# my settings", "# cosy", "mystery = 1", "[extra]", "keep = true", "[look]", "[playback]", "[account]"]
    {
        assert!(text.contains(kept), "{kept} lost from\n{text}");
    }
    assert!(text.contains("good_vibes_only = false") && text.contains("animations = false"), "{text}");
    assert_eq!(text.matches("[ui]").count(), 1, "{text}");
    let (c, _) = Config::load(&path);
    assert!((c.player.good_vibes_only == GoodVibes::Off) && c.ui.theme() == "gruvbox" && !c.ui.animations);
}

#[test]
fn browse_pins_default_on_and_keeps_unknown_section_keys() {
    let (c, w) =
        Config::parse("[browse]\nbrowse = true\nartists_picked_for_you = false\nfuture_shelf = true\n").unwrap();
    assert!(!c.browse.pinned_on_menu("Artists Picked For You"));
    assert!(c.browse.pinned_on_menu("New Music"), "missing section pin defaults on");
    assert!(c.browse.pinned_key("future_shelf"));
    // Retired master [browse] browse is ignored, not Unknown.
    assert!(w.iter().all(|x| !matches!(x, ConfigWarning::Unknown(_))), "{w:?}");
    assert_eq!(browse_section_key("Trendsetters: Artists on the rise"), "trendsetters_artists_on_the_rise");
}

/// (old table, old key, a non-default value, new table, new key, is that value read?)
type Renamed = (&'static str, &'static str, &'static str, &'static str, &'static str, fn(&Config) -> bool);
const RENAMED: &[Renamed] = &[
    ("pandora", "username", "\"me@x.io\"", "account", "email_or_username", |c| c.pandora.username == "me@x.io"),
    ("account", "username", "\"me@x.io\"", "account", "email_or_username", |c| c.pandora.username == "me@x.io"),
    ("ui", "check_updates", "false", "boxvibe", "look_for_new_versions_on_startup", |c| !c.ui.check_updates),
    ("boxvibe", "check_updates", "false", "boxvibe", "look_for_new_versions_on_startup", |c| !c.ui.check_updates),
    ("ui", "look_for_new_versions_on_startup", "false", "boxvibe", "look_for_new_versions_on_startup", |c| {
        !c.ui.check_updates
    }),
    ("ui", "notify", "false", "playback", "song_change_notifications", |c| !c.ui.notifications),
    ("ui", "notifications", "false", "playback", "song_change_notifications", |c| !c.ui.notifications),
    ("playback", "notifications", "false", "playback", "song_change_notifications", |c| !c.ui.notifications),
    ("ui", "art_tint", "\"duotone\"", "look", "tint_art_with_theme_colors", |c| c.ui.art_tint == ArtTint::Duotone),
    ("look", "art_tint", "\"duotone\"", "look", "tint_art_with_theme_colors", |c| c.ui.art_tint == ArtTint::Duotone),
    ("ui", "splash", "false", "look", "boot_screen", |c| !c.ui.splash),
    ("boxvibe", "splash", "false", "look", "boot_screen", |c| !c.ui.splash),
    ("look", "splash", "false", "look", "boot_screen", |c| !c.ui.splash),
    ("ui", "boot_screen", "false", "look", "boot_screen", |c| !c.ui.splash),
];

#[test]
fn every_old_name_of_the_five_renamed_settings_still_loads() {
    for (table, key, value, new_table, new_key, read) in RENAMED {
        let (c, w) = Config::parse(&format!("[{table}]\n{key} = {value}\n")).unwrap();
        assert!(read(&c) && w.is_empty(), "[{table}] {key}: {w:?}");
        // And the new name, in its own table.
        let (c, w) = Config::parse(&format!("[{new_table}]\n{new_key} = {value}\n")).unwrap();
        assert!(read(&c) && w.is_empty(), "[{new_table}] {new_key}: {w:?}");
    }
    // The retired spellings in a table of their own kind are not "unknown" noise-free either way:
    // a key nobody knows is still reported, next to the ones that load.
    let (c, w) = Config::parse("[look]\nsplash = false\nsplosh = 1\n").unwrap();
    assert!(!c.ui.splash && w.len() == 1, "{w:?}");
}

#[test]
fn the_new_name_wins_over_every_old_one_and_a_save_leaves_exactly_one_key() {
    let path = temp_dir("five-renames").join("config.toml");
    for (table, key, value, new_table, new_key, read) in RENAMED {
        // The new name says the opposite of the old one (the default), wherever the old one is.
        let default = Config::default();
        assert!(!read(&default), "{key}: the test value must differ from the default");
        let both = format!("[{table}]\n{key} = {value}\n[{new_table}]\n{new_key} = DEFAULT\n");
        let default_value = match *new_key {
            "email_or_username" => "\"\"",
            "tint_art_with_theme_colors" => "\"off\"",
            "look_for_new_versions_on_startup" | "song_change_notifications" | "boot_screen" => "true",
            _ => unreachable!(),
        };
        let both = both.replace("DEFAULT", default_value);
        if table == new_table {
            // Same table: a file with the old and the new key side by side.
            let same = format!("[{table}]\n{key} = {value}\n{new_key} = {default_value}\n");
            let (c, _) = Config::parse(&same).unwrap();
            assert!(!read(&c), "{same}");
            std::fs::write(&path, &same).unwrap();
        } else {
            let (c, _) = Config::parse(&both).unwrap();
            assert!(!read(&c), "{both}");
            std::fs::write(&path, &both).unwrap();
        }
        set_values(&path, &[("look", "icons", "emoji".into())]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        assert_eq!(lines.iter().filter(|l| l.starts_with(&format!("{new_key} ="))).count(), 1, "{text}");
        assert!(
            key == new_key || !lines.iter().any(|l| l.starts_with(&format!("{key} ="))),
            "{key} is left in\n{text}"
        );
        assert!(!read(&Config::load(&path).0), "the new value stays: {text}");
        // The old name alone: the next save renames it, with its comment and value.
        std::fs::write(&path, format!("[{table}]\n# mine\n{key} = {value} # why\n")).unwrap();
        set_values(&path, &[("look", "icons", "emoji".into())]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let doc: toml_edit::DocumentMut = text.parse().unwrap();
        assert!(doc[*new_table][*new_key].is_value(), "{key} → {new_key}:\n{text}");
        assert!(text.contains(&format!("# mine\n{new_key} = {value} # why")), "{text}");
        assert!(read(&Config::load(&path).0), "{text}");
        assert_eq!(text.matches(new_key).count(), 1, "{text}");
        if table != new_table && doc.get(table).is_some() {
            assert!(doc[*table].get(key).is_none(), "{text}");
        }
    }
}

#[test]
fn an_old_file_with_all_five_old_names_migrates_keeping_comments_unknown_keys_and_mode() {
    let path = temp_dir("five-full").join("config.toml");
    let old = "# my config\n[pandora]\n# who I am\nusername = \"me@x.io\" # login\nsecret_extra = 1\n\
                   [ui]\n# updates\ncheck_updates = false # no thanks\nnotify = false\nart_tint = \"palette\"\n\
                   splash = false # quiet\nweird = true\n[boxvibe]\ngenre_edit_popup = false\n[extra]\nkeep = true\n";
    std::fs::write(&path, old).unwrap();
    let (c, w) = Config::load(&path);
    assert!(c.pandora.username == "me@x.io" && !c.ui.check_updates && !c.ui.notifications && !c.ui.splash);
    assert_eq!((c.ui.art_tint, c.ui.genre_edit_popup), (ArtTint::Palette, false));
    assert!(w.iter().any(|x| matches!(x, ConfigWarning::Unknown(k) if k == "ui.weird")), "{w:?}");
    set_values(&path, &[("playback", "volume", 42_i64.into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    for new in [
        "email_or_username = \"me@x.io\" # login",
        "look_for_new_versions_on_startup = false # no thanks",
        "song_change_notifications = false",
        "tint_art_with_theme_colors = \"palette\"",
        "boot_screen = false # quiet",
        "genre_edit_popup = false",
        "# my config",
        "# who I am",
        "# updates",
        "secret_extra = 1",
        "weird = true",
        "[extra]\nkeep = true",
    ] {
        assert!(text.contains(new), "{new} missing from\n{text}");
    }
    for gone in ["\nusername", "\ncheck_updates", "notify", "\nart_tint", "splash", "\nnotifications"] {
        assert!(!text.contains(gone), "{gone} left in\n{text}");
    }
    let doc: toml_edit::DocumentMut = text.parse().unwrap();
    assert!(doc.get("pandora").is_some_and(|t| t.as_table().is_some_and(|t| t.len() == 1)), "{text}");
    let (c, w) = Config::load(&path);
    assert!(c.pandora.username == "me@x.io" && !c.ui.check_updates && c.player.volume == 42);
    assert_eq!(w.len(), 3, "the unknown keys and table are still reported: {w:?}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[test]
fn a_file_with_only_the_retired_now_playing_size_loads_and_loses_the_ui_table_on_save() {
    let path = temp_dir("retired-ui").join("config.toml");
    std::fs::write(&path, "[ui]\nnow_playing_size = 23\n\n[account]\nemail_or_username = \"me@x.io\"\n").unwrap();
    let (c, w) = Config::load(&path);
    assert!(w.is_empty(), "no warning: {w:?}");
    assert_eq!(c.ui.menu_width, Config::default().ui.menu_width, "it is not the menu width");
    assert_eq!(c.pandora.username, "me@x.io");
    set_values(&path, &[("playback", "volume", 60_i64.into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("[ui]") && !text.contains("now_playing_size"), "{text}");
    assert!(text.contains("email_or_username = \"me@x.io\"") && text.contains("volume = 60"), "{text}");
    assert_eq!(Config::load(&path).0.ui.menu_width, Config::default().ui.menu_width);
}

#[test]
fn retired_keys_go_on_save_but_unknown_keys_and_their_tables_stay() {
    let path = temp_dir("retired-mixed").join("config.toml");
    std::fs::write(
            &path,
            "# top\n[ui]\n# old\nnow_playing_size = \"big\"\nmy_note = 1\ntheme = \"nord\"\n\n[boxvibe]\ndependency_check = false\n\
             mine = 2\n\n[pandora]\npartner = \"x\"\nrpc_host = \"y\"\n\n[player]\nvolume = 30\n\n[extra]\nkeep = true\n",
        )
        .unwrap();
    set_values(&path, &[("look", "icons", "emoji".into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    for gone in ["now_playing_size", "dependency_check", "partner", "rpc_host", "[pandora]", "[player]"] {
        assert!(!text.contains(gone), "{gone} left in\n{text}");
    }
    for kept in
        ["# top", "[ui]\n", "my_note = 1", "mine = 2", "[extra]\nkeep = true", "theme_select = \"nord\"", "volume = 30"]
    {
        assert!(text.contains(kept) || text.contains(&kept.replace("[ui]\n", "[ui]")), "{kept} lost from\n{text}");
    }
    let doc: toml_edit::DocumentMut = text.parse().unwrap();
    assert_eq!(doc["ui"].as_table().unwrap().len(), 1, "[ui] keeps only my_note: {text}");
    // Saving again changes nothing more.
    set_values(&path, &[("look", "icons", "emoji".into())]).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
}

#[test]
fn a_v1_config_still_loads() {
    // boxvibe 0.1's HE-AAC qualities read as standard; the new key is known.
    let text = "[pandora]\nusername = \"me@example.com\"\naudio_quality = \"low\"\nuser_agent = \"\"\n";
    let (c, w) = Config::parse(text).unwrap();
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(crate::pandora::Quality::parse(&c.pandora.audio_quality), crate::pandora::Quality::Standard);
    assert_eq!(c.pandora.username, "me@example.com");
}

#[test]
fn notifications_on_by_default_and_the_old_name_reads() {
    assert!(Config::default().ui.notifications);
    let (c, w) = Config::parse("[ui]\nnotify = false\n").unwrap();
    assert!(!c.ui.notifications && w.is_empty(), "{w:?}");
    let (c, w) = Config::parse("[ui]\nnotify = false\nnotifications = true\ntheme = \"gruvbox\"\n").unwrap();
    assert!(c.ui.notifications && w.is_empty(), "{w:?}");
    assert_eq!(c.ui.theme(), "gruvbox", "the rest still applies");
    let path = temp_dir("notify").join("config.toml");
    std::fs::write(&path, "[ui]\nnotify = true # old\n").unwrap();
    set_values(&path, &[("playback", "song_change_notifications", false.into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("song_change_notifications = false # old") && !text.contains("notify"), "{text}");
}

#[test]
fn retired_now_playing_size_is_ignored_quietly() {
    let (c, w) = Config::parse("[ui]\nnow_playing_size = 11\nmenu_width = 24\n").unwrap();
    assert_eq!((c.ui.menu_width, w.len()), (24, 0));
    let (_, w) = Config::parse("[ui]\nnow_playing_size = \"big\"\n").unwrap();
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn favorites_read_and_tolerate_junk() {
    let text = "[ui]\nfavorites = [{ id = \"1\", name = \"Jazz\" }, { id = \"2\", extra = 3 }, { name = \"no id\" }]\n";
    let (c, _) = Config::parse(text).unwrap();
    let ids: Vec<&str> = c.ui.favorites.iter().map(|f| f.id.as_str()).collect();
    assert_eq!((ids, c.ui.favorites[0].name.as_str()), (vec!["1", "2"], "Jazz"));
    let (c, w) = Config::parse("[ui]\nfavorites = \"jazz\"\nmenu_width = 24\n").unwrap();
    assert_eq!((c.ui.favorites.len(), c.ui.menu_width, w.len()), (0, 24, 1));
}

#[test]
fn sizes_step_and_stop_at_the_ends() {
    assert_eq!((MENU_WIDTH.step_by(30, 1), MENU_WIDTH.step_by(30, -1)), (32, 28));
    assert_eq!((MENU_WIDTH.step_by(40, 1), MENU_WIDTH.step_by(20, -1)), (40, 20));
}

#[test]
fn art_tint_parses_tolerantly() {
    let (c, w) = Config::parse("").unwrap();
    assert_eq!((c.ui.art_tint, w.len()), (ArtTint::Off, 0));
    let (c, w) = Config::parse("[ui]\nart_tint = \"duotone\"\n").unwrap();
    assert_eq!((c.ui.art_tint, w.len()), (ArtTint::Duotone, 0));
    let (c, w) = Config::parse("[ui]\nart_tint = \"sepia\"\nalbum_art = \"blocks\"\n").unwrap();
    assert_eq!((c.ui.art_tint, c.ui.album_art, w.len()), (ArtTint::Off, AlbumArt::Picture, 1));
    assert!(matches!(&w[0], ConfigWarning::BadValue { key, .. } if key == "look.tint_art_with_theme_colors"), "{w:?}");
    let (c, _) = Config::parse("[ui]\nart_tint = 3\n").unwrap();
    assert_eq!(c.ui.art_tint, ArtTint::Off);
    assert!(
        ArtTint::ALL
            .iter()
            .all(|a| { Config::parse(&format!("[ui]\nart_tint = \"{}\"\n", a.as_str())).unwrap().0.ui.art_tint == *a })
    );
    assert_eq!(ArtTint::Palette.next(), ArtTint::Off);
}

#[test]
fn album_art_and_retro_mode() {
    let (c, w) = Config::parse("").unwrap();
    assert_eq!((c.ui.album_art, c.ui.retro_mode, w.len()), (AlbumArt::Picture, RetroMode::Hd, 0));
    let (c, w) = Config::parse("[ui]\nalbum_art = \"blocks\"\nretro_mode = \"4bit\"\n").unwrap();
    assert_eq!((c.ui.album_art, c.ui.retro_mode, w.len()), (AlbumArt::Picture, RetroMode::Bit4, 0));
    let (c, w) = Config::parse("[look]\nalbum_art = \"blocks\"\n").unwrap();
    assert_eq!((c.ui.album_art, w.len()), (AlbumArt::Picture, 0), "0.3's blocks is Picture now");
    assert_eq!(AlbumArt::ALL.map(AlbumArt::as_str), ["picture", "off"]);
    assert_eq!(AlbumArt::Off.next(), AlbumArt::Picture);
    let (c, w) = Config::parse("[ui]\nalbum_art = \"hologram\"\nretro_mode = 7\n").unwrap();
    assert_eq!((c.ui.album_art, c.ui.retro_mode, w.len()), (AlbumArt::Picture, RetroMode::Hd, 2));
    assert!(RetroMode::ALL.iter().all(|a| {
        Config::parse(&format!("[ui]\nretro_mode = \"{}\"\n", a.as_str())).unwrap().0.ui.retro_mode == *a
    }));
    assert_eq!(RetroMode::Bit2.next(), RetroMode::Hd);
    assert_eq!(RetroMode::Hd.next(), RetroMode::Bit64, "the mildest step first");
    assert_eq!(RetroMode::Bit64.next(), RetroMode::Bit32);
    assert_eq!(
        RetroMode::ALL.map(RetroMode::grid),
        [None, Some(256), Some(128), Some(64), Some(32), Some(16), Some(8)]
    );
    assert_eq!(RetroMode::ALL.map(RetroMode::label), ["HD", "64-bit", "32-bit", "16-bit", "8-bit", "4-bit", "2-bit"]);
    let (c, w) = Config::parse("[ui]\nretro_mode = \"64bit\"\n").unwrap();
    assert_eq!((c.ui.retro_mode, w.len()), (RetroMode::Bit64, 0));
}

#[test]
fn the_old_art_pixelation_key_and_values_still_read() {
    for (old, new) in [
        ("off", RetroMode::Hd),
        ("extra_low", RetroMode::Bit32),
        ("extra-low", RetroMode::Bit32),
        ("low", RetroMode::Bit16),
        ("medium", RetroMode::Bit8),
        ("high", RetroMode::Bit4),
        ("max", RetroMode::Bit2),
    ] {
        let (c, w) = Config::parse(&format!("[ui]\nart_pixelation = \"{old}\"\n")).unwrap();
        assert_eq!((c.ui.retro_mode, w.len()), (new, 0), "{old}");
    }
    let both = "[ui]\nart_pixelation = \"max\"\nretro_mode = \"16bit\"\n";
    let (c, w) = Config::parse(both).unwrap();
    assert_eq!((c.ui.retro_mode, w.len()), (RetroMode::Bit16, 0), "the new key wins");
}

#[test]
fn border_style_and_menu_width() {
    let (c, w) = Config::parse("[ui]\nborder_style = \"rounded\"\nmenu_width = 24\n").unwrap();
    assert_eq!((c.ui.border_style, c.ui.menu_width, w.len()), (BorderStyle::Rounded, 24, 0));
    let (c, w) = Config::parse("[ui]\nborder_style = \"wavy\"\nmenu_width = 5\n").unwrap();
    assert_eq!((c.ui.border_style, c.ui.menu_width, w.len()), (BorderStyle::Plain, 30, 2));
    let (c, w) = Config::parse("[ui]\nmenu_width = 70000\n").unwrap();
    assert_eq!((c.ui.menu_width, w.len()), (30, 1));
    assert_eq!((MENU_WIDTH.next(30), MENU_WIDTH.next(40), MENU_WIDTH.next(39)), (32, 20, 20));
    assert_eq!(BorderStyle::None.next(), BorderStyle::Plain);
    assert!(
        BorderStyle::ALL.iter().all(|b| Config::parse(&format!("[ui]\nborder_style = \"{}\"\n", b.as_str()))
            .unwrap()
            .0
            .ui
            .border_style
            == *b)
    );
}

#[test]
fn autoplay_last_station_and_its_old_name() {
    assert!(Config::default().ui.autoplay_last_station, "0.1 started the last station: still on");
    let (c, w) = Config::parse("[ui]\nautoplay_last_station = false\n").unwrap();
    assert!(!c.ui.autoplay_last_station && w.is_empty());
    let (c, w) = Config::parse("[player]\nautoplay = false\nvolume = 40\n").unwrap();
    assert_eq!((c.ui.autoplay_last_station, c.player.volume, w.len()), (false, 40, 0));
    let (c, _) = Config::parse("[player]\nautoplay = false\n[ui]\nautoplay_last_station = true\n").unwrap();
    assert!(c.ui.autoplay_last_station, "the new name wins");
    let (c, w) = Config::parse("[ui]\nautoplay_last_station = \"yes\"\n[player]\nautoplay = 3\n").unwrap();
    assert!(c.ui.autoplay_last_station, "a wrong type: the default");
    assert_eq!(w.len(), 1, "{w:?}");
}

#[test]
fn missing_file_loads_defaults() {
    let (c, w) = Config::load(Path::new("/definitely/not/here/config.toml"));
    assert_eq!(c, Config::default());
    assert!(w.is_empty());
}

/// Junk that must never stop boxvibe: (file, what must still apply).
#[test]
fn junk_configs_always_start() {
    let junk = [
        // Leftovers from the JSON-API boxvibe and from testing.
        "[pandora]\npartner_user = \"android\"\npartner_password = \"x\"\ndevice = \"android-generic\"\n\
             decrypt_password = \"R=U!LH$O2B#\"\nencrypt_password = \"6#26FRL$ZWD\"\nrpc_host = \"tuner.pandora.com\"\n\
             audio_quality = \"medium\"\nlast_station = \"123:abc\"\nusername = \"me@example.com\"\n",
        // Wrong types everywhere.
        "[pandora]\nusername = 42\npassword = true\naudio_quality = [1, 2]\nuser_agent = { a = 1 }\n\
             [player]\nvolume = 300\nautoplay = \"yes\"\n[ui]\ntheme = 7\nicons = \"fancy\"\nnotifications = \"sure\"\n\
             check_updates = 1.5\nborder_style = 9\nmenu_width = \"wide\"\n",
        // Sections that are values, values that are sections, arrays of tables.
        "pandora = \"me\"\nplayer = [1, 2]\n[[ui]]\ntheme = \"gruvbox\"\n[ui2]\nx = 1\n",
        // Deeply nested and odd things.
        "[ui.extra.deep]\nx = 1\n[player.eq]\nbass = 3\n[test]\ndebug = true\nwhen = 2026-10-20T21:00:00Z\n",
        // Both names of the notifications setting, and an empty one.
        "[ui]\nnotify = 1\nnotifications = false\n[pandora]\n",
    ];
    for text in junk {
        let (c, warnings) = Config::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert!(!warnings.is_empty() || text.contains("notifications = false"), "{text}");
        assert!(c.player.volume <= 100);
        assert!(ConfigWarning::summary(&warnings).is_some() || warnings.is_empty());
    }
    let (c, w) = Config::parse(junk[0]).unwrap();
    assert_eq!(c.pandora.username, "me@example.com");
    assert!(w.iter().all(|w| matches!(w, ConfigWarning::Old(_))), "{w:?}");
    assert!(ConfigWarning::summary(&w).unwrap().contains("old Pandora API"));
    assert_eq!(crate::pandora::Quality::parse(&c.pandora.audio_quality), crate::pandora::Quality::Standard);
    let (c, _) = Config::parse(junk[1]).unwrap();
    assert_eq!((c.player.volume, c.ui.icons, c.ui.notifications), (70, IconSet::Nerd, true));
}

#[test]
fn a_broken_file_is_moved_aside_and_the_defaults_start() {
    let dir = temp_dir("broken");
    let path = dir.join("config.toml");
    let broken = "[pandora\nusername = \"me@example.com\"\npassword = \"x\n";
    std::fs::write(&path, broken).unwrap();
    let (c, w) = Config::load(&path);
    assert_eq!(c, Config::default());
    let [ConfigWarning::Broken { backup: Some(backup), .. }] = w.as_slice() else { panic!("{w:?}") };
    assert!(!path.exists(), "moved, so saving makes a fresh file");
    assert_eq!(std::fs::read_to_string(backup).unwrap(), broken, "kept as it was");
    let name = backup.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("config.toml.bad-20") && name.ends_with('Z'), "{name}");
    assert!(ConfigWarning::summary(&w).unwrap().contains("moved to config.toml.bad-"));
    // Not text at all, too.
    std::fs::write(&path, [0xff, 0xfe, 0x00, 0x9f]).unwrap();
    let (c, w) = Config::load(&path);
    assert_eq!((c, w.len()), (Config::default(), 1));
    // Saving still works afterwards.
    set_values(&path, &[("look", "theme_auto_mode", "off".into()), ("look", "theme_select", "gruvbox".into())])
        .unwrap();
    assert_eq!(Config::load(&path).0.ui.theme(), "gruvbox");
}

#[test]
fn an_unreadable_path_starts_with_defaults() {
    let dir = temp_dir("unreadable");
    // A folder where the file should be.
    let path = dir.join("config.toml");
    std::fs::create_dir_all(&path).unwrap();
    let (c, w) = Config::load(&path);
    assert_eq!(c, Config::default());
    assert!(matches!(w.as_slice(), [ConfigWarning::Unreadable(_)]), "{w:?}");
    assert!(path.is_dir(), "left alone");
}

const OLD_FILE: &str = "# my settings\n[pandora]\nusername = \"me@example.com\" # login\npassword = \"hunter2\"\n\
        audio_quality = \"standard\"\nlast_station = \"42\"\n\n[player]\nvolume = 55 # quiet\n\n[ui]\n\
        theme = \"gruvbox\"\nicons = \"unicode\"\nborder_style = \"thick\"\nmenu_width = 30\n\
        album_art = \"blocks\"\nretro_mode = \"16bit\"\nart_tint = \"duotone\"\nsplash = false\n\
        check_updates = false\nautoplay_last_station = true\nnotifications = false\n\
        favorites = [{ id = \"1\", name = \"Jazz\" }]\nfuture = 1\n";

#[test]
fn the_tables_are_the_settings_groups() {
    let tables: Vec<&str> = LAYOUT.iter().map(|(t, ..)| *t).fold(Vec::new(), |mut all, t| {
        if !all.contains(&t) {
            all.push(t);
        }
        all
    });
    assert_eq!(
        tables,
        ["account", "boxvibe", "playback", "look", "browse"],
        "ACCOUNT, BOXVIBE, PLAYBACK, LOOK, BROWSE"
    );
}

#[test]
fn the_old_layout_still_loads_with_no_warnings() {
    let (old, w) = Config::parse(OLD_FILE).unwrap();
    // Only the unknown `future` key is reported.
    assert_eq!(w, [ConfigWarning::Unknown("ui.future".into())]);
    assert_eq!(old.pandora.username, "me@example.com");
    assert_eq!((old.player.volume, old.ui.theme(), old.ui.icons), (55, "gruvbox", IconSet::Unicode));
    assert_eq!(
        (old.ui.album_art, old.ui.retro_mode, old.ui.art_tint),
        (AlbumArt::Picture, RetroMode::Bit16, ArtTint::Duotone)
    );
    assert!(!old.ui.splash && !old.ui.check_updates && old.ui.autoplay_last_station && !old.ui.notifications);
    assert_eq!((old.ui.favorites.len(), old.pandora.last_station.as_str()), (1, "42"));
    // Written the new way, it is the same configuration.
    let new_text = "[boxvibe]\nsplash = false\ncheck_updates = false\n\n[account]\nusername = \"me@example.com\"\n\
            password = \"hunter2\"\n\n[playback]\naudio_quality = \"standard\"\nlast_station = \"42\"\nvolume = 55\n\
            autoplay_last_station = true\nnotifications = false\nfavorites = [{ id = \"1\", name = \"Jazz\" }]\n\n\
            [look]\ntheme = \"gruvbox\"\nicons = \"unicode\"\nborder_style = \"thick\"\nmenu_width = 30\n\
            album_art = \"picture\"\nretro_mode = \"16bit\"\nart_tint = \"duotone\"\n";
    let (new, w) = Config::parse(new_text).unwrap();
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(new, old);
}

#[test]
fn a_setting_in_its_new_table_beats_the_old_one_and_old_names_read() {
    let text = "[ui]\ntheme = \"gruvbox\"\nnotify = false\nart_pixelation = \"8bit\"\n[look]\ntheme = \"catppuccin\"\n\
                    [pandora]\nvolume = 5\n[playback]\nvolume = 40\nnotifications = true\n[boxvibe]\nnonsense = 1\n";
    let (c, w) = Config::parse(text).unwrap();
    assert_eq!(c.ui.theme(), "catppuccin", "[look] wins over [ui]");
    assert_eq!(c.player.volume, 40, "[playback] wins over a stray old volume");
    assert!(c.ui.notifications, "the new name wins over `notify`");
    assert_eq!(c.ui.retro_mode, RetroMode::Bit8, "art_pixelation reads as retro_mode");
    // `pandora.volume` is not a thing it ever had; `boxvibe.nonsense` is unknown.
    assert!(w.contains(&ConfigWarning::Unknown("boxvibe.nonsense".into())), "{w:?}");
    assert!(w.contains(&ConfigWarning::Unknown("pandora.volume".into())), "{w:?}");
    // A bad value is named as the file names it, in either layout.
    let (_, w) = Config::parse("[look]\nicons = \"fancy\"\n").unwrap();
    assert!(matches!(&w[..], [ConfigWarning::BadValue { key, .. }] if key == "look.icons"), "{w:?}");
    let (_, w) = Config::parse("[ui]\nicons = \"fancy\"\n[player]\nvolume = \"loud\"\n").unwrap();
    let keys: Vec<&str> = w
        .iter()
        .filter_map(|w| if let ConfigWarning::BadValue { key, .. } = w { Some(key.as_str()) } else { None })
        .collect();
    assert!(keys.contains(&"look.icons") && keys.contains(&"playback.volume"), "{keys:?}");
}

#[test]
fn an_old_plaintext_password_is_read_once_and_then_deleted_on_request() {
    for text in [
        "[pandora]\nusername = \"me@example.com\"\npassword = \"hunter2\"\n",
        "[account]\nusername = \"me@example.com\"\npassword = \"hunter2\" # old\n",
    ] {
        let (c, w) = Config::parse(text).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(c.pandora.password.expose(), "hunter2");
        assert!(c.has_login());
    }
    // [account] beats [pandora].
    let (c, _) = Config::parse("[pandora]\npassword = \"old\"\n[account]\npassword = \"new\"\n").unwrap();
    assert_eq!(c.pandora.password.expose(), "new");
    // Saving anything else never writes it; deleting it keeps everything else.
    let path = temp_dir("plain-password").join("config.toml");
    std::fs::write(
        &path,
        "# mine\n[account]\nusername = \"me@example.com\" # who\npassword = \"hunter2\"\n[look]\ntheme = \"gruvbox\"\n",
    )
    .unwrap();
    assert!(forget_plaintext_password(&path).unwrap());
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("hunter2") && !text.contains("password"), "{text}");
    assert!(text.contains("# mine") && text.contains("# who") && text.contains("gruvbox"), "{text}");
    assert!(!forget_plaintext_password(&path).unwrap(), "nothing left to delete");
    let (c, _) = Config::load(&path);
    assert!(!c.has_login() && c.pandora.username == "me@example.com");
    // A [pandora] table that only held the password goes with it.
    std::fs::write(&path, "[pandora]\npassword = \"hunter2\"\n").unwrap();
    assert!(forget_plaintext_password(&path).unwrap());
    assert!(!std::fs::read_to_string(&path).unwrap().contains("pandora"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[test]
fn the_password_is_hidden_from_debug_output_and_never_serialized() {
    let (c, _) = Config::parse("[account]\nusername = \"me@example.com\"\npassword = \"hunter2\"\n").unwrap();
    let shown = format!("{c:?}");
    assert!(!shown.contains("hunter2") && shown.contains("<hidden>") && shown.contains("me@example.com"), "{shown}");
    assert_eq!(format!("{:?}", Secret::default()), "<empty>");
    let text = toml::to_string(&c.pandora).unwrap();
    assert!(!text.contains("hunter2") && !text.contains("password"), "{text}");
}

#[test]
fn saving_moves_an_old_file_to_the_new_layout_and_keeps_the_rest() {
    let path = temp_dir("migrate").join("config.toml");
    std::fs::write(&path, OLD_FILE).unwrap();
    let before = Config::load(&path).0;
    set_values(&path, &[("look", "theme_auto_mode", "off".into()), ("look", "theme_select", "catppuccin".into())])
        .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let doc: toml_edit::DocumentMut = text.parse().unwrap();
    // The old tables are gone, except for the one key nobody knows.
    assert!(doc.get("player").is_none(), "{text}");
    // (a saved password stays in [pandora] until the keychain has it: see forget_plaintext_password)
    assert_eq!(doc["pandora"].as_table().unwrap().len(), 1, "{text}");
    assert_eq!(doc["pandora"]["password"].as_str(), Some("hunter2"), "not lost by an unrelated save");
    assert_eq!(doc["ui"].as_table().unwrap().len(), 1, "{text}");
    assert_eq!(doc["ui"]["future"].as_integer(), Some(1));
    // Every setting is in its place, with its comment and its value.
    assert_eq!(doc["account"]["email_or_username"].as_str(), Some("me@example.com"));
    assert_eq!(doc["playback"]["volume"].as_integer(), Some(55));
    assert_eq!(doc["look"]["theme_select"].as_str(), Some("catppuccin"));
    assert!(doc["look"].get("theme").is_none(), "the old key is split: {text}");
    assert_eq!(doc["look"]["boot_screen"].as_bool(), Some(false), "the boot screen is under [look] now");
    assert!(doc.get("boxvibe").and_then(|t| t.get("splash")).is_none() && !text.contains("splash"), "{text}");
    assert_eq!(doc["boxvibe"]["look_for_new_versions_on_startup"].as_bool(), Some(false));
    assert!(text.contains("# my settings") && text.contains("# login") && text.contains("# quiet"), "{text}");
    assert!(text.contains("album_art = \"picture\"") && !text.contains("blocks"), "{text}");
    // Nothing but the saved theme changed (and blocks, which is picture now).
    let after = Config::load(&path).0;
    assert_eq!(after.ui.theme(), "catppuccin");
    let mut want = before;
    want.ui.set_theme("catppuccin");
    assert_eq!(after, want);
    // Saving again is steady: no duplicates, no new tables.
    set_values(&path, &[("playback", "volume", 60_i64.into())]).unwrap();
    let again: toml_edit::DocumentMut = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    assert_eq!(again["playback"]["volume"].as_integer(), Some(60));
    assert!(again.get("player").is_none());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[test]
fn saving_removes_old_names_and_keeps_the_new_value() {
    let path = temp_dir("migrate-names").join("config.toml");
    std::fs::write(
        &path,
        "[player]\nautoplay = true\n[ui]\nnotify = false\nart_pixelation = \"4bit\"\n[look]\nretro_mode = \"2bit\"\n",
    )
    .unwrap();
    set_values(&path, &[("playback", "autoplay_last_station", false.into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("autoplay =") && !text.contains("notify") && !text.contains("art_pixelation"), "{text}");
    assert!(text.contains("autoplay_last_station = false"), "{text}");
    assert!(text.contains("song_change_notifications = false"), "the value of `notify` moved: {text}");
    assert!(text.contains("retro_mode = \"2bit\""), "the new key wins: {text}");
    assert!(!text.contains("[ui]") && !text.contains("[player]"), "{text}");
}

#[test]
fn a_new_layout_file_is_saved_in_place_and_a_new_file_gets_the_new_tables() {
    let path = temp_dir("fresh").join("config.toml");
    set_values(
        &path,
        &[("look", "theme_auto_mode", "omarchy".into()), ("account", "email_or_username", "a@b.c".into())],
    )
    .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("[look]") && text.contains("[account]") && !text.contains("[ui]"), "{text}");
    assert!(!text.contains("[pandora]"), "an [account] section, not [pandora]: {text}");
    let (c, w) = Config::load(&path);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!((c.ui.theme(), c.pandora.username.as_str()), ("omarchy", "a@b.c"));
}

#[test]
fn an_odd_old_table_is_left_alone_when_saving() {
    let path = temp_dir("odd").join("config.toml");
    std::fs::write(&path, "ui = 5\n").unwrap();
    // `ui = 5` is not a table: nothing to move, and the new keys still save.
    set_values(&path, &[("look", "theme_select", "gruvbox".into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("ui = 5") && text.contains("theme_select = \"gruvbox\""), "{text}");
}

#[test]
fn the_old_theme_key_becomes_auto_mode_and_select() {
    // REMOVE AT 1.0 (with split_old_theme).
    let (c, w) = Config::parse("[look]\ntheme = \"gruvbox\"\n").unwrap();
    assert!(w.is_empty(), "{w:?}");
    assert_eq!((c.ui.theme_auto_mode, c.ui.theme_select.as_str()), (ThemeAutoMode::Off, "gruvbox"));
    let (c, _) = Config::parse("[ui]\ntheme = \"omarchy\"\n").unwrap();
    assert_eq!((c.ui.theme_auto_mode, c.ui.theme()), (ThemeAutoMode::Omarchy, "omarchy"));
    // The new keys win over the old one.
    let (c, _) =
        Config::parse("[look]\ntheme = \"omarchy\"\ntheme_auto_mode = \"off\"\ntheme_select = \"nord\"\n").unwrap();
    assert_eq!(c.ui.theme(), "nord");
    // A key boxvibe doesn't know is still only a warning.
    let (c, w) = Config::parse("[look]\ntheme_select = \"nord\"\ntheme_auto_mode = \"off\"\nsparkles = 1\n").unwrap();
    assert_eq!((c.ui.theme(), w.len()), ("nord", 1));
    // Saving rewrites it, keeping its comment.
    let path = temp_dir("split-theme").join("config.toml");
    std::fs::write(&path, "[look]\n# mine\ntheme = \"dracula\"\n").unwrap();
    set_values(&path, &[("look", "icons", "emoji".into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("# mine\ntheme_auto_mode = \"off\"") && text.contains("theme_select = \"dracula\""),
        "{text}"
    );
    assert!(!text.contains("theme ="), "{text}");
    assert_eq!(Config::load(&path).0.ui.theme(), "dracula");
}

#[test]
fn utc_stamps() {
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_792_544_100);
    assert_eq!(utc_stamp(at), "20261021T005500Z");
    assert_eq!(utc_stamp(std::time::UNIX_EPOCH), "19700101T000000Z");
}

#[test]
fn the_menu_toggles_default_on_and_save_under_look() {
    let (c, w) = Config::parse("").unwrap();
    assert!(c.ui.favorites_in_menu && c.ui.pinned_in_menu && w.is_empty());
    let (c, w) = Config::parse("[look]\nfavorites_in_menu = false\npinned_in_menu = false\n").unwrap();
    assert!(!c.ui.favorites_in_menu && !c.ui.pinned_in_menu && w.is_empty(), "{w:?}");
    let path = temp_dir("menu-toggles").join("config.toml");
    set_values(&path, &[("look", "pinned_in_menu", false.into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("[look]") && text.contains("pinned_in_menu = false"), "{text}");
}

#[test]
fn animations_default_on_and_load_tolerantly() {
    let (c, w) = Config::parse("").unwrap();
    assert!(c.ui.animations && w.is_empty());
    let (c, _) = Config::parse("[look]\nanimations = false\n").unwrap();
    assert!(!c.ui.animations);
    let (c, w) = Config::parse("[look]\nanimations = \"sometimes\"\nicons = \"emoji\"\n").unwrap();
    assert!(c.ui.animations, "a bad value keeps the default");
    assert_eq!(c.ui.icons, IconSet::Emoji, "and the others still load");
    assert_eq!(w.len(), 1, "{w:?}");
}

#[test]
fn the_boot_screen_moved_to_look_and_the_old_place_still_reads() {
    let (c, w) = Config::parse("[boxvibe]\nsplash = false\n").unwrap();
    assert!(!c.ui.splash && w.is_empty(), "{w:?}");
    let (c, w) = Config::parse("[look]\nsplash = false\n").unwrap();
    assert!(!c.ui.splash && w.is_empty(), "{w:?}");
    // The new place wins over the old one, whichever comes first.
    let (c, _) = Config::parse("[look]\nsplash = true\n[boxvibe]\nsplash = false\n").unwrap();
    assert!(c.ui.splash);
    let (c, _) = Config::parse("[boxvibe]\nsplash = false\n[look]\nsplash = true\n").unwrap();
    assert!(c.ui.splash);
    // A bad value is named by where it should be now.
    let (c, w) = Config::parse("[boxvibe]\nsplash = \"never\"\n").unwrap();
    assert!(c.ui.splash);
    assert!(matches!(&w[..], [ConfigWarning::BadValue { key, .. }] if key == "look.boot_screen"), "{w:?}");
}

#[test]
fn saving_moves_a_boxvibe_splash_to_look_keeping_comments_and_unknown_keys() {
    let path = temp_dir("splash-move").join("config.toml");
    std::fs::write(
            &path,
            "# top\n[boxvibe]\ncheck_updates = false # no\nsplash = false # quiet start\nmystery = 1\n\n[look]\ntheme = \"nord\"\n",
        )
        .unwrap();
    set_values(&path, &[("look", "icons", "emoji".into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let doc: toml_edit::DocumentMut = text.parse().unwrap();
    assert_eq!(doc["look"]["boot_screen"].as_bool(), Some(false), "{text}");
    assert!(doc["boxvibe"].as_table().unwrap().get("splash").is_none(), "{text}");
    assert_eq!(doc["boxvibe"]["mystery"].as_integer(), Some(1), "unknown keys stay");
    assert!(text.contains("# top") && text.contains("# no") && text.contains("# quiet start"), "{text}");
    assert_eq!(text.matches("boot_screen").count(), 1, "{text}");
    assert!(!text.contains("splash") && !text.contains("check_updates"), "{text}");
    // Saved from the Settings entry: the new place is written, no stale copy comes back.
    set_values(&path, &[("look", "boot_screen", true.into())]).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text.matches("boot_screen").count(), 1, "{text}");
    assert!(Config::load(&path).0.ui.splash);
}

#[test]
fn a_retired_dependency_check_key_loads_without_a_warning() {
    let (c, w) = Config::parse("[boxvibe]\ndependency_check = false\ncheck_updates = false\n").unwrap();
    assert!(w.is_empty(), "{w:?}");
    assert!(!c.ui.check_updates, "the rest of the table still applies");
    let (_, w) = Config::parse("[boxvibe]\ndependency_check = \"whatever\"\n").unwrap();
    assert!(w.is_empty(), "{w:?}");
    let (_, w) = Config::parse("[boxvibe]\nsomething_else = 1\n").unwrap();
    assert_eq!(w.len(), 1, "other unknown keys still warn: {w:?}");
}

#[test]
fn a_scratch_directory_name_yields_its_pid() {
    assert_eq!(scratch_pid("boxvibe-test-1234-first-run"), Some(1234));
    assert_eq!(scratch_pid("boxvibe-test-1-a-b-c"), Some(1), "the name may hold dashes");
    assert_eq!(scratch_pid("boxvibe-test-notapid-x"), None);
    assert_eq!(scratch_pid("something-else"), None);
}

#[test]
fn a_dead_runs_scratch_directory_is_swept_and_a_live_one_is_kept() {
    // The sweep runs once per process, so drive the pieces directly: a pid
    // that cannot be running (0 is never a user process) and this one.
    let root = scratch_root();
    let dead = root.join("boxvibe-test-0-swept");
    let live = root.join(format!("boxvibe-test-{}-kept", std::process::id()));
    std::fs::create_dir_all(&dead).unwrap();
    std::fs::create_dir_all(&live).unwrap();

    assert!(!running(0), "pid 0 is not a running process");
    assert!(running(std::process::id()), "this process is running");

    // What the sweep does with each.
    if !running(0) {
        let _ = std::fs::remove_dir_all(&dead);
    }
    assert!(!dead.exists(), "a dead run's directory goes");
    assert!(live.exists(), "a live run's directory stays");

    let _ = std::fs::remove_dir_all(&live);
}
