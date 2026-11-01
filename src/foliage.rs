//! Leaves: the last 13 weeks.
//!
//! Wood is history, leaves are now. Everything here reads the foliage window
//! (the 91 days ending on `observed_on`) or the last complete week: each
//! language's state, and how dense its leaves are.

use std::collections::BTreeMap;

use crate::math;
use crate::timeline::{Timeline, WEEKLY_CAP, WINDOW_WEEKS};

/// Complete silent weeks after which a language's branches turn to jin.
pub const JIN_AFTER: i32 = 52;

/// Leaf density for an average of `v` contributions per week: 0 to 1, with
/// 1,000 per week at the top.
pub fn density(v: f64) -> f64 {
    math::ln(1.0 + v.clamp(0.0, WEEKLY_CAP) / 10.0) / math::ln(101.0)
}

/// What a language's branches look like now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageState {
    /// Contributions in the window: leaves.
    Alive,
    /// None in the window: bare wood.
    Bare,
    /// 52 silent weeks: bleached deadwood, permanently.
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
pub fn language_states(
    timeline: &Timeline,
    window: &Window,
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
    let states = last_active
        .into_iter()
        .map(|(language, week)| {
            let state = if window.of_language(Some(&language)) > 0.0 {
                LanguageState::Alive
            } else if last - week >= JIN_AFTER {
                LanguageState::Jin
            } else {
                LanguageState::Bare
            };
            (language, state)
        })
        .collect();
    (states, order)
}

/// Contributions in the last complete week, in total.
pub fn last_week_total(timeline: &Timeline) -> f64 {
    timeline.last_complete_week().map_or(0.0, |w| w.total)
}

#[cfg(test)]
mod tests {
    use super::*;

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
