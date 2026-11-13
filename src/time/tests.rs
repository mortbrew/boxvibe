use super::*;
use std::time::{Duration, SystemTime};

#[test]
fn dates_are_written_in_utc() {
    let at = |s: u64, ms: u64| SystemTime::UNIX_EPOCH + Duration::from_millis(s * 1000 + ms);
    assert_eq!(iso_utc(at(0, 0)), "1970-01-01T00:00:00.000Z");
    assert_eq!(iso_utc(at(951_782_400, 7)), "2000-02-29T00:00:00.007Z");
    assert_eq!(iso_utc(at(1_709_251_199, 999)), "2024-02-29T23:59:59.999Z");
}

#[test]
fn today_is_the_first_ten_characters_of_now() {
    let today = today();
    assert_eq!(today.len(), 10, "{today}");
    assert_eq!(&today[4..5], "-");
    assert_eq!(&today[7..8], "-");
    assert!(today.starts_with(&iso_utc(SystemTime::now())[..4]), "{today}");
}
