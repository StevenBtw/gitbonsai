//! The snapshot: everything collection learned about an account.
//!
//! A snapshot is the only input of the tree besides the config. It is sparse:
//! `days` lists only days with contributions, and every other date in `range`
//! is zero. Collection guarantees the range is complete.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::color::Rgb;
use crate::date::{Date, DateRange};

/// The schema version this crate reads and writes: 3 adds the dates of
/// achievements and archived repositories, which gitbonsai 0.2 can't read.
pub const SCHEMA: u32 = 3;

/// Snapshots before 0.2.1: no private count, so every contribution the
/// languages leave is taken as private work.
pub const LEGACY_SCHEMA: u32 = 1;

/// An account's contribution history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    /// The schema version, [`SCHEMA`].
    pub schema: u32,
    /// The login at collection time. Informational: renames don't matter.
    pub login: String,
    /// GitHub's numeric account id: the identity, and the seed of every
    /// random choice in the tree.
    pub account_id: u64,
    /// The days covered.
    pub range: DateRange,
    /// The last complete UTC day at collection time.
    pub observed_on: Date,
    /// Which crate version collected this.
    pub collected_with: String,
    /// GitHub's color for each language; `None` if it has none.
    pub language_colors: BTreeMap<String, Option<Rgb>>,
    /// Days with at least one contribution, in date order.
    pub days: Vec<Day>,
    /// Windows whose contributions could not all be attributed.
    #[serde(default)]
    pub truncated_windows: Vec<DateRange>,
    /// GitHub achievements by slug (`pull-shark`, `starstruck`, ...), tier 1
    /// to 4, from the public achievements page.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub achievements: BTreeMap<String, u8>,
    /// Archived repositories the account owns, oldest archive first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub archived_repos: Vec<String>,
    /// The day each achievement tier was reached, tier 1 first: tier 1 from
    /// its "first unlocked" date, higher tiers from the first run that saw
    /// them. Without dates an achievement stays out of past views.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub achievement_dates: BTreeMap<String, Vec<Date>>,
    /// The day each archived repository was archived. Without a date it stays
    /// out of past views.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub archived_dates: BTreeMap<String, Date>,
    /// The day growth starts, kept by a past view cut from a longer history
    /// ([`Snapshot::truncated`]); never written to a file.
    #[serde(skip)]
    pub start: Option<Date>,
}

/// One day of contributions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Day {
    /// The UTC date.
    pub date: Date,
    /// The calendar count: every kind of contribution, private ones included.
    pub total: u32,
    /// Commits, opened PRs and issues, reviews and created repositories, by
    /// the repository's primary language.
    #[serde(default)]
    pub by_language: BTreeMap<String, u32>,
    /// Private contributions (schema 2): GitHub's private count for the
    /// window, spread over its days. Zero is left out of the file.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub private: u32,
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl Snapshot {
    /// Reads and validates a snapshot file.
    ///
    /// # Errors
    ///
    /// When the file can't be read, isn't a snapshot, or breaks its invariants.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            Error::Snapshot(format!(
                "can't read {}: {e}. Create one with `gitbonsai collect <login> --output {}`",
                path.display(),
                path.display()
            ))
        })?;
        let snapshot: Self = serde_json::from_str(&text).map_err(|e| {
            Error::Snapshot(format!("{} is not a valid snapshot: {e}", path.display()))
        })?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    /// The snapshot as pretty JSON, ending in a newline.
    ///
    /// # Panics
    ///
    /// Never: every field serializes.
    pub fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("a snapshot always serializes");
        json.push('\n');
        json
    }

    /// Checks the invariants the rest of the pipeline relies on.
    ///
    /// # Errors
    ///
    /// When the schema, range or day order is wrong.
    pub fn validate(&self) -> Result<(), Error> {
        let fail = |msg: String| Err(Error::Snapshot(msg));
        if !(LEGACY_SCHEMA..=SCHEMA).contains(&self.schema) {
            return fail(format!(
                "schema {} is not supported; this gitbonsai reads schemas {LEGACY_SCHEMA} to {SCHEMA}. Re-run `gitbonsai collect`",
                self.schema
            ));
        }
        if self.range.from > self.range.to {
            return fail(format!(
                "range starts after it ends: {} > {}",
                self.range.from, self.range.to
            ));
        }
        if self.observed_on != self.range.to {
            return fail(format!(
                "observed_on {} must be the last day of the range, {}",
                self.observed_on, self.range.to
            ));
        }
        let mut previous: Option<Date> = None;
        for day in &self.days {
            if previous.is_some_and(|p| p >= day.date) {
                return fail(format!(
                    "days must be in date order without repeats; {} is out of place",
                    day.date
                ));
            }
            if !self.range.contains(day.date) {
                return fail(format!(
                    "day {} is outside the range {}..{}",
                    day.date, self.range.from, self.range.to
                ));
            }
            previous = Some(day.date);
        }
        Ok(())
    }

    /// The snapshot as it would have been collected on `as_of`: later days
    /// dropped and the range ended there (`--as-of`).
    #[must_use]
    pub fn truncated(&self, as_of: Date) -> Self {
        if as_of >= self.observed_on {
            return self.clone();
        }
        self.with_start().cut(as_of)
    }

    /// The snapshot keeping the start its whole history gives: cut from it,
    /// a past tree folds its stray days into that day as today's does, so
    /// it never starts earlier and then grows younger. Before its start the
    /// pot is empty.
    #[must_use]
    pub(crate) fn with_start(&self) -> Self {
        let mut snapshot = self.clone();
        snapshot.start = self
            .start
            .or_else(|| crate::timeline::Timeline::new(self).first_active);
        snapshot
    }

    /// [`Snapshot::truncated`] for a snapshot already [`Snapshot::with_start`].
    #[must_use]
    pub(crate) fn cut(&self, as_of: Date) -> Self {
        let mut snapshot = self.clone();
        if as_of >= self.observed_on {
            return snapshot;
        }
        snapshot.days.retain(|d| d.date <= as_of);
        // A day before the range shows that day: an empty pot.
        snapshot.range.from = snapshot.range.from.min(as_of);
        snapshot.range.to = as_of;
        snapshot.observed_on = as_of;
        snapshot.truncated_windows.retain(|w| w.from <= as_of);
        // Only what was reached by then; an undated one is left out.
        let reached = |slug: &str| {
            self.achievement_dates
                .get(slug)
                .map_or(0, |d| d.iter().filter(|d| **d <= as_of).count())
        };
        snapshot.achievements = self
            .achievements
            .iter()
            .filter_map(|(slug, &tier)| {
                let by_then = u8::try_from(reached(slug)).unwrap_or(u8::MAX).min(tier);
                (by_then > 0).then(|| (slug.clone(), by_then))
            })
            .collect();
        snapshot
            .archived_repos
            .retain(|r| self.archived_dates.get(r).is_some_and(|d| *d <= as_of));
        snapshot
    }

    /// The first day with a contribution.
    pub fn first_active(&self) -> Option<Date> {
        self.days.iter().find(|d| d.total > 0).map(|d| d.date)
    }

    /// The last day with a contribution.
    pub fn last_active(&self) -> Option<Date> {
        self.days.iter().rev().find(|d| d.total > 0).map(|d| d.date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal(schema: u32) -> Snapshot {
        let day: Date = "2026-10-05".parse().unwrap();
        Snapshot {
            schema,
            login: "t".into(),
            account_id: 1,
            range: DateRange { from: day, to: day },
            observed_on: day,
            collected_with: "test".into(),
            language_colors: BTreeMap::new(),
            days: vec![Day {
                date: day,
                total: 3,
                by_language: BTreeMap::new(),
                private: 0,
            }],
            truncated_windows: Vec::new(),
            achievements: BTreeMap::new(),
            archived_repos: Vec::new(),
            achievement_dates: BTreeMap::new(),
            archived_dates: BTreeMap::new(),
            start: None,
        }
    }

    #[test]
    fn a_past_view_keeps_the_start_the_whole_history_gives() {
        let d = |s: &str| -> Date { s.parse().unwrap() };
        let day = |date: Date, total: u32| Day {
            date,
            total,
            by_language: [("Rust".to_string(), total)].into(),
            private: 0,
        };
        let mut s = minimal(SCHEMA);
        let start = d("2003-08-05");
        s.days = vec![day(d("2001-04-09"), 1), day(d("2002-03-01"), 1)];
        s.days.extend((0..20).map(|k| day(start.add_days(k), 2)));
        s.range = DateRange {
            from: d("2001-01-01"),
            to: d("2003-12-31"),
        };
        s.observed_on = d("2003-12-31");
        let before = crate::timeline::Timeline::new(&s.truncated(d("2003-08-01")));
        assert_eq!(
            before.first_active, None,
            "before its start the pot is empty"
        );
        let after = s.truncated(d("2003-08-10"));
        assert!(after.validate().is_ok());
        let timeline = crate::timeline::Timeline::new(&after);
        assert_eq!(timeline.first_active, Some(start));
        assert!(
            (timeline.days[0].total - 4.0).abs() < 1e-9,
            "the strays fold into the start"
        );
    }

    #[test]
    fn a_day_before_the_range_shows_that_day() {
        let early: Date = "2026-01-01".parse().unwrap();
        let s = minimal(SCHEMA).truncated(early);
        assert_eq!(s.observed_on, early, "labelled with the day asked for");
        assert_eq!((s.range.from, s.range.to), (early, early));
        assert_eq!(s.days, []);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn the_past_has_only_the_achievements_and_archives_of_its_day() {
        let d = |s: &str| -> Date { s.parse().unwrap() };
        let mut s = minimal(SCHEMA);
        s.achievements = [("pull-shark", 3), ("yolo", 1), ("starstruck", 2)]
            .map(|(k, v)| (k.to_string(), v))
            .into();
        s.achievement_dates = [
            (
                "pull-shark",
                vec![d("2024-12-13"), d("2025-06-01"), d("2026-09-01")],
            ),
            ("yolo", vec![d("2026-03-01")]),
        ]
        .map(|(k, v)| (k.to_string(), v))
        .into();
        s.archived_repos = vec!["old".into(), "undated".into(), "later".into()];
        s.archived_dates = [("old", d("2023-01-01")), ("later", d("2026-02-01"))]
            .map(|(k, v)| (k.to_string(), v))
            .into();
        let past = s.truncated(d("2025-07-01"));
        assert_eq!(
            past.achievements,
            [("pull-shark".to_string(), 2)].into(),
            "tier 2 by then; yolo later; starstruck undated"
        );
        assert_eq!(past.archived_repos, ["old"]);
        let now = s.truncated(s.observed_on);
        assert_eq!(now.achievements.len(), 3, "today shows everything");
        assert_eq!(now.archived_repos.len(), 3);
    }

    #[test]
    fn schema_one_and_two_load() {
        assert!(minimal(LEGACY_SCHEMA).validate().is_ok());
        assert!(minimal(2).validate().is_ok(), "0.2 snapshots still load");
        assert!(minimal(SCHEMA).validate().is_ok());
        assert!(minimal(SCHEMA + 1).validate().is_err());
        assert_eq!(SCHEMA, 3);
    }

    #[test]
    fn a_day_without_private_work_leaves_the_key_out() {
        let json = minimal(SCHEMA).to_json();
        assert!(!json.contains("private"), "{json}");
        let mut with = minimal(SCHEMA);
        with.days[0].private = 2;
        let back: Snapshot = serde_json::from_str(&with.to_json()).unwrap();
        assert_eq!(back.days[0].private, 2);
        let old = json.replace("\"schema\": 2", "\"schema\": 1");
        let legacy: Snapshot = serde_json::from_str(&old).unwrap();
        assert_eq!(legacy.days[0].private, 0, "schema 1 has no private field");
    }
}
