//! Leaves: the last 13 weeks.
//!
//! Wood is history, leaves are now. Everything here reads the foliage window
//! (the 91 days ending on `observed_on`) or the last complete week: each
//! language's state, and how dense its leaves are.

use std::collections::BTreeMap;

use crate::date::Date;
use crate::growth::life::Life;
use crate::math;
use crate::timeline::{Timeline, WEEKLY_CAP, WINDOW_WEEKS};

/// Complete silent weeks after which a language's branches can turn to jin,
/// once it is also dropped.
pub const JIN_AFTER: i32 = 52;

/// Leaf density for an average of `v` contributions per week: 0 to 1, with
/// 1,000 per week at the top.
pub fn density(v: f64) -> f64 {
    math::ln(1.0 + v.clamp(0.0, WEEKLY_CAP) / 10.0) / math::ln(101.0)
}

/// How much of a language's foliage is left after `silent` complete weeks
/// without it: all of it through the 13-week window, then less each week,
/// none after a year. Leaves fall before wood dies or a tree rests.
pub fn fullness(silent: i32) -> f64 {
    let window = WINDOW_WEEKS;
    let year = f64::from(JIN_AFTER);
    (1.0 - (f64::from(silent) - window) / (year - window)).clamp(0.0, 1.0)
}

/// Each language's foliage left ([`fullness`]), by its last complete week
/// with work.
pub fn fullness_by_language(timeline: &Timeline) -> BTreeMap<String, f64> {
    let weeks = timeline.complete_weeks();
    let last = weeks.last().map_or(0, |w| w.no);
    let mut seen: BTreeMap<String, i32> = BTreeMap::new();
    for week in weeks {
        for (language, amount) in &week.by_language {
            if *amount > 0.0 {
                seen.insert(language.clone(), week.no);
            }
        }
    }
    seen.into_iter()
        .map(|(l, week)| (l, fullness(last - week)))
        .collect()
}

/// What a language's branches look like now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageState {
    /// Contributions in the window: leaves.
    Alive,
    /// None in the window: bare wood.
    Bare,
    /// Dropped: 52 silent weeks while the account kept working at least 30
    /// more days without it. Bleached deadwood, permanently.
    Jin,
}

/// The foliage window, as weekly averages.
#[derive(Debug, Clone, Default)]
pub struct Window {
    /// Every contribution.
    pub total: f64,
    /// Unattributed.
    pub unattributed: f64,
    /// By language.
    pub by_language: BTreeMap<String, f64>,
}

impl Window {
    /// The window of `timeline`, divided by 13 weeks.
    pub fn of(timeline: &Timeline) -> Self {
        let mut window = Self::default();
        for day in timeline.window_days() {
            window.total += day.total / WINDOW_WEEKS;
            window.unattributed += day.unattributed / WINDOW_WEEKS;
            for (language, amount) in &day.by_language {
                *window.by_language.entry(language.clone()).or_default() += amount / WINDOW_WEEKS;
            }
        }
        window
    }

    /// The weekly average of `language` (or of everything, for `None`).
    pub fn of_language(&self, language: Option<&str>) -> f64 {
        match language {
            Some(l) => self.by_language.get(l).copied().unwrap_or(0.0),
            None => self.total,
        }
    }
}

/// Every language's state, and the order languages first appeared in.
/// Languages declared for private work in `periods` count as used then, as
/// they do in the tree. Languages without a complete week yet (picked up this
/// week, or only declared) come after the others.
pub fn language_states(
    timeline: &Timeline,
    window: &Window,
    periods: &[(Date, Date, Vec<String>)],
) -> (BTreeMap<String, LanguageState>, Vec<String>) {
    let weeks = timeline.complete_weeks();
    let last = weeks.last().map_or(0, |w| w.no);
    let mut last_active: BTreeMap<String, i32> = BTreeMap::new();
    let mut order = Vec::new();
    for week in weeks {
        for (language, amount) in &week.by_language {
            if *amount > 0.0 {
                if !last_active.contains_key(language) {
                    order.push(language.clone());
                }
                last_active.insert(language.clone(), week.no);
            }
        }
    }
    // Dropped means the account kept working without it, by the rule the
    // tree follows; a resting account drops nothing.
    let life = Life::of(timeline, periods);
    let dropped = |language: &str| life.as_ref().is_some_and(|l| l.dropped(language));
    let mut states: BTreeMap<String, LanguageState> = last_active
        .into_iter()
        .map(|(language, week)| {
            let state = if window.of_language(Some(&language)) > 0.0 {
                LanguageState::Alive
            } else if last - week >= JIN_AFTER && dropped(&language) {
                LanguageState::Jin
            } else {
                LanguageState::Bare
            };
            (language, state)
        })
        .collect();
    if let Some(life) = &life {
        for (language, _) in &life.lang {
            if !states.contains_key(language) {
                let state = if window.of_language(Some(language)) > 0.0 {
                    LanguageState::Alive
                } else if dropped(language) {
                    LanguageState::Jin
                } else {
                    LanguageState::Bare
                };
                states.insert(language.clone(), state);
                order.push(language.clone());
            }
        }
    }
    (states, order)
}

/// Contributions in the last complete week, in total.
pub fn last_week_total(timeline: &Timeline) -> f64 {
    timeline.last_complete_week().map_or(0.0, |w| w.total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_thin_from_the_window_to_a_year_of_silence() {
        assert!((fullness(0) - 1.0).abs() < 1e-9);
        assert!((fullness(13) - 1.0).abs() < 1e-9, "the window keeps them");
        assert!((fullness(32) - 0.5).abs() < 0.02, "half way");
        assert!(fullness(52).abs() < 1e-9, "gone after a year");
        assert!(fullness(80).abs() < 1e-9);
    }

    /// The density table.
    #[test]
    fn density_follows_the_documented_curve() {
        let pct = |v: f64| (density(v) * 100.0).round();
        assert_eq!(pct(1.0), 2.0);
        assert_eq!(pct(10.0), 15.0);
        assert_eq!(pct(100.0), 52.0);
        assert_eq!(pct(1000.0), 100.0);
        assert_eq!(pct(5000.0), 100.0);
    }
}
