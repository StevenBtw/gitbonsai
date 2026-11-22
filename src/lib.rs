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
pub mod galaxy;
pub mod growth;
pub mod html;
pub mod legend;
pub mod maple;
pub mod math;
pub mod noise;
pub mod output;
pub mod paint;
pub mod pot;
#[cfg(feature = "png")]
pub mod raster;
pub mod render;
pub mod rng;
pub mod row;
pub mod scene;
pub mod snapshot;
pub mod sprout;
pub mod style;
pub mod timelapse;
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
    /// Whose seasons the tree keeps.
    pub hemisphere: config::Hemisphere,
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
    /// The pot's size on the canvas, one of [`POTS`]: it grows a size at a
    /// repotting and never shrinks.
    pub pot: f64,
    /// The girth at its foot on the canvas the trunk was last repotted for,
    /// with room to grow: the pot's depth follows it, so like its size the
    /// depth changes only at a repotting.
    pub pot_girth: f64,
}

/// The pot sizes a tree is repotted through, smallest first: each about a
/// quarter larger than the one before.
pub const POTS: [f64; 9] = [80.0, 100.0, 125.0, 156.0, 195.0, 244.0, 305.0, 381.0, 400.0];

/// The pot a crown's full size calls for, as a share of that size: what
/// most trees need ([`growth::dome::full_size`]).
const FULL_POT: f64 = 1.6;

/// The room a repotting leaves to grow: about a year's growth of most trees,
/// so the pot still fits when the next spring comes. A tree in one of the two
/// smallest pots gets none: a size up it would look like a seedling in a tray.
const AHEAD: f64 = 1.15;

/// Gives the pot size a tree needs from its height on the canvas, the pot
/// its full size calls for, and its crown's spread: 0.62 of the height, the
/// full size's pot, or six tenths of the spread, whichever is most.
fn pot_need_of(height: f64, full: f64, spread: f64) -> f64 {
    (height * 0.62).max(full).max(0.6 * spread)
}

/// The smallest pot that holds `need`, or the largest.
fn pot_for(need: f64) -> f64 {
    POTS.iter()
        .copied()
        .find(|&p| p >= need)
        .unwrap_or(POTS[POTS.len() - 1])
}

/// The pot for a tree that needs `now` today and needed `then` at the last
/// start of spring: repotted then, with room to grow, and out of season only
/// in a growth spurt, once it outgrows that pot by two sizes; merely
/// pot-bound, it waits for spring.
fn pot_size(now: f64, then: Option<f64>) -> f64 {
    let need = then.unwrap_or(now);
    let potted = if pot_for(need) < POTS[2] {
        pot_for(need)
    } else {
        pot_for(need * AHEAD)
    };
    if now > potted * 1.25 * 1.25 {
        potted.max(pot_for(now))
    } else {
        potted
    }
}

/// The first day of the last spring on or before `day`: March in the north,
/// September in the south.
fn last_spring(day: Date, hemisphere: config::Hemisphere) -> Date {
    let month = if hemisphere == config::Hemisphere::South {
        9
    } else {
        3
    };
    let (y, m, _) = day.ymd();
    let year = if m >= month { y } else { y - 1 };
    Date::from_ymd(year, month, 1).unwrap_or(day)
}

impl Bonsai {
    /// Grows the tree for `snapshot` as of `as_of` (default: its
    /// `observed_on`). Reads no clock and no network.
    ///
    /// Its pot is repotted in steps: the size that fit the tree at the last
    /// start of spring, or one that fits it now if it has outgrown that by two
    /// sizes since; its depth fits the trunk as it was that spring. Each leaves
    /// room for a year's growth. So any day's pot follows from its date alone.
    pub fn grow(snapshot: &Snapshot, config: &Config, as_of: Option<Date>) -> Self {
        let mut b = Self::grow_at(snapshot, config, as_of);
        let spring = last_spring(b.observed_on, b.hemisphere);
        let then = (spring < b.observed_on && spring >= snapshot.range.from)
            .then(|| Self::grow_at(snapshot, config, Some(spring)));
        let need = then.as_ref().map(Self::pot_need);
        b.pot = pot_size(b.pot_need(), need);
        let ahead = if b.pot < POTS[2] { 1.0 } else { AHEAD };
        // Repotted out of season in a growth spurt, the pot deepens in step
        // with its width.
        let spring_pot = need.map_or(b.pot, |n| pot_size(n, Some(n)));
        b.pot_girth = then.as_ref().unwrap_or(&b).foot_girth() * ahead * b.pot / spring_pot;
        b
    }

    /// The trunk's girth at its foot on the canvas, as drawn.
    pub(crate) fn foot_girth(&self) -> f64 {
        scene::WORLD
            * scene::Scene::build(self)
                .chains
                .first()
                .and_then(|c| c.0.first())
                .map_or(1.0, |p| p.1)
    }

    /// How big a pot the tree needs: 0.62 of its height on the canvas, the
    /// pot its crown's full size calls for, or six tenths of its crown's
    /// spread ([`Self::crown_spread`]), whichever is most.
    fn pot_need(&self) -> f64 {
        pot_need_of(
            self.skeleton.height * scene::WORLD,
            growth::dome::full_size(&self.timeline, &self.periods) * FULL_POT,
            self.crown_spread(),
        )
    }

    /// Measures how wide the crown spreads on the canvas above the rim: its
    /// leaf clouds and its wood. A cascade's limb hanging below the rim
    /// doesn't count, so a cascade's pot never grows with its spill. You get
    /// 0 for a tree with nothing above the soil.
    pub fn crown_spread(&self) -> f64 {
        let sk = &self.skeleton;
        let wood = sk
            .nodes
            .iter()
            .filter(|n| n.p.1 < style::SOIL_Y)
            .map(|n| (n.p.0, n.p.0));
        let clouds = sk
            .clouds
            .iter()
            .filter(|c| c.c.1 < style::SOIL_Y)
            .map(|c| (c.c.0 - c.rx, c.c.0 + c.rx));
        let (lo, hi) = wood
            .chain(clouds)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), (a, b)| {
                (lo.min(a), hi.max(b))
            });
        (hi - lo).max(0.0)
    }

    /// The tree for `snapshot` as of `as_of`, before its pot is chosen.
    fn grow_at(snapshot: &Snapshot, config: &Config, as_of: Option<Date>) -> Self {
        let snapshot = match as_of {
            Some(date) => snapshot.truncated(date),
            None => snapshot.clone(),
        };
        let wind = config.tree.wind;
        // A configured wind sets the side too: the lean, the light and the
        // eggs follow it, so no tree streams toward its lit side.
        let side = match wind {
            Some(config::Wind::Left) => -1.0,
            Some(config::Wind::Right) => 1.0,
            None => crate::style::side(&snapshot.days),
        };
        let leader = match config.tree.style {
            Some(style) => Leader::new(style, wind, side),
            None => Leader::mixed(crate::style::Mix::of(&snapshot), wind, side),
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
            hemisphere: config.tree.hemisphere,
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
            pot: POTS[0],
            pot_girth: 0.0,
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

    /// Checks whether the crown draws leaves of `language`: a language too
    /// small for a cloud of its own has none, so the legend leaves it out.
    pub fn shows_leaves(&self, language: &str) -> bool {
        self.skeleton
            .clouds
            .iter()
            .any(|c| c.patches.iter().any(|p| p.2.as_deref() == Some(language)))
    }

    /// Names the classic language of a semi-cascade's white trunk: the one
    /// with the most work in all. You get `None` while the trunk isn't white.
    pub fn white_trunk(&self) -> Option<&str> {
        if self.skeleton.legacy_jin <= 0.0 {
            return None;
        }
        let total = |l: &str| -> f64 {
            self.timeline
                .days
                .iter()
                .filter_map(|d| d.by_language.get(l))
                .sum()
        };
        self.order
            .iter()
            .filter(|l| crate::style::is_legacy(l))
            .max_by(|a, b| total(a).total_cmp(&total(b)).then(b.cmp(a)))
            .map(String::as_str)
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
        // Only languages the crown draws, as in the legend.
        let mut ranked: Vec<(&String, &f64)> = self
            .window
            .by_language
            .iter()
            .filter(|(l, _)| self.shows_leaves(l))
            .collect();
        ranked.sort_by(|a, c| c.1.total_cmp(a.1).then(a.0.cmp(c.0)));
        let leaves = self.window.total.max(1e-9);
        // After the first, a language under 15% of the leaves is "a bit of".
        let (main, bit): (Vec<_>, Vec<_>) = ranked
            .iter()
            .take(3)
            .enumerate()
            .partition(|(i, (_, v))| *i == 0 || **v / leaves >= 0.15);
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
        if let Some(language) = self.white_trunk() {
            let _ = write!(
                text,
                " A white {language} trunk: a classic language, still growing."
            );
        }
        if self.dormant() {
            text.push_str(" Dormant, waiting for spring.");
        }
        text
    }
}

/// Joins `items` the way a sentence lists them: "A", "A and B", "A, B and C".
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
    let mut newest = first;
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
            newest = day;
        }
        leaves.push(leaf);
    }
    // The newest month fades in over its month as the oldest of the last
    // three fades out: the average moves a little each day, not a step on
    // the 2nd.
    let fade = (f64::from(snapshot.observed_on.days_since(newest))
        / f64::from(newest.month_days()))
    .min(1.0);
    let n = months.len();
    let recent: Vec<(&Recent, f64)> = months
        .iter()
        .enumerate()
        .skip(n.saturating_sub(4))
        .map(|(i, r)| match n - i {
            4 => (r, 1.0 - fade),
            1 => (r, fade),
            _ => (r, 1.0),
        })
        .collect();
    let last_leaves = leaves.len().saturating_sub(2);
    (
        out,
        Recent::weighted(&recent),
        mean_leaves(&leaves[last_leaves..]),
    )
}

#[cfg(test)]
mod leaf_tests {
    use super::*;

    /// A broad crown asks for a pot six tenths its spread, even when its
    /// height and full size ask for less.
    #[test]
    fn a_broad_crown_asks_for_a_wider_pot() {
        assert!((pot_need_of(100.0, 80.0, 350.0) - 210.0).abs() < 1e-9);
        assert!((pot_need_of(100.0, 80.0, 50.0) - 80.0).abs() < 1e-9);
    }

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
