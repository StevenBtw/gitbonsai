//! Calendar dates and ISO weeks, without a date library.
//!
//! A [`Date`] is a UTC day, stored as days since 1970-01-01. Weeks start on
//! Monday (ISO 8601) and are numbered by [`Date::week`]: consecutive weeks
//! have consecutive numbers, so week arithmetic is integer arithmetic. The
//! conversions are Howard Hinnant's `days_from_civil` and `civil_from_days`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A UTC calendar day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(i32);

/// The number of a Monday-to-Sunday week: consecutive weeks differ by one.
pub type WeekNo = i32;

impl Date {
    /// The date `year-month-day`, or `None` if there is no such day.
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Option<Self> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        let y = if month <= 2 { year - 1 } else { year };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = month.cast_signed();
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day.cast_signed() - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        Some(Self(era * 146_097 + doe - 719_468))
    }

    /// The year, month and day.
    pub fn ymd(self) -> (i32, u32, u32) {
        let z = self.0 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i32::from(month <= 2);
        #[allow(
            clippy::cast_sign_loss,
            reason = "month is in 1..=12 and day in 1..=31"
        )]
        (year, month as u32, day as u32)
    }

    /// The year.
    pub fn year(self) -> i32 {
        self.ymd().0
    }

    /// Days since 1970-01-01.
    pub fn days(self) -> i32 {
        self.0
    }

    /// The day `n` days later (earlier for negative `n`).
    #[must_use]
    pub fn add_days(self, n: i32) -> Self {
        Self(self.0 + n)
    }

    /// Days from `earlier` to `self`.
    pub fn days_since(self, earlier: Self) -> i32 {
        self.0 - earlier.0
    }

    /// The day of the week, 0 for Monday through 6 for Sunday.
    pub fn weekday(self) -> u32 {
        // 1970-01-01 was a Thursday.
        #[allow(clippy::cast_sign_loss, reason = "rem_euclid(7) is in 0..7")]
        let weekday = (self.0 + 3).rem_euclid(7) as u32;
        weekday
    }

    /// The number of the week containing this day.
    pub fn week(self) -> WeekNo {
        (self.0 + 3).div_euclid(7)
    }

    /// The Monday of week `week`.
    pub fn monday_of(week: WeekNo) -> Self {
        Self(week * 7 - 3)
    }

    /// The ISO 8601 year and week number, for labels such as `2026-W44`.
    ///
    /// # Panics
    ///
    /// Never: every year has a January 1.
    pub fn iso_week(self) -> (i32, u32) {
        let thursday = self.add_days(3 - self.weekday().cast_signed());
        let year = thursday.year();
        let jan1 = Self::from_ymd(year, 1, 1).expect("January 1 exists");
        #[allow(
            clippy::cast_sign_loss,
            reason = "a Thursday is on or after January 1 of its own year"
        )]
        let week = (thursday.days_since(jan1) / 7 + 1) as u32;
        (year, week)
    }

    /// Today in UTC, from the system clock. Only collection reads the clock.
    pub fn today_utc() -> Self {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        #[allow(
            clippy::cast_possible_truncation,
            reason = "days since 1970 fit an i32 for five million years"
        )]
        let days = (seconds / 86_400) as i32;
        Self(days)
    }
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.ymd();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

/// A date that is not `YYYY-MM-DD`, or names a day that doesn't exist.
#[derive(Debug, Clone, thiserror::Error)]
#[error("`{0}` is not a date; expected YYYY-MM-DD")]
pub struct ParseDateError(String);

impl FromStr for Date {
    type Err = ParseDateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseDateError(s.to_string());
        let mut parts = s.splitn(3, '-');
        let mut next = || -> Result<&str, ParseDateError> { parts.next().ok_or_else(err) };
        let (y, m, d) = (next()?, next()?, next()?);
        if y.len() != 4 || m.len() != 2 || d.len() != 2 {
            return Err(err());
        }
        let year = y.parse().map_err(|_| err())?;
        let month = m.parse().map_err(|_| err())?;
        let day = d.parse().map_err(|_| err())?;
        Self::from_ymd(year, month, day).ok_or_else(err)
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// An inclusive range of days.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateRange {
    /// The first day.
    pub from: Date,
    /// The last day, inclusive.
    pub to: Date,
}

impl DateRange {
    /// Whether `date` falls in the range.
    pub fn contains(&self, date: Date) -> bool {
        self.from <= date && date <= self.to
    }

    /// The number of days in the range.
    pub fn len_days(&self) -> i32 {
        self.to.days_since(self.from) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    #[test]
    fn dates_round_trip_through_days() {
        for days in -800_000..800_000 {
            if days % 997 != 0 {
                continue;
            }
            let date = Date(days);
            let (y, m, dd) = date.ymd();
            assert_eq!(Date::from_ymd(y, m, dd), Some(date));
        }
        assert_eq!(d("1970-01-01").days(), 0);
        assert_eq!(d("2026-10-31").to_string(), "2026-10-31");
    }

    #[test]
    fn invalid_dates_are_refused() {
        assert!("2023-02-29".parse::<Date>().is_err());
        assert!("2024-02-29".parse::<Date>().is_ok());
        assert!("2024-13-01".parse::<Date>().is_err());
        assert!("24-01-01".parse::<Date>().is_err());
        assert!("2024-1-01".parse::<Date>().is_err());
    }

    #[test]
    fn weeks_start_on_monday() {
        // 2026-10-26 is a Monday, 2026-11-01 a Sunday.
        assert_eq!(d("2026-10-26").weekday(), 0);
        assert_eq!(d("2026-11-01").weekday(), 6);
        assert_eq!(d("2026-10-26").week(), d("2026-11-01").week());
        assert_eq!(d("2026-11-02").week(), d("2026-11-01").week() + 1);
        assert_eq!(Date::monday_of(d("2026-10-29").week()), d("2026-10-26"));
    }

    #[test]
    fn iso_week_labels_match_the_standard() {
        assert_eq!(d("2026-10-31").iso_week(), (2026, 44));
        // Week 53, and days that belong to the neighbouring ISO year.
        assert_eq!(d("2020-12-31").iso_week(), (2020, 53));
        assert_eq!(d("2021-01-03").iso_week(), (2020, 53));
        assert_eq!(d("2024-12-30").iso_week(), (2025, 1));
        assert_eq!(d("2010-01-01").iso_week(), (2009, 53));
    }
}
