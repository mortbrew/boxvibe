use super::*;

#[test]
fn every_failure_domain_has_a_word_and_no_two_share_one() {
    // `ALL` is written by hand, so this catches a variant added without a word
    // (the match in `what` would not compile) and two domains that would read
    // the same on the status line.
    let mut words: Vec<&str> = Failed::ALL.iter().map(|f| f.what()).collect();
    assert!(words.iter().all(|w| !w.is_empty()), "{words:?}");
    words.sort_unstable();
    let before = words.len();
    words.dedup();
    assert_eq!(words.len(), before, "two domains share a word: {words:?}");
}

#[test]
fn a_failure_carries_its_domain_and_message() {
    let reply = failed(Failed::Playlist, Error::Http("offline".into()));
    match reply {
        Reply::Failed { what, message } => {
            assert_eq!(what, Failed::Playlist);
            assert_eq!(what.what(), "playlist");
            assert!(message.contains("offline"), "{message}");
        }
        other => panic!("expected a failure, got {}", matches!(other, Reply::Failed { .. })),
    }
}
