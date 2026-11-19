//! # gitbonsai
//!
//! Draws a GitHub account as a Japanese maple bonsai, grown from its public
//! activity. A newcomer is a thin sprout; age makes the tree short, stout,
//! wide and full, never tall. Each main language grows its own trunk, older
//! years become lower limbs under a crown of the recent years, the languages
//! you work in color the leaves (private work silver), and achievements
//! leave small easter eggs in the pot.
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
pub mod row;
pub mod scene;
pub mod snapshot;
pub mod sprout;
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

/// Weeks without a contribution before the canopy goes dormant.
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
    /// Declared private work: first day, last day, languages.
    pub periods: Vec<(Date, Date, Vec<String>)>,
    /// Each language's foliage left, 0 to 1 ([`foliage::fullness`]).
    pub fullness: BTreeMap<String, f64>,
    /// The share of its leaves the crown is drawn with, 1 unless the SVG
    /// would be too heavy ([`render::SVG_BUDGET`]).
    pub leaf_share: f64,
}

impl Bonsai {
    /// Grows the tree for `snapshot` as of `as_of` (default: its
    /// `observed_on`). Reads no clock and no network.
    pub fn grow(snapshot: &Snapshot, config: &Config, as_of: Option<Date>) -> Self {
        let snapshot = match as_of {
            Some(date) => snapshot.truncated(date),
            None => snapshot.clone(),
        };
        let (id, wind) = (snapshot.account_id, config.tree.wind);
        let side = crate::style::side(&snapshot.days);
        let leader = match config.tree.style {
            Some(style) => Leader::new(style, id, wind, side),
            None => Leader::mixed(crate::style::Mix::of(&snapshot), id, wind, side),
        };
        let timeline = Timeline::new(&snapshot);
        let periods = config.private_periods();
        let (past, recent, leaves) = past_reach(&snapshot, &periods, config.tree.style);
        // The leaves follow the work half at once, half as the last two
        // months saw it.
        let (window, fullness) = settle_leaves(
            Window::of(&timeline),
            foliage::fullness_by_language(&timeline),
            leaves,
        );
        let (states, order) = foliage::language_states(&timeline, &window, &periods);
        let skeleton = growth::grow(
            &timeline,
            snapshot.account_id,
            &leader,
            &periods,
            past,
            recent,
        );
        let maple = MaplePalette::new(
            &window,
            &order,
            &snapshot.language_colors,
            &config.leaves.colors,
            config.leaves.private,
        );
        let extras = Extras::new(&timeline, timeline.age_weeks());
        Self {
            login: snapshot.login.clone(),
            account_id: snapshot.account_id,
            observed_on: snapshot.observed_on,
            leader,
            last_week_total: foliage::last_week_total(&timeline),
            skeleton,
            window,
            states,
            order,
            maple,
            extras,
            achievements: snapshot.achievements.clone(),
            archived: snapshot.archived_repos.clone(),
            truncated_windows: snapshot.truncated_windows.clone(),
            periods,
            fullness,
            timeline,
            leaf_share: 1.0,
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

    /// How much of `language`'s foliage is left, 0 to 1, by its last week
    /// with work, the week under way included; private work by the
    /// account's.
    pub fn fullness(&self, language: Option<&str>) -> f64 {
        let busy = (self.timeline.weeks.last()).is_some_and(|w| !w.complete && w.total > 0.0);
        let account = match self.silent_weeks() {
            Some(w) if !busy => foliage::fullness(i32::try_from(w).unwrap_or(i32::MAX)),
            _ => 1.0,
        };
        language
            .and_then(|l| self.fullness.get(l).copied())
            .unwrap_or(account)
    }

    /// Complete weeks since the last contribution.
    pub fn silent_weeks(&self) -> Option<u32> {
        let weeks = self.timeline.complete_weeks();
        let last = weeks.iter().rposition(|w| w.total > 0.0)?;
        u32::try_from(weeks.len() - 1 - last).ok()
    }

    /// A year of total silence: the canopy is bare. The trunk never becomes jin.
    /// The same rule the drawing follows, so the legend and `explain` agree
    /// with what the tree shows.
    pub fn dormant(&self) -> bool {
        self.skeleton.dormant
    }

    /// The text of the SVG's `<desc>`, also good for the README's `alt`.
    pub fn description(&self) -> String {
        if self.is_empty() {
            return "A maple bonsai seedling, waiting for its first contribution.".into();
        }
        let grown = match self.timeline.age_weeks() {
            0 => "just planted".to_string(),
            weeks => format!("grown over {}", legend::age(weeks)),
        };
        let kind = if sprout::is_sprout(self) {
            "sprout"
        } else {
            "bonsai"
        };
        let mut text = format!(
            "Japanese maple {kind} {grown}, level {}.",
            self.extras.level
        );
        // The first contribution is a shoot still in its seed coat.
        if sprout::count(self).round() < 2.0 {
            text.push_str(" A shoot in its seed coat.");
            return text;
        }
        let mut ranked: Vec<(&String, &f64)> = self.window.by_language.iter().collect();
        ranked.sort_by(|a, c| c.1.total_cmp(a.1).then(a.0.cmp(c.0)));
        let named: f64 = self.window.by_language.values().sum::<f64>().max(1e-9);
        // After the first, a language under 15% of the leaves is "a bit of".
        let (main, bit): (Vec<_>, Vec<_>) = ranked
            .iter()
            .take(3)
            .enumerate()
            .partition(|(i, (_, v))| *i == 0 || **v / named >= 0.15);
        let main: Vec<&str> = main.iter().map(|(_, (l, _))| l.as_str()).collect();
        let bit: Vec<&str> = bit.iter().map(|(_, (l, _))| l.as_str()).collect();
        if !main.is_empty() {
            let _ = write!(text, " Leaves of {}", main.join(", "));
            if !bit.is_empty() {
                let _ = write!(text, ", a bit of {}", and_list(&bit));
            }
            text.push('.');
        }
        if self.window.unattributed > 0.5 * self.window.total {
            text.push_str(" Mostly private work.");
        }
        // Only dead wood the tree shows, as in the legend.
        let jin: Vec<&str> = self
            .states
            .iter()
            .filter(|(l, s)| {
                **s == LanguageState::Jin && self.skeleton.deadwood.contains(l.as_str())
            })
            .map(|(l, _)| l.as_str())
            .collect();
        if !jin.is_empty() {
            let _ = write!(text, " {} turned to deadwood.", and_list(&jin));
        }
        if self.dormant() {
            text.push_str(" Dormant, waiting for spring.");
        }
        text
    }
}

/// "A", "A and B", "A, B and C".
fn and_list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Half `window` and `fullness`, half the average of `past` months'.
fn settle_leaves(
    window: Window,
    fullness: BTreeMap<String, f64>,
    past: Option<Leaves>,
) -> (Window, BTreeMap<String, f64>) {
    let Some((pw, pf)) = past else {
        return (window, fullness);
    };
    let mut by_language: BTreeMap<String, f64> = BTreeMap::new();
    for (l, v) in window.by_language.iter().chain(&pw.by_language) {
        *by_language.entry(l.clone()).or_default() += 0.5 * v;
    }
    // A language the past months didn't know yet keeps today's fullness:
    // missing there means the account's own, not none.
    let full: BTreeMap<String, f64> = fullness
        .iter()
        .map(|(l, v)| (l.clone(), 0.5 * v + 0.5 * pf.get(l).copied().unwrap_or(*v)))
        .collect();
    (
        Window {
            total: f64::midpoint(window.total, pw.total),
            unattributed: f64::midpoint(window.unattributed, pw.unattributed),
            by_language,
        },
        full,
    )
}

/// A month's leaves: its foliage window and each language's fullness.
type Leaves = (Window, BTreeMap<String, f64>);

/// The average of `all` months' leaves.
fn mean_leaves(all: &[Leaves]) -> Option<Leaves> {
    if all.is_empty() {
        return None;
    }
    let n = all.len() as f64;
    let mut window = Window::default();
    let mut full: BTreeMap<String, f64> = BTreeMap::new();
    for (w, f) in all {
        window.total += w.total / n;
        window.unattributed += w.unattributed / n;
        for (l, v) in &w.by_language {
            *window.by_language.entry(l.clone()).or_default() += v / n;
        }
        for (l, v) in f {
            *full.entry(l.clone()).or_default() += v / n;
        }
    }
    Some((window, full))
}

/// The furthest the tree reached on the first of any month before
/// `snapshot`'s observed day: a trunk never shrinks, so today's never falls
/// short of it. Each month is grown from its own history, remembered by a
/// hash of that history, so a timelapse grows each month once.
fn past_reach(
    snapshot: &Snapshot,
    periods: &[(Date, Date, Vec<String>)],
    style: Option<crate::config::Style>,
) -> (
    growth::dome::Reach,
    Option<growth::dome::Recent>,
    Option<Leaves>,
) {
    use growth::dome::{Reach, Recent};
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    // shortcut: a process-wide memo that is cleared whole past 100 000
    // months; enough for a gallery or test run, revisit for a server.
    // The month's measures, leaves and how formal upright it stood.
    type Month = (Option<(Reach, Recent)>, Leaves, f64);
    static SEEN: OnceLock<Mutex<HashMap<(u64, i32), Month>>> = OnceLock::new();
    let seen = SEEN.get_or_init(Mutex::default);
    let Some(first) = snapshot.first_active() else {
        return (Reach::default(), None, None);
    };
    // The start once: each month is cut from it.
    let folded = snapshot.with_start();
    let mut hash = rng::siphash13(
        format!(
            "{}/{}/{periods:?}/{:?}/{style:?}",
            snapshot.account_id, snapshot.schema, folded.start
        )
        .as_bytes(),
    );
    let mut days = folded.days.iter().peekable();
    let (y, m, _) = first.ymd();
    let mut out = Reach::default();
    let mut months: Vec<Recent> = Vec::new();
    let mut leaves: Vec<Leaves> = Vec::new();
    // The month before's style too: a trunk thickened over a month in one
    // style keeps that girth as the next month starts in another.
    let mut chokkan_before = 0.0_f64;
    for no in (y * 12 + m.cast_signed())..i32::MAX {
        let Some(day) = Date::from_ymd(no / 12, (no % 12 + 1).cast_unsigned(), 1) else {
            break;
        };
        if day >= snapshot.observed_on {
            break;
        }
        let mut bytes = hash.to_le_bytes().to_vec();
        while let Some(d) = days.next_if(|d| d.date <= day) {
            bytes.extend_from_slice(format!("{d:?}").as_bytes());
        }
        hash = rng::siphash13(&bytes);
        let known = seen.lock().ok().and_then(|m| m.get(&(hash, no)).cloned());
        let month: Month = known.unwrap_or_else(|| {
            let cut = folded.cut(day);
            // How formal upright that month stood, as its style then.
            let chokkan = style.map_or_else(
                || crate::style::Mix::of(&cut).weight(crate::config::Style::Chokkan),
                |s| f64::from(u8::from(s == crate::config::Style::Chokkan)),
            );
            let then = Timeline::new(&cut);
            let tree = growth::life::Life::of(&then, periods).and_then(|life| {
                growth::dome::measures(&life, snapshot.account_id, chokkan.max(chokkan_before))
            });
            let month = (
                tree,
                (Window::of(&then), foliage::fullness_by_language(&then)),
                chokkan,
            );
            if let Ok(mut m) = seen.lock() {
                if m.len() > 100_000 {
                    m.clear();
                }
                m.insert((hash, no), month.clone());
            }
            month
        });
        let (tree, leaf, chokkan) = month;
        chokkan_before = chokkan;
        if let Some((r, recent)) = tree {
            // As that month showed it: settled by its own three before.
            let before = Recent::mean(&months[months.len().saturating_sub(3)..]);
            out = out.max(before.map_or(r, |b| r.settled(&b)));
            months.push(recent);
        }
        leaves.push(leaf);
    }
    let last = months.len().saturating_sub(3);
    let last_leaves = leaves.len().saturating_sub(2);
    (
        out,
        Recent::mean(&months[last..]),
        mean_leaves(&leaves[last_leaves..]),
    )
}

#[cfg(test)]
mod leaf_tests {
    use super::*;

    /// A language too new for the past months keeps today's leaves: missing
    /// there is not zero.
    #[test]
    fn a_new_language_keeps_its_leaves() {
        let now: BTreeMap<String, f64> = [("Rust".to_string(), 1.0)].into();
        let past: Leaves = (Window::default(), BTreeMap::new());
        let (_, full) = settle_leaves(Window::default(), now, Some(past));
        assert_eq!(full["Rust"], 1.0);
    }
}
