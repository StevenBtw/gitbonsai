//! Days to capped ISO weeks.
//!
//! Attribution happens per day: private work the languages don't explain is
//! `unattributed` (silver leaves); other work without a language joins the
//! week's languages. Then each Monday-to-Sunday week
//! is capped at [`WEEKLY_CAP`], scaling every amount in it alike, fractions
//! kept. Every lifetime number downstream is a sum of capped amounts.

use std::collections::BTreeMap;

use crate::date::{Date, WeekNo};
use crate::snapshot::{LEGACY_SCHEMA, Snapshot};

/// The top of every activity scale: contributions per week.
pub const WEEKLY_CAP: f64 = 1000.0;

/// The foliage window in days: 13 weeks ending on `observed_on`.
pub const WINDOW_DAYS: i32 = 91;

/// The foliage window in weeks, the divisor of every window average.
pub const WINDOW_WEEKS: f64 = 13.0;

/// One day after attribution and the weekly cap.
#[derive(Debug, Clone, PartialEq)]
pub struct CappedDay {
    /// The UTC date.
    pub date: Date,
    /// Every contribution.
    pub total: f64,
    /// Contributions explained by a repository language.
    pub by_language: BTreeMap<String, f64>,
    /// Private work: contributions without a language, shown silver.
    pub unattributed: f64,
}

/// One ISO week after the cap.
#[derive(Debug, Clone, PartialEq)]
pub struct Week {
    /// The week's number ([`Date::week`]).
    pub no: WeekNo,
    /// Every contribution, capped.
    pub total: f64,
    /// By language, capped.
    pub by_language: BTreeMap<String, f64>,
    /// Unattributed, capped.
    pub unattributed: f64,
    /// Days with at least one contribution.
    pub active_days: u8,
    /// Whether the week ended on or before `observed_on`.
    pub complete: bool,
}

/// An account's history as capped days and weeks.
#[derive(Debug, Clone)]
pub struct Timeline {
    /// The last complete UTC day.
    pub observed_on: Date,
    /// The first day with a contribution; `None` for an empty account, or
    /// for a past view cut before the day its whole history starts.
    pub first_active: Option<Date>,
    /// Active days only, in date order.
    pub days: Vec<CappedDay>,
    /// Every week from the one containing the first activity through the
    /// one containing `observed_on`, including empty weeks. The last may be
    /// in progress.
    pub weeks: Vec<Week>,
}

impl Timeline {
    /// Attributes and caps a snapshot.
    pub fn new(snapshot: &Snapshot) -> Self {
        let observed_on = snapshot.observed_on;
        let legacy = snapshot.schema == LEGACY_SCHEMA;
        let attributed: Vec<(CappedDay, f64)> = snapshot
            .days
            .iter()
            .filter(|d| d.total > 0 && d.date <= observed_on)
            .map(|d| attribute(d, legacy))
            .collect();
        let attributed = fold_strays(spread_rest(attributed), snapshot.start);
        let first_active = attributed.first().map(|d| d.date);

        // Raw weekly totals decide each week's scale factor.
        let mut raw_totals: BTreeMap<WeekNo, f64> = BTreeMap::new();
        for day in &attributed {
            *raw_totals.entry(day.date.week()).or_default() += day.total;
        }
        let scale = |week: WeekNo| {
            let raw = raw_totals.get(&week).copied().unwrap_or(0.0);
            if raw > WEEKLY_CAP {
                WEEKLY_CAP / raw
            } else {
                1.0
            }
        };

        let days: Vec<CappedDay> = attributed
            .into_iter()
            .map(|mut day| {
                let s = scale(day.date.week());
                if s < 1.0 {
                    day.total *= s;
                    day.unattributed *= s;
                    day.by_language.values_mut().for_each(|v| *v *= s);
                }
                day
            })
            .collect();

        let mut weeks = Vec::new();
        if let Some(first) = first_active {
            let last_week = observed_on.week();
            let mut cursor = days.iter().peekable();
            for no in first.week()..=last_week {
                let mut week = Week {
                    no,
                    total: 0.0,
                    by_language: BTreeMap::new(),
                    unattributed: 0.0,
                    active_days: 0,
                    complete: Date::monday_of(no).add_days(6) <= observed_on,
                };
                while let Some(day) = cursor.next_if(|d| d.date.week() == no) {
                    week.total += day.total;
                    week.unattributed += day.unattributed;
                    week.active_days += 1;
                    for (language, amount) in &day.by_language {
                        *week.by_language.entry(language.clone()).or_default() += amount;
                    }
                }
                weeks.push(week);
            }
        }

        Self {
            observed_on,
            first_active,
            days,
            weeks,
        }
    }

    /// The complete weeks: the ones wood grows from.
    pub fn complete_weeks(&self) -> &[Week] {
        let n = self.weeks.iter().take_while(|w| w.complete).count();
        &self.weeks[..n]
    }

    /// Age in complete weeks, from the week of the first activity through
    /// the last complete week.
    pub fn age_weeks(&self) -> u32 {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "fewer than 4 billion weeks"
        )]
        let age = self.complete_weeks().len() as u32;
        age
    }

    /// The capped lifetime through `observed_on`, in-progress week included.
    pub fn capped_lifetime(&self) -> f64 {
        self.days.iter().map(|d| d.total).sum()
    }

    /// The active days in the foliage window: the 91 days ending on
    /// `observed_on`.
    pub fn window_days(&self) -> impl Iterator<Item = &CappedDay> {
        let from = self.observed_on.add_days(1 - WINDOW_DAYS);
        self.days.iter().filter(move |d| d.date >= from)
    }

    /// The last complete week, if any.
    pub fn last_complete_week(&self) -> Option<&Week> {
        self.complete_weeks().last()
    }
}

/// Contributions within 28 days that make activity sustained.
pub const SUSTAINED: f64 = 10.0;

/// Age starts at the first sustained activity: the first active day whose
/// next 28 days hold at least [`SUSTAINED`] contributions. Stray days before
/// it are folded into that day, so they still count toward lifetime and girth
/// but a lone contribution years earlier doesn't make the tree old. A past
/// view passes the start its whole history gives: before it, nothing grows.
fn fold_strays(days: Vec<CappedDay>, start: Option<Date>) -> Vec<CappedDay> {
    let start = match start {
        Some(start) => match days.iter().position(|d| d.date >= start) {
            Some(i) => Some(i),
            None => return Vec::new(),
        },
        None => days.iter().position(|d| {
            let end = d.date.add_days(27);
            days.iter()
                .filter(|x| x.date >= d.date && x.date <= end)
                .map(|x| x.total)
                .sum::<f64>()
                >= SUSTAINED
        }),
    };
    let Some(start) = start.filter(|&i| i > 0) else {
        return days;
    };
    let mut days = days;
    let strays: Vec<CappedDay> = days.drain(..start).collect();
    let first = &mut days[0];
    for day in strays {
        first.total += day.total;
        first.unattributed += day.unattributed;
        for (language, amount) in day.by_language {
            *first.by_language.entry(language).or_default() += amount;
        }
    }
    days
}

/// Attribution for one day: languages explain what they can,
/// scaled down if GitHub counted them above the calendar total.
/// What the languages leave splits into private work (silver) and the rest,
/// returned beside the day: work in public without a language (discussions,
/// repositories without one). A schema 1 day takes all of it as private.
fn attribute(day: &crate::snapshot::Day, legacy: bool) -> (CappedDay, f64) {
    let total = f64::from(day.total);
    let explained: f64 = day.by_language.values().map(|&v| f64::from(v)).sum();
    let scale = if explained > total {
        total / explained
    } else {
        1.0
    };
    let by_language: BTreeMap<String, f64> = day
        .by_language
        .iter()
        .filter(|(_, v)| **v > 0)
        .map(|(language, &v)| (language.clone(), f64::from(v) * scale))
        .collect();
    let leftover = (total - explained * scale).max(0.0);
    let private = if legacy {
        leftover
    } else {
        f64::from(day.private).min(leftover)
    };
    (
        CappedDay {
            date: day.date,
            total,
            by_language,
            unattributed: private,
        },
        leftover - private,
    )
}

/// Each day's public work without a language joins its week's languages,
/// in their shares that week; a week without any borrows the nearest week
/// that has some, the earlier one on a tie. Only an account without any
/// language keeps it unattributed.
fn spread_rest(days: Vec<(CappedDay, f64)>) -> Vec<CappedDay> {
    let mut shares: BTreeMap<WeekNo, BTreeMap<String, f64>> = BTreeMap::new();
    for (day, _) in &days {
        for (language, amount) in &day.by_language {
            *shares
                .entry(day.date.week())
                .or_default()
                .entry(language.clone())
                .or_default() += amount;
        }
    }
    days.into_iter()
        .map(|(mut day, rest)| {
            if rest <= 0.0 {
                return day;
            }
            let week = day.date.week();
            let nearest = shares
                .iter()
                .filter(|(_, langs)| langs.values().sum::<f64>() > 0.0)
                .min_by_key(|(no, _)| ((**no - week).abs(), **no > week))
                .map(|(_, langs)| langs);
            match nearest {
                Some(langs) => {
                    let sum: f64 = langs.values().sum();
                    for (language, amount) in langs {
                        *day.by_language.entry(language.clone()).or_default() +=
                            rest * amount / sum;
                    }
                }
                None => day.unattributed += rest,
            }
            day
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::DateRange;
    use crate::snapshot::{Day, LEGACY_SCHEMA};

    fn day<'a>(t: &'a Timeline, date: &str) -> &'a CappedDay {
        let date: Date = date.parse().unwrap();
        t.days.iter().find(|d| d.date == date).unwrap()
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn schema_one_keeps_all_leftover_silver() {
        let t = Timeline::new(&snapshot(
            &[("2026-10-05", 10, &[("Rust", 6)])],
            "2026-10-11",
        ));
        assert!(near(day(&t, "2026-10-05").unattributed, 4.0));
    }

    #[test]
    fn only_private_work_is_silver() {
        let t = Timeline::new(&snapshot2(
            &[("2026-10-05", 10, 1, &[("Rust", 6)])],
            "2026-10-11",
        ));
        let d = day(&t, "2026-10-05");
        assert!(near(d.unattributed, 1.0), "{d:?}");
        assert!(near(d.by_language["Rust"], 9.0), "{d:?}");
        // GitHub may count more private work than is left: silver takes
        // only what the languages leave.
        let t = Timeline::new(&snapshot2(
            &[("2026-10-05", 5, 3, &[("Rust", 4)])],
            "2026-10-11",
        ));
        assert!(near(day(&t, "2026-10-05").unattributed, 1.0));
    }

    #[test]
    fn language_less_work_joins_the_week_languages() {
        let t = Timeline::new(&snapshot2(
            &[
                ("2026-10-05", 4, 0, &[("Rust", 4)]),
                ("2026-10-06", 6, 0, &[("Go", 2)]),
            ],
            "2026-10-11",
        ));
        let d = day(&t, "2026-10-06");
        assert!(near(d.unattributed, 0.0), "{d:?}");
        assert!(near(d.by_language["Rust"], 8.0 / 3.0), "{d:?}");
        assert!(near(d.by_language["Go"], 2.0 + 4.0 / 3.0), "{d:?}");
        assert!(near(d.total, 6.0));
    }

    #[test]
    fn a_week_without_languages_borrows_the_nearest() {
        let t = Timeline::new(&snapshot2(
            &[
                ("2026-09-28", 3, 0, &[("Rust", 3)]),
                ("2026-10-05", 5, 0, &[]),
                ("2026-10-12", 2, 0, &[("Go", 2)]),
            ],
            "2026-10-18",
        ));
        let d = day(&t, "2026-10-05");
        assert!(
            near(d.by_language["Rust"], 5.0),
            "the earlier week wins a tie: {d:?}"
        );
        assert!(!d.by_language.contains_key("Go"));
    }

    #[test]
    fn an_account_without_languages_stays_silver() {
        let t = Timeline::new(&snapshot2(&[("2026-10-05", 5, 0, &[])], "2026-10-11"));
        let d = day(&t, "2026-10-05");
        assert!(
            near(d.unattributed, 5.0) && d.by_language.is_empty(),
            "{d:?}"
        );
    }

    #[test]
    fn private_is_capped_with_the_week() {
        let t = Timeline::new(&snapshot2(
            &[("2026-10-05", 2000, 400, &[("Rust", 1600)])],
            "2026-10-11",
        ));
        let d = day(&t, "2026-10-05");
        assert!(
            near(d.unattributed, 200.0) && near(d.by_language["Rust"], 800.0),
            "{d:?}"
        );
    }

    /// A schema 1 snapshot: everything the languages leave is silver.
    fn snapshot(days: &[(&str, u32, &[(&str, u32)])], observed_on: &str) -> Snapshot {
        let days: Vec<(&str, u32, u32, &[(&str, u32)])> =
            days.iter().map(|(d, t, l)| (*d, *t, 0, *l)).collect();
        let mut s = snapshot2(&days, observed_on);
        s.schema = LEGACY_SCHEMA;
        s
    }

    /// A schema 2 snapshot from `(date, total, private, languages)` days.
    fn snapshot2(days: &[(&str, u32, u32, &[(&str, u32)])], observed_on: &str) -> Snapshot {
        let days: Vec<Day> = days
            .iter()
            .map(|(date, total, private, langs)| Day {
                date: date.parse().unwrap(),
                total: *total,
                by_language: langs.iter().map(|(l, v)| ((*l).to_string(), *v)).collect(),
                private: *private,
            })
            .collect();
        let observed_on: Date = observed_on.parse().unwrap();
        Snapshot {
            schema: 2,
            login: "test".into(),
            account_id: 1,
            range: DateRange {
                from: days.first().map_or(observed_on, |d| d.date),
                to: observed_on,
            },
            observed_on,
            collected_with: "test".into(),
            language_colors: BTreeMap::new(),
            days,
            truncated_windows: Vec::new(),
            achievements: BTreeMap::new(),
            archived_repos: Vec::new(),
            achievement_dates: BTreeMap::new(),
            archived_dates: BTreeMap::new(),
            start: None,
        }
    }

    #[test]
    fn what_languages_dont_explain_is_unattributed() {
        let t = Timeline::new(&snapshot(
            &[("2026-10-05", 5, &[("Rust", 3)])],
            "2026-10-11",
        ));
        assert_eq!(t.days[0].unattributed, 2.0);
        assert_eq!(t.days[0].by_language["Rust"], 3.0);
    }

    #[test]
    fn languages_counted_above_the_total_scale_down() {
        let t = Timeline::new(&snapshot(
            &[("2026-10-05", 4, &[("Rust", 6), ("Go", 2)])],
            "2026-10-11",
        ));
        assert_eq!(t.days[0].by_language["Rust"], 3.0);
        assert_eq!(t.days[0].by_language["Go"], 1.0);
        assert_eq!(t.days[0].unattributed, 0.0);
    }

    #[test]
    fn a_week_over_the_cap_scales_every_amount_alike() {
        let t = Timeline::new(&snapshot(
            &[
                ("2026-10-05", 1000, &[("Rust", 1000)]),
                ("2026-10-06", 1000, &[]),
            ],
            "2026-10-11",
        ));
        let week = &t.weeks[0];
        assert_eq!(week.total, 1000.0);
        assert_eq!(week.by_language["Rust"], 500.0);
        assert_eq!(week.unattributed, 500.0);
        assert!(week.complete);
        assert_eq!(week.active_days, 2);
    }

    #[test]
    fn the_week_containing_observed_on_is_in_progress() {
        // 2026-10-31 is a Saturday.
        let t = Timeline::new(&snapshot(
            &[("2026-10-19", 1, &[]), ("2026-10-30", 1, &[])],
            "2026-10-31",
        ));
        assert_eq!(t.weeks.len(), 2);
        assert!(t.weeks[0].complete);
        assert!(!t.weeks[1].complete);
        assert_eq!(t.age_weeks(), 1);
        assert_eq!(t.capped_lifetime(), 2.0);
    }

    #[test]
    fn stray_contributions_count_but_do_not_age_the_tree() {
        let t = Timeline::new(&snapshot(
            &[
                ("2018-04-14", 1, &[]),
                ("2026-10-05", 6, &[]),
                ("2026-10-12", 6, &[]),
            ],
            "2026-10-31",
        ));
        assert_eq!(t.first_active.unwrap().to_string(), "2026-10-05");
        assert_eq!(t.capped_lifetime(), 13.0);
    }

    #[test]
    fn an_empty_account_has_no_weeks() {
        let t = Timeline::new(&snapshot(&[], "2026-10-31"));
        assert!(t.first_active.is_none());
        assert_eq!(t.weeks, []);
        assert_eq!(t.age_weeks(), 0);
    }
}
