use super::*;

const FONTS: &str = "DejaVu Sans\nNoto Sans,Noto Sans Display\nJetBrainsMono Nerd Font,JetBrainsMono Nerd Font Mono\n";
const PLAIN: &str = "DejaVu Sans\nNoto Sans,Noto Sans Display\nLiberation Mono\nFont Awesome 6 Free\n";

fn plain_facts() -> Facts {
    Facts { families: Some(PLAIN.into()), glyph_fonts: Some(String::new()), notify_send: false, keychain: Some(false) }
}

#[test]
fn nerd_font_is_found_by_family_name() {
    assert_eq!(nerd_font(Some(FONTS), None), Presence::Present);
    for family in [
        "Symbols Nerd Font",
        "Symbols Nerd Font Mono",
        "Hack Nerd Font Propo",
        "FiraCode NF",
        "Mono NFM",
        "x,JetBrainsMono NFP",
    ] {
        assert_eq!(nerd_font(Some(family), None), Presence::Present, "{family}");
    }
    assert_eq!(nerd_font(Some(PLAIN), None), Presence::Missing, "FontAwesome and friends are not a Nerd Font");
    assert_eq!(
        nerd_font(Some("Inform Sans\nSans NFL Condensed"), None),
        Presence::Missing,
        "NF has to be a word of its own"
    );
    assert_eq!(nerd_font(Some(""), Some("")), Presence::Missing, "fontconfig with no fonts at all");
}

#[test]
fn nerd_font_is_found_by_its_glyphs_when_the_name_gives_nothing_away() {
    // A renamed font that has the logo and the station glyph.
    assert_eq!(nerd_font(Some(PLAIN), Some("My Patched Mono\n")), Presence::Present);
    assert_eq!(nerd_font(Some(PLAIN), Some("\n")), Presence::Missing);
    // The glyph query alone is no use without the answer to the first one.
    assert_eq!(nerd_font(None, Some("My Patched Mono\n")), Presence::Unknown);
}

#[test]
fn no_fontconfig_means_unknown_and_no_nag() {
    assert_eq!(nerd_font(None, None), Presence::Unknown);
    let facts = Facts { notify_send: true, ..Facts::default() };
    let list = checks(true, &facts);
    assert_eq!((list[0].presence, list[2].presence), (Presence::Unknown, Presence::Unknown));
    assert!(list.iter().all(Check::ok), "can't tell is not missing");
}

#[test]
fn every_item_is_listed_with_whether_it_is_there() {
    let mut facts = plain_facts();
    let list = checks(true, &facts);
    assert_eq!(list.iter().map(|c| c.dep).collect::<Vec<_>>(), [Dep::NerdFont, Dep::NotifySend, Dep::Keychain]);
    assert_eq!(list.iter().map(Check::ok).collect::<Vec<_>>(), [false, false, false]);
    facts.families = Some(FONTS.into());
    assert_eq!(checks(true, &facts).iter().map(Check::ok).collect::<Vec<_>>(), [true, false, false]);
    facts.notify_send = true;
    facts.keychain = Some(true);
    assert!(checks(true, &facts).iter().all(Check::ok));
}

#[test]
fn the_font_is_needed_only_with_the_nerd_icon_set_and_notify_send_and_the_keychain_never() {
    let facts = plain_facts();
    let needed = |nerd| checks(nerd, &facts).iter().map(|c| c.needed).collect::<Vec<_>>();
    assert_eq!(needed(true), [true, false, false]);
    assert_eq!(needed(false), [false, false, false], "emoji icons need no Nerd Font");
}

#[test]
fn the_probe_gives_up_instead_of_hanging() {
    // A limit of nothing can't wait for fc-list: unknown, and unknown says nothing.
    let start = std::time::Instant::now();
    let _ = probe(Duration::ZERO);
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn what_each_dependency_says_for_itself() {
    for dep in [Dep::NerdFont, Dep::NotifySend, Dep::Keychain] {
        assert!(!dep.name().is_empty() && !dep.why().is_empty() && !dep.package().is_empty());
    }
    assert!(Dep::NerdFont.package().starts_with("ttf-nerd-fonts-symbols"));
    assert!(Dep::NotifySend.package().starts_with("libnotify"));
    assert!(Dep::Keychain.package().contains("gnome-keyring") && Dep::Keychain.package().contains("KeePassXC"));
}
