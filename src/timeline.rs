//! Days to capped ISO weeks.
//!
//! Attribution happens per day: what the languages don't explain is
//! `unattributed` and feeds the trunk only. Then each Monday-to-Sunday week
//! is capped at [`WEEKLY_CAP`], scaling every amount in it alike, fractions
//! kept. Every lifetime number downstream is a sum of capped amounts.

use std::collections::BTreeMap;

use crate::date::{Date, WeekNo};
use crate::snapshot::Snapshot;

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
    /// The rest: private work, issues, reviews, repos without a language.
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
    /// The first day with a contribution; `None` for an empty account.
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
        let attributed: Vec<CappedDay> = snapshot
            .days
            .iter()
            .filter(|d| d.total > 0 && d.date <= observed_on)
            .map(attribute)
            .collect();
        let attributed = fold_strays(attributed);
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
/// but a lone contribution years earlier doesn't make the tree old.
fn fold_strays(days: Vec<CappedDay>) -> Vec<CappedDay> {
    let start = days.iter().position(|d| {
        let end = d.date.add_days(27);
        days.iter()
            .filter(|x| x.date >= d.date && x.date <= end)
            .map(|x| x.total)
            .sum::<f64>()
            >= SUSTAINED
    });
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
fn attribute(day: &crate::snapshot::Day) -> CappedDay {
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
    let unattributed = (total - explained * scale).max(0.0);
    CappedDay {
        date: day.date,
        total,
        by_language,
        unattributed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::DateRange;
    use crate::snapshot::{Day, SCHEMA};

    fn snapshot(days: &[(&str, u32, &[(&str, u32)])], observed_on: &str) -> Snapshot {
        let days: Vec<Day> = days
            .iter()
            .map(|(date, total, langs)| Day {
                date: date.parse().unwrap(),
                total: *total,
                by_language: langs.iter().map(|(l, v)| ((*l).to_string(), *v)).collect(),
            })
            .collect();
        let observed_on: Date = observed_on.parse().unwrap();
        Snapshot {
            schema: SCHEMA,
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
