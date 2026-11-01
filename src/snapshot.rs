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

/// The schema version this crate reads and writes.
pub const SCHEMA: u32 = 1;

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
}

/// One day of contributions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Day {
    /// The UTC date.
    pub date: Date,
    /// The calendar count: every kind of contribution, private ones included.
    pub total: u32,
    /// Commits plus opened PRs, by the repository's primary language.
    #[serde(default)]
    pub by_language: BTreeMap<String, u32>,
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
        if self.schema != SCHEMA {
            return fail(format!(
                "schema {} is not supported; this gitbonsai reads schema {SCHEMA}. Re-run `gitbonsai collect`",
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
        let mut snapshot = self.clone();
        if as_of >= self.observed_on {
            return snapshot;
        }
        snapshot.days.retain(|d| d.date <= as_of);
        snapshot.range.to = as_of.max(snapshot.range.from);
        snapshot.observed_on = snapshot.range.to;
        snapshot.truncated_windows.retain(|w| w.from <= as_of);
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
