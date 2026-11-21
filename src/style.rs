//! Styles: how the trunk bends, leans, streams in the wind and cascades.
//!
//! A style only shapes the wood. It never changes growth amounts, language
//! states or colors.
//!
//! Unless one is configured, the last three years pick the style
//! ([`Mix::of`]): steady work in one language stands formal upright
//! (chokkan), uneven work curves it (moyogi), a second big language slants
//! it (shakan), and both make it windswept (fukinagashi). Legacy languages
//! spill it over the rim (kengai). The styles blend, so a tree changes style
//! a little each month.

use std::collections::BTreeMap;

use crate::config::{Style, Wind};
use crate::math;
use crate::snapshot::{Day, Snapshot};

/// The months a new account takes to grow into its style.
const WINDOW_MONTHS: i32 = 36;

/// Languages that cascade a tree over the rim: kengai. GitHub's names, and
/// a few spellings of the same.
/// Checks whether `language` is one of the classic languages in `LEGACY`,
/// ignoring case: the work that grows a semi-cascade.
#[must_use]
pub fn is_legacy(language: &str) -> bool {
    LEGACY.iter().any(|x| x.eq_ignore_ascii_case(language))
}

const LEGACY: [&str; 23] = [
    "COBOL",
    "Fortran",
    "Fortran Free Form",
    "Assembly",
    "Pascal",
    "Ada",
    "PLI",
    "PL/I",
    "RPGLE",
    "ALGOL",
    "Common Lisp",
    "Smalltalk",
    "BASIC",
    "FreeBasic",
    "QuickBASIC",
    "Visual Basic 6.0",
    "VBA",
    "Delphi",
    "ABAP",
    "JCL",
    "Perl",
    "Tcl",
    "VBScript",
];

/// A month with this many active days or fewer is quiet, a pause; one with
/// [`BUSY_DAYS`] or more is not, and those between are partly quiet.
const QUIET_DAYS: u32 = 2;
/// See [`QUIET_DAYS`].
const BUSY_DAYS: u32 = 8;

/// A month counts half as much for the style 22 months on: the last three
/// years hold most of it.
const HALF_LIFE_MONTHS: f64 = 22.0;

/// The soil line on the 512-unit canvas.
pub const SOIL_Y: f64 = 404.0;

/// The canvas center.
pub const CENTER_X: f64 = 256.0;

/// The contributions that make the sprout: their rhythm picks the side.
pub const SPROUT: u32 = 60;

/// Picks the side a tree leans and streams to from its sprout's rhythm:
/// `+1` right or `-1` left. The first [`SPROUT`] contributions of `days`
/// (in date order) decide: more than 2/7 of them on a Saturday or Sunday
/// (UTC) is weekend-heavy and keeps the right; weekday-heavy gets the left.
/// It settles at the 60th contribution and never changes after; with
/// nothing yet, it's the right.
pub fn side(days: &[Day]) -> f64 {
    let (mut seen, mut weekend) = (0, 0);
    for d in days {
        let n = d.total.min(SPROUT - seen);
        seen += n;
        if d.date.weekday() >= 5 {
            weekend += n;
        }
        if seen == SPROUT {
            break;
        }
    }
    if seen == 0 || 7 * weekend > 2 * seen {
        1.0
    } else {
        -1.0
    }
}

/// A style made concrete for one account: its bends, lean and wind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leader {
    /// The style.
    pub style: Style,
    /// How far the trunk's own bends swing, 1 for an informal upright.
    pub bend: f64,
    /// How far the trunk leans as it rises, signed by the side it leans to.
    pub tilt: f64,
    /// How far the crown streams downwind, signed: 1 or -1 for a whole
    /// fukinagashi, 0 for no wind.
    pub sweep: f64,
    /// How far the trunk cascades over the rim, signed by the side it
    /// spills to: 1 or -1 for a whole kengai.
    pub cascade: f64,
    /// How far the crown stands in tiers over a straight trunk: 1 for a
    /// whole chokkan, 0 for none.
    pub tier: f64,
    /// The side the tree leans to when it leans, `+1` right or `-1` left:
    /// the account's [`side`] unless a wind is configured, so nothing
    /// changes side as its style does.
    pub downwind: f64,
    /// The account's [`side`]: a left tree is the right one's mirror image.
    pub side: f64,
    /// Where the last three years put the tree between the styles; `None`
    /// when the style is configured.
    pub mix: Option<Mix>,
}

impl Leader {
    /// The leader of `style` on `side` ([`side`]).
    pub fn new(style: Style, wind: Option<Wind>, side: f64) -> Self {
        let downwind = match wind {
            Some(Wind::Left) => -1.0,
            Some(Wind::Right) => 1.0,
            None => side,
        };
        let (bend, tilt) = match style {
            Style::Chokkan => (0.4, 0.0),
            // Informal upright: deep S-bends.
            Style::Moyogi => (2.2, 0.0),
            // Slanting about 25°, the trunk as straight as a formal upright.
            Style::Shakan => (0.4, side * 0.42),
            // Windswept: leaning downwind, and everything streams that way.
            Style::Fukinagashi => (1.0, downwind * 0.75),
            // Cascade: the trunk rises straight, then spills over the rim.
            Style::Kengai => (0.5, 0.0),
        };
        Self {
            style,
            bend,
            tilt,
            sweep: if style == Style::Fukinagashi {
                downwind
            } else {
                0.0
            },
            // A cascade spills to the tree's side.
            cascade: if style == Style::Kengai { side } else { 0.0 },
            tier: if style == Style::Chokkan { 1.0 } else { 0.0 },
            downwind,
            side,
            mix: None,
        }
    }

    /// Gives this leader as it grows: on the right. A left tree grows as the
    /// right one and is mirrored after ([`crate::growth::grow`]), so the left
    /// is a true mirror image of the right.
    #[must_use]
    pub fn canonical(&self) -> Self {
        let s = self.side;
        Self {
            tilt: s * self.tilt,
            sweep: s * self.sweep,
            cascade: s * self.cascade,
            downwind: s * self.downwind,
            side: 1.0,
            ..*self
        }
    }

    /// The leader `mix` blends from the five styles. Shakan leans the way
    /// the wind blows, so it and fukinagashi blend into one lean.
    pub fn mixed(mix: Mix, wind: Option<Wind>, side: f64) -> Self {
        let fu = Self::new(Style::Fukinagashi, wind, side);
        let mut sh = Self::new(Style::Shakan, wind, side);
        if sh.tilt.signum() != fu.tilt.signum() {
            sh.tilt = -sh.tilt;
        }
        let corners = [
            (
                Self::new(Style::Chokkan, wind, side),
                mix.weight(Style::Chokkan),
            ),
            (
                Self::new(Style::Moyogi, wind, side),
                mix.weight(Style::Moyogi),
            ),
            (sh, mix.weight(Style::Shakan)),
            (fu, mix.weight(Style::Fukinagashi)),
            (
                Self::new(Style::Kengai, wind, side),
                mix.weight(Style::Kengai),
            ),
        ];
        let sum = |f: fn(&Self) -> f64| corners.iter().map(|(l, w)| f(l) * w).sum::<f64>();
        Self {
            style: mix.style(),
            bend: sum(|l| l.bend),
            tilt: sum(|l| l.tilt),
            // An informal upright's calm keeps the wind off: past a trace of
            // moyogi, the more of it, the less the crown and limbs sweep.
            sweep: sum(|l| l.sweep) * {
                let t = ((mix.weight(Style::Moyogi) - 0.2) / 0.8).clamp(0.0, 1.0);
                1.0 - t * t * (3.0 - 2.0 * t)
            },
            cascade: sum(|l| l.cascade),
            tier: sum(|l| l.tier),
            downwind: fu.downwind,
            side,
            mix: Some(mix),
        }
    }
}

/// Where a tree stands between the five styles, from its last three years.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Mix {
    /// How much of the time went in pauses, 0 (steady) to 1.
    pub irregular: f64,
    /// How far the work spread past one language, 0 (one) to 1.
    pub spread: f64,
    /// How much of the work is in legacy languages (`LEGACY`), 0 to 1:
    /// a whole kengai from half the work.
    pub legacy: f64,
}

impl Mix {
    /// The mix of `snapshot`'s complete months, the recent ones counting
    /// most: a month's weight halves every `HALF_LIFE_MONTHS`, so on a long
    /// account the last three years hold about two thirds of it (older months
    /// keep 0.5^(36/22), about 32%). A resting account keeps its mix of
    /// languages, while its rest reads as uneven work.
    pub fn of(snapshot: &Snapshot) -> Self {
        let month = |d: crate::date::Date| {
            let (y, m, _) = d.ymd();
            y * 12 + m.cast_signed() - 1
        };
        let Some(first) = snapshot.days.iter().find(|d| d.total > 0) else {
            return Self::default();
        };
        let (start, now) = (month(first.date), month(snapshot.observed_on));
        let mut months: Vec<(u32, BTreeMap<&str, u32>)> =
            vec![(0, BTreeMap::new()); usize::try_from(now - start).unwrap_or(0)];
        for day in &snapshot.days {
            let Ok(i) = usize::try_from(month(day.date) - start) else {
                continue;
            };
            if let Some((days, langs)) = months.get_mut(i) {
                *days += u32::from(day.total > 0);
                for (l, n) in &day.by_language {
                    *langs.entry(l.as_str()).or_default() += n;
                }
            }
        }
        Self::from_months(&months, now - start)
    }

    /// The mix of monthly active days and languages, oldest first, `age`
    /// months after the first contribution: everyone starts formal upright
    /// and grows into a style over the first three years.
    fn from_months(months: &[(u32, BTreeMap<&str, u32>)], age: i32) -> Self {
        let n = months.len();
        let weight = |i: usize| math::pow(0.5, (n - 1 - i) as f64 / HALF_LIFE_MONTHS);
        // Pauses, not volume: a month with only a few active days is a quiet
        // one, however much was done on them.
        let (mut mass, mut quiet) = (0.0, 0.0);
        let mut langs: BTreeMap<&str, f64> = BTreeMap::new();
        for (i, (days, ls)) in months.iter().enumerate() {
            let w = weight(i);
            mass += w;
            let busy =
                f64::from(days.saturating_sub(QUIET_DAYS)) / f64::from(BUSY_DAYS - QUIET_DAYS);
            quiet += w * (1.0 - busy.min(1.0));
            let named: u32 = ls.values().sum();
            for (l, v) in ls {
                *langs.entry(l).or_default() += w * f64::from(*v) / f64::from(named);
            }
        }
        let all: f64 = langs.values().sum();
        let top = langs.values().copied().fold(0.0, f64::max);
        let ramp = |x: f64, lo: f64, hi: f64| ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
        let grown = (f64::from(age) / f64::from(WINDOW_MONTHS)).clamp(0.0, 1.0);
        let irregular = if mass > 0.0 {
            ramp(quiet / mass, 0.25, 0.65)
        } else {
            0.0
        };
        let spread = if all > 0.0 {
            1.0 - ramp(top / all, 0.5, 0.85)
        } else {
            0.0
        };
        let old: f64 = langs
            .iter()
            .filter(|(l, _)| is_legacy(l))
            .map(|(_, v)| v)
            .sum();
        let legacy = if all > 0.0 {
            ramp(old / all, 0.0, 0.5)
        } else {
            0.0
        };
        Self {
            irregular: irregular * grown,
            spread: spread * grown,
            legacy: legacy * grown,
        }
    }

    /// How much of `style` the tree has, 0 to 1; the five add up to 1. A
    /// 2x2 of steady or uneven work in one language or more: steady in one
    /// is chokkan, new languages bring moyogi, pauses in one language
    /// shakan, and both fukinagashi. A steady one-language tree in a legacy
    /// language cascades instead (kengai).
    pub fn weight(self, style: Style) -> f64 {
        let (x, y, k) = (self.irregular, self.spread, self.legacy);
        match style {
            Style::Chokkan => (1.0 - x) * (1.0 - y) * (1.0 - k),
            Style::Kengai => (1.0 - x) * (1.0 - y) * k,
            Style::Moyogi => (1.0 - x) * y,
            Style::Shakan => x * (1.0 - y),
            Style::Fukinagashi => x * y,
        }
    }

    /// The style the tree has most of.
    pub fn style(self) -> Style {
        [
            Style::Chokkan,
            Style::Moyogi,
            Style::Shakan,
            Style::Fukinagashi,
            Style::Kengai,
        ]
        .into_iter()
        .fold((Style::Chokkan, -1.0), |best, s| {
            if self.weight(s) > best.1 {
                (s, self.weight(s))
            } else {
                best
            }
        })
        .0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;

    #[test]
    fn chokkan_stands_straight_and_shakan_leans() {
        assert_eq!(Leader::new(Style::Chokkan, None, 1.0).tilt, 0.0);
        assert!(Leader::new(Style::Shakan, None, 1.0).tilt > 0.3);
    }

    #[test]
    fn fukinagashi_follows_the_configured_wind() {
        for (wind, sign) in [(Wind::Left, -1.0), (Wind::Right, 1.0)] {
            let leader = Leader::new(Style::Fukinagashi, Some(wind), 1.0);
            assert!(leader.tilt * sign > 0.5);
            assert_eq!(leader.sweep, sign);
        }
    }

    /// `n` days of `per_day` contributions each, from `from` on.
    fn days(from: (i32, u32, u32), n: i32, per_day: u32) -> Vec<Day> {
        let start = Date::from_ymd(from.0, from.1, from.2).unwrap();
        (0..n)
            .map(|i| Day {
                date: start.add_days(i),
                total: per_day,
                by_language: BTreeMap::new(),
                private: 0,
            })
            .collect()
    }

    #[test]
    fn the_sprouts_rhythm_picks_the_side() {
        // 2026-11-16 is a Monday: a week of one a day is 2/7 on the weekend,
        // which is not more than 2/7, so it leans left.
        let week = days((2026, 11, 16), 7, 1);
        assert_eq!(side(&week), -1.0);
        // One more Saturday tips it right.
        let mut more = week.clone();
        more.extend(days((2026, 11, 28), 1, 1));
        assert_eq!(side(&more), 1.0);
        // Weekdays only, and weekends only.
        assert_eq!(side(&days((2026, 11, 16), 5, 12)), -1.0);
        assert_eq!(side(&days((2026, 11, 21), 2, 30)), 1.0);
        // Nothing yet keeps the right.
        assert_eq!(side(&[]), 1.0);
    }

    #[test]
    fn the_side_is_settled_by_the_sixtieth_contribution() {
        // 60 on weekdays, then a flood of weekend work: still left.
        let mut d = days((2026, 1, 5), 5, 12);
        for w in 0..40 {
            let (y, m, dd) = Date::from_ymd(2026, 1, 10).unwrap().add_days(7 * w).ymd();
            d.extend(days((y, m, dd), 2, 50));
        }
        assert_eq!(side(&d), -1.0);
        // A day that crosses the 60th counts only up to it: 58 on Saturday,
        // then 100 on Monday, of which 2 count.
        let mut d = days((2026, 1, 10), 1, 58);
        d.extend(days((2026, 1, 12), 1, 100));
        assert_eq!(side(&d), 1.0);
        let mut d = days((2026, 1, 10), 1, 10);
        d.extend(days((2026, 1, 12), 1, 100));
        assert_eq!(side(&d), -1.0);
    }

    #[test]
    fn a_left_leader_mirrors_the_right() {
        for style in Style::ALL {
            let right = Leader::new(style, None, 1.0);
            let left = Leader::new(style, None, -1.0);
            assert_eq!(left.downwind, -right.downwind);
            assert_eq!(left.cascade, -right.cascade);
            assert_eq!(left.tilt, -right.tilt);
            assert_eq!(left.canonical(), right);
        }
        // The configured wind still wins.
        let left = Leader::new(Style::Fukinagashi, Some(Wind::Right), -1.0);
        assert_eq!(left.downwind, 1.0);
    }

    fn mix(totals: &[f64], langs: &[(&'static str, u32)], age: i32) -> Mix {
        let months: Vec<_> = totals
            .iter()
            .map(|&t| {
                (
                    // An active month has 20 active days, a quiet one none.
                    u32::from(t > 0.0) * 20,
                    if t > 0.0 {
                        langs.iter().copied().collect()
                    } else {
                        BTreeMap::new()
                    },
                )
            })
            .collect();
        Mix::from_months(&months, age)
    }

    #[test]
    fn three_years_pick_the_style() {
        let steady = [100.0; 36];
        let uneven: Vec<f64> = (0..36)
            .map(|i| if i % 6 == 0 { 600.0 } else { 0.0 })
            .collect();
        let one = [("C", 3600)];
        let two = [("Python", 1800), ("Rust", 1700)];
        assert_eq!(mix(&steady, &one, 120).style(), Style::Chokkan);
        assert_eq!(mix(&uneven, &one, 120).style(), Style::Shakan);
        assert_eq!(mix(&steady, &two, 120).style(), Style::Moyogi);
        assert_eq!(mix(&uneven, &two, 120).style(), Style::Fukinagashi);
        // Everyone starts formal upright.
        assert_eq!(mix(&uneven[..3], &two, 3).style(), Style::Chokkan);
        assert_eq!(Mix::default().style(), Style::Chokkan);
    }

    #[test]
    fn the_styles_blend() {
        let m = Mix {
            irregular: 0.3,
            spread: 0.6,
            legacy: 0.4,
        };
        let total: f64 = Style::ALL.iter().map(|&s| m.weight(s)).sum();
        assert!((total - 1.0).abs() < 1e-12);
        // A whole corner is that style's own leader.
        for (style, irregular, spread, legacy) in [
            (Style::Chokkan, 0.0, 0.0, 0.0),
            (Style::Moyogi, 0.0, 1.0, 0.0),
            (Style::Fukinagashi, 1.0, 1.0, 0.0),
            (Style::Kengai, 0.0, 0.0, 1.0),
        ] {
            let mix = Mix {
                irregular,
                spread,
                legacy,
            };
            let corner = Leader::mixed(mix, None, 1.0);
            assert_eq!(mix.style(), style);
            assert_eq!(
                Leader {
                    mix: None,
                    ..corner
                },
                Leader::new(style, None, 1.0)
            );
        }
        // Halfway into pauses, a one-language tree leans about half as far
        // as a whole shakan.
        let lean = |irregular| {
            Leader::mixed(
                Mix {
                    irregular,
                    spread: 0.0,
                    legacy: 0.0,
                },
                None,
                1.0,
            )
            .tilt
        };
        let (full, half) = (lean(1.0), lean(0.5));
        assert!(
            half.abs() > 0.3 * full.abs() && half.abs() < 0.7 * full.abs(),
            "{half} of {full}"
        );
    }
}
