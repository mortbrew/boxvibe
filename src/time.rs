//! Times written the way the site and the logs write them: UTC, ISO 8601.
//! Skip limits use [`day_at_offset`] or [`local_day`], and wall-clock hours.

pub use vibekit::time::{iso_utc, today};

use std::time::{SystemTime, UNIX_EPOCH};

/// `YYYY-MM-DD` for a Unix second, in UTC.
pub fn date_of_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Today's date at a fixed offset from UTC (seconds, east positive).
pub fn day_at_offset(offset_secs: i32) -> String {
    let unix = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    date_of_unix(unix.saturating_add(i64::from(offset_secs)))
}

/// Today's date in the computer's timezone. UTC if the libc call fails.
pub fn local_day() -> String {
    let unix = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    local_date(unix).unwrap_or_else(|| date_of_unix(unix))
}

fn local_date(unix: i64) -> Option<String> {
    #[repr(C)]
    struct Tm {
        sec: i32,
        min: i32,
        hour: i32,
        mday: i32,
        mon: i32,
        year: i32,
        wday: i32,
        yday: i32,
        isdst: i32,
        gmtoff: i64,
        zone: *const i8,
    }
    unsafe extern "C" {
        fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
    }
    let mut tm = std::mem::MaybeUninit::<Tm>::zeroed();
    // localtime_r writes `tm` and returns null on failure.
    let ptr = unsafe { localtime_r(&unix, tm.as_mut_ptr()) };
    if ptr.is_null() {
        return None;
    }
    let tm = unsafe { tm.assume_init() };
    Some(format!("{:04}-{:02}-{:02}", tm.year + 1900, tm.mon + 1, tm.mday))
}

#[cfg(test)]
mod tests;
