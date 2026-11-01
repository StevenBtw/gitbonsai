//! # gitbonsai
//!
//! Draws a GitHub account as a Japanese maple bonsai, grown from its public
//! activity. Age makes the tree tall, contributions make it thick, the
//! languages you work in color the leaves, and achievements leave small
//! easter eggs in the pot.
//!
//! The pipeline: [`collect`] is the only step with I/O. It writes a
//! [`Snapshot`]. Everything after it is a pure function of the snapshot, the
//! [`Config`] and an optional as-of date:
//!
//! ```text
//! snapshot -> timeline -> skeleton growth -> scene -> lit painting -> SVG
//! ```
//!
//! ```
//! use gitbonsai::{Bonsai, Config, Theme, fixtures};
//!
//! let snapshot = fixtures::by_name("steady").unwrap().snapshot;
//! let bonsai = Bonsai::grow(&snapshot, &Config::default(), None);
//! let svg = gitbonsai::render::svg(&bonsai, &Config::default(), Theme::Light);
//! assert!(svg.starts_with("<svg"));
//! ```

#![deny(unsafe_code)]

pub mod collect;
pub mod color;
pub mod config;
pub mod date;
pub mod eggs;
mod error;
pub mod explain;
pub mod extras;
pub mod fixtures;
pub mod foliage;
pub mod growth;
pub mod html;
pub mod legend;
pub mod maple;
pub mod math;
pub mod noise;
pub mod output;
pub mod paint;
#[cfg(feature = "png")]
pub mod raster;
pub mod render;
pub mod rng;
pub mod scene;
pub mod snapshot;
pub mod style;
pub mod timeline;

use std::collections::BTreeMap;
use std::fmt::Write as _;

pub use config::{Config, Style};
pub use error::Error;
pub use render::Theme;
pub use snapshot::Snapshot;

use date::{Date, DateRange};
use extras::Extras;
use foliage::{LanguageState, Window};
use growth::Skeleton;
use maple::MaplePalette;
use style::Leader;
use timeline::Timeline;

/// The crate version, as recorded in snapshots and SVG metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Weeks of total silence before the canopy goes dormant.
pub const DORMANT_AFTER: u32 = 52;

/// A grown tree: every derived number, before painting.
#[derive(Debug, Clone)]
pub struct Bonsai {
    /// The login, for the title.
    pub login: String,
    /// The seed of every random choice.
    pub account_id: u64,
    /// The day the tree shows.
    pub observed_on: Date,
    /// The style, made concrete for this account.
    pub leader: Leader,
    /// Capped days and weeks.
    pub timeline: Timeline,
    /// The wood.
    pub skeleton: Skeleton,
    /// The foliage window, as weekly averages.
    pub window: Window,
    /// Each language's state now.
    pub states: BTreeMap<String, LanguageState>,
    /// Languages in the order they first appeared.
    pub order: Vec<String>,
    /// Leaf colors.
    pub maple: MaplePalette,
    /// Contributions in the last complete week.
    pub last_week_total: f64,
    /// Levels, streaks, moss, pot tier.
    pub extras: Extras,
    /// GitHub achievements by slug, tier 1 to 4.
    pub achievements: BTreeMap<String, u8>,
    /// Archived repositories, oldest archive first.
    pub archived: Vec<String>,
    /// Windows collection could not fully attribute.
    pub truncated_windows: Vec<DateRange>,
}

impl Bonsai {
    /// Grows the tree for `snapshot` as of `as_of` (default: its
    /// `observed_on`). Reads no clock and no network.
    pub fn grow(snapshot: &Snapshot, config: &Config, as_of: Option<Date>) -> Self {
        let snapshot = match as_of {
            Some(date) => snapshot.truncated(date),
            None => snapshot.clone(),
        };
        let leader = Leader::new(config.tree.style, snapshot.account_id, config.tree.wind);
        let timeline = Timeline::new(&snapshot);
        let window = Window::of(&timeline);
        let (states, order) = foliage::language_states(&timeline, &window);
        let skeleton = growth::grow(&timeline, snapshot.account_id, &leader, &order);
        let maple = MaplePalette::new(
            &window,
            &order,
            &snapshot.language_colors,
            &config.leaves.colors,
        );
        let extras = Extras::new(&timeline, timeline.age_weeks());
        Self {
            login: snapshot.login.clone(),
            account_id: snapshot.account_id,
            observed_on: snapshot.observed_on,
            leader,
            last_week_total: foliage::last_week_total(&timeline),
            timeline,
            skeleton,
            window,
            states,
            order,
            maple,
            extras,
            achievements: snapshot.achievements.clone(),
            archived: snapshot.archived_repos.clone(),
            truncated_windows: snapshot.truncated_windows.clone(),
        }
    }

    /// No contribution ever: a sprout in a training pot.
    pub fn is_empty(&self) -> bool {
        self.timeline.first_active.is_none()
    }

    /// The state of `language`'s branches; `None` is the leader and crown,
    /// alive while there is any activity in the window.
    pub fn state(&self, language: Option<&str>) -> LanguageState {
        match language {
            None if self.window.total > 0.0 => LanguageState::Alive,
            None => LanguageState::Bare,
            Some(l) => self.states.get(l).copied().unwrap_or(LanguageState::Bare),
        }
    }

    /// Complete weeks since the last contribution.
    pub fn silent_weeks(&self) -> Option<u32> {
        let weeks = self.timeline.complete_weeks();
        let last = weeks.iter().rposition(|w| w.total > 0.0)?;
        u32::try_from(weeks.len() - 1 - last).ok()
    }

    /// A year of total silence: the canopy is bare. The trunk never becomes jin.
    pub fn dormant(&self) -> bool {
        self.window.total <= 0.0 && self.silent_weeks().is_some_and(|w| w >= DORMANT_AFTER)
    }

    /// The text of the SVG's `<desc>`, also good for the README's `alt`.
    pub fn description(&self) -> String {
        if self.is_empty() {
            return "A maple bonsai seedling, waiting for its first contribution.".into();
        }
        let age_weeks = self.timeline.age_weeks();
        let years = f64::from(age_weeks) / 52.18;
        let age = if years >= 2.0 {
            format!("{} years", years.floor())
        } else if age_weeks >= 9 {
            format!("{} months", (years * 12.0).floor())
        } else {
            format!("{age_weeks} weeks")
        };
        let mut text = format!(
            "Japanese maple bonsai grown over {age}, level {}.",
            self.extras.level
        );
        let mut ranked: Vec<(&String, &f64)> = self.window.by_language.iter().collect();
        ranked.sort_by(|a, c| c.1.total_cmp(a.1).then(a.0.cmp(c.0)));
        let top: Vec<&str> = ranked.iter().take(3).map(|(l, _)| l.as_str()).collect();
        if !top.is_empty() {
            let _ = write!(text, " Leaves of {}.", top.join(", "));
        }
        let jin = self
            .states
            .values()
            .filter(|s| **s == LanguageState::Jin)
            .count();
        if jin > 0 {
            let _ = write!(
                text,
                " {} turned to deadwood.",
                plural(jin, "language has", "languages have")
            );
        }
        if self.dormant() {
            text.push_str(" Dormant, waiting for spring.");
        }
        text
    }
}

fn count_word(n: usize) -> String {
    const WORDS: [&str; 10] = [
        "No", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine",
    ];
    WORDS
        .get(n)
        .map_or_else(|| n.to_string(), |w| (*w).to_string())
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", count_word(n), if n == 1 { one } else { many })
}
