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
    /// Months from the first to the last active month, both included.
    pub span_months: i32,
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
        let mut lang: BTreeMap<String, f64> = BTreeMap::new();
        for d in &active {
            let (y, m, _) = d.date.ymd();
            let year = years.entry(y).or_insert_with(|| Year {
                y,
                ..Year::default()
            });
            year.days += 1;
            year.total += d.total;
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
        let month_no = |m: u32| i32::try_from(m).unwrap_or(1);
        let span_months = (ly - fy) * 12 + month_no(lm) - month_no(fm) + 1;
        let mut lang: Vec<(String, f64)> = lang.into_iter().collect();
        lang.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let total = lang.iter().map(|l| l.1).sum::<f64>().max(1e-9);
        Some(Self {
            years,
            lang,
            total,
            life_cv: cv(&series),
            span_months,
            first,
            last_active,
            observed,
            last_used,
            active_days: active.iter().map(|d| d.date).collect(),
        })
    }

    /// Whether `lang` was dropped: silent for a year while the account kept
    /// working at least 30 more days without it. A resting account drops
    /// nothing.
    pub fn dropped(&self, lang: &str) -> bool {
        self.last_used
            .get(lang)
            .is_none_or(|last| dropped(*last, self.observed, &self.active_days))
    }

    /// Years since the first active day.
    pub fn age(&self) -> f64 {
        f64::from(self.observed.days_since(self.first)) / 365.25
    }

    /// Whether the account has been silent for a year: the tree rests.
    pub fn dormant(&self) -> bool {
        self.observed.days_since(self.last_active) >= DROP_AFTER_DAYS
    }

    /// Whether year `i` counts as a year of growth: five active days or
    /// twenty contributions. A stray day or two does not.
    pub fn is_active_year(&self, i: usize) -> bool {
        self.years
            .get(i)
            .is_some_and(|y| y.days >= 5 || y.total >= 20.0)
    }
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
/// without language data. Largest first.
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

    /// `n` consecutive active days of Python from `from`.
    fn python_days(from: Date, n: i32) -> Vec<(Date, u32, &'static [(&'static str, u32)])> {
        (0..n)
            .map(|i| (from.add_days(i), 3, &[("Python", 3)][..]))
            .collect()
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
