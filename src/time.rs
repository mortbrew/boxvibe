//! Times written the way the site and the logs write them: UTC, ISO 8601.

use std::time::{SystemTime, UNIX_EPOCH};

/// `2026-10-25T14:03:05.123Z`.
pub fn iso_utc(at: SystemTime) -> String {
    let d = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    let (days, rest) = (d.as_secs() / 86_400, d.as_secs() % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3600,
        rest / 60 % 60,
        rest % 60,
        d.subsec_millis()
    )
}

/// Today, `2026-10-29` (UTC).
pub fn today() -> String {
    iso_utc(SystemTime::now())[..10].to_owned()
}

#[cfg(test)]
mod tests;
