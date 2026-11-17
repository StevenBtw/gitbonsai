//! Test helpers for the growth modules.

use std::collections::BTreeMap;

use crate::date::{Date, DateRange};
use crate::snapshot::{Day, SCHEMA, Snapshot};

/// A snapshot from `(date, total, [(language, amount)])` days.
pub fn snap(days: &[(Date, u32, &[(&str, u32)])], observed: Date) -> Snapshot {
    Snapshot {
        schema: SCHEMA,
        login: "t".into(),
        account_id: 1,
        range: DateRange {
            from: days.first().map_or(observed, |d| d.0),
            to: observed,
        },
        observed_on: observed,
        collected_with: "test".into(),
        language_colors: BTreeMap::new(),
        days: days
            .iter()
            .map(|(date, total, langs)| Day {
                date: *date,
                total: *total,
                by_language: langs.iter().map(|(l, v)| ((*l).to_string(), *v)).collect(),
                // What the languages leave is private work, as in fixtures.
                private: total.saturating_sub(langs.iter().map(|(_, v)| v).sum()),
            })
            .collect(),
        truncated_windows: Vec::new(),
        achievements: BTreeMap::new(),
        archived_repos: Vec::new(),
        achievement_dates: BTreeMap::new(),
        archived_dates: BTreeMap::new(),
    }
}
