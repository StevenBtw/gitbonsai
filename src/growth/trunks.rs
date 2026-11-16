//! Which languages become trunks.
//!
//! A language with at least 23% of the lifetime contributions that have
//! language data (private work does not count) earns its own trunk: one trunk is common, two happen, three are rare, four almost never.
//! The busiest is the main trunk. A trunk whose language was dropped becomes
//! deadwood. A first-year seedling is always a single stem.

use crate::growth::life::{Lang, Life};

/// The lifetime share a language needs for its own trunk.
const TRUNK_SHARE: f64 = 0.23;

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
}

/// The trunks of `life`, main trunk first.
pub fn trunks(life: &Life) -> Vec<Trunk> {
    let mut out: Vec<Trunk> = life
        .lang
        .iter()
        .filter(|(_, v)| v / life.total >= TRUNK_SHARE)
        .take(4)
        .map(|(l, v)| Trunk {
            lang: Some(l.clone()),
            share: v / life.total,
            dropped: life.dropped(l),
        })
        .collect();
    if life.span_months < 12 {
        out.truncate(1);
    }
    if out.is_empty() {
        out.push(Trunk {
            lang: life.lang.first().map(|(l, _)| l.clone()),
            share: 1.0,
            dropped: life.lang.first().is_some_and(|(l, _)| life.dropped(l)),
        });
    }
    out
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
        assert_eq!(
            langs(&trunks(&fixture_life("comeback", None))),
            [Some("Python"), Some("Rust")]
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
    fn an_account_without_languages_has_one_plain_trunk() {
        let start: crate::date::Date = "2021-01-04".parse().unwrap();
        let days: Vec<_> = (0..400).map(|i| (start.add_days(i), 2, &[][..])).collect();
        let life = Life::of(&Timeline::new(&snap(&days, start.add_days(400))), &[]).unwrap();
        assert_eq!(langs(&trunks(&life)), [None]);
    }
}
