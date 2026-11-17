//! The account read as calendar years, months and languages.
//!
//! Everything the growth needs from a timeline: per year and month the active
//! days, contributions and contributions per language; per language when it
//! was last used; and the rules that read history: which years count, which
//! languages were dropped, whether the account is resting, and how old
//! foliage thins.

use std::collections::BTreeMap;

use crate::date::Date;
use crate::math;
use crate::timeline::Timeline;

/// A leaf's language; `None` is work GitHub reports without one (private).
pub type Lang = Option<String>;

/// Contributions every year is measured against: a full, busy year.
pub const FULL_YEAR: f64 = 3200.0;

/// Days of silence before a language can count as dropped.
const DROP_AFTER_DAYS: i32 = 52 * 7;
/// Days over which a change in the tree's structure grows in.
pub const RAMP_DAYS: f64 = 91.0;
/// Days of total silence before the tree stands bare for the winter.
const DORMANT_AFTER_DAYS: i32 = crate::DORMANT_AFTER.cast_signed() * 7;
/// Active days the account must keep working without a language to drop it.
const DROP_AFTER_ACTIVE: usize = 30;

/// One calendar month of activity.
#[derive(Debug, Default, Clone)]
pub struct Month {
    /// Active days.
    pub days: usize,
    /// Contributions, private ones included.
    pub total: f64,
    /// Contributions per language.
    pub langs: BTreeMap<String, f64>,
}

impl Month {
    /// The month's busiest language.
    pub fn top(&self) -> Lang {
        self.langs
            .iter()
            .filter(|(_, v)| **v > 0.0)
            .max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(a.0)))
            .map(|(l, _)| l.clone())
    }
}

/// One calendar year of activity.
#[derive(Debug, Default, Clone)]
pub struct Year {
    /// The year.
    pub y: i32,
    /// Its active months, by month number (1 to 12).
    pub months: BTreeMap<u32, Month>,
    /// Active days.
    pub days: usize,
    /// Contributions, private ones included.
    pub total: f64,
    /// Contributions per language.
    pub langs: BTreeMap<String, f64>,
    /// The day it became a year of growth ([`Life::is_active_year`]).
    pub active_on: Option<Date>,
    /// Work per language that was later dropped (a year of silence while the
    /// account kept working): (all of it, weighted by how far it has died
    /// back). It stays dead wood even when the language comes back.
    pub dead: BTreeMap<String, (f64, f64)>,
}

impl Year {
    /// Irregularity of the year's months.
    pub fn cv(&self) -> f64 {
        months_cv(&self.months)
    }
}

/// An account's whole history up to the observed day.
#[derive(Debug, Clone)]
pub struct Life {
    /// Every calendar year from the first to the last active one, quiet ones
    /// included.
    pub years: Vec<Year>,
    /// Lifetime contributions per language, largest first.
    pub lang: Vec<(String, f64)>,
    /// Lifetime contributions with a language.
    pub total: f64,
    /// Coefficient of variation of monthly totals over the whole life.
    pub life_cv: f64,
    first: Date,
    last_active: Date,
    observed: Date,
    last_used: BTreeMap<String, Date>,
    active_days: Vec<Date>,
}

impl Life {
    /// Reads a timeline; `None` without a single active day. Work without
    /// language data inside a declared `periods` entry (first day, last day,
    /// languages) is split evenly over its languages.
    pub fn of(timeline: &Timeline, periods: &[(Date, Date, Vec<String>)]) -> Option<Self> {
        let observed = timeline.observed_on;
        let active: Vec<_> = timeline
            .days
            .iter()
            .filter(|d| d.total > 0.0 && d.date <= observed)
            .collect();
        let first = active.first()?.date;
        let last_active = active.last()?.date;
        let mut years: BTreeMap<i32, Year> = BTreeMap::new();
        let mut last_used = BTreeMap::new();
        let mut uses: BTreeMap<String, Vec<(Date, f64)>> = BTreeMap::new();
        let mut lang: BTreeMap<String, f64> = BTreeMap::new();
        for d in &active {
            let (y, m, _) = d.date.ymd();
            let year = years.entry(y).or_insert_with(|| Year {
                y,
                ..Year::default()
            });
            year.days += 1;
            year.total += d.total;
            if year.active_on.is_none() && (year.days >= 5 || year.total >= 20.0) {
                year.active_on = Some(d.date);
            }
            let month = year.months.entry(m).or_default();
            month.days += 1;
            month.total += d.total;
            let declared = periods
                .iter()
                .find(|(a, b, langs)| *a <= d.date && d.date <= *b && !langs.is_empty())
                .map(|(_, _, langs)| langs);
            let mut by_language = d.by_language.clone();
            if let Some(langs) = declared {
                let each = d.unattributed / langs.len() as f64;
                for l in langs {
                    *by_language.entry(l.clone()).or_default() += each;
                }
            }
            for (l, v) in &by_language {
                if *v <= 0.0 {
                    continue;
                }
                *month.langs.entry(l.clone()).or_default() += v;
                *year.langs.entry(l.clone()).or_default() += v;
                *lang.entry(l.clone()).or_default() += v;
                last_used.insert(l.clone(), d.date);
                uses.entry(l.clone()).or_default().push((d.date, *v));
            }
        }
        // Each stretch of a language's use that ended in a drop is dead wood,
        // dying back over the year after the drop.
        let active_days: Vec<Date> = active.iter().map(|d| d.date).collect();
        for (l, list) in &uses {
            let mut start = 0;
            for k in 0..list.len() {
                let next = list.get(k + 1).map(|u| u.0);
                // Only a year's gap can be a drop.
                if next.unwrap_or(observed).days_since(list[k].0) < DROP_AFTER_DAYS {
                    continue;
                }
                let Some(on) = drop_day(list[k].0, &active_days)
                    .filter(|on| next.map_or(*on <= observed, |n| *on < n))
                else {
                    continue;
                };
                let died = (f64::from(observed.days_since(on)) / f64::from(DROP_AFTER_DAYS))
                    .clamp(0.0, 1.0);
                for &(date, v) in &list[start..=k] {
                    if let Some(year) = years.get_mut(&date.year()) {
                        let e = year.dead.entry(l.clone()).or_default();
                        e.0 += v;
                        e.1 += v * died;
                    }
                }
                start = k + 1;
            }
        }
        let (fy, fm, _) = first.ymd();
        let (ly, lm, _) = last_active.ymd();
        for y in fy..=ly {
            years.entry(y).or_insert_with(|| Year {
                y,
                ..Year::default()
            });
        }
        let years: Vec<Year> = years.into_values().collect();
        let mut series = Vec::new();
        for y in &years {
            let lo = if y.y == fy { fm } else { 1 };
            let hi = if y.y == ly { lm } else { 12 };
            for m in lo..=hi {
                series.push(y.months.get(&m).map_or(0.0, |x| x.total));
            }
        }
        let mut lang: Vec<(String, f64)> = lang.into_iter().collect();
        lang.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let total = lang.iter().map(|l| l.1).sum::<f64>().max(1e-9);
        Some(Self {
            years,
            lang,
            total,
            life_cv: cv(&series),
            first,
            last_active,
            observed,
            last_used,
            active_days,
        })
    }

    /// Whether `lang` was dropped: silent for a year while the account kept
    /// working at least 30 more days without it. A resting account drops
    /// nothing, and a language never used is not dropped.
    pub fn dropped(&self, lang: &str) -> bool {
        self.last_used
            .get(lang)
            .is_some_and(|last| dropped(*last, self.observed, &self.active_days))
    }

    /// Whether the account has worked for under a year, first active day to
    /// last: a seedling, not yet a tree.
    pub fn is_seedling(&self) -> bool {
        self.last_active.days_since(self.first) < 365
    }

    /// How far a young tree has filled out, from 0 the day it stops being a
    /// seedling to 1 a year of work later; it stays young while it rests.
    pub fn maturity(&self) -> f64 {
        (f64::from(self.last_active.days_since(self.first) - 365) / 365.0).clamp(0.0, 1.0)
    }

    /// The observed day as a fractional year (2026.5 is about July 2).
    pub fn now(&self) -> f64 {
        let (y, _, _) = self.observed.ymd();
        let jan1 = Date::from_ymd(y, 1, 1).unwrap_or(self.observed);
        f64::from(y) + f64::from(self.observed.days_since(jan1)) / 365.25
    }

    /// How long ago year `i` grew: from the middle of its part up to the
    /// observed day, in years. It grows a little every day, never in steps.
    pub fn years_ago(&self, i: usize) -> f64 {
        let now = self.now();
        let y = f64::from(self.years[i].y);
        (now - f64::midpoint(y, (y + 1.0).min(now))).max(0.0)
    }

    /// How far `lang` has died back, from 0 the day it was dropped to 1 a
    /// year later: its branches draw back as its dead wood grows in. 0 while
    /// it isn't dropped.
    pub fn drop_progress(&self, lang: &str) -> f64 {
        let Some(&last) = self.last_used.get(lang) else {
            return 0.0;
        };
        if !self.dropped(lang) {
            return 0.0;
        }
        // Dropped on the later of a year after its last use and the 30th
        // active day without it.
        let busy = self
            .active_days
            .iter()
            .filter(|a| **a > last)
            .nth(DROP_AFTER_ACTIVE - 1)
            .copied()
            .unwrap_or(last);
        let on = busy.max(last.add_days(DROP_AFTER_DAYS));
        (f64::from(self.observed.days_since(on)) / f64::from(DROP_AFTER_DAYS)).clamp(0.0, 1.0)
    }

    /// How bare `lang`'s branches stand, 0 to 1: as its leaves fall
    /// ([`crate::foliage::fullness`]) its twigs show. Private work goes by
    /// the account's silence.
    pub fn bare(&self, lang: &Lang) -> f64 {
        let last = lang
            .as_deref()
            .and_then(|l| self.last_used.get(l))
            .copied()
            .unwrap_or(self.last_active);
        1.0 - crate::foliage::fullness(self.observed.days_since(last) / 7)
    }

    /// Years of real work: each calendar year counts fully at 300
    /// contributions or more, in proportion below. A busy account's work
    /// years keep pace with its age; a few contributions over many years
    /// add up to a fraction of one.
    pub fn work_years(&self) -> f64 {
        self.years.iter().map(|y| (y.total / 300.0).min(1.0)).sum()
    }

    /// How much of year `i`'s `lang` work is dead wood: (the share later
    /// dropped, the share that has died back so far), each 0 to 1.
    pub fn dead_share(&self, i: usize, lang: &Lang) -> (f64, f64) {
        let Some(y) = self.years.get(i) else {
            return (0.0, 0.0);
        };
        let total = match lang {
            Some(l) => y.langs.get(l).copied().unwrap_or(0.0),
            None => return (0.0, 0.0),
        };
        lang.as_deref()
            .and_then(|l| y.dead.get(l))
            .map_or((0.0, 0.0), |(full, grown)| {
                (
                    (full / total.max(1e-9)).min(1.0),
                    (grown / total.max(1e-9)).min(1.0),
                )
            })
    }

    /// A language's dead wood over the whole life: (all of it, how much has
    /// died back so far), in contributions.
    pub fn dead_total(&self, lang: &str) -> (f64, f64) {
        self.years
            .iter()
            .filter_map(|y| y.dead.get(lang))
            .fold((0.0, 0.0), |a, d| (a.0 + d.0, a.1 + d.1))
    }

    /// The day the life is seen on.
    pub fn observed(&self) -> Date {
        self.observed
    }

    /// The observed month, as `year * 12 + month`.
    pub fn observed_month(&self) -> i32 {
        let (y, m, _) = self.observed.ymd();
        y * 12 + m.cast_signed()
    }

    /// Years since the first active day.
    pub fn age(&self) -> f64 {
        f64::from(self.observed.days_since(self.first)) / 365.25
    }

    /// Whether the account has been silent for a year: the tree rests.
    pub fn dormant(&self) -> bool {
        self.observed.days_since(self.last_active) >= DORMANT_AFTER_DAYS
    }

    /// Whether year `i` counts as a year of growth: five active days or
    /// twenty contributions. A stray day or two does not.
    pub fn is_active_year(&self, i: usize) -> bool {
        self.years.get(i).is_some_and(|y| y.active_on.is_some())
    }

    /// How far year `i` has grown in, from 0 the day it became a year of
    /// growth to 1 a quarter later: what it adds to the tree arrives over a
    /// quarter, not in one day.
    pub fn grown_in(&self, i: usize) -> f64 {
        self.years
            .get(i)
            .and_then(|y| y.active_on)
            .map_or(0.0, |on| {
                (f64::from(self.observed.days_since(on)) / RAMP_DAYS).clamp(0.0, 1.0)
            })
    }
}

/// The day a language last used on `last_use` counts as dropped if it isn't
/// used again: a year later, once the account has 30 more active days;
/// `None` before it has them.
fn drop_day(last_use: Date, active_days: &[Date]) -> Option<Date> {
    let busy = active_days
        .iter()
        .filter(|a| **a > last_use)
        .nth(DROP_AFTER_ACTIVE - 1)?;
    Some((*busy).max(last_use.add_days(DROP_AFTER_DAYS)))
}

/// Whether a language last used on `last_use` is dropped by `observed`:
/// silent for a year, with at least 30 active days of the account after it.
pub fn dropped(last_use: Date, observed: Date, active_days: &[Date]) -> bool {
    observed.days_since(last_use) >= DROP_AFTER_DAYS
        && active_days.iter().filter(|a| **a > last_use).count() >= DROP_AFTER_ACTIVE
}

/// Coefficient of variation of `xs`; 0 for fewer than two or no activity.
fn cv(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    if mean <= 0.0 {
        return 0.0;
    }
    let var = xs.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
    var.sqrt() / mean
}

/// Irregularity from a coefficient of variation: 0 steady, up to 1.2 bursty.
pub fn irr(cv: f64) -> f64 {
    (cv / 1.5).min(1.2)
}

/// Coefficient of variation of monthly totals from the first to the last
/// month present, quiet months in between counting as zero.
pub fn months_cv(months: &BTreeMap<u32, Month>) -> f64 {
    let (Some(a), Some(b)) = (months.keys().next(), months.keys().last()) else {
        return 0.0;
    };
    let xs: Vec<f64> = (*a..=*b)
        .map(|m| months.get(&m).map_or(0.0, |x| x.total))
        .collect();
    cv(&xs)
}

/// How much of a year's foliage is left `years_ago` on: half after four
/// years, never below half. The wood stays.
pub fn decay(years_ago: f64) -> f64 {
    math::pow(0.5, years_ago.max(0.0) / 4.0).max(0.5)
}

/// Shares of a set of months: each language, plus `None` for contributions
/// that are private. Largest first.
pub fn shares_of(months: &BTreeMap<u32, Month>, total: f64) -> Vec<(Lang, f64)> {
    let mut langs: BTreeMap<String, f64> = BTreeMap::new();
    for m in months.values() {
        for (l, v) in &m.langs {
            *langs.entry(l.clone()).or_default() += v;
        }
    }
    let known: f64 = langs.values().sum();
    let mut out: Vec<(Lang, f64)> = langs
        .into_iter()
        .filter(|(_, v)| *v > 0.0)
        .map(|(l, v)| (Some(l), v))
        .collect();
    let grey = (total - known).max(0.0);
    if grey > 0.5 {
        out.push((None, grey));
    }
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;
    use crate::growth::testkit::snap;
    use crate::timeline::Timeline;

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    fn life(days: &[(Date, u32, &[(&str, u32)])], observed: Date) -> Life {
        Life::of(&Timeline::new(&snap(days, observed)), &[]).unwrap()
    }

    #[test]
    fn a_dropped_language_dies_back_over_a_year() {
        // Go for 100 days, then Python every day.
        let start = d("2020-01-01");
        let go: Vec<_> = (0..100)
            .map(|i| (start.add_days(i), 3, &[("Go", 3)][..]))
            .chain((100..1200).map(|i| (start.add_days(i), 3, &[("Python", 3)][..])))
            .collect();
        let at = |n: i32| {
            let days: Vec<_> = go
                .iter()
                .filter(|x| x.0 <= start.add_days(n))
                .copied()
                .collect();
            life(&days, start.add_days(n)).drop_progress("Go")
        };
        let dropped = 99 + 364;
        assert!(at(dropped - 1).abs() < 1e-9, "not dropped yet");
        assert!(at(dropped).abs() < 1e-9, "dropped today");
        assert!((at(dropped + 182) - 0.5).abs() < 0.01, "half way");
        assert!((at(dropped + 500) - 1.0).abs() < 1e-9, "dead wood");
        assert!(at(1100).abs() > 0.99 && at(300).abs() < 1e-9);
    }

    /// `n` consecutive active days of Python from `from`.
    fn python_days(from: Date, n: i32) -> Vec<(Date, u32, &'static [(&'static str, u32)])> {
        (0..n)
            .map(|i| (from.add_days(i), 3, &[("Python", 3)][..]))
            .collect()
    }

    #[test]
    fn a_language_never_used_is_not_dropped() {
        let l = life(&python_days(d("2020-01-06"), 400), d("2022-01-01"));
        assert!(!l.dropped("Cobol"));
    }

    #[test]
    fn dropped_needs_a_year_of_silence_and_thirty_more_active_days() {
        let start = d("2020-01-06");
        let go: Vec<_> = (0..30)
            .map(|i| (start.add_days(i), 4, &[("Go", 4)][..]))
            .collect();
        let mut days = go.clone();
        days.extend(python_days(start.add_days(400), 40));
        let observed = start.add_days(460);
        assert!(life(&days, observed).dropped("Go"));

        let mut few = go;
        few.extend(python_days(start.add_days(400), 2));
        assert!(!life(&few, observed).dropped("Go"));
    }

    #[test]
    fn a_silent_account_drops_nothing() {
        let start = d("2020-01-06");
        let mut days = python_days(start, 60);
        days.push((start.add_days(61), 2, &[("Go", 2)][..]));
        let l = life(&days, start.add_days(61 + 400));
        assert!(l.dormant());
        assert!(!l.dropped("Go"));
        assert!(!l.dropped("Python"));
    }

    #[test]
    fn stray_days_are_not_years() {
        let mut days = python_days(d("2020-02-03"), 30);
        days.push((d("2021-05-03"), 2, &[][..]));
        days.push((d("2021-05-04"), 1, &[][..]));
        days.extend(python_days(d("2022-03-01"), 5));
        days.push((d("2023-03-01"), 20, &[][..]));
        let l = life(&days, d("2023-06-01"));
        let active = |y: i32| l.is_active_year(l.years.iter().position(|x| x.y == y).unwrap());
        assert!(active(2020));
        assert!(!active(2021), "two days, three contributions");
        assert!(active(2022), "five days");
        assert!(active(2023), "twenty contributions");
    }

    #[test]
    fn decay_halves_every_four_years_with_a_floor_of_half() {
        assert!((decay(0.0) - 1.0).abs() < 1e-12);
        assert!((decay(2.0) - 0.5f64.sqrt()).abs() < 1e-9);
        assert!((decay(4.0) - 0.5).abs() < 1e-9);
        assert!((decay(20.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn declared_periods_color_private_work() {
        let days: Vec<_> = (0..10)
            .map(|i| (d("2020-03-02").add_days(i), 1, &[][..]))
            .collect();
        let t = Timeline::new(&snap(&days, d("2020-03-31")));
        let periods = [(d("2020-01-01"), d("2020-12-31"), vec!["Python".to_string()])];
        let l = Life::of(&t, &periods).unwrap();
        let y = &l.years[0];
        assert_eq!(
            shares_of(&y.months, y.total),
            vec![(Some("Python".to_string()), 10.0)]
        );
    }

    #[test]
    fn private_work_is_its_own_share() {
        let l = life(
            &[(d("2024-03-04"), 10, &[("Rust", 6)][..])],
            d("2024-03-10"),
        );
        let y = &l.years[0];
        assert_eq!(
            shares_of(&y.months, y.total),
            vec![(Some("Rust".to_string()), 6.0), (None, 4.0)]
        );
    }
}
