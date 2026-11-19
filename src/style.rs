//! Styles: the leader's path and which side foliage pads favor.
//!
//! A style only bends the leader and biases pad placement. It never changes
//! growth amounts, language states or colors. The leader's position is a
//! function of height only, so growing taller never moves the wood below.
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
use crate::noise::fbm;
use crate::rng::Rng;
use crate::snapshot::Snapshot;

/// The months a new account takes to grow into its style.
const WINDOW_MONTHS: i32 = 36;

/// Languages that cascade a tree over the rim: kengai. GitHub's names, and
/// a few spellings of the same.
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

/// A style made concrete for one account: its seeded lean and wind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leader {
    /// The style.
    pub style: Style,
    seed: u64,
    /// Sideways lean per unit of height (shakan, fukinagashi); signed.
    lean: f64,
    /// How far the leader meanders, as a share of height.
    meander: f64,
    /// `+1` if foliage favors the right, `-1` the left, `0` for no preference.
    favor: f64,
    /// How often a pad goes to the favored side, 0 to 1.
    pull: f64,
    /// How far the trunk's own bends swing, 1 for an informal upright.
    pub bend: f64,
    /// How far the trunk leans as it rises, signed like the lean.
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
    /// fixed for the account, so nothing changes side as its style does.
    pub downwind: f64,
    /// Where the last three years put the tree between the styles; `None`
    /// when the style is configured.
    pub mix: Option<Mix>,
}

impl Leader {
    /// The leader of `style` for the account `seed`.
    pub fn new(style: Style, seed: u64, wind: Option<Wind>) -> Self {
        let mut rng = Rng::keyed(seed, "lean");
        let sign = rng.sign();
        let amount = rng.range(0.0, 1.0);
        let downwind = match wind {
            Some(Wind::Left) => -1.0,
            Some(Wind::Right) => 1.0,
            None => Rng::keyed(seed, "wind").sign(),
        };
        let (lean, meander, favor, pull, bend, tilt) = match style {
            Style::Chokkan => (0.0, 0.02, 0.0, 0.0, 0.4, 0.0),
            // Informal upright: deep S-bends.
            Style::Moyogi => (0.0, 0.14, 0.0, 0.0, 2.2, 0.0),
            // Slanting about 25°, the trunk as straight as a formal upright;
            // foliage favors the side opposite the lean.
            Style::Shakan => (sign * (0.22 + 0.16 * amount), 0.05, -sign, 0.7, 0.4, 0.42),
            // Windswept: leaning downwind, and everything streams that way.
            Style::Fukinagashi => (
                downwind * (0.3 + 0.18 * amount),
                0.05,
                downwind,
                0.9,
                1.0,
                0.75,
            ),
            // Cascade: the trunk rises straight, then spills over the rim.
            Style::Kengai => (0.0, 0.03, downwind, 0.8, 0.5, 0.0),
        };
        Self {
            style,
            seed,
            lean,
            meander,
            favor,
            pull,
            bend,
            tilt: tilt * lean.signum(),
            sweep: if style == Style::Fukinagashi {
                downwind
            } else {
                0.0
            },
            // A cascade always spills to the right.
            cascade: if style == Style::Kengai { 1.0 } else { 0.0 },
            tier: if style == Style::Chokkan { 1.0 } else { 0.0 },
            downwind,
            mix: None,
        }
    }

    /// The leader `mix` blends from the four styles. Shakan leans the way
    /// the wind blows, so it and fukinagashi blend into one lean.
    pub fn mixed(mix: Mix, seed: u64, wind: Option<Wind>) -> Self {
        let fu = Self::new(Style::Fukinagashi, seed, wind);
        let mut sh = Self::new(Style::Shakan, seed, wind);
        if sh.lean.signum() != fu.lean.signum() {
            sh.lean = -sh.lean;
            sh.favor = -sh.favor;
            sh.tilt = -sh.tilt;
        }
        let corners = [
            (
                Self::new(Style::Chokkan, seed, wind),
                mix.weight(Style::Chokkan),
            ),
            (
                Self::new(Style::Moyogi, seed, wind),
                mix.weight(Style::Moyogi),
            ),
            (sh, mix.weight(Style::Shakan)),
            (fu, mix.weight(Style::Fukinagashi)),
            (
                Self::new(Style::Kengai, seed, wind),
                mix.weight(Style::Kengai),
            ),
        ];
        let sum = |f: fn(&Self) -> f64| corners.iter().map(|(l, w)| f(l) * w).sum::<f64>();
        Self {
            style: mix.style(),
            seed,
            lean: sum(|l| l.lean),
            meander: sum(|l| l.meander),
            favor: if mix.spread > 0.0 { fu.favor } else { 0.0 },
            pull: sum(|l| l.pull),
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
            mix: Some(mix),
        }
    }

    /// The leader's horizontal offset from the trunk base at height `h`.
    pub fn x_at(&self, h: f64) -> f64 {
        self.lean * h + self.meander * h * fbm(self.seed, h / 80.0)
    }

    /// The side a new pad goes to: its natural side, or flipped toward the
    /// style's favored side when `u` (uniform in `[0, 1)`) says so.
    pub fn pad_side(&self, natural: f64, u: f64) -> f64 {
        if self.favor != 0.0 && u < self.pull {
            self.favor
        } else {
            natural
        }
    }
}

/// Where a tree stands between the four styles, from its last three years.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Mix {
    /// How much of the time went in pauses, 0 (steady) to 1.
    pub irregular: f64,
    /// How far the work spread past one language, 0 (one) to 1.
    pub spread: f64,
    /// How much of the work is in legacy languages ([`LEGACY`]), 0 to 1:
    /// a whole kengai from half the work.
    pub legacy: f64,
}

impl Mix {
    /// The mix of `snapshot`'s complete months, the recent ones counting
    /// most: a month's weight halves every [`HALF_LIFE_MONTHS`], so the last
    /// three years hold almost all of it. A resting account keeps its mix of
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
            .filter(|(l, _)| LEGACY.iter().any(|x| x.eq_ignore_ascii_case(l)))
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

    #[test]
    fn chokkan_is_nearly_straight_and_shakan_leans() {
        let straight = Leader::new(Style::Chokkan, 5, None);
        assert!(straight.x_at(250.0).abs() < 10.0);
        let slant = Leader::new(Style::Shakan, 5, None);
        assert!(slant.x_at(250.0).abs() > 40.0);
    }

    #[test]
    fn fukinagashi_follows_the_configured_wind() {
        for (wind, sign) in [(Wind::Left, -1.0), (Wind::Right, 1.0)] {
            let leader = Leader::new(Style::Fukinagashi, 9, Some(wind));
            assert!(leader.x_at(200.0) * sign > 40.0);
            assert_eq!(leader.pad_side(-sign, 0.1), sign);
        }
    }

    #[test]
    fn the_leader_depends_on_height_only() {
        let leader = Leader::new(Style::Moyogi, 42, None);
        assert_eq!(leader.x_at(120.0), leader.x_at(120.0));
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
            let corner = Leader::mixed(mix, 3, None);
            assert_eq!(mix.style(), style);
            assert_eq!(corner.x_at(200.0), Leader::new(style, 3, None).x_at(200.0));
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
                3,
                None,
            )
            .x_at(200.0)
        };
        let (full, half) = (lean(1.0), lean(0.5));
        assert!(
            half.abs() > 0.3 * full.abs() && half.abs() < 0.7 * full.abs(),
            "{half} of {full}"
        );
    }
}
