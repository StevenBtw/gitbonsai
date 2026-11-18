//! Which languages become trunks.
//!
//! A language with at least 23% of the lifetime contributions that have
//! language data (private work does not count) earns its own trunk, and keeps
//! it until its share falls below 15%: one trunk is common, two happen, three
//! are rare, four almost never.
//! The busiest is the main trunk. A trunk whose language was dropped becomes
//! deadwood. A first-year seedling is always a single stem.

use std::collections::BTreeMap;

use crate::growth::life::{Lang, Life};

/// The lifetime share a language needs for its own trunk.
const TRUNK_SHARE: f64 = 0.23;
/// The share below which a trunk goes back to being a branch: well below
/// the share it took to become one, so a trunk doesn't come and go.
const KEEP_SHARE: f64 = 0.15;

/// One trunk: its language, lifetime share, and whether it was dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct Trunk {
    /// The language; `None` for an account with only private work.
    pub lang: Lang,
    /// Share of lifetime contributions with a language; 1 when no language
    /// reaches a trunk's share.
    pub share: f64,
    /// The language was dropped: the trunk is deadwood.
    pub dropped: bool,
    /// The month it became a trunk, as `year * 12 + month`.
    pub since: i32,
}

/// Month by month from the end of the first year, when trunks first grow: a
/// language becomes a trunk at 23% of the work so far and stays one until it
/// falls below 15%. Today's shares always count. The trunks at the end with
/// the month each became one, and the trunk languages after every month.
fn walk(life: &Life) -> (BTreeMap<&str, i32>, Vec<(i32, Vec<String>)>) {
    let mut so_far: BTreeMap<&str, f64> = BTreeMap::new();
    let mut is_trunk: BTreeMap<&str, i32> = BTreeMap::new();
    let mut history = Vec::new();
    let months: Vec<_> = life
        .years
        .iter()
        .flat_map(|y| {
            y.months
                .iter()
                .map(move |(m, month)| (y.y * 12 + m.cast_signed(), month))
        })
        .collect();
    let first = months.first().map_or(0, |m| m.0);
    for (i, (no, month)) in months.iter().enumerate() {
        for (l, v) in &month.langs {
            *so_far.entry(l).or_default() += v;
        }
        if no - first < 11 && i + 1 < months.len() {
            continue;
        }
        let total: f64 = so_far.values().sum::<f64>().max(1e-9);
        for (l, v) in &so_far {
            if v / total >= TRUNK_SHARE {
                is_trunk.entry(l).or_insert(*no);
            } else if v / total < KEEP_SHARE {
                is_trunk.remove(l);
            }
        }
        history.push((*no, is_trunk.keys().map(|l| (*l).to_string()).collect()));
    }
    (is_trunk, history)
}

/// The trunk languages after every month (`year * 12 + month`), from the
/// first month trunks grow.
pub fn history(life: &Life) -> Vec<(i32, Vec<String>)> {
    walk(life).1
}

/// The trunks of `life`, main trunk first.
pub fn trunks(life: &Life) -> Vec<Trunk> {
    let (is_trunk, _) = walk(life);
    let mut out: Vec<Trunk> = life
        .lang
        .iter()
        .filter_map(|(l, v)| Some((l, v, *is_trunk.get(l.as_str())?)))
        .take(4)
        .map(|(l, v, since)| Trunk {
            lang: Some(l.clone()),
            share: v / life.total,
            dropped: life.dropped(l),
            since,
        })
        .collect();
    if life.is_seedling() {
        out.truncate(1);
    }
    if out.is_empty() {
        out.push(Trunk {
            lang: life.lang.first().map(|(l, _)| l.clone()),
            share: 1.0,
            dropped: life.lang.first().is_some_and(|(l, _)| life.dropped(l)),
            since: 0,
        });
    }
    out
}

/// A spare strand along the trunk, claimed by a language that grew past
/// 10% of the work: it comes away from the trunk as its language works, and
/// leans out under the weight of a quick change.
#[derive(Debug, Clone, PartialEq)]
pub struct Strand {
    /// The language that claimed it.
    pub lang: String,
    /// How far it has come away from the trunk, 0 to 1.
    pub peel: f64,
    /// How far it leans out under its own weight, 0 to 1: a language that
    /// grew fast weighs on young wood, and wood once bent stays bent.
    pub sag: f64,
    /// The largest share of all the work its language has reached.
    pub share: f64,
}

/// The language that founded the trunk, and the strands claimed so far in
/// the order they were claimed. The first language to reach 10% is the
/// trunk's own and claims none.
pub fn strands(life: &Life) -> (Option<String>, Vec<Strand>) {
    const CLAIM: f64 = 0.10;
    // Every calendar month up to today, quiet ones included, so a strand
    // keeps coming away (and its lean keeps easing) through a break.
    let active: BTreeMap<i32, &BTreeMap<String, f64>> = life
        .years
        .iter()
        .flat_map(|y| {
            y.months
                .iter()
                .map(move |(m, month)| (y.y * 12 + m.cast_signed(), &month.langs))
        })
        .collect();
    let empty = BTreeMap::new();
    let first = active.keys().next().copied().unwrap_or(0);
    let now = life.observed_month();
    let months: Vec<(i32, &BTreeMap<String, f64>)> = (first..=now.max(first))
        .map(|no| (no, active.get(&no).copied().unwrap_or(&empty)))
        .collect();
    let mut so_far: BTreeMap<&str, f64> = BTreeMap::new();
    let mut history: Vec<(i32, BTreeMap<&str, f64>)> = Vec::new();
    let mut founder: Option<&str> = None;
    // language, peel, sag, the largest share it reached
    let mut claimed: Vec<(&str, f64, f64, f64)> = Vec::new();
    for (k, (no, langs)) in months.iter().enumerate() {
        for (l, v) in *langs {
            *so_far.entry(l).or_default() += v;
        }
        let total: f64 = so_far.values().sum::<f64>().max(1e-9);
        let share: BTreeMap<&str, f64> = so_far.iter().map(|(l, v)| (*l, v / total)).collect();
        history.push((*no, share.clone()));
        // The last twelve months' work, by language.
        let mut recent: BTreeMap<&str, f64> = BTreeMap::new();
        for (m, ls) in &months[..=k] {
            if no - m < 12 {
                for (l, v) in *ls {
                    *recent.entry(l).or_default() += v;
                }
            }
        }
        let recent_total: f64 = recent.values().sum::<f64>().max(1e-9);
        let mut order: Vec<(&str, f64)> = share.iter().map(|(l, v)| (*l, *v)).collect();
        order.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
        for (l, v) in order {
            if v >= CLAIM && founder.is_none() {
                founder = Some(l);
            } else if v >= CLAIM && founder != Some(l) && !claimed.iter().any(|c| c.0 == l) {
                claimed.push((l, 0.0, 0.0, 0.0));
            }
        }
        // Each month a strand comes away as far as its language worked,
        // and leans under the share it gained in six months; young wood
        // straightens slowly once the load stops growing.
        for c in &mut claimed {
            let r = recent.get(c.0).copied().unwrap_or(0.0) / recent_total;
            c.1 = (c.1 + 0.35 * r).min(1.0);
            let now_share = share.get(c.0).copied().unwrap_or(0.0);
            let before = history
                .iter()
                .rev()
                .find(|h| no - h.0 >= 6)
                .and_then(|h| h.1.get(c.0))
                .copied()
                .unwrap_or(0.0);
            let momentum = ((now_share - before) * 2.0).clamp(0.0, 1.0);
            // A strand only ever hangs further: once bent, wood stays bent.
            c.2 = c.2.max(momentum);
            c.3 = c.3.max(now_share);
        }
    }
    let claimed = claimed
        .into_iter()
        .map(|(l, peel, sag, share)| Strand {
            lang: l.to_string(),
            peel,
            sag,
            share,
        })
        .collect();
    (founder.map(str::to_string), claimed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::growth::life::Life;
    use crate::growth::testkit::snap;
    use crate::timeline::Timeline;

    fn langs(t: &[Trunk]) -> Vec<Option<&str>> {
        t.iter().map(|t| t.lang.as_deref()).collect()
    }

    fn fixture_life(name: &str, as_of: Option<&str>) -> Life {
        let mut snapshot = fixtures::by_name(name).unwrap().snapshot;
        if let Some(day) = as_of {
            snapshot = snapshot.truncated(day.parse().unwrap());
        }
        Life::of(&Timeline::new(&snapshot), &[]).unwrap()
    }

    #[test]
    fn one_trunk_is_common_two_happen() {
        assert_eq!(trunks(&fixture_life("steady", Some("2022-03-06"))).len(), 1);
        assert_eq!(trunks(&fixture_life("mono", None)).len(), 1);
        // JavaScript led before the break: dropped since, it stays a snag.
        assert_eq!(
            langs(&trunks(&fixture_life("comeback", None))),
            [Some("Python"), Some("Rust"), Some("JavaScript")]
        );
        assert_eq!(
            langs(&trunks(&fixture_life("veteran", None))),
            [Some("Rust"), Some("TypeScript")]
        );
    }

    #[test]
    fn three_trunk_languages_stay_apart() {
        let start: crate::date::Date = "2021-01-04".parse().unwrap();
        let days: Vec<_> = (0..900)
            .map(|i| {
                let l = ["Go", "Rust", "Python"][usize::try_from(i % 3).unwrap()];
                (start.add_days(i), 3, vec![(l, 3)])
            })
            .collect();
        let days: Vec<_> = days.iter().map(|(d, t, l)| (*d, *t, &l[..])).collect();
        let life = Life::of(&Timeline::new(&snap(&days, start.add_days(900))), &[]).unwrap();
        assert_eq!(trunks(&life).len(), 3);
    }

    #[test]
    fn a_trunk_stays_until_its_share_falls_well_below() {
        let start: crate::date::Date = "2021-01-04".parse().unwrap();
        // 400 days of Go 3 and Rust 1 (Rust 25%), then Go alone.
        let days: Vec<(crate::date::Date, u32, Vec<(&str, u32)>)> = (0..1000)
            .map(|i| {
                let langs = if i < 400 {
                    vec![("Go", 3), ("Rust", 1)]
                } else {
                    vec![("Go", 3)]
                };
                (start.add_days(i), langs.iter().map(|l| l.1).sum(), langs)
            })
            .collect();
        let days: Vec<_> = days.iter().map(|(d, t, l)| (*d, *t, &l[..])).collect();
        let at = |n: i32| {
            let life = Life::of(
                &Timeline::new(&snap(
                    &days[..usize::try_from(n).unwrap()],
                    start.add_days(n),
                )),
                &[],
            );
            langs(&trunks(&life.unwrap()))
                .iter()
                .map(|l| l.unwrap_or("-").to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(at(400), ["Go", "Rust"], "25%: a trunk");
        assert_eq!(at(600), ["Go", "Rust"], "18%: still a trunk");
        assert_eq!(at(1000), ["Go"], "13%: no longer");
    }

    #[test]
    fn an_account_without_languages_has_one_plain_trunk() {
        let start: crate::date::Date = "2021-01-04".parse().unwrap();
        let days: Vec<_> = (0..400).map(|i| (start.add_days(i), 2, &[][..])).collect();
        let life = Life::of(&Timeline::new(&snap(&days, start.add_days(400))), &[]).unwrap();
        assert_eq!(langs(&trunks(&life)), [None]);
    }
}
