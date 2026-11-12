use super::*;

#[test]
fn a_line_is_one_line_whatever_it_was_given() {
    let dir = crate::config::scratch_dir(&format!("boxvibe-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("boxvibe.log");
    append(&file, CAP, "a\nb\tc");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "a b c\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_full_log_moves_aside_and_starts_again() {
    let dir = crate::config::scratch_dir(&format!("boxvibe-log-cap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("boxvibe.log");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, vec![b'x'; CAP as usize + 1]).unwrap();
    append(&file, CAP, "after the cap");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "after the cap\n");
    assert!(file.with_extension("log.old").exists(), "the old log is kept");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_head_of_a_payload_is_hex() {
    assert_eq!(head(&[0xff, 0xfb, 0x90]), "ff fb 90");
    assert_eq!(head(&[]), "");
}
