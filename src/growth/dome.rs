//! The exhibition maple: trunks, era limbs and a domed crown.
//!
//! Each living trunk language rises from one nebari (two in a clear V) and
//! owns a sector of one rounded dome of overlapping domed clouds. The dome is
//! the crown: the newest run of three to six active years led by the trunk
//! languages. Older years are eras, split at a silent year or a change of
//! lead language, and become low limbs under the dome, oldest lowest, with
//! sparse decayed tufts and deadwood for dropped languages. A pause before
//! the crown leaves a bare window of trunk. A resting account stands bare,
//! every cloud a fan of winter twigs.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "indices, years and month numbers on a 512-unit canvas"
)]

use std::collections::BTreeMap;

use crate::date::Date;
use crate::growth::clouds::{cloud, merge_clouds, pad};
use crate::growth::life::{
    FULL_YEAR, Lang, Life, Month, RAMP_DAYS, Year, decay, irr, months_cv, shares_of,
};
use crate::growth::trunks::{Trunk, trunks};
use crate::growth::wood::{
    Limb, PI, at_frac, at_height, bleach_from, flare, jin_limb, limb_to, root_only, trunk3,
    winter_fan, zof,
};
use crate::growth::{GOLDEN, JIN, P, Skeleton};
use crate::math;
use crate::noise::hash01;
use crate::rng::Rng;
use crate::style::{CENTER_X, Leader, SOIL_Y};
use crate::timeline::Timeline;

/// One language's part of one crown year.
#[derive(Clone)]
struct Arm {
    lang: Lang,
    v: f64,
    vd: f64,
    year: i32,
    days: f64,
    irr: f64,
    zdir: f64,
    side: f64,
    total: f64,
    ag: f64,
    /// How far the arm has grown in (or not yet left): 0 to 1.
    w: f64,
    /// Dead wood: the work was dropped. How bleached it is, 0 to 1.
    dead: Option<f64>,
    /// How far dead wood has broken back: the share of its length left, its
    /// girth unchanged.
    back: f64,
}

/// `w` eased in and out: smoothstep.
fn ease(w: f64) -> f64 {
    let w = w.clamp(0.0, 1.0);
    w * w * (3.0 - 2.0 * w)
}

/// `x` held between `lo` and `hi`; `lo` wins when the range is empty (a
/// small tree's upper bound can fall below a fixed minimum).
fn within(x: f64, lo: f64, hi: f64) -> f64 {
    x.min(hi).max(lo)
}

/// Weight of contributions on the same scale for every account.
fn wt(v: f64) -> f64 {
    math::pow((v / FULL_YEAR).clamp(0.0, 1.5), 0.5)
}

/// The language leading `months` and its share; `None` for private work.
fn lead(months: &BTreeMap<u32, Month>, total: f64) -> (Lang, f64) {
    shares_of(months, total)
        .first()
        .map_or((None, 0.0), |s| (s.0.clone(), s.1 / total.max(1e-9)))
}

/// First and last active month of a year, as absolute month numbers.
fn span_of(y: &Year) -> (i32, i32) {
    let ms: Vec<i32> = y
        .months
        .iter()
        .filter(|(_, m)| m.total > 0.0)
        .map(|(k, _)| y.y * 12 + *k as i32)
        .collect();
    (
        ms.first().copied().unwrap_or(y.y * 12 + 1),
        ms.last().copied().unwrap_or(y.y * 12 + 12),
    )
}

/// The months of several years, keyed apart, and their total.
fn months_of(life: &Life, years: &[usize]) -> (BTreeMap<u32, Month>, f64) {
    let mut ms = BTreeMap::new();
    let mut t = 0.0;
    for &i in years {
        let y = &life.years[i];
        for (m, mo) in &y.months {
            ms.insert(y.y as u32 * 12 + m, mo.clone());
        }
        t += y.total;
    }
    (ms, t)
}

/// How the years divide: the crown languages (trunk index, language), the
/// crown years, the older eras (indices into `life.years`) and the silent
/// months between the last era and the crown.
#[derive(Clone)]
struct YearPlan {
    upper: Vec<(usize, Lang)>,
    crown: Vec<usize>,
    eras: Vec<Vec<usize>>,
    pause_m: i32,
    /// Years leaving the crown for era limbs, each with how far it has gone
    /// (0 to 1).
    leaving: Vec<(usize, f64)>,
    /// Years that left the crown within the past two years, with how far
    /// through those years they are (0 to 1) and the day they left: their
    /// dead arms still stand where they stood, breaking back and falling one
    /// by one.
    lingering: Vec<(usize, f64, Date)>,
    /// How far each crown language's sector has grown in, 0 to 1.
    sector: Vec<f64>,
    /// While years leave, oldest change first: the plan as it stood the day
    /// before each change began, and how far that change has gone.
    before: Vec<(YearPlan, f64)>,
}

/// Quiet months between an old year (if any) and the next year.
fn pause_months(life: &Life, last_old: Option<usize>, first_crown: usize) -> i32 {
    last_old.map_or(0, |i| {
        let a = span_of(&life.years[i]).1;
        let b = span_of(&life.years[first_crown]).0;
        (b - a - 1).max(0)
    })
}

/// The trunk languages that lead the crown, at most three, in the order
/// they became trunks: the founder first. A dropped one keeps its place, bare
/// dead wood, so the crown's sectors never trade places as shares shift or a
/// language is dropped.
fn trunk_upper(trunks: &[Trunk]) -> Vec<(usize, Lang)> {
    let mut upper: Vec<(usize, Lang)> = trunks
        .iter()
        .enumerate()
        .map(|(k, t)| (k, t.lang.clone()))
        .collect();
    upper.sort_by_key(|u| (trunks[u.0].since, u.0));
    upper.truncate(3);
    upper
}

/// The share of year `y`'s work in the `upper` languages.
fn up_share(upper: &[(usize, Lang)], y: &Year) -> f64 {
    shares_of(&y.months, y.total)
        .iter()
        .filter(|s| upper.iter().any(|u| u.1 == s.0))
        .map(|s| s.1)
        .sum::<f64>()
        / y.total.max(1e-9)
}

/// The years of growth by day `at`; a stray day or two is not a year of
/// growth, unless nothing else grew.
fn active_at(life: &Life, at: Date) -> Vec<usize> {
    let ny = life.years.len();
    let a: Vec<usize> = (0..ny)
        .filter(|&i| life.years[i].active_on.is_some_and(|on| on <= at))
        .collect();
    if a.is_empty() {
        (0..ny).filter(|&i| life.years[i].days > 0).collect()
    } else {
        a
    }
}

/// Where the crown starts among `act`, the years of growth by day `at`.
fn crown_start(life: &Life, trunks: &[Trunk], act: &[usize], at: Date) -> usize {
    let upper = trunk_upper(trunks);
    let history = crate::growth::trunks::history(life);
    // A pause splits the crown once the year that ended it is a year old:
    // until then the old crown stands and the comeback grows above it.
    let paused =
        |a: &Year, b: &Year| b.y - a.y > 1 && b.active_on.is_some_and(|on| at >= on.add_days(365));
    // The crown is the newest run of years led by the crown languages...
    let mut d0 = act.len().saturating_sub(1);
    while d0 > 0 {
        let (a, b) = (&life.years[act[d0 - 1]], &life.years[act[d0]]);
        // ...judged by the trunks as they stood when the later year began: a
        // trunk that arrives or fades later never moves old limbs in or out
        // of the crown.
        let then: Vec<(usize, Lang)> = history
            .iter()
            .take_while(|h| h.0 <= b.y * 12 + 1)
            .last()
            .map(|h| h.1.iter().map(|l| (0, Some(l.clone()))).collect())
            .unwrap_or_default();
        // No language trunk then (private work only): today's trunks judge.
        let then = if then.is_empty() { &upper } else { &then };
        if paused(a, b) || up_share(then, a) < 0.5 {
            break;
        }
        d0 -= 1;
    }
    // ...at most the newest six years, so a long steady life still shows its
    // age below...
    d0 = d0.max(act.len().saturating_sub(6));
    // ...and at least the last three years of a long life.
    while d0 > 0 && act.len() - d0 < 3 && act.len() > 4 {
        if paused(&life.years[act[d0 - 1]], &life.years[act[d0]]) {
            break;
        }
        d0 -= 1;
    }
    d0
}

fn plan_years(life: &Life, trunks: &[Trunk]) -> YearPlan {
    let today = life.observed();
    // The crown can only change when a year arrives, or when a comeback
    // turns a year old: replayed from the start, each loss of crown years is
    // known by day, and they leave over their own time.
    let act = active_at(life, today);
    let mut dates: Vec<(Date, bool)> = Vec::new();
    for (k, &i) in act.iter().enumerate() {
        let Some(on) = life.years[i].active_on else {
            continue;
        };
        dates.push((on, false));
        let later = on.add_days(365);
        if k > 0 && life.years[i].y - life.years[act[k - 1]].y > 1 && later <= today {
            dates.push((later, true));
        }
    }
    dates.sort_unstable();
    let mut left: Vec<(Date, Vec<usize>, f64)> = Vec::new();
    let mut prev: Option<usize> = None;
    for (day, pause) in dates {
        let act = active_at(life, day);
        let Some(&first) = act.get(crown_start(life, trunks, &act, day)) else {
            continue;
        };
        if let Some(p) = prev {
            if first > p {
                let gone: Vec<usize> = act
                    .iter()
                    .copied()
                    .filter(|&i| i >= p && i < first)
                    .collect();
                // A quarter a year, at most a year; an old crown after a
                // pause takes the year.
                let ramp = if pause {
                    365.0
                } else {
                    (RAMP_DAYS * gone.len() as f64).min(365.0)
                };
                if !gone.is_empty() {
                    left.push((day, gone, ramp));
                }
            } else if first < p {
                for l in &mut left {
                    l.1.retain(|&i| i < first);
                }
            }
        }
        prev = Some(first);
    }
    let since = |d: Date| f64::from(today.days_since(d));
    let leaving: Vec<(usize, f64)> = left
        .iter()
        .filter(|(d, _, ramp)| since(*d) < *ramp)
        .flat_map(|(d, g, ramp)| g.iter().map(move |&i| (i, since(*d) / ramp)))
        .collect();
    // Their dead arms stand on for two years, breaking back and falling.
    const LINGER: f64 = 730.0;
    let lingering: Vec<(usize, f64, Date)> = left
        .iter()
        .filter(|(d, _, _)| since(*d) < LINGER)
        .flat_map(|(d, g, _)| g.iter().map(move |&i| (i, since(*d) / LINGER, *d)))
        .collect();
    let mut plan = plan_of(life, trunks, today, &leaving);
    // While years leave, the plans from before each change, for the
    // measures to blend from.
    plan.before = left
        .iter()
        .filter(|(d, _, ramp)| since(*d) < *ramp)
        .map(|(d, _, ramp)| (plan_of(life, trunks, d.add_days(-1), &[]), since(*d) / ramp))
        .collect();
    plan.leaving = leaving;
    plan.lingering = lingering;
    plan
}

/// The plan of `life` as it stood on day `at`, with `leaving` years fading
/// from the crown.
fn plan_of(life: &Life, trunks: &[Trunk], at: Date, leaving: &[(usize, f64)]) -> YearPlan {
    let act = active_at(life, at);
    let d0 = crown_start(life, trunks, &act, at);
    let crown: Vec<usize> = act[d0..].to_vec();
    // The crown's years by weight: leaving years count for what is left of
    // them, and the newest as far as it has grown in, as its arms do; so the
    // crown's languages change as the years come and go, never at once.
    let newest = life.years.len().checked_sub(1);
    let weights: Vec<(usize, f64)> = crown
        .iter()
        .map(|&i| {
            (
                i,
                if Some(i) == newest {
                    life.grown_in(i)
                } else {
                    1.0
                },
            )
        })
        .chain(leaving.iter().map(|&(i, w)| (i, 1.0 - w)))
        .collect();
    let mut by_lang: BTreeMap<Lang, f64> = BTreeMap::new();
    let mut mass = 0.0;
    for &(i, k) in &weights {
        let y = &life.years[i];
        mass += y.total * k;
        for (l, v) in shares_of(&y.months, y.total) {
            *by_lang.entry(l).or_default() += v * k;
        }
    }
    let mass = f64::max(mass, 1e-9);
    // The crown's languages are the ones with a share of its work, at most
    // three: each owns a sector that grows in from 14% to 22% of the crown,
    // so none appears or vanishes at once. A dropped one keeps its sector,
    // bare dead wood, while its years are in the crown.
    let mut named: Vec<(Lang, f64)> = by_lang
        .iter()
        .filter(|(l, _)| l.is_some())
        .map(|(l, v)| (l.clone(), v / mass))
        .collect();
    named.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    named.truncate(3);
    named.retain(|(_, share)| *share >= 0.14);
    if named.is_empty() {
        // Private work, or no language strong enough: the crown's own lead.
        let lead = by_lang
            .iter()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map_or_else(|| trunks[0].lang.clone(), |(l, _)| l.clone());
        named.push((lead, 1.0));
    }
    // The sectors keep their places: in the order the languages were first
    // used.
    let first_use = |l: &Lang| {
        life.years
            .iter()
            .position(|y| l.as_deref().is_some_and(|l| y.langs.contains_key(l)))
            .unwrap_or(usize::MAX)
    };
    named.sort_by_key(|(l, _)| first_use(l));
    let upper: Vec<(usize, Lang)> = named.iter().map(|(l, _)| (0, l.clone())).collect();
    let sector: Vec<f64> = named
        .iter()
        .map(|(_, share)| ((share - 0.14) / 0.08).clamp(0.0, 1.0))
        .collect();
    let old: Vec<usize> = act[..d0].to_vec();
    // The longest pause the wood has lived through: once past, it stays a
    // bare stretch of trunk, also when its years move from crown to limb.
    let pause_m = old
        .iter()
        .chain(std::iter::once(&crown[0]))
        .collect::<Vec<_>>()
        .windows(2)
        .map(|w| pause_months(life, Some(*w[0]), *w[1]))
        .max()
        .unwrap_or(0);
    // Old years split at a silent year or a change of lead language.
    let mut eras: Vec<Vec<usize>> = Vec::new();
    for &i in &old {
        let y = &life.years[i];
        let new = match eras.last() {
            None => true,
            Some(e) => {
                let p = &life.years[*e.last().unwrap_or(&i)];
                let (lp, _) = lead(&p.months, p.total);
                let (ly, sy) = lead(&y.months, y.total);
                y.y - p.y > 1 || (lp != ly && sy >= 0.4)
            }
        };
        if new {
            eras.push(vec![i]);
        } else if let Some(e) = eras.last_mut() {
            e.push(i);
        }
    }
    // A run too small to carry a limb joins its neighbour: a sliver of the
    // life, or a few dozen contributions however long the run.
    let all_total: f64 = life.years.iter().map(|y| y.total).sum::<f64>().max(1.0);
    while eras.len() > 1 {
        let Some(i) = eras.iter().position(|e| {
            let t: f64 = e.iter().map(|&i| life.years[i].total).sum();
            let d: usize = e.iter().map(|&i| life.years[i].days).sum();
            (t < 0.01 * all_total && d < 12) || t < 60.0
        }) else {
            break;
        };
        let e = eras.remove(i);
        if i == 0 {
            eras[0].splice(0..0, e);
        } else {
            eras[i - 1].extend(e);
        }
    }
    // A long run is several limbs.
    let mut split: Vec<Vec<usize>> = Vec::new();
    for e in eras {
        let parts = e.len().div_ceil(5);
        let per = e.len().div_ceil(parts);
        for c in e.chunks(per) {
            split.push(c.to_vec());
        }
    }
    YearPlan {
        upper,
        crown,
        eras: split,
        pause_m,
        leaving: Vec::new(),
        lingering: Vec::new(),
        sector,
        before: Vec::new(),
    }
}

/// The tree's measures: dome size and shape, era spacing, the window.
struct Frame {
    big_s: f64,
    vv: f64,
    th_lo: f64,
    cell: f64,
    gap_h: f64,
    window: f64,
    h_vis: f64,
    /// How busy the years were, 0 to 1.
    vigor: f64,
}

impl Frame {
    /// The measures of `plan`; while years leave the crown, a blend of the
    /// measures with them in the crown and with them in their era limbs,
    /// change after change, so one starting never cuts another short.
    fn of(life: &Life, plan: &YearPlan) -> Self {
        let Some((first, _)) = plan.before.first() else {
            return Self::of_plan(life, plan);
        };
        let mut f = Self::of_plan(life, first);
        for (k, (_, w)) in plan.before.iter().enumerate() {
            let next = plan
                .before
                .get(k + 1)
                .map_or_else(|| Self::of_plan(life, plan), |b| Self::of_plan(life, &b.0));
            f = f.toward(&next, *w);
        }
        f
    }

    /// `self` moved `w` of the way to `next`.
    fn toward(&self, next: &Self, w: f64) -> Self {
        let mix = |a: f64, b: f64| a + (b - a) * w;
        Self {
            big_s: mix(self.big_s, next.big_s),
            vv: mix(self.vv, next.vv),
            th_lo: mix(self.th_lo, next.th_lo),
            cell: mix(self.cell, next.cell),
            gap_h: mix(self.gap_h, next.gap_h),
            window: mix(self.window, next.window),
            h_vis: mix(self.h_vis, next.h_vis),
            vigor: mix(self.vigor, next.vigor),
        }
    }

    fn of_plan(life: &Life, plan: &YearPlan) -> Self {
        let age = life.age();
        let recent: f64 = plan
            .crown
            .iter()
            .map(|&i| life.years[i].total * decay(life.years_ago(i)))
            .sum();
        // Grows with the work done, a little every day, not in steps at
        // each new year; a few contributions over many years stay small.
        let span = life.now() - f64::from(life.years[0].y);
        let work = life.work_years().min(span);
        // How busy the years were, 0 to 1: sparse years space limbs closer.
        let vigor = (work / span.max(1.0)).clamp(0.0, 1.0).sqrt();
        let fill = wt(recent / 3.0).min(1.2);
        // A young tree starts at the seedling's size and fills out over its
        // second year of work.
        let young = 0.25 + 0.75 * life.maturity();
        let big_s = (40.0 + 29.0 * math::ln(1.5 + work))
            * (0.86 + 0.16 * fill)
            * (0.86 + 0.014 * age.min(18.0))
            * young;
        let cell = (big_s * 0.2).clamp(13.0 * young, 31.0);
        let n_old = plan.eras.len();
        // Many eras sit closer together: the tree never grows tall from them.
        let gap_h = if n_old > 1 {
            (30.0 / (n_old - 1) as f64).min(15.0)
        } else {
            15.0
        } * (0.4 + 0.6 * vigor);
        let window = if n_old > 0 {
            (14.0 + 22.0 * (f64::from(plan.pause_m) / 12.0).min(1.0)) * (0.4 + 0.6 * vigor)
        } else {
            0.0
        };
        let h_clear = 40.0 + 16.0 * (1.0 - math::exp(-age / 5.0));
        let mut frame = Self {
            big_s,
            vv: (0.7 - 0.012 * age).clamp(0.48, 0.7),
            th_lo: -0.06 - 0.006 * age,
            cell,
            gap_h,
            window,
            h_vis: h_clear,
            vigor,
        };
        if n_old > 0 {
            frame.h_vis = h_clear.max(frame.limb_h(n_old - 1) + 12.0 + window + cell * 0.55);
        }
        // A tree is never shorter than the seedling it grew from: the young
        // tree keeps the seedling's stem while its crown fills out.
        frame.h_vis = frame.h_vis.max(SEEDLING_TOP - frame.vv * big_s);
        frame
    }

    /// Height above the soil of era limb `i`, oldest lowest.
    fn limb_h(&self, i: usize) -> f64 {
        28.0 + self.gap_h * i as f64
    }
}

/// The crown's size at its fullest for the work in `timeline`: it grows with
/// the work done, the age and the maturity, and never shrinks, so a pot sized
/// to it is never too big a year later. 0 for a timeline without work.
#[doc(hidden)]
pub fn full_size(timeline: &Timeline, periods: &[(Date, Date, Vec<String>)]) -> f64 {
    let Some(life) = Life::of(timeline, periods) else {
        return 0.0;
    };
    let span = life.now() - f64::from(life.years[0].y);
    let work = life.work_years().min(span);
    (40.0 + 29.0 * math::ln(1.5 + work))
        * 1.05
        * (0.86 + 0.014 * life.age().min(18.0))
        * (0.25 + 0.75 * life.maturity())
}

/// The crown years of a timeline, oldest first, with private work declared
/// in `periods`.
#[doc(hidden)]
pub fn crown_years(timeline: &Timeline, periods: &[(Date, Date, Vec<String>)]) -> Vec<i32> {
    let Some(life) = Life::of(timeline, periods) else {
        return Vec::new();
    };
    let plan = plan_years(&life, &trunks(&life));
    plan.crown.iter().map(|&i| life.years[i].y).collect()
}

/// Every year the tree grows from: the crown years and every era's years.
#[doc(hidden)]
pub fn years_grown(timeline: &Timeline) -> std::collections::BTreeSet<i32> {
    let Some(life) = Life::of(timeline, &[]) else {
        return std::collections::BTreeSet::new();
    };
    let plan = plan_years(&life, &trunks(&life));
    plan.crown
        .iter()
        .chain(plan.eras.iter().flatten())
        .map(|&i| life.years[i].y)
        .collect()
}

/// The band of heights above the soil, between the last era limb and the
/// crown, that a pause keeps clear; `None` without eras.
#[doc(hidden)]
pub fn window_band(timeline: &Timeline) -> Option<(f64, f64)> {
    let life = Life::of(timeline, &[])?;
    let plan = plan_years(&life, &trunks(&life));
    if plan.eras.is_empty() {
        return None;
    }
    let frame = Frame::of(&life, &plan);
    let lo = frame.limb_h(plan.eras.len() - 1) + 16.0;
    Some((lo, lo + frame.window * 0.5))
}

/// What every stage reads: the life, its plan and measures, and the choices
/// made once for the whole tree.
struct Ctx<'a> {
    life: &'a Life,
    seed: u64,
    plan: YearPlan,
    frame: Frame,
    age: f64,
    y_first: i32,
    /// The main trunk's side: -1 or 1.
    s_a: f64,
    dormant: bool,
    /// The dome's flat bottom.
    y_b: f64,
    /// The furthest the tree has reached, today included.
    reach: Reach,
    /// The recent months' average, when today is half of it.
    recent: Option<Recent>,
}

/// What a tree has grown to: a trunk only ever grows, or hangs further, so
/// each is the furthest it reached at any month before, or today.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Reach {
    /// Height of the trunk below the crown.
    pub h_vis: f64,
    /// The crown's size.
    pub big_s: f64,
    /// The crown's rise above the trunk.
    pub top: f64,
    /// How irregular the work was, which bends the trunk.
    pub irr: f64,
    /// The trunk's girth at the root, as flow.
    pub girth: f64,
    /// What a formal upright's trunk carries for its steady years, as flow:
    /// kept once grown, like the girth.
    pub trunk: f64,
}

/// What the tree of `life` reaches today, and its measures for averaging;
/// `None` for a seedling. `chokkan` is how formal upright it stands, 0 to 1
/// ([`Leader::tier`]).
pub fn measures(life: &Life, seed: u64, chokkan: f64) -> Option<(Reach, Recent)> {
    if life.is_seedling() {
        return None;
    }
    let c = Ctx::new(life, seed, ease(chokkan));
    Some((c.reach, Recent::of_ctx(&c)))
}

impl Reach {
    /// Half these, half `recent`'s average, as the tree shows them.
    #[must_use]
    pub fn settled(self, recent: &Recent) -> Self {
        let vv = f64::midpoint(self.top / self.big_s.max(1e-9), recent.frame[1]);
        let big_s = f64::midpoint(self.big_s, recent.frame[0]);
        Self {
            h_vis: f64::midpoint(self.h_vis, recent.frame[6]),
            big_s,
            top: vv * big_s,
            irr: f64::midpoint(self.irr, recent.irr),
            girth: self.girth,
            trunk: self.trunk,
        }
    }

    /// The furthest of both.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self {
            h_vis: self.h_vis.max(other.h_vis),
            big_s: self.big_s.max(other.big_s),
            top: self.top.max(other.top),
            irr: self.irr.max(other.irr),
            girth: self.girth.max(other.girth),
            trunk: self.trunk.max(other.trunk),
        }
    }
}

/// A tree's measures on one day, for averaging over recent months.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recent {
    frame: [f64; 8],
    irr: f64,
    strands: BTreeMap<String, [f64; 3]>,
    sector: BTreeMap<String, f64>,
}

impl Recent {
    /// The measures of the tree in `c`.
    fn of_ctx(c: &Ctx) -> Self {
        let life = c.life;
        let f = &c.frame;
        Self {
            frame: [
                f.big_s, f.vv, f.th_lo, f.cell, f.gap_h, f.window, f.h_vis, f.vigor,
            ],
            irr: c.reach.irr,
            strands: crate::growth::trunks::strands(life)
                .1
                .into_iter()
                .map(|s| (s.lang, [s.peel, s.sag, s.share]))
                .collect(),
            sector: c
                .plan
                .upper
                .iter()
                .zip(&c.plan.sector)
                .map(|(u, w)| (key(&u.1), *w))
                .collect(),
        }
    }

    /// The average of `all`; a strand missing in a month counts as not yet
    /// there.
    pub fn mean(all: &[Self]) -> Option<Self> {
        Self::weighted(&all.iter().map(|r| (r, 1.0)).collect::<Vec<_>>())
    }

    /// The average of `all`, each month by its weight.
    pub fn weighted(all: &[(&Self, f64)]) -> Option<Self> {
        let n: f64 = all.iter().map(|a| a.1).sum();
        if n <= 0.0 {
            return None;
        }
        let mut out = Self::default();
        for &(r, w) in all {
            for (o, v) in out.frame.iter_mut().zip(r.frame) {
                *o += v * w / n;
            }
            out.irr += r.irr * w / n;
            for (l, v) in &r.strands {
                let o = out.strands.entry(l.clone()).or_default();
                for (o, v) in o.iter_mut().zip(v) {
                    *o += v * w / n;
                }
            }
            for (l, v) in &r.sector {
                *out.sector.entry(l.clone()).or_default() += v * w / n;
            }
        }
        Some(out)
    }
}

/// The trunk's flow at the root before the age factor, and that factor.
fn girth_target(life: &Life) -> (f64, f64) {
    let lifetime: f64 = life.years.iter().map(|y| y.total).sum();
    let age = life.age();
    let f = math::pow((age / 14.0).clamp(0.0, 1.0), 1.2).max(0.35);
    let target =
        lifetime * (SEEDLING_FLOW / (FLARE * f)).max(1.5 + 0.25 * math::pow(age / 9.0, 4.0));
    (target, f)
}

/// What a formal upright's trunk carries on top of [`girth_target`]'s
/// `target`, as flow before the age factor: its steady years, so years of
/// constant work stand as a massive trunk. `tier` is how formal upright the
/// tree stands, eased.
fn trunk_extra(life: &Life, tier: f64, target: f64) -> f64 {
    let steady: f64 = life
        .years
        .iter()
        .map(|y| (y.days as f64 / 120.0).min(1.0))
        .sum();
    target * tier * GIRTH_K * ((steady - 4.0) / 12.0).clamp(0.0, 1.0)
}

impl<'a> Ctx<'a> {
    /// The lowest a pause's window reaches, or the sky without a pause.
    fn window_lo(&self) -> f64 {
        match self.plan.eras.len() {
            n if n > 0 && self.plan.pause_m > 0 => self.frame.limb_h(n - 1) + 16.0,
            _ => f64::INFINITY,
        }
    }

    /// The formal upright's triangle ([`Cone`]), its base above any
    /// pause's window.
    fn cone(&self) -> Cone {
        let f = &self.frame;
        let mut k = Cone::of(f.big_s, f.h_vis, f.vv);
        if self.window_lo().is_finite() {
            k.h_b = k.h_b.max(self.window_lo() + f.window * 0.5 + f.cell * 0.5);
        }
        k
    }

    /// The same tree measured by another plan: as it stood on an earlier day.
    fn with_plan(&self, plan: YearPlan) -> Self {
        let frame = Frame::of_plan(self.life, &plan);
        Self {
            life: self.life,
            seed: self.seed,
            age: self.age,
            y_first: self.y_first,
            s_a: self.s_a,
            dormant: self.dormant,
            y_b: SOIL_Y - frame.h_vis,
            plan,
            frame,
            reach: self.reach,
            recent: self.recent.clone(),
        }
    }

    /// Half today's measures, half the average of the last three months:
    /// a burst of work shows half at once and the rest over a season.
    fn settled(mut self, recent: Option<Recent>) -> Self {
        let Some(r) = recent else {
            return self;
        };
        let f = &mut self.frame;
        let now = [
            f.big_s, f.vv, f.th_lo, f.cell, f.gap_h, f.window, f.h_vis, f.vigor,
        ];
        let mix: Vec<f64> = now
            .iter()
            .zip(r.frame)
            .map(|(a, b)| 0.5 * a + 0.5 * b)
            .collect();
        (f.big_s, f.vv, f.th_lo, f.cell) = (mix[0], mix[1], mix[2], mix[3]);
        (f.gap_h, f.window, f.h_vis, f.vigor) = (mix[4], mix[5], mix[6], mix[7]);
        self.y_b = SOIL_Y - f.h_vis;
        self.reach.h_vis = f.h_vis;
        self.reach.big_s = f.big_s;
        self.reach.top = f.vv * f.big_s;
        self.reach.irr = 0.5 * self.reach.irr + 0.5 * r.irr;
        // A sector new to the crown grows in from nothing, half at once.
        for (u, w) in self.plan.upper.iter().zip(self.plan.sector.iter_mut()) {
            *w = 0.5 * *w + 0.5 * r.sector.get(&key(&u.1)).copied().unwrap_or(0.0);
        }
        self.recent = Some(r);
        self
    }

    /// Never less than the tree reached before (`past`): the trunk keeps its
    /// height, the crown its size and rise.
    fn reaching(mut self, past: Reach) -> Self {
        let r = self.reach.max(past);
        self.frame.h_vis = r.h_vis;
        self.frame.big_s = r.big_s;
        self.frame.vv = r.top / r.big_s.max(1e-9);
        self.y_b = SOIL_Y - r.h_vis;
        self.reach = r;
        self
    }

    fn new(life: &'a Life, seed: u64, tier: f64) -> Self {
        let plan = plan_years(life, &trunks(life));
        let frame = Frame::of(life, &plan);
        let reach = Reach {
            h_vis: frame.h_vis,
            big_s: frame.big_s,
            top: frame.vv * frame.big_s,
            irr: irr(life.life_cv),
            girth: {
                let (target, f) = girth_target(life);
                (target + trunk_extra(life, tier, target)) * f
            },
            trunk: {
                let (target, f) = girth_target(life);
                trunk_extra(life, tier, target) * f
            },
        };
        Self {
            reach,
            recent: None,
            life,
            seed,
            age: life.age(),
            y_first: life.years[0].y,
            // The main trunk's side: the right, as every tree grows there
            // ([`grow`] mirrors a left one).
            s_a: 1.0,
            dormant: life.dormant(),
            y_b: SOIL_Y - frame.h_vis,
            plan,
            frame,
        }
    }

    /// How far `l`'s crown sector has grown in: 0 (none) to 1.
    fn sector_of(&self, l: &Lang) -> f64 {
        self.plan
            .upper
            .iter()
            .zip(&self.plan.sector)
            .find(|(u, _)| &u.1 == l)
            .map_or(0.0, |(_, w)| *w)
    }

    /// Whether `l` was dropped.
    fn gone(&self, l: &Lang) -> bool {
        l.as_deref().is_some_and(|l| self.life.dropped(l))
    }

    /// How far `l` has died back since it was dropped, 0 to 1.
    fn dying(&self, l: &Lang) -> f64 {
        l.as_deref().map_or(0.0, |l| self.life.drop_progress(l))
    }
}

/// The trunks as grown: chains main first, the side each stands on, the
/// shared foot's length, and the dome's center line.
struct Wood {
    chains: Vec<Vec<usize>>,
    sides: Vec<f64>,
    foot: usize,
    s_a: f64,
    center: f64,
    /// The second chain is a dead strand.
    dead_b: bool,
    /// How far that dead strand has died back, 0 to 1.
    dead_g: f64,
    /// How far the wind sweeps the tree, signed ([`Leader::sweep`]).
    sweep: f64,
    /// How far the second trunk has come away, 0 without one.
    peel_b: f64,
    /// Months since the second trunk's language claimed its strand.
    months_b: i32,
    /// Where along the second chain its free part begins.
    free_b: usize,
    /// How far the main trunk cascades over the rim, signed ([`Leader::cascade`]).
    cascade: f64,
    /// The side the tree leans to when it leans ([`Leader::downwind`]).
    downwind: f64,
    /// How far the crown is a formal upright's triangle, eased
    /// ([`Leader::tier`]); a young tree grows into it.
    tier: f64,
}

impl Wood {
    /// Chain `k`, or the main trunk: nothing living grows from a dead
    /// strand, and dead arms hang from the trunk, so no strand thins as they
    /// break back and fall.
    fn chain_for(&self, k: usize, dead: bool) -> usize {
        if dead || self.dead_b { 0 } else { k }
    }

    /// The chain on `side`: the second trunk's when there are two.
    fn chain_on(&self, side: f64) -> usize {
        usize::from(self.sides.len() > 1 && side != self.s_a)
    }

    /// How far along chain `k` the dome rim (at `y_b`) sits.
    fn rim_frac(&self, k: usize, sk: &Skeleton, y_b: f64) -> f64 {
        let ch = &self.chains[k];
        let i = ch
            .iter()
            .position(|&n| sk.nodes[n].p.1 < y_b + 4.0)
            .unwrap_or(ch.len() - 1);
        i as f64 / (ch.len() - 1) as f64
    }
}

/// Each crown language's slice of the rim, from u = 0 on the main trunk's
/// side to u = 1 on the other.
struct Sectors {
    bounds: Vec<f64>,
    upper: Vec<(usize, Lang)>,
    s_a: f64,
    th_lo: f64,
}

impl Sectors {
    fn of(c: &Ctx, arms: &[Arm]) -> Self {
        let upper = &c.plan.upper;
        let mass: Vec<f64> = upper
            .iter()
            .map(|(_, l)| {
                arms.iter()
                    .filter(|a| &a.lang == l)
                    .map(|a| a.vd * a.w)
                    .sum::<f64>()
            })
            .collect();
        let mut widths: Vec<f64> = mass.iter().map(|m| math::pow(m.max(1.0), 0.6)).collect();
        let wsum: f64 = widths.iter().sum::<f64>().max(1e-9);
        // A fair slice each, of a sector still growing in only a part.
        for (w, k) in widths.iter_mut().zip(&c.plan.sector) {
            *w = (*w / wsum * k).max(if upper.len() > 1 { 0.3 * k } else { 0.0 });
        }
        let wsum: f64 = widths.iter().sum::<f64>().max(1e-9);
        let mut bounds = vec![0.0];
        for w in &widths {
            let last = *bounds.last().unwrap_or(&0.0);
            bounds.push(last + w / wsum);
        }
        Self {
            bounds,
            upper: upper.clone(),
            s_a: c.s_a,
            th_lo: c.frame.th_lo,
        }
    }

    /// The dome angle of rim position `u`.
    fn theta(&self, u: f64) -> f64 {
        let (th_lo, u) = (self.th_lo, u.clamp(0.0, 1.0));
        if self.s_a < 0.0 {
            PI - th_lo - u * (PI - 2.0 * th_lo)
        } else {
            th_lo + u * (PI - 2.0 * th_lo)
        }
    }

    /// The language owning rim position `u`.
    fn lang_at(&self, u: f64) -> Lang {
        let n = self.upper.len();
        let i = self
            .bounds
            .iter()
            .skip(1)
            .position(|b| u < *b)
            .unwrap_or(n - 1);
        self.upper[i.min(n - 1)].1.clone()
    }
}

/// The dome's outline: where the crown arms reach, by angle.
struct Dome {
    center: f64,
    y_b: f64,
    vv: f64,
    rz: f64,
    r_mean: f64,
    w_mean: f64,
    /// (angle, reach, weight) of every placed arm.
    env: Vec<(f64, f64, f64)>,
    /// How far the wind sweeps the crown, signed (fukinagashi).
    sweep: f64,
    /// How far the crown parts in two over two trunks, 0 to 1.
    split: f64,
    /// How far the crown is a formal upright's triangle, 0 to 1.
    tier: f64,
    /// The dome's lowest rim angle.
    th_lo: f64,
    /// The triangle ([`Cone`]) and its layers on each side.
    cone: Cone,
    layers: f64,
}

impl Dome {
    /// The dome's radius at angle `th` and how densely arms crowd there; a
    /// formal upright's is the same on both sides.
    fn envelope(&self, th: f64) -> (f64, f64) {
        let (r, d) = self.envelope_at(th);
        if self.tier <= 0.0 {
            return (r, d);
        }
        let (rm, dm) = self.envelope_at(PI - th);
        let k = 0.5 * self.tier;
        (r + k * (rm - r), d + k * (dm - d))
    }

    /// [`Self::envelope`] as the arms lie.
    fn envelope_at(&self, th: f64) -> (f64, f64) {
        let mut num = self.r_mean * 0.12 * self.w_mean;
        let mut den = 0.12 * self.w_mean;
        for (t, r, w) in &self.env {
            let q = (th - t) / 0.36;
            let k = math::exp(-(q * q));
            num += k * w * r;
            den += k * w;
        }
        (num / den, (den / self.w_mean).min(3.0))
    }

    /// The point at angle `th` and radius `r` on the dome's face. In the
    /// wind the dome streams downwind, its top furthest, its windward side
    /// pulled in, and it lies lower.
    fn pt(&self, th: f64, r: f64) -> P {
        let p = self.domed(th, r);
        if self.tier <= 0.0 {
            return p;
        }
        let q = self.triangle(th, r);
        (p.0 + self.tier * (q.0 - p.0), p.1 + self.tier * (q.1 - p.1))
    }

    /// The point at angle `th` and radius `r` in a formal upright's crown: a
    /// symmetric triangle, widest at the bottom and narrowing evenly to the
    /// apex, its foliage in soft layers, left and right a half layer apart.
    /// Up the rim from either side walks the edge to the apex; further in
    /// lies further toward the trunk at the same height.
    fn triangle(&self, th: f64, r: f64) -> P {
        let a = ((th - self.th_lo) / (PI - 2.0 * self.th_lo)).clamp(0.0, 1.0);
        let v = 1.0 - (2.0 * a - 1.0).abs();
        let side = if a < 0.5 { 1.0 } else { -1.0 };
        let rf = (r / self.envelope(th).0.max(1.0)).max(0.0);
        // Soft layers: each a little apart from the next, with a narrow gap.
        let off = if side > 0.0 { 0.0 } else { 0.25 };
        let q = v * self.layers + off;
        let vl = (q.floor() + 0.5 + 0.3 * (q - q.floor() - 0.5) - off) / self.layers;
        let Cone { w_b, h_b, h_top } = self.cone;
        let h = h_b + (h_top - h_b) * vl.clamp(0.0, 1.0);
        (self.center + side * rf * w_b * (1.0 - v), SOIL_Y - h)
    }

    /// [`Self::pt`] on the dome, before any triangle.
    fn domed(&self, th: f64, r: f64) -> P {
        let (s, d) = (self.sweep.abs(), self.sweep.signum());
        let (cx, sy) = (math::cos(th), math::sin(th));
        // Over two trunks the windward crown stands in the lee of its own
        // trunk: rounder, swept less.
        let s = s * (1.0 - 0.7 * self.split * (-3.0 * cx * d).clamp(0.0, 1.0));
        let windward = if cx * d < 0.0 { 1.0 - 0.6 * s } else { 1.0 };
        // Downwind, the crown hangs lower the further out it streams.
        // Over two trunks its low side hangs lower still, all together, with
        // air between it and the crown streaming over it.
        let low = ((0.65 - sy) / 0.25).clamp(0.0, 1.0);
        let low = low * low * (3.0 - 2.0 * low);
        let droop = (0.4 * s + 0.4 * self.split * low) * r * (cx * d).max(0.0);
        let x = self.center + r * cx * windward + d * s * r * (0.35 + 0.45 * sy);
        let y = self.y_b - self.vv * r * sy * (1.0 - 0.3 * s) + droop;
        if self.split <= 0.0 {
            return (x, y);
        }
        // Over two trunks the crown parts in two, steeply but smoothly: the
        // windward one higher, the downwind one lower, space between.
        let z = (x - self.center) / (0.08 * self.r_mean.max(1.0));
        let t = 1.0 - 2.0 / (math::exp(2.0 * z.clamp(-20.0, 20.0)) + 1.0);
        let down = t * d;
        let rise = if down > 0.0 { 0.35 } else { 0.15 };
        (
            x + 0.45 * self.r_mean * self.split * t,
            y + rise * self.r_mean * self.split * down,
        )
    }
}

/// A formal upright's foliage triangle, heights above the soil: about as
/// wide as the tree is tall ([`CONE_WIDE`]), its base about a third of the way up.
#[derive(Debug, Clone, Copy)]
struct Cone {
    /// Half-width at the base.
    w_b: f64,
    /// The base's height.
    h_b: f64,
    /// The apex's height.
    h_top: f64,
}

impl Cone {
    /// The triangle for a dome of size `big_s` and flatness `vv` over a
    /// trunk `h_vis` tall.
    fn of(big_s: f64, h_vis: f64, vv: f64) -> Self {
        let w_b = big_s * 0.95;
        let h_top = (2.0 * w_b / CONE_WIDE).max(h_vis + vv * big_s);
        Self {
            w_b,
            h_b: CLEAR * h_top,
            h_top,
        }
    }

    /// The half-width at height `h`, wider still below the base.
    fn width(&self, h: f64) -> f64 {
        self.w_b * (self.h_top - h) / (self.h_top - self.h_b)
    }
}

/// How much wider than tall a formal upright's triangle stands: about as
/// wide as it is tall, so an old one stands up and never squats as a dome.
const CONE_WIDE: f64 = 1.05;

/// How much of a formal upright's height its trunk stands bare before the
/// first limb: the straight trunk shows, the triangle stands on it.
const CLEAR: f64 = 0.36;

/// How steeply a formal upright's low limbs rise: up per unit out.
const LIMB_RISE: f64 = 0.6;

/// A formal upright's foliage layers on each side.
const LAYERS: f64 = 4.0;

/// The share of a formal upright's extra girth laid evenly up its trunk
/// above the nebari: the trunk tapers less, its top keeps some girth.
const TRUNK_UP: f64 = 0.25;

/// A crown arm placed: (index into the arms, angle, reach, chain).
type Placed = (usize, f64, f64, usize);

/// A language as a map key.
fn key(l: &Lang) -> String {
    l.clone().unwrap_or_else(|| "-".into())
}

fn dist(a: P, b: P) -> f64 {
    ((a.0 - b.0) * (a.0 - b.0) + (a.1 - b.1) * (a.1 - b.1)).sqrt()
}

/// Grows the tree of a life of a year or more into `sk` (which holds only
/// the root); returns the trunk chains, main trunk first. Each stage below
/// is one step of the composition, in order: the random draws of the shared
/// `bdome` stream follow that order.
pub fn tree(
    sk: &mut Skeleton,
    life: &Life,
    seed: u64,
    leader: &Leader,
    past: Reach,
    recent: Option<Recent>,
) -> Vec<Vec<usize>> {
    let c = Ctx::new(life, seed, ease(leader.tier))
        .settled(recent)
        .reaching(past);
    let mut rng = Rng::keyed(seed, "bdome");
    sk.dormant = c.dormant;
    let wood = grow_trunks(sk, &c, leader, &mut rng);
    let eras_from = sk.nodes.len();
    let limbs = grow_eras(sk, &c, &wood);
    let eras = eras_from..sk.nodes.len();
    let frozen = frozen_dead(&c, &wood);
    let (arms, lower) = crown_arms(&c, &frozen);
    let sectors = Sectors::of(&c, &arms);
    let (mut placed, mut dome) = place_arms(&c, &wood, &arms, &sectors);
    // The wind's sweep, and a cascade's flow to the right.
    dome.sweep = leader.sweep + 0.8 * leader.cascade;
    dome.split = leader.sweep.abs() * wood.peel_b;
    // Dead arms of years gone from the crown stay where they stood.
    placed.retain(|p| !frozen.contains_key(&(arms[p.0].year, arms[p.0].lang.clone())));
    for (i, a) in arms.iter().enumerate() {
        if let Some(&(th, r, _, k)) = frozen.get(&(a.year, a.lang.clone())) {
            placed.push((i, th, r, k));
        }
    }
    let (by_lang, dead_by_lang) = grow_arms(sk, &c, &wood, &dome, &arms, &placed);
    let mut low = limbs;
    // The skirt downwind runs out low with them; upwind it stays in its crown.
    let downwind = leader.sweep.signum();
    low.extend(
        grow_skirt(sk, &c, &wood, &dome, &lower)
            .into_iter()
            .filter(|l| l.1 * downwind > 0.0),
    );
    // How much of each language's dead arms still stands: its dead fans break
    // back with them.
    let mut dead_w: BTreeMap<String, f64> = BTreeMap::new();
    for a in arms.iter().filter(|a| a.dead.is_some()) {
        let w = dead_w.entry(key(&a.lang)).or_insert(0.0);
        *w = w.max(a.w * a.back);
    }
    dome_clouds(
        sk,
        &c,
        &wood,
        &dome,
        &sectors,
        (&by_lang, &dead_by_lang, &dead_w),
        arms.len(),
    );
    trim_to_cone(sk, &wood, &dome);
    wind_shape(sk, &wood, &eras);
    wind_limbs(sk, &wood, &low);
    wind_branches(sk, &wood, &low);
    above_rim(sk, &wood);
    no_lone_top(sk, &wood);
    no_lone_tuft(sk, &wood);
    cascade_shape(sk, &wood);
    girth(sk, &c, &wood);
    crate::growth::wood::hug(sk);
    wood.chains
}

// The cascade's main trunk, from the foot (0) to the tip (10): sideways
// toward the spill and up, as shares of the trunk's height. A semi-cascade
// (han-kengai): it rises, rounds over at the crest, drops over the rim and
// runs out flatter to its end just outside it, about level with the rim,
// never below the pot.
const CASCADE: [(f64, f64); 11] = [
    (-0.12, 0.0),
    (-0.05, 0.18),
    (0.04, 0.33),
    (0.15, 0.44),
    (0.28, 0.5),
    (0.41, 0.49),
    (0.52, 0.42),
    (0.62, 0.27),
    (0.73, 0.1),
    (0.88, 0.02),
    (1.08, -0.01),
];

// Where a cascade's foliage pads sit, as (along the trunk, sideways and up
// from that point, as shares of its height): one over the crest, then down
// the arc past the rim, the last a little below it.
const PADS: [(f64, f64, f64); 4] = [
    (0.36, -0.04, 0.24),
    (0.55, 0.03, 0.025),
    (0.78, 0.12, 0.07),
    (1.0, 0.14, -0.04),
];

/// How much of their spread a pad's clouds keep around its center, across
/// and up, and at most how far out they lie, as a share of the trunk's
/// height: wide enough that a pad's clouds lie side by side, its foliage
/// showing, not stacked on one another.
const SPREAD: (f64, f64, f64) = (0.6, 0.25, 0.2);
/// How much a cascade's clouds shrink, and the widest a cloud may be, as a
/// share of the trunk's height.
const SHRINK: (f64, f64) = (0.1, 0.26);

/// Bends the main trunk into a cascade over the rim; everything grown from
/// it follows the point it grew from, and each cloud its branch.
fn cascade_shape(sk: &mut Skeleton, wood: &Wood) {
    let w = wood.cascade.abs();
    let k = w * w * (3.0 - 2.0 * w);
    let Some(main) = wood.chains.first().filter(|_| k > 0.0) else {
        return;
    };
    let d = wood.cascade.signum();
    let foot = sk.nodes[main[0]].p;
    let tip = sk.nodes[*main.last().unwrap_or(&0)].p;
    let h = (foot.1 - tip.1).max(1.0);
    let at = |u: f64| {
        let x = u.clamp(0.0, 1.0) * 10.0;
        let i = (x as usize).min(9);
        let f = x - i as f64;
        let (p, q) = (CASCADE[i], CASCADE[i + 1]);
        (
            foot.0 + d * h * (p.0 + (q.0 - p.0) * f),
            foot.1 - h * (p.1 + (q.1 - p.1) * f),
        )
    };
    let hangs = hangs_of(sk, &sk.nodes.iter().map(|n| n.p).collect::<Vec<P>>(), false);
    let mut delta: Vec<Option<P>> = vec![None; sk.nodes.len()];
    let n = main.len().max(2) - 1;
    let on_main: std::collections::BTreeSet<usize> = main.iter().copied().collect();
    for (i, &node) in main.iter().enumerate() {
        let (p, t) = (sk.nodes[node].p, at(i as f64 / n as f64));
        delta[node] = Some((k * (t.0 - p.0), k * (t.1 - p.1)));
    }
    // A strand lies along the trunk: it bends with the trunk at its height.
    for i in 0..sk.nodes.len() {
        if sk.nodes[i].strand && delta[i].is_none() {
            let y = sk.nodes[i].p.1;
            delta[i] = main
                .iter()
                .min_by(|&&x, &&z| {
                    (sk.nodes[x].p.1 - y)
                        .abs()
                        .total_cmp(&(sk.nodes[z].p.1 - y).abs())
                })
                .and_then(|&m| delta[m]);
        }
    }
    // Everything else follows the point it grew from, first.
    for i in 0..sk.nodes.len() {
        if delta[i].is_none() {
            delta[i] = sk.nodes[i].parent.and_then(|p| delta[p]);
        }
    }
    let follow_of = |c: usize| delta[hangs[c]].unwrap_or((0.0, 0.0));
    // The clouds gather in a few pads along the arc, left to right: each
    // pad takes its share of them in order across the crown, keeping a
    // little of their spread around its center: a stray cloud far out (a
    // low limb's) folds into the pad, so each pad stands apart.
    // Private work goes last, down the arc: the crest belongs to a public
    // language.
    let mut order: Vec<usize> = (0..sk.clouds.len()).collect();
    let private = |c: usize| crate::growth::clouds::lead_of(&sk.clouds[c]).is_none();
    order.sort_by(|&a, &b| {
        private(a)
            .cmp(&private(b))
            .then(sk.clouds[a].c.0.total_cmp(&sk.clouds[b].c.0))
    });
    let pads = PADS.len().min(order.len().div_ceil(3)).max(1);
    let mut gather: Vec<P> = vec![(0.0, 0.0); sk.clouds.len()];
    for (j, group) in order.chunks(order.len().div_ceil(pads).max(1)).enumerate() {
        let (u, sx, sy) = PADS[j.min(PADS.len() - 1)];
        let base = at(u);
        let slot = (base.0 + d * sx * h, base.1 - sy * h);
        let m = group.len() as f64;
        let moved = |c: usize| {
            (
                sk.clouds[c].c.0 + follow_of(c).0,
                sk.clouds[c].c.1 + follow_of(c).1,
            )
        };
        let mid = (
            group.iter().map(|&c| moved(c).0).sum::<f64>() / m,
            group.iter().map(|&c| moved(c).1).sum::<f64>() / m,
        );
        for &c in group {
            let p = moved(c);
            let reach = SPREAD.2 * h;
            let to = (
                slot.0 + (SPREAD.0 * (p.0 - mid.0)).clamp(-reach, reach),
                slot.1 + (SPREAD.1 * (p.1 - mid.1)).clamp(-reach, reach),
            );
            gather[c] = (k * (to.0 - p.0), k * (to.1 - p.1));
        }
    }
    // Each branch reaches out to its pad: from nothing where it leaves the
    // trunk to its cloud's whole move at the cloud.
    let before: Vec<P> = sk.nodes.iter().map(|n| n.p).collect();
    let nearest_cloud = |p: P| {
        (0..sk.clouds.len())
            .min_by(|&a, &b| dist(sk.clouds[a].c, p).total_cmp(&dist(sk.clouds[b].c, p)))
    };
    let mut attach: Vec<Option<usize>> = vec![None; sk.nodes.len()];
    // How far out each node has reached, never less than the wood it grew
    // from: a limb that has reached its pad doesn't fall back below it.
    let mut out = vec![0.0f64; sk.nodes.len()];
    for i in 0..sk.nodes.len() {
        if on_main.contains(&i) {
            attach[i] = Some(i);
        } else {
            attach[i] = sk.nodes[i].parent.and_then(|p| attach[p]);
        }
        let reach = match (attach[i], nearest_cloud(before[i])) {
            // Dead wood bears no leaves: it keeps its place on the trunk.
            (Some(a), Some(c))
                if !on_main.contains(&i) && sk.nodes[i].language.as_deref() != Some(JIN) =>
            {
                let t = dist(before[i], before[a]) / dist(sk.clouds[c].c, before[a]).max(1e-6);
                let t = t.max(sk.nodes[i].parent.map_or(0.0, |p| out[p]));
                out[i] = t;
                let g = gather[c];
                (g.0 * t.min(1.0), g.1 * t.min(1.0))
            }
            _ => (0.0, 0.0),
        };
        if let Some((dx, dy)) = delta[i] {
            sk.nodes[i].p.0 += dx + reach.0;
            sk.nodes[i].p.1 += dy + reach.1;
        }
    }
    // Its foliage lies in flat, level pads, smaller than a crown's clouds so
    // the pads stand apart; no cloud wider than a pad.
    let flat = 1.0 - 0.55 * k;
    for ((cloud, &n), g) in sk.clouds.iter_mut().zip(&hangs).zip(&gather) {
        let small = 1.0 - SHRINK.0 * k;
        let small = small * (1.0 - k + k * (SHRINK.1 * h / (cloud.rx * small).max(1e-6)).min(1.0));
        let (dx, dy) = delta[n].unwrap_or((0.0, 0.0));
        let (dx, dy) = (dx + g.0, dy + g.1);
        let cy = cloud.c.1;
        cloud.c.0 += dx;
        cloud.c.1 += dy;
        let cx = cloud.c.0;
        cloud.rx *= small;
        cloud.ry *= flat * small;
        for patch in &mut cloud.patches {
            patch.0.0 = cx + (patch.0.0 + dx - cx) * small;
            patch.0.1 = cy + (patch.0.1 - cy) * flat * small + dy;
        }
    }
    // Twigs that stray from their pad, out past it or hanging off it, are
    // drawn back into it; the branch carrying them out stays. The trunk's own
    // tip, and what grows from it, ends inside the last pad.
    let tip_from = main.len() * 85 / 100;
    let tip: std::collections::BTreeSet<usize> = main[tip_from..].iter().copied().collect();
    for i in 0..sk.nodes.len() {
        let at_tip = tip.contains(&i) || attach[i].is_some_and(|a| tip.contains(&a));
        let Some(a) = attach[i].filter(|&a| a != i || at_tip) else {
            continue;
        };
        let a = if at_tip {
            main[tip_from.saturating_sub(1)]
        } else {
            a
        };
        let p = sk.nodes[i].p;
        let Some(c) = (0..sk.clouds.len())
            .min_by(|&x, &y| dist(sk.clouds[x].c, p).total_cmp(&dist(sk.clouds[y].c, p)))
        else {
            continue;
        };
        let (cc, rx, base) = (sk.clouds[c].c, sk.clouds[c].rx, sk.nodes[a].p);
        let (ax, ay) = (cc.0 - base.0, cc.1 - base.1);
        let along = ((p.0 - base.0) * ax + (p.1 - base.1) * ay) / (ax * ax + ay * ay).max(1e-9);
        let off = dist(p, cc);
        if along > 0.6 && off > rx {
            let keep = 1.0 + k * (rx / off - 1.0);
            sk.nodes[i].p = (cc.0 + (p.0 - cc.0) * keep, cc.1 + (p.1 - cc.1) * keep);
        }
    }
    // Bare twigs that end outside every pad fold back, gone well before
    // the tree is wholly kengai.
    fold_bare(sk, &on_main, (2.5 * k).min(1.0));
    // Judged where they are drawn: wood toward the viewer shows lower.
    let (ts, tc, w) = (
        math::sin(crate::scene::TILT),
        math::cos(crate::scene::TILT),
        crate::scene::WORLD,
    );
    let drawn = |p: P, z: f64| {
        (
            CENTER_X + w * (p.0 - CENTER_X),
            SOIL_Y - w * (SOIL_Y - p.1) * tc + w * z * ts,
        )
    };
    let pads: Vec<(P, f64, f64)> = sk
        .clouds
        .iter()
        .map(|c| (drawn(c.c, c.z), w * c.rx.max(1.0), w * tc * c.ry.max(1.0)))
        .collect();
    // Nor does any twig hang below its pad: what dips under one is lifted
    // to its lower edge, as drawn.
    for i in 1..sk.nodes.len() {
        if on_main.contains(&i) || sk.nodes[i].strand {
            continue;
        }
        let at = drawn(sk.nodes[i].p, zof(sk, i));
        let Some(&(c, rx, ry)) = pads
            .iter()
            .min_by(|a, b| (a.0.0 - at.0).abs().total_cmp(&(b.0.0 - at.0).abs()))
        else {
            continue;
        };
        let floor = c.1 + ry * 1.2;
        if (at.0 - c.0).abs() < rx * 1.3 && at.1 > floor {
            sk.nodes[i].p.1 -= (2.5 * k).min(1.0) * (at.1 - floor) / (w * tc);
        }
    }
    // With a second language's trunk lying along it, the legacy trunk
    // turns to white deadwood, full size, and the living strand runs over it
    // as its lifeline, feeding the pads.
    // It dies over a year and a half from when that language claimed its
    // strand.
    if wood.chains.len() > 1 {
        // White well before the tree is wholly kengai.
        sk.legacy_jin = (1.6 * k).min(1.0) * ease(f64::from(wood.months_b) / 18.0);
    }
    // Spread along its pads, no cloud reaches past its wood.
    on_wood(sk, k);
}

/// A formal upright's outline stays clean: wood and foliage that stray past
/// its triangle are drawn back inside, as far as the tree is formal upright.
fn trim_to_cone(sk: &mut Skeleton, wood: &Wood, dome: &Dome) {
    let k = wood.tier;
    if k <= 0.0 {
        return;
    }
    // Dead wood shows history where it stands: it stays.
    let dead = |n: &crate::growth::Node| n.language.as_deref() == Some(crate::growth::JIN);
    let trunk: std::collections::BTreeSet<usize> = wood
        .chains
        .iter()
        .flatten()
        .copied()
        .chain((0..sk.nodes.len()).filter(|&i| dead(&sk.nodes[i])))
        .collect();
    let cx = dome.center;
    let room = |y: f64, pad: f64| (dome.cone.width(SOIL_Y - y) - pad).max(0.0);
    for (i, n) in sk.nodes.iter_mut().enumerate() {
        if trunk.contains(&i) || n.strand {
            continue;
        }
        let (off, w) = (n.p.0 - cx, room(n.p.1, 0.0));
        if off.abs() > w {
            n.p.0 -= k * (off.abs() - w) * off.signum();
        }
    }
    fold_bare(sk, &trunk, k);
    for cl in &mut sk.clouds {
        let (off, w) = (cl.c.0 - cx, room(cl.c.1, 0.7 * cl.rx));
        if off.abs() > w {
            let dx = -k * (off.abs() - w) * off.signum();
            cl.c.0 += dx;
            for patch in &mut cl.patches {
                patch.0.0 += dx;
            }
        }
    }
}

/// Bare twigs that end outside every cloud fold back `fold` of the way
/// into the wood they grew from, tips first: nothing dangles below or past
/// the foliage. Wood in `keep` and strands stay.
fn fold_bare(sk: &mut Skeleton, keep: &std::collections::BTreeSet<usize>, fold: f64) {
    // A bare tree has no foliage to fold into: its winter twigs stay, or
    // they would fold flat and point straight back.
    if sk.clouds.is_empty() {
        return;
    }
    let on_main = keep;
    // Bare twigs that end outside every pad fold back into the wood they
    // grew from, tips first: nothing dangles below or past the foliage.
    let mut kids = vec![0usize; sk.nodes.len()];
    for n in &sk.nodes {
        if let Some(p) = n.parent {
            kids[p] += 1;
        }
    }
    // Judged where they are drawn: wood toward the viewer shows lower.
    let (ts, tc, w) = (
        math::sin(crate::scene::TILT),
        math::cos(crate::scene::TILT),
        crate::scene::WORLD,
    );
    let drawn = |p: P, z: f64| {
        (
            CENTER_X + w * (p.0 - CENTER_X),
            SOIL_Y - w * (SOIL_Y - p.1) * tc + w * z * ts,
        )
    };
    let pads: Vec<(P, f64, f64)> = sk
        .clouds
        .iter()
        .map(|c| (drawn(c.c, c.z), w * c.rx.max(1.0), w * tc * c.ry.max(1.0)))
        .collect();
    let outside = |p: P| {
        pads.iter()
            .all(|&(c, rx, ry)| math::sq((p.0 - c.0) / rx) + math::sq((p.1 - c.1) / ry * 0.5) > 1.6)
    };
    // Which twigs are bare, from the tips in: a tip outside every pad is,
    // and so is the wood that only carried bare tips.
    let mut bare = vec![false; sk.nodes.len()];
    for i in (1..sk.nodes.len()).rev() {
        let n = &sk.nodes[i];
        if kids[i] > 0 || on_main.contains(&i) || n.strand || !outside(drawn(n.p, zof(sk, i))) {
            continue;
        }
        bare[i] = true;
        if let Some(p) = n.parent {
            kids[p] -= 1;
        }
    }
    // Each folds back to where it leaves the wood that stays.
    for i in 1..sk.nodes.len() {
        if !bare[i] {
            continue;
        }
        let mut at = sk.nodes[i].parent;
        while let Some(a) = at.filter(|&a| bare[a]) {
            at = sk.nodes[a].parent;
        }
        if let Some(a) = at {
            let (to, q) = (sk.nodes[a].p, sk.nodes[i].p);
            sk.nodes[i].p = (q.0 + fold * (to.0 - q.0), q.1 + fold * (to.1 - q.1));
        }
    }
}

/// In the wind every branch leans downwind: one reaching upwind keeps its
/// side but swings up and over around where it leaves the trunk, as far as
/// the tree is windswept, and its leaves go with it. At its fullest it
/// stands up and leans a little downwind; nothing crosses to crowd the
/// other side.
/// In the wind a few clouds left high above the rest of the crown, on long
/// bare arms, read as a lollipop: the top of the tree eases down until they
/// sit just above the crown, their arms shorter, every height in order.
fn no_lone_top(sk: &mut Skeleton, wood: &Wood) {
    if wood.sweep == 0.0 || sk.clouds.len() < 4 {
        return;
    }
    let mut tops: Vec<f64> = sk.clouds.iter().map(|c| c.c.1 - c.ry).collect();
    tops.sort_by(f64::total_cmp);
    // The top: a few clouds, at most a quarter of them, above a wide gap.
    let few = (sk.clouds.len() / 4).clamp(1, 6);
    let Some(lone) = (1..=few).find(|&i| tops[i] - tops[i - 1] >= LONE_GAP) else {
        return;
    };
    let (top, next) = (tops[0], tops[lone]);
    // Above the crown, heights draw in toward it: the top lands just above.
    let ease = |y: f64| {
        if y >= next {
            y
        } else {
            next - (next - y) * (LONE_KEEP + tops[lone - 1] - top) / (next - top)
        }
    };
    for n in &mut sk.nodes {
        n.p.1 = ease(n.p.1);
    }
    for cl in &mut sk.clouds {
        let dy = ease(cl.c.1) - cl.c.1;
        cl.c.1 += dy;
        for p in &mut cl.patches {
            p.0.1 += dy;
        }
    }
}

/// A cloud standing alone on a windswept tree, further than [`TUFT_GAP`] from
/// every other cloud, out on a long bare arm, comes in to the nearest: its
/// arm bends with it, from nothing where it leaves the wood that carries
/// other leaves to the whole move at the cloud. The move eases in over
/// [`LONE_RAMP`] past the gap, so a cloud drifting apart never jumps.
fn no_lone_tuft(sk: &mut Skeleton, wood: &Wood) {
    if wood.sweep == 0.0 || sk.clouds.len() < 4 {
        return;
    }
    let n = sk.nodes.len();
    let trunk: std::collections::BTreeSet<usize> = wood.chains.iter().flatten().copied().collect();
    for ci in 0..sk.clouds.len() {
        let at: Vec<P> = sk.nodes.iter().map(|x| x.p).collect();
        let hangs = hangs_of(sk, &at, true);
        let cl = &sk.clouds[ci];
        // The nearest other cloud, box to box, and the way to it.
        let Some((gap, to)) = sk
            .clouds
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != ci)
            .map(|(_, o)| {
                let (dx, dy) = (o.c.0 - cl.c.0, o.c.1 - cl.c.1);
                let (gx, gy) = (dx.abs() - o.rx - cl.rx, dy.abs() - o.ry - cl.ry);
                if gx > gy {
                    (gx, (dx.signum(), 0.0))
                } else {
                    (gy, (0.0, dy.signum()))
                }
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
        else {
            continue;
        };
        let k = ((gap - TUFT_GAP) / LONE_RAMP).clamp(0.0, 1.0);
        if k <= 0.0 {
            continue;
        }
        let shift = k * (gap - LONE_KEEP);
        let delta = (to.0 * shift, to.1 * shift);
        // The arm: up from the cloud's node to where the wood carries others.
        let mut carry = vec![0usize; n];
        for &h in &hangs {
            carry[h] += 1;
        }
        for i in (1..n).rev() {
            if let Some(p) = sk.nodes[i].parent {
                carry[p] += carry[i];
            }
        }
        let h = hangs[ci];
        // A cloud with no wood of its own in it (a dropped language's, its
        // twigs gone) is no tuft: it bends nobody's arm.
        if dist(at[h], sk.clouds[ci].c) > sk.clouds[ci].rx {
            continue;
        }
        let mut arm = h;
        while let Some(p) = sk.nodes[arm].parent {
            if carry[p] > carry[h] || trunk.contains(&p) {
                break;
            }
            arm = p;
        }
        let Some(fork) = sk.nodes[arm].parent.filter(|_| !trunk.contains(&arm)) else {
            continue;
        };
        let reach = dist(at[fork], at[h]).max(1e-6);
        let mut on = vec![false; n];
        on[arm] = true;
        for i in arm + 1..n {
            on[i] = sk.nodes[i].parent.is_some_and(|p| on[p]);
        }
        for i in (arm..n).filter(|&i| on[i]) {
            let t = (dist(at[fork], at[i]) / reach).min(1.0);
            sk.nodes[i].p.0 += delta.0 * t;
            sk.nodes[i].p.1 += delta.1 * t;
        }
        let cl = &mut sk.clouds[ci];
        cl.c.0 += delta.0;
        cl.c.1 += delta.1;
        for p in &mut cl.patches {
            p.0.0 += delta.0;
            p.0.1 += delta.1;
        }
    }
}

/// How far a cloud may stand from every other before [`no_lone_tuft`] draws
/// it in, and over how much more gap that move eases in.
const TUFT_GAP: f64 = 15.0;
const LONE_RAMP: f64 = 10.0;

/// How wide a gap under the top clouds [`no_lone_top`] draws in, and how far
/// above the crown their lowest sits after.
const LONE_GAP: f64 = 25.0;
const LONE_KEEP: f64 = 4.0;

/// How far above the rim a windswept crown's lowest clouds stay, and over how
/// much height above that the droop eases onto the floor.
const RIM_GAP: f64 = 6.0;
const RIM_EASE: f64 = 24.0;

/// In the wind the crown may hang low downwind, but only a cascade goes past
/// the rim: what the wind swept lower eases onto a floor just above it,
/// branches and their clouds alike, so a long arm runs out level instead.
/// The trunks stay where they stand.
fn above_rim(sk: &mut Skeleton, wood: &Wood) {
    if wood.sweep == 0.0 {
        return;
    }
    let floor = SOIL_Y - RIM_GAP;
    let knee = floor - RIM_EASE;
    let ease = |y: f64| {
        if y <= knee {
            return y;
        }
        let z = (y - knee) / RIM_EASE;
        knee + RIM_EASE * (1.0 - 2.0 / (math::exp(2.0 * z) + 1.0))
    };
    let trunk: std::collections::BTreeSet<usize> = wood.chains.iter().flatten().copied().collect();
    for (i, n) in sk.nodes.iter_mut().enumerate() {
        if !trunk.contains(&i) && !n.strand {
            n.p.1 = ease(n.p.1);
        }
    }
    for cl in &mut sk.clouds {
        let dy = ease(cl.c.1) - cl.c.1;
        cl.c.1 += dy;
        for p in &mut cl.patches {
            p.0.1 += dy;
        }
    }
}

fn wind_branches(sk: &mut Skeleton, wood: &Wood, low: &[(usize, f64)]) {
    let w = wood.sweep.abs();
    let s = w * w * (3.0 - 2.0 * w);
    if s <= 0.0 {
        return;
    }
    let d = wood.sweep.signum();
    // A strand is trunk, not a branch: it never swings about the foot.
    let trunk: std::collections::BTreeSet<usize> = wood
        .chains
        .iter()
        .flatten()
        .copied()
        .chain((0..sk.nodes.len()).filter(|&i| sk.nodes[i].strand))
        .collect();
    let before: Vec<P> = sk.nodes.iter().map(|n| n.p).collect();
    // Each branch: the trunk node it leaves and its first node off it.
    let mut attach: Vec<Option<(usize, usize)>> = vec![None; sk.nodes.len()];
    // A low limb of two trunks already runs level into open air
    // ([`wind_limbs`]): it swings only as far as it has not.
    let kl = if wood.chains.len() > 1 {
        let wl = wood.sweep.abs() * wood.peel_b;
        wl * wl * (3.0 - 2.0 * wl)
    } else {
        0.0
    };
    let sl = s * (1.0 - kl);
    let low_of = limb_of(sk, low);
    // Each branch's reach: its node furthest from where it leaves the trunk.
    let mut reach: Vec<P> = vec![(0.0, 0.0); sk.nodes.len()];
    for i in 0..sk.nodes.len() {
        if trunk.contains(&i) {
            continue;
        }
        attach[i] = match sk.nodes[i].parent {
            Some(p) if trunk.contains(&p) => Some((p, i)),
            Some(p) => attach[p],
            None => None,
        };
        if let Some((a, root)) = attach[i] {
            let o = (before[i].0 - before[a].0, before[i].1 - before[a].1);
            if math::sq(o.0) + math::sq(o.1) > math::sq(reach[root].0) + math::sq(reach[root].1) {
                reach[root] = o;
            }
        }
    }
    // Each cloud moves with the branch that has the most living wood inside
    // it, its own twigs, at that branch's node nearest it: a branch merely
    // passing close, or a dead arm, never carries it off. Without such wood
    // it moves with the nearest node, as the wood around it does.
    let hangs: Vec<usize> = sk
        .clouds
        .iter()
        .map(|cl| {
            let near =
                |a: &usize, b: &usize| dist(before[*a], cl.c).total_cmp(&dist(before[*b], cl.c));
            let mut votes: BTreeMap<usize, usize> = BTreeMap::new();
            for i in 1..sk.nodes.len() {
                let inside = math::sq((before[i].0 - cl.c.0) / cl.rx.max(1.0))
                    + math::sq((before[i].1 - cl.c.1) / cl.ry.max(1.0))
                    <= 1.0;
                if inside
                    && sk.nodes[i].language.as_deref() != Some(JIN)
                    && let Some((_, root)) = attach[i]
                {
                    *votes.entry(root).or_insert(0) += 1;
                }
            }
            let own = votes
                .into_iter()
                .max_by_key(|&(root, n)| (n, std::cmp::Reverse(root)))
                .map(|v| v.0);
            own.and_then(|root| {
                (1..sk.nodes.len())
                    .filter(|&i| attach[i].is_some_and(|a| a.1 == root))
                    .min_by(near)
            })
            .or_else(|| (1..sk.nodes.len()).min_by(near))
            .unwrap_or(0)
        })
        .collect();
    let mut moved: Vec<P> = vec![(0.0, 0.0); sk.nodes.len()];
    for i in 0..sk.nodes.len() {
        let Some((a, root)) = attach[i] else {
            continue;
        };
        // Private work stays low as well: swung up, its silver would top
        // the crown, which belongs to a public language.
        let tame = low_of[i].is_some() || sk.nodes[root].language.is_none();
        // Upwind branches only, judged where they leave the trunk.
        let side = before[root].0 - before[a].0;
        if side * d >= 0.0 {
            continue;
        }
        // An upwind branch swings over the top, and at least far enough to
        // stream out downwind: one that ran out level would otherwise stand
        // straight up, and a few of those make a pillar of the crown.
        let rise = math::atan2(-reach[root].1, -reach[root].0 * d);
        let turn = (std::f64::consts::PI - rise - STREAM_RISE).max(1.75);
        let turn = d * turn * if tame { sl } else { s };
        let (sin, cos) = (math::sin(turn), math::cos(turn));
        let (ox, oy) = (before[i].0 - before[a].0, before[i].1 - before[a].1);
        let to = (
            before[a].0 + ox * cos - oy * sin,
            before[a].1 + ox * sin + oy * cos,
        );
        moved[i] = (to.0 - before[i].0, to.1 - before[i].1);
        sk.nodes[i].p = to;
    }
    for (cloud, &n) in sk.clouds.iter_mut().zip(&hangs) {
        let (dx, dy) = moved[n];
        cloud.c.0 += dx;
        cloud.c.1 += dy;
        for patch in &mut cloud.patches {
            patch.0.0 += dx;
            patch.0.1 += dy;
        }
    }
}

/// Leaves grow on wood: a cascade's cloud, spread along its pad, that
/// reaches further across than the wood in it, past its twigs by more than
/// [`LEAF_REACH`] or 0.4 of its width, is drawn in to them by `k`, so no
/// leaves hang in the air beside a pad.
fn on_wood(sk: &mut Skeleton, k: f64) {
    let nodes = &sk.nodes;
    for cl in &mut sk.clouds {
        // Wood outside the cloud counts for less the further out it lies, so
        // the cloud eases in as wood moves instead of jumping.
        let (lo, hi) = nodes
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), n| {
                let out = ((n.p.0 - cl.c.0).abs() - cl.rx).max(0.0)
                    + ((n.p.1 - cl.c.1).abs() - cl.ry - LEAF_REACH).max(0.0);
                (lo.min(n.p.0 + 3.0 * out), hi.max(n.p.0 - 3.0 * out))
            });
        let m = LEAF_REACH.max(0.4 * cl.rx);
        let (a, b) = ((cl.c.0 - cl.rx).max(lo - m), (cl.c.0 + cl.rx).min(hi + m));
        if lo > hi || b - a >= 2.0 * cl.rx - 1e-9 {
            continue;
        }
        let c = cl.c.0 + k * (f64::midpoint(a, b) - cl.c.0);
        let f = 1.0 - k * (1.0 - (b - a) / (2.0 * cl.rx));
        for patch in &mut cl.patches {
            patch.0.0 = c + (patch.0.0 - cl.c.0) * f;
        }
        cl.c.0 = c;
        cl.rx *= f;
    }
}

/// How far past its twigs a cloud's leaves may reach, at the least.
const LEAF_REACH: f64 = 8.0;

/// Each cloud's node, the wood it moves with: the node nearest it at the
/// positions `at`, a living one when `live`. Dead wood bears no leaves, so
/// where branches swing, a dead arm passing close never carries a cloud off
/// its own twigs.
fn hangs_of(sk: &Skeleton, at: &[P], live: bool) -> Vec<usize> {
    let living = |i: &usize| !live || sk.nodes[*i].language.as_deref() != Some(JIN);
    sk.clouds
        .iter()
        .map(|cl| {
            let near = |a: &usize, b: &usize| dist(at[*a], cl.c).total_cmp(&dist(at[*b], cl.c));
            (1..at.len())
                .filter(living)
                .min_by(near)
                .or_else(|| (1..at.len()).min_by(near))
                .unwrap_or(0)
        })
        .collect()
}

/// The steepest a swung branch points downwind, in radians above level.
const STREAM_RISE: f64 = 0.6;

/// Each node's low limb, by the limb's first node: `None` off the low limbs.
fn limb_of(sk: &Skeleton, limbs: &[(usize, f64)]) -> Vec<Option<usize>> {
    let roots: std::collections::BTreeSet<usize> = limbs.iter().map(|l| l.0).collect();
    let mut of: Vec<Option<usize>> = vec![None; sk.nodes.len()];
    for i in 0..sk.nodes.len() {
        of[i] = if roots.contains(&i) {
            Some(i)
        } else {
            sk.nodes[i].parent.and_then(|p| of[p])
        };
    }
    of
}

/// In the wind each low limb of two trunks runs out level into the open air
/// under the crowns. It climbs behind the trunk on its side, along its curve,
/// to the height where the air beside that trunk is most open, then comes
/// out and runs straight, bending up a little at its end, its twigs and
/// leaves with it. It eases in with the wind.
fn wind_limbs(sk: &mut Skeleton, wood: &Wood, limbs: &[(usize, f64)]) {
    let w = wood.sweep.abs() * wood.peel_b;
    let k = w * w * (3.0 - 2.0 * w);
    if wood.chains.len() < 2 || k <= 0.0 || limbs.is_empty() {
        return;
    }
    let n = sk.nodes.len();
    let d = wood.sweep.signum();
    let hangs = hangs_of(sk, &sk.nodes.iter().map(|n| n.p).collect::<Vec<P>>(), false);
    let limb_of = limb_of(sk, limbs);
    // The trunk on a side: the one whose top lies further that way.
    let (ca, cb) = (&wood.chains[0], &wood.chains[1]);
    let top_x = |ch: &Vec<usize>| sk.nodes[*ch.last().unwrap_or(&0)].p.0;
    let (xa, xb) = (top_x(ca), top_x(cb));
    for &(r, side) in limbs {
        if r >= n || limb_of[r] != Some(r) {
            continue;
        }
        let Some(at) = sk.nodes[r].parent else {
            continue;
        };
        let a0 = sk.nodes[at].p;
        let mine = |i: usize| limb_of.get(i).copied().flatten() == Some(r);
        // The limb's own line: on from its first node, the next node made.
        let mut path = vec![r];
        while let Some(next) = (path[path.len() - 1] + 1..n)
            .find(|&j| sk.nodes[j].parent == Some(path[path.len() - 1]))
        {
            path.push(next);
        }
        let tip = sk.nodes[path[path.len() - 1]].p;
        let reach = dist(tip, a0).max(1.0);
        let pad_r = sk
            .clouds
            .iter()
            .zip(&hangs)
            .filter(|(_, h)| mine(**h))
            .map(|(cl, _)| cl.rx)
            .fold(6.0, f64::max);
        // How open a point is: its distance to the nearest other wood or leaf.
        let open = |p: P| {
            let wood_d = (1..n)
                .filter(|&i| !mine(i))
                .map(|i| dist(p, sk.nodes[i].p) - 5.0)
                .fold(f64::MAX, f64::min);
            let leaf_d = sk
                .clouds
                .iter()
                .zip(&hangs)
                .filter(|(_, h)| !mine(**h))
                .map(|(cl, _)| dist(p, (cl.c.0, cl.c.1 - 0.4 * cl.ry)) - 0.8 * cl.rx)
                .fold(f64::MAX, f64::min);
            wood_d.min(leaf_d)
        };
        // How far under the canopy, 0 to 1: foliage above it, clear of the
        // soil. Graded, so the pick moves smoothly as the tree grows.
        let covered = |p: P| {
            let soil = ((SOIL_Y - 14.0 - p.1) / 10.0).clamp(0.0, 1.0);
            let over = sk
                .clouds
                .iter()
                .zip(&hangs)
                .filter(|(_, h)| !mine(**h))
                .map(|(cl, _)| {
                    let across = 1.0 - (cl.c.0 - p.0).abs() / cl.rx.max(1.0);
                    // Air between its leaves and the crown above.
                    let above = (p.1 - 0.5 * pad_r - 8.0 - cl.c.1) / pad_r;
                    across.min(above).clamp(0.0, 1.0)
                })
                .fold(0.0, f64::max);
            soil * over
        };
        let ch = if (xa - xb) * side >= 0.0 { ca } else { cb };
        let pt = |j: usize| sk.nodes[ch[j]].p;
        // Where it reaches that trunk: nearest where it grew. From there it
        // runs up or down along it.
        let j0 = (0..ch.len())
            .min_by(|&x, &y| dist(pt(x), a0).total_cmp(&dist(pt(y), a0)))
            .unwrap_or(0);
        if ch.len() < 3 {
            continue;
        }
        // The run out, downwind: straight and level, bending up a little at
        // its end.
        let out = |e: P, run: f64, t: f64| (e.0 + d * run * t, e.1 - 0.12 * run * t * t);
        // The height along the trunk and the length where its run has the
        // most air around it and its leaves, picked softly so it never leaps
        // as the tree grows.
        let mut cands: Vec<(f64, f64, f64)> = Vec::new();
        for j in 1..ch.len() - 1 {
            for f in [0.5, 0.65, 0.8] {
                let (e, run) = (pt(j), f * reach);
                let end = out(e, run, 1.0);
                let air = open(out(e, run, 0.55))
                    .min(open(end))
                    .min(open((end.0, end.1 - pad_r)));
                cands.push((air.min(3.0 * pad_r) * covered(end), j as f64, run));
            }
        }
        let best = cands.iter().map(|c| c.0).fold(0.0, f64::max);
        // With little air anywhere it keeps more of its own way.
        let k = k * (best / 3.0).clamp(0.0, 1.0);
        if k <= 0.0 {
            continue;
        }
        let (mut sw, mut s, mut run) = (0.0, 0.0, 0.0);
        for (score, j, l) in &cands {
            let w = math::exp((score - best) / 2.0);
            sw += w;
            s += w * j;
            run += w * l;
        }
        let (s, run) = (s / sw, run / sw);
        let js = (s.floor() as usize).min(ch.len() - 2);
        let f = s - js as f64;
        let e = (
            pt(js).0 + (pt(js + 1).0 - pt(js).0) * f,
            pt(js).1 + (pt(js + 1).1 - pt(js).1) * f,
        );
        // Its new line: up behind the trunk, then out.
        let mut line = vec![a0];
        if js >= j0 {
            line.extend((j0..=js).map(pt));
        } else {
            line.extend((js + 1..=j0).rev().map(pt));
        }
        line.push(e);
        let hidden = line.len();
        line.extend((1..=12).map(|i| out(e, run, f64::from(i) / 12.0)));
        let mut at_len = vec![0.0];
        for s in line.windows(2) {
            at_len.push(at_len[at_len.len() - 1] + dist(s[0], s[1]));
        }
        let full = at_len[at_len.len() - 1].max(1e-9);
        let shown = at_len[hidden - 1] / full;
        let along = |u: f64| {
            let x = u.clamp(0.0, 1.0) * full;
            let i = at_len.partition_point(|&l| l < x).clamp(1, line.len() - 1);
            let g = ((x - at_len[i - 1]) / (at_len[i] - at_len[i - 1]).max(1e-9)).clamp(0.0, 1.0);
            (
                line[i - 1].0 + (line[i].0 - line[i - 1].0) * g,
                line[i - 1].1 + (line[i].1 - line[i - 1].1) * g,
            )
        };
        // The old line's length along it, for where each node goes.
        let mut lens = vec![dist(a0, sk.nodes[r].p)];
        for s in path.windows(2) {
            lens.push(lens[lens.len() - 1] + dist(sk.nodes[s[0]].p, sk.nodes[s[1]].p));
        }
        let total = lens[lens.len() - 1].max(1e-9);
        // Twigs keep their place on the limb, rising from it, a
        // little flatter.
        let old: Vec<P> = sk.nodes.iter().map(|and| and.p).collect();
        let mut new: Vec<Option<P>> = vec![None; n];
        for (j, &i) in path.iter().enumerate() {
            let u = lens[j] / total;
            new[i] = Some(along(u));
            // Up the trunk it runs behind it, and comes out past it.
            let back = ((shown + 0.1 - u) / 0.1).clamp(0.0, 1.0);
            if let Some(z) = sk.node_z.get_mut(i) {
                *z -= 20.0 * k * back;
            }
        }
        for i in r..n {
            if !mine(i) || new[i].is_some() {
                continue;
            }
            let Some(p) = sk.nodes[i].parent else {
                continue;
            };
            let (ox, oy) = (old[i].0 - old[p].0, old[i].1 - old[p].1);
            new[i] = new[p].map(|q| (q.0 + ox, q.1 + 0.6 * oy));
        }
        let delta = |i: usize| new[i].map(|q| (k * (q.0 - old[i].0), k * (q.1 - old[i].1)));
        for i in r..n {
            if let Some((dx, dy)) = delta(i).filter(|_| mine(i)) {
                sk.nodes[i].p.0 += dx;
                sk.nodes[i].p.1 += dy;
            }
        }
        for (cloud, &h) in sk.clouds.iter_mut().zip(&hangs) {
            if let Some((dx, dy)) = delta(h).filter(|_| mine(h)) {
                cloud.c.0 += dx;
                cloud.c.1 += dy;
                for patch in &mut cloud.patches {
                    patch.0.0 += dx;
                    patch.0.1 += dy;
                }
            }
        }
    }
}

/// How far a windswept tree's windward trunk runs its top out downwind, over
/// the opening of its C, as a share of the height.
const ARM: f64 = 0.1;

/// Bends two trunks into their windswept shape: the downwind trunk over and
/// down, the windward one up. Everything grown from a trunk follows the
/// point it grew from, and each crown follows its trunk's top.
fn wind_shape(sk: &mut Skeleton, wood: &Wood, eras: &std::ops::Range<usize>) {
    // It eases in: hardly at first, then fully as the tree turns windswept.
    let w = wood.sweep.abs() * wood.peel_b;
    let k = w * w * (3.0 - 2.0 * w);
    if wood.chains.len() < 2 || k <= 0.0 {
        return;
    }
    let d = wood.sweep.signum();
    let foot = sk.nodes[wood.chains[0][0]].p;
    let top = |ch: &Vec<usize>| sk.nodes[*ch.last().unwrap_or(&0)].p;
    let (a, b) = (&wood.chains[0], &wood.chains[1]);
    let a_down = (top(a).0 - foot.0) * d >= (top(b).0 - foot.0) * d;
    // Each trunk bends from where it runs free: the main trunk from the foot,
    // the second where it leaves the main one.
    let ((down, down_from), (up, up_from)) = if a_down {
        ((a, 0), (b, wood.free_b))
    } else {
        ((b, wood.free_b), (a, 0))
    };
    let h = (foot.1 - top(down).1).max(1.0);
    // How far a point `v` of the way up a trunk's free part moves, as shares
    // of the height, x downwind and y down. The downwind trunk bows over in
    // one smooth arc; the windward one curls upwind like a C, its top back
    // over its foot and higher.
    let down_f = |v: f64| (0.2 * v * v, 0.24 * v * v);
    let up_f = |v: f64| {
        (
            -0.24 * math::sin(PI * v) * (1.0 - 0.3 * v) + 0.06 * v * v,
            -0.2 * v * v,
        )
    };
    let mut delta: Vec<Option<P>> = vec![None; sk.nodes.len()];
    for (ch, from, f) in [
        (down, down_from, &down_f as &dyn Fn(f64) -> P),
        (up, up_from, &up_f),
    ] {
        let n = ch.len().max(from + 2) - 1;
        for (i, &node) in ch.iter().enumerate() {
            let v = i.saturating_sub(from) as f64 / (n - from) as f64;
            let (x, y) = f(v.clamp(0.0, 1.0));
            delta[node].get_or_insert((k * h * d * x, k * h * y));
        }
    }
    // The C closes over its opening: the windward trunk's top turns and runs
    // out level downwind, an arm for its crown to sit on.
    let n = up.len().max(up_from + 2) - 1;
    let v_of = |i: usize| i.saturating_sub(up_from) as f64 / (n - up_from) as f64;
    if let Some(ib) = (0..up.len()).find(|&i| v_of(i) >= 0.55) {
        let y_bend = sk.nodes[up[ib]].p.1;
        for (i, &node) in up.iter().enumerate().skip(ib) {
            let u = ((v_of(i) - 0.55) / 0.45).clamp(0.0, 1.0);
            if let Some((dx, dy)) = delta[node].as_mut() {
                *dx += k * h * d * ARM * u * u;
                *dy += k * (y_bend - sk.nodes[node].p.1) * 0.75 * u;
            }
        }
    }
    // Each cloud follows the branch it hangs from: the nearest wood.
    let hangs = hangs_of(sk, &sk.nodes.iter().map(|n| n.p).collect::<Vec<P>>(), false);
    // Parents come before children: each node follows the one it grew from.
    for i in 0..sk.nodes.len() {
        if delta[i].is_none() {
            // A low limb moves with the trunk where it leaves it and keeps
            // its own place further out; a crown arm goes with its trunk.
            let keep = if eras.contains(&i) { 0.8 } else { 1.0 };
            delta[i] = sk.nodes[i]
                .parent
                .and_then(|p| delta[p])
                .map(|(dx, dy)| (dx * keep, dy * keep));
        }
        if let Some((dx, dy)) = delta[i] {
            sk.nodes[i].p.0 += dx;
            sk.nodes[i].p.1 += dy;
        }
    }
    for (cloud, &n) in sk.clouds.iter_mut().zip(&hangs) {
        let (dx, dy) = delta[n].unwrap_or((0.0, 0.0));
        cloud.c.0 += dx;
        cloud.c.1 += dy;
        for patch in &mut cloud.patches {
            patch.0.0 += dx;
            patch.0.1 += dy;
        }
    }
}

/// Trunks: one nebari, a V of two; dropped trunk languages as snags,
/// dropped languages as stubs; the apex and height.
fn grow_trunks(sk: &mut Skeleton, c: &Ctx, leader: &Leader, rng: &mut Rng) -> Wood {
    let (life, s_a) = (c.life, c.s_a);
    let Frame { big_s, vv, .. } = c.frame;
    let h_vis = c.frame.h_vis;
    let (founder, mut claimed) = crate::growth::trunks::strands(life);
    if let Some(r) = &c.recent {
        for st in &mut claimed {
            let was = r.strands.get(&st.lang).copied().unwrap_or_default();
            st.peel = 0.5 * st.peel + 0.5 * was[0];
            st.sag = 0.5 * st.sag + 0.5 * was[1];
            st.share = 0.5 * st.share + 0.5 * was[2];
        }
    }
    // A cascade has one trunk: its languages' strands stay along it, and
    // show as branches instead.
    let kc = leader.cascade.abs();
    // A formal upright keeps one trunk too: its strands hug the leader. A
    // young tree grows into its shape from the seedling's over its second
    // year.
    let tier = ease(leader.tier) * ease(life.maturity());
    for st in &mut claimed {
        st.peel *= (1.0 - kc * kc * (3.0 - 2.0 * kc)) * (1.0 - tier);
    }
    // In the wind, of two trunks the downwind one leans and the windward
    // one stands: a strand downwind leaves the trunk upright, as far as it
    // has come away.
    let (ws, wd) = (leader.sweep.abs(), leader.sweep.signum());
    let strand_downwind = claimed
        .first()
        .filter(|_| -s_a * wd > 0.0)
        .map_or(0.0, |st| ws * st.peel.clamp(0.0, 1.0));
    let (amp_k, lean) = (leader.bend, leader.tilt * (1.0 - 0.7 * strand_downwind));
    // The trunk bends as far as the work's irregularity ever took it.
    let c_irr = c.reach.irr;
    let amp = (0.012 + 0.025 * c_irr) * (0.6 + 0.06 * c.age) * amp_k;
    let freq = 0.45 + 0.4 * c_irr;
    let ph = rng.range(0.0, math::TAU);
    let foot = limb_to(
        sk,
        0,
        (CENTER_X, SOIL_Y - 6.0),
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        &None,
    );
    let base = *foot.last().unwrap_or(&0);
    let mut foot_a = vec![0];
    foot_a.extend(&foot);
    // A formal upright's trunk runs up through the crown toward its apex.
    let h_a = h_vis
        + vv * big_s * 0.32
        + tier * (0.78 * Cone::of(big_s, h_vis, vv).h_top - h_vis - vv * big_s * 0.32).max(0.0);
    let mut sides = vec![s_a];
    // Three spare strands lie along the trunk; a language past 10% of the
    // work claims one, which comes away as the language works.
    let d = big_s * (0.42 + 0.022 * c.age.min(16.0));
    let mut a = foot_a.clone();
    a.extend(
        trunk3(
            sk,
            base,
            h_a - 6.0,
            s_a * 0.08 + lean,
            amp * 1.4,
            freq,
            ph,
            0.0,
            &None,
            // The leaning trunk of a windswept tree bends over low.
            ws * (1.0 - strand_downwind),
        )
        .into_iter()
        .skip(1),
    );
    let main_top = sk.nodes[*a.last().unwrap_or(&0)].p;
    // The trunk outlives the language that founded it: dropped, it leaves a
    // shari, dead wood up the living trunk.
    let dropped_founder = founder.filter(|l| life.dropped(l));
    sk.shari = dropped_founder.as_deref().map(|l| life.drop_progress(l));
    sk.deadwood.extend(dropped_founder);
    let jin = Some(JIN.to_string());
    let mut first: Option<(Vec<usize>, f64, usize)> = None;
    let mut dead_strand: Option<f64> = None;
    for slot in 0u8..3 {
        let side = if slot % 2 == 0 { -s_a } else { s_a };
        let n0 = sk.nodes.len();
        let (lang, peel, sag, share) = match claimed.get(usize::from(slot)) {
            Some(st) => (Some(st.lang.clone()), st.peel, st.sag, st.share),
            None => (None, 0.0, 0.0, 0.0),
        };
        // Only the first claimed strand becomes a trunk and carries a crown;
        // a later one holds no wood of its own, so it is light: it lies along
        // the trunk, parts just under the crown and never sags out.
        let (sag, share) = if slot == 0 { (sag, share) } else { (0.0, 0.0) };
        // In the wind a strand on the downwind side bends further out and
        // lower than the trunk; one on the windward side keeps its swing out
        // and stands higher, its top turned back downwind ([`wind_shape`]).
        let downwind = if side * wd > 0.0 { ws } else { 0.0 };
        let windward = if side * wd < 0.0 { ws } else { 0.0 };
        let top = (
            main_top.0 + side * d * (0.3 + 0.5 * share) * peel * (1.0 + 1.5 * downwind),
            main_top.1 + (h_a * 0.1 + sag * 18.0) * peel + 0.3 * h_a * downwind * peel
                - 0.12 * h_a * windward * peel,
        );
        let wind = crate::growth::wood::Wind {
            turns: 0.3,
            radius: 0.085 * h_a,
            // A thin strand is light: it parts near the crown and hardly
            // sags; a heavy one parts low and leans out.
            part_at: (0.8 - 1.5 * share).clamp(0.2, 0.72) * (1.0 - 0.55 * downwind),
            sag: sag * (share / 0.3).min(1.0),
            offset: f64::from(slot) / 3.0,
        };
        // A dropped language's strand dies with it: dead wood that
        // bleaches, its free part breaking back.
        let died = lang
            .as_deref()
            .filter(|l| life.dropped(l))
            .map(|l| life.drop_progress(l));
        let wood_lang = if died.is_some() { &jin } else { &lang };
        let (mut nodes, free) =
            crate::growth::wood::strand(sk, &a, foot_a.len() - 1, wind, peel, side, top, wood_lang);
        if let Some(g) = died {
            sk.deadwood.extend(lang.clone());
            let n_free = nodes.len().saturating_sub(free);
            let keep = free + ((n_free as f64 * (1.0 - 0.6 * g)).ceil() as usize).max(1);
            if keep < nodes.len() {
                let cut = nodes[keep];
                sk.nodes.truncate(cut);
                sk.node_z.truncate(cut);
                nodes.truncate(keep);
                if let Some(w) = sk.wound.last_mut() {
                    w.1.retain(|&n| n < cut);
                }
            }
            bleach_from(sk, n0, g);
        }
        for n in &mut sk.nodes[n0..] {
            n.strand = true;
        }
        if lang.is_some() && first.is_none() {
            let mut b = foot_a.clone();
            b.extend(nodes.into_iter().skip(1));
            let free_b = foot_a.len() + free - 1;
            first = Some((b, side, free_b));
            dead_strand = died;
        }
    }
    let mut chains = vec![a];
    let mut free_b = 0;
    if let Some((b, side, f)) = first {
        chains.push(b);
        sides.push(side);
        free_b = f;
    }
    // Every language with dead wood (1% or more of the lifetime) leaves a jin
    // stub on the trunk, also once it came back.
    let dropped: Vec<(String, f64)> = life
        .lang
        .iter()
        .map(|(l, _)| (l.clone(), life.dead_total(l)))
        .filter(|(_, (full, _))| full / life.total >= 0.01)
        .take(3)
        .map(|(l, (full, _))| (l, full))
        .collect();
    for (j, (l, v)) in dropped.iter().enumerate() {
        let side = if j % 2 == 0 { -s_a } else { s_a };
        // Dead wood hangs only from the trunk or a dead strand.
        let k = usize::from(sides.len() > 1 && side != s_a && dead_strand.is_some());
        let from = at_height(
            sk,
            &chains[k],
            14.0 + 11.0 * j as f64 + 6.0 * (j % 2) as f64,
        );
        let el = if j % 3 == 1 { 0.6 } else { -0.4 };
        let w = Limb::new(from, math::atan2(el, side), 0.5 + 0.1 * j as f64);
        let (full, died) = life.dead_total(l);
        let grown = died / full.max(1e-9);
        // A stub dies at its full length and bleaches.
        let rng = &mut Rng::keyed(c.seed, &format!("bdome/stub/{l}"));
        let n0 = sk.nodes.len();
        jin_limb(sk, rng, w, 9.0 + 14.0 * (v / life.total).sqrt(), v * 0.06);
        bleach_from(sk, n0, grown);
        sk.deadwood.insert((*l).clone());
    }
    // The crown sits over the trunks.
    let tops: Vec<P> = chains
        .iter()
        .map(|ch| sk.nodes[*ch.last().unwrap_or(&0)].p)
        .collect();
    let center = tops.iter().map(|p| p.0).sum::<f64>() / tops.len() as f64;
    sk.apex = (center, c.y_b - vv * big_s);
    sk.height = h_vis + vv * big_s;
    let (peel_b, months_b) = if chains.len() > 1 {
        claimed
            .first()
            .map_or((0.0, 0), |st| (st.peel.clamp(0.0, 1.0), st.months))
    } else {
        (0.0, 0)
    };
    Wood {
        chains,
        sides,
        foot: foot_a.len(),
        s_a,
        center,
        dead_b: dead_strand.is_some(),
        dead_g: dead_strand.unwrap_or(0.0),
        sweep: leader.sweep,
        peel_b,
        months_b,
        free_b,
        cascade: leader.cascade,
        downwind: leader.downwind,
        tier,
    }
}

/// Old eras: low limbs, oldest lowest, sides alternating. Returns each
/// limb's first node and its side.
fn grow_eras(sk: &mut Skeleton, c: &Ctx, wood: &Wood) -> Vec<(usize, f64)> {
    let mut limbs = Vec::new();
    let (life, seed) = (c.life, c.seed);
    let Frame { big_s, cell, .. } = c.frame;
    // The eras alternate sides, counted from the oldest: a new era moves no
    // older limb, and no crown change swings one across.
    // The oldest, lowest limb grows on the side away from any lean.
    let first = -wood.downwind;
    let era_side = |ei: usize| -> f64 { if ei.is_multiple_of(2) { first } else { -first } };
    // A year leaving the crown grows into its era limb; a young tree grows
    // its first limbs, leaves and all, over its second year, from nothing the
    // day the seedling becomes a tree.
    let yw = |i: usize| {
        c.plan
            .leaving
            .iter()
            .find(|l| l.0 == i)
            .map_or(1.0, |l| l.1)
            * life.maturity()
    };
    for (ei, era) in c.plan.eras.iter().enumerate() {
        let era_w = era.iter().map(|&i| yw(i)).fold(0.0, f64::max);
        if era_w < 0.02 {
            continue;
        }
        let side = era_side(ei);
        let ag = life.years_ago(*era.last().unwrap_or(&0));
        let era_total: f64 = era.iter().map(|&i| life.years[i].total).sum();
        let named: f64 = era.iter().flat_map(|&i| life.years[i].langs.values()).sum();
        // The era's work later dropped, and how far it has died back. An era
        // dropped almost whole dies where it grew, and everything on it.
        let gone_v: f64 = era
            .iter()
            .flat_map(|&i| life.years[i].dead.values())
            .map(|d| d.0)
            .sum();
        // Its public work speaks for the era only when there is enough of it:
        // one dropped contribution among years of private work kills nothing.
        let era_dead = named >= 0.25 * era_total && named > 0.0 && gone_v / named >= 0.8;
        // A limb dies with the last drop that made it dead: it starts
        // bark-brown and bleaches from then, never already white.
        let died = era
            .iter()
            .flat_map(|&i| life.years[i].dead.values())
            .filter(|d| d.0 > 0.0)
            .map(|d| d.1 / d.0)
            .fold(1.0, f64::min);
        let k = wood.chain_for(wood.chain_on(side), era_dead);
        // In the wind a living limb on the downwind side leaves the trunk
        // lower, under the crown that streams that way.
        let low = if side * wood.sweep >= 0.0 && !era_dead {
            1.0 - 0.35 * wood.sweep.abs()
        } else {
            1.0
        };
        // A formal upright's first limb leaves the trunk at the triangle's
        // base, a third of the way up; the rest keep their spacing above it.
        // A pause's window is bare trunk already: its limbs stay below it.
        let lift = if c.window_lo().is_finite() {
            0.0
        } else {
            wood.tier * (c.cone().h_b - c.frame.limb_h(0)).max(0.0)
        };
        let limb_h = |ei: usize| c.frame.limb_h(ei) + lift;
        let from = at_height(sk, &wood.chains[k], limb_h(ei) * low);
        let p0 = sk.nodes[from].p;
        let grown: f64 = era
            .iter()
            .map(|&i| yw(i) * (5.0 + 6.0 * (life.years[i].days as f64 / 60.0).sqrt().min(1.6)))
            .sum();
        let len = (16.0 + grown) * (1.0 + 0.08 * ag.min(14.0));
        // Sparse years reach less far: size follows the work, so a few
        // contributions spread over many years don't spread a wide skirt.
        let vigor = c.frame.vigor;
        let len = within(len, 30.0 + 10.0 * vigor, big_s * (0.6 + 0.45 * vigor)) * era_w;
        // A formal upright's low limbs rise steeply into its lowest layers,
        // out to the triangle's edge.
        let cone = c.cone();
        let edge = cone.width(limb_h(ei)) / (1.0 + LIMB_RISE * cone.w_b / (cone.h_top - cone.h_b))
            - cell * 0.45;
        let len = len + wood.tier * era_w * (edge.max(20.0) - len / era_w.max(1e-9));
        // Never up into a pause's window.
        let rise = (LIMB_RISE * len).min((c.window_lo() - cell * 0.5 - limb_h(ei)).max(0.0));
        // Keyed by the era's first year: a new era moves no other limb.
        let y0 = life.years[era[0]].y;
        let mut er = Rng::keyed(seed, &format!("bdome/era/{y0}"));
        let ecv = irr(months_cv(&months_of(life, era).0));
        // Dead wood stops thickening, and weathers thinner as it bleaches.
        let thrive = if era_dead { 1.0 - 0.9 * died } else { 1.0 };
        sk.nodes[from].own += era_total * 0.03 * ag.min(12.0) * era_w * thrive;
        // A dead limb breaks back as it weathers.
        let len = if era_dead {
            len * (1.0 - 0.4 * died)
        } else {
            len
        };
        // In the wind a limb on the windward side keeps a little shorter, and
        // one downwind reaches a little further.
        let (ws, wd) = (wood.sweep.abs(), wood.sweep.signum());
        let len = len
            * if side * wd < 0.0 {
                1.0 - 0.2 * ws
            } else {
                1.0 + 0.1 * ws
            };
        // Limbs leave the trunk to the side or behind it, never toward the
        // viewer: the trunk hides where they join, as on a bonsai.
        let ez = er.range(-10.0, -3.0);
        let jin = Some(JIN.to_string());
        let limb_n0 = sk.nodes.len();
        limbs.push((limb_n0, side));
        let end = (
            p0.0 + side * len,
            p0.1 + (0.55 * ag.min(14.0) - 2.0) * era_w - wood.tier * rise,
        );
        // In the wind both limbs reach downwind and nearly level: the windward
        // one rising a little into the open space between the trunks.
        let aim = if side * wd < 0.0 {
            (p0.0 + wd * len * 0.85, p0.1 - len * 0.3)
        } else {
            (p0.0 + wd * len * 0.95, p0.1 - len * 0.05)
        };
        // Only a tree mostly windswept leans its limbs in; a light blend of
        // wind keeps them to their sides.
        let lean_in = ((ws - 0.4) / 0.37).clamp(0.0, 1.0);
        let lean_in = lean_in * lean_in * (3.0 - 2.0 * lean_in);
        // It swings round to its aim, keeping its reach, up and over: a
        // straight blend would fold a windward limb back onto the trunk.
        let (v0, v1) = ((end.0 - p0.0, end.1 - p0.1), (aim.0 - p0.0, aim.1 - p0.1));
        let (a0, a1) = (math::atan2(v0.1, v0.0), math::atan2(v1.1, v1.0));
        let turn = (a1 - a0 + PI).rem_euclid(math::TAU) - PI;
        let (ang, reach) = (
            a0 + lean_in * turn,
            math::lerp(dist(p0, end), dist(p0, aim), lean_in),
        );
        let end = if lean_in > 0.0 {
            (p0.0 + reach * math::cos(ang), p0.1 + reach * math::sin(ang))
        } else {
            end
        };
        let gnarl = 0.004 * ag.min(14.0);
        let limb = limb_to(
            sk,
            from,
            end,
            ez,
            // The downwind limb of a windswept tree tilts down as it leaves.
            side * len * (0.05 + if side * wd > 0.0 { 0.15 * lean_in } else { 0.0 }),
            len * (0.03 + 0.05 * ecv + gnarl),
            1.5 + ecv + 2.0 * gnarl / 0.056,
            er.range(0.0, math::TAU),
            if era_dead { &jin } else { &None },
        );
        if era_dead {
            // A dead limb stays grey while its crown arms still stand,
            // whitening as they fall.
            let standing = era
                .iter()
                .filter_map(|&i| c.plan.lingering.iter().find(|x| x.0 == i).map(|x| x.1))
                .fold(1.0, f64::min);
            bleach_from(sk, limb_n0, died.min(0.35 + 0.65 * standing));
            for &i in era {
                sk.deadwood.extend(
                    life.years[i]
                        .dead
                        .iter()
                        .filter(|d| d.1.0 > 0.0)
                        .map(|d| d.0.clone()),
                );
            }
        }
        // Old wood keeps thickening: the limb carries its years.
        if let Some(&mid) = limb.get(limb.len() * 6 / 10) {
            sk.nodes[mid].own +=
                (era_total * 0.1 * ag.min(12.0) + 130.0 * era.len() as f64 * ag.min(14.0)) * thrive;
        }
        let n = era.len();
        for (j, &i) in era.iter().enumerate() {
            let y = &life.years[i];
            let yag = life.years_ago(i);
            let t = 0.42 + 0.58 * (j + 1) as f64 / n as f64;
            let at = limb[((limb.len() - 1) as f64 * t) as usize];
            let ap = sk.nodes[at].p;
            let az0 = zof(sk, at);
            let shares = shares_of(&y.months, y.total);
            let ys = y.total.max(1e-9);
            if yw(i) < 0.02 {
                continue;
            }
            let tw_len = (6.0 + 8.0 * (y.days as f64 / 250.0).sqrt().min(1.2)) * yw(i);
            // The era's two biggest dropped languages show in their biggest
            // year, even when they were a small part of it.
            let extra: Vec<&String> = {
                let mut dropped: BTreeMap<&String, f64> = BTreeMap::new();
                for &k in era {
                    for (l, d) in &life.years[k].dead {
                        *dropped.entry(l).or_default() += d.0;
                    }
                }
                let mut top: Vec<(&String, f64)> =
                    dropped.into_iter().filter(|d| d.1 > 0.0).collect();
                top.sort_by(|a, b| b.1.total_cmp(&a.1));
                let v = |l: &String, k: usize| life.years[k].langs.get(l).copied().unwrap_or(0.0);
                top.into_iter()
                    .take(2)
                    .map(|(l, _)| l)
                    .filter(|l| {
                        era.iter()
                            .copied()
                            .max_by(|a, b| v(l, *a).total_cmp(&v(l, *b)))
                            == Some(i)
                    })
                    .collect()
            };
            let mut yr = Rng::keyed(seed, &format!("bdome/old/{}", y.y));
            for (slot, (l, v)) in shares
                .iter()
                .filter(|s| s.1 / ys >= 0.12)
                .take(3)
                .chain(shares.iter().filter(|s| {
                    s.1 / ys < 0.12 && s.0.as_ref().is_some_and(|l| extra.contains(&l))
                }))
                .enumerate()
            {
                let f = match slot {
                    0 => 0.0,
                    s if s % 2 == 1 => -1.0,
                    _ => 1.0,
                };
                let to = (
                    ap.0 + (side + (wd - side) * lean_in) * tw_len * (0.5 + 0.2 * f)
                        + yr.range(-2.0, 2.0),
                    ap.1 - tw_len * (0.9 - 0.25 * f64::abs(f)),
                );
                let tz = az0 + f * 9.0 + yr.range(-3.0, 3.0);
                // Dropped work's twig dies where it grew, as does every twig
                // on a dead limb.
                let (full, g) = life.dead_share(i, l);
                let dead = if era_dead {
                    Some(died)
                } else {
                    (full >= 0.5).then(|| g / full)
                };
                let twig_n0 = sk.nodes.len();
                // A dead twig breaks back as it weathers, but never to a stub.
                let to = dead.map_or(to, |b| {
                    let k = 1.0 - 0.3 * b;
                    (ap.0 + (to.0 - ap.0) * k, ap.1 + (to.1 - ap.1) * k)
                });
                let twig = limb_to(
                    sk,
                    at,
                    to,
                    tz,
                    0.0,
                    1.5 + 1.5 * irr(y.cv()),
                    1.2,
                    yr.range(0.0, math::TAU),
                    if dead.is_some() { &jin } else { l },
                );
                let vd = v * decay(yag);
                let rx = within(8.5 + 22.0 * (vd / FULL_YEAR).sqrt(), 8.5, cell * 0.62) * yw(i);
                let mut under = twig;
                under.push(at);
                if let Some(b) = dead {
                    let fan_rng = &mut Rng::keyed(seed, &format!("bdome/fan/old/{}/{slot}", y.y));
                    let at = (to.0, to.1 - rx * 0.3);
                    // Its fan thins as it weathers, and keeps a few bare twigs.
                    let keep = (1.0 - b).max(0.35);
                    winter_fan(sk, fan_rng, &under, at, rx * 0.8, tz, side, &jin, keep);
                    bleach_from(sk, twig_n0, b);
                    sk.deadwood.extend(l.clone());
                    continue;
                }
                // Twigs show as the leaves fall; a dropped language's leaves
                // are gone: bare wood.
                let bare = c.life.bare(l);
                if bare > 0.02 {
                    winter_fan(
                        sk,
                        &mut Rng::keyed(seed, &format!("bdome/fan/old/{}/{slot}", y.y)),
                        &under,
                        (to.0, to.1 - rx * 0.3),
                        rx * 0.8,
                        tz,
                        side,
                        l,
                        bare,
                    );
                }
                if !c.dormant && !c.gone(l) {
                    pad(sk, &mut yr, l, *v, to, tz, rx, side, &under, from);
                }
                sk.nodes[at].own += v * 0.05 * yag.min(12.0);
            }
        }
    }
    limbs
}

/// Where each dead arm of a year gone from the crown stood the day before
/// its year left: its angle, reach, share of the crown and trunk chain.
fn frozen_dead(c: &Ctx, wood: &Wood) -> BTreeMap<(i32, Lang), (f64, f64, f64, usize)> {
    let life = c.life;
    let days: std::collections::BTreeSet<Date> = c.plan.lingering.iter().map(|l| l.2).collect();
    let all = trunks(life);
    let mut out = BTreeMap::new();
    for day in days {
        let gone: Vec<i32> = c
            .plan
            .lingering
            .iter()
            .filter(|l| l.2 == day)
            .map(|l| life.years[l.0].y)
            .collect();
        let cb = c.with_plan(plan_of(life, &all, day.add_days(-1), &[]));
        let (arms, _) = crown_arms(&cb, &BTreeMap::new());
        let sectors = Sectors::of(&cb, &arms);
        let (placed, _) = place_arms(&cb, wood, &arms, &sectors);
        for (i, th, r, k) in placed {
            let a = &arms[i];
            if a.dead.is_some() && gone.contains(&a.year) {
                out.insert((a.year, a.lang.clone()), (th, r, a.w, k));
            }
        }
    }
    out
}

/// When each dead arm of a year gone from the crown falls, as a fraction of
/// the two years after its year left: evenly spaced, in an order of their
/// own.
fn break_moments(c: &Ctx) -> BTreeMap<(i32, Lang), f64> {
    let life = c.life;
    let hash = |y: i32, l: &Lang| {
        Rng::keyed(c.seed, &format!("bdome/break/{y}/{}", key(l))).range(0.0, 1.0)
    };
    let mut out = BTreeMap::new();
    // Years that left together fall in one sequence.
    let mut groups: BTreeMap<i64, Vec<(i32, Lang)>> = BTreeMap::new();
    for &(i, f, _) in &c.plan.lingering {
        let y = &life.years[i];
        for (l, _) in shares_of(&y.months, y.total) {
            if life.dead_share(i, &l).0 >= 0.5 {
                groups
                    .entry((f * 1e6).round() as i64)
                    .or_default()
                    .push((y.y, l));
            }
        }
    }
    for arms in groups.into_values() {
        let mut arms: Vec<((i32, Lang), f64)> = arms
            .into_iter()
            .map(|a| (a.clone(), hash(a.0, &a.1)))
            .collect();
        arms.sort_by(|a, b| a.1.total_cmp(&b.1));
        let n = arms.len() as f64;
        for (k, (a, _)) in arms.into_iter().enumerate() {
            out.insert(a, (k as f64 + 1.0) / (n + 1.0));
        }
    }
    out
}

/// The crown years as arms, one per language and year: the crown
/// languages' arms, the minor languages' (at most two a year) for the skirt,
/// and each year's dropped share for deadwood.
fn crown_arms(
    c: &Ctx,
    frozen: &BTreeMap<(i32, Lang), (f64, f64, f64, usize)>,
) -> (Vec<Arm>, Vec<Arm>) {
    let breaks = break_moments(c);
    let life = c.life;
    let phi0 = Rng::keyed(c.seed, "bdome/phi").range(0.0, math::TAU);
    let mut arms: Vec<Arm> = Vec::new();
    let mut lower: Vec<Arm> = Vec::new();
    // Leaving years' living arms fade out of the crown as they go; their
    // dead arms stand on, and break back and fall one by one over the two
    // years after.
    let newest = c.plan.crown.last().copied();
    let fading = c.plan.leaving.iter().map(|&(i, w)| (i, 1.0 - w, None));
    let standing = c.plan.lingering.iter().map(|&(i, f, _)| (i, 1.0, Some(f)));
    let staying = c.plan.crown.iter().map(|&i| {
        (
            i,
            if Some(i) == newest {
                life.grown_in(i)
            } else {
                1.0
            },
            None,
        )
    });
    for (i, yw, gone) in fading.chain(standing).chain(staying) {
        let y = &life.years[i];
        let ag = life.years_ago(i);
        let az = phi0 + math::rad(f64::from(y.y - c.y_first) * GOLDEN);
        let ys = y.total.max(1e-9);
        let mut n_low = 0;
        for (l, v) in &shares_of(&y.months, y.total) {
            if yw < 0.02 {
                continue;
            }
            // Work later dropped dies where it grew, and bleaches; a language
            // that came back grows new arms from living wood.
            let (full, g) = life.dead_share(i, l);
            let dead = (full >= 0.5).then(|| g / full);
            let w = yw;
            let mut back = 1.0;
            match gone {
                // A year gone from the crown keeps only its dead arms, each
                // until it falls.
                Some(f) => {
                    let Some(at) = dead.and(breaks.get(&(y.y, l.clone())).copied()) else {
                        continue;
                    };
                    if !frozen.contains_key(&(y.y, l.clone())) {
                        continue;
                    }
                    if f >= at {
                        continue;
                    }
                    // It breaks back over the year before it falls, a
                    // steady share each month, to a stub; never before its
                    // year left the crown.
                    let span = at.min(0.5);
                    back = math::pow(0.03, ((f - (at - span)) / span).clamp(0.0, 1.0));
                }
                None if dead.is_some() && c.plan.leaving.iter().any(|x| x.0 == i) => continue,
                None => {}
            }
            let a = Arm {
                lang: l.clone(),
                v: *v,
                vd: v * decay(ag),
                year: y.y,
                days: y.days as f64 * (v / ys).sqrt(),
                irr: irr(y.cv()),
                zdir: math::sin(az),
                side: if math::cos(az) >= 0.0 { 1.0 } else { -1.0 },
                total: y.total,
                ag,
                w,
                dead,
                back,
            };
            // A language whose sector is still growing in is part crown
            // arm, part skirt limb; a dead arm that stays where it stood
            // keeps the share it had then.
            let up = frozen
                .get(&(y.y, l.clone()))
                .filter(|_| gone.is_some())
                .map_or_else(|| c.sector_of(l), |f| f.2);
            if up > 0.0 {
                arms.push(Arm {
                    w: a.w * up,
                    ..a.clone()
                });
            }
            if up < 1.0 && v / ys >= 0.04 && n_low < 2 {
                n_low += 1;
                lower.push(Arm {
                    w: a.w * (1.0 - up),
                    ..a
                });
            }
        }
    }
    (arms, lower)
}

/// Crown arms: their angle in the dome and their reach; and the dome they
/// outline.
fn place_arms(c: &Ctx, wood: &Wood, arms: &[Arm], sectors: &Sectors) -> (Vec<Placed>, Dome) {
    let (upper, s_a) = (&c.plan.upper, c.s_a);
    let Frame { big_s, vv, .. } = c.frame;
    let mut placed: Vec<Placed> = Vec::new();
    let mut env: Vec<(f64, f64, f64)> = Vec::new();
    let reach_of = |days: f64| big_s * (0.6 + 0.4 * (days / 220.0).min(1.0).sqrt());
    for (ui, (_, l)) in upper.iter().enumerate() {
        let (lo, hi) = (sectors.bounds[ui], sectors.bounds[ui + 1]);
        let mine: Vec<usize> = (0..arms.len()).filter(|&i| &arms[i].lang == l).collect();
        if mine.is_empty() {
            continue;
        }
        let oldest = mine.iter().map(|&i| arms[i].ag).fold(0.0, f64::max);
        let single = upper.len() == 1;
        let (b0, b1) = if f64::midpoint(lo, hi) < 0.5 {
            (lo, hi)
        } else {
            (hi, lo)
        };
        let at_t = |t: f64, alt: f64| -> f64 {
            if single {
                0.5 + alt * (1.0 - t) * 0.42
            } else {
                b0 + (b1 - b0) * (0.1 + 0.82 * t)
            }
        };
        // Oldest at 0, a complete newest year (half a year ago) at 1; the
        // year growing now stays at 1, so each arm moves a little each day.
        // The span is at least a season: an only arm turning half a year old
        // walks over in three months, never across at once.
        let span = oldest - 0.5;
        let t_of = |ag: f64| {
            if span >= 0.25 {
                ((oldest - ag) / span).clamp(0.0, 1.0)
            } else {
                ((0.75 - ag) / 0.25).clamp(0.0, 1.0)
            }
        };
        let k = if ui == 0 { 0 } else { wood.chain_on(-s_a) };
        for &i in &mine {
            let a = &arms[i];
            let th = sectors.theta(at_t(t_of(a.ag), a.side));
            let r = reach_of(a.days);
            placed.push((i, th, r, k));
            env.push((th, r, (a.vd.sqrt() + 5.0) * a.w));
        }
    }
    let r_mean = if env.is_empty() {
        big_s * 0.8
    } else {
        env.iter().map(|e| e.1).sum::<f64>() / env.len() as f64
    };
    let w_mean = if env.is_empty() {
        1.0
    } else {
        env.iter().map(|e| e.2).sum::<f64>() / env.len() as f64
    };
    let dome = Dome {
        center: wood.center,
        y_b: c.y_b,
        vv,
        rz: big_s * 0.62,
        r_mean,
        w_mean,
        env,
        sweep: 0.0,
        split: 0.0,
        tier: wood.tier,
        th_lo: c.frame.th_lo,
        cone: c.cone(),
        layers: LAYERS,
    };
    (placed, dome)
}

/// Grows the crown arms, each from its own trunk inside the dome; returns
/// the outer nodes of each language's arms, for its clouds to hang from.
fn grow_arms(
    sk: &mut Skeleton,
    c: &Ctx,
    wood: &Wood,
    dome: &Dome,
    arms: &[Arm],
    placed: &[Placed],
) -> (BTreeMap<String, Vec<usize>>, BTreeMap<String, Vec<usize>>) {
    let rz = dome.rz;
    // Living and dead arms apart: leaves hang only from living wood, dead
    // twigs only from dead wood.
    let mut by_lang: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut dead_by_lang: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let forks = (c.age / 5.0).floor().clamp(0.0, 3.0) as usize;
    for &(i, th, r, k) in placed {
        let a = arms[i].clone();
        let k = wood.chain_for(k, a.dead.is_some());
        let f_lo = wood.rim_frac(k, sk, c.y_b);
        let (er, _) = dome.envelope(th);
        // A growing (or leaving) arm reaches part of the way out, as does
        // dead wood breaking back.
        let full = dome.pt(th, er.min(r) * 0.66);
        // A formal upright's arm leaves the trunk just under its layer.
        let ch = &wood.chains[k];
        let under_f = ch
            .iter()
            .position(|&n| n == at_height(sk, ch, SOIL_Y - full.1 - 8.0))
            .unwrap_or(0) as f64
            / (ch.len().max(2) - 1) as f64;
        let dome_f = f_lo + (0.98 - f_lo) * math::sin(th).clamp(0.0, 1.0) * 0.9;
        let from = at_frac(ch, dome_f + wood.tier * (under_f - dome_f));
        if a.w < 0.02 {
            continue;
        }
        let p0 = sk.nodes[from].p;
        let reach = a.w * a.back;
        let to = (
            p0.0 + (full.0 - p0.0) * reach,
            p0.1 + (full.1 - p0.1) * reach,
        );
        let tz = rz * 0.42 * a.zdir * reach;
        let len = dist(to, p0);
        let sgn = if to.0 >= p0.0 { 1.0 } else { -1.0 };
        let mut yr = Rng::keyed(c.seed, &format!("bdome/arm/{}/{}", a.year, key(&a.lang)));
        let n0 = sk.nodes.len();
        let wood_lang = if a.dead.is_some() {
            sk.deadwood.extend(a.lang.clone());
            Some(JIN.to_string())
        } else {
            a.lang.clone()
        };
        let nodes = limb_to(
            sk,
            from,
            to,
            tz,
            sgn * len * 0.08,
            len * (0.02 + 0.06 * a.irr),
            1.0 + 1.5 * a.irr,
            yr.range(0.0, math::TAU),
            &wood_lang,
        );
        if let Some(&tip) = nodes.last() {
            sk.nodes[tip].own += a.v * 0.55 * a.w;
        }
        sk.nodes[from].own += (a.v * 0.2 + a.total * 0.05 * a.ag.min(12.0) * 0.5) * a.w;
        let hang = if a.dead.is_some() {
            &mut dead_by_lang
        } else {
            &mut by_lang
        };
        hang.entry(key(&a.lang))
            .or_default()
            .extend_from_slice(&nodes[nodes.len() * 45 / 100..]);
        // Ramification: an older tree's arm has forked more often.
        for f in 0..forks {
            let at = nodes[nodes.len() * (40 + 15 * f) / 100];
            let ap = sk.nodes[at].p;
            // A formal upright's fork keeps to its own layer.
            let dth = if f % 2 == 0 { 0.32 } else { -0.32 }
                * (1.0 + 0.3 * f as f64)
                * (1.0 - 0.8 * wood.tier);
            let (er2, _) = dome.envelope(th + dth);
            let tgt = dome.pt(th + dth, er2.min(r) * yr.range(0.5, 0.68));
            // A dead arm's fine branches break off as it weathers, and break
            // back with it.
            let snap = (1.0 - 0.5 * a.dead.unwrap_or(0.0)) * a.back;
            let tgt = (ap.0 + (tgt.0 - ap.0) * snap, ap.1 + (tgt.1 - ap.1) * snap);
            let fz = tz + yr.range(-0.3, 0.3) * rz;
            let fl = dist(tgt, ap);
            let tw = limb_to(
                sk,
                at,
                tgt,
                fz,
                sgn * fl * 0.06,
                fl * (0.03 + 0.05 * a.irr),
                1.0 + a.irr,
                yr.range(0.0, math::TAU),
                &wood_lang,
            );
            if let Some(&tip) = tw.last() {
                sk.nodes[tip].own += a.v * 0.06;
            }
            let hang = if a.dead.is_some() {
                &mut dead_by_lang
            } else {
                &mut by_lang
            };
            hang.entry(key(&a.lang))
                .or_default()
                .extend_from_slice(&tw[tw.len() / 2..]);
        }
        if let Some(b) = a.dead {
            bleach_from(sk, n0, b);
        }
    }
    (by_lang, dead_by_lang)
}

/// The dome's skirt: minor languages and private work on short limbs just
/// under the rim, each keeping to one side.
fn grow_skirt(
    sk: &mut Skeleton,
    c: &Ctx,
    wood: &Wood,
    dome: &Dome,
    lower: &[Arm],
) -> Vec<(usize, f64)> {
    let mut limbs = Vec::new();
    let (s_a, th_lo, cell) = (c.s_a, c.frame.th_lo, c.frame.cell);
    // Sides alternate in the order the languages first grew, which a new
    // year never changes.
    let first_year = |l: &str| {
        c.life
            .years
            .iter()
            .position(|y| shares_of(&y.months, y.total).iter().any(|s| key(&s.0) == l))
            .unwrap_or(usize::MAX)
    };
    let mut lorder: Vec<String> = lower.iter().map(|a| key(&a.lang)).collect();
    lorder.sort_by_key(|l| (first_year(l), l.clone()));
    lorder.dedup();
    let oldest = lower.iter().map(|a| a.ag).fold(0.0, f64::max);
    for a in lower {
        let rank = lorder.iter().position(|x| *x == key(&a.lang)).unwrap_or(0);
        let side = if rank % 2 == 0 { -s_a } else { s_a };
        let k = wood.chain_for(wood.chain_on(side), a.dead.is_some());
        let f_lo = wood.rim_frac(k, sk, c.y_b);
        let from = at_frac(&wood.chains[k], (f_lo - 0.02).max(0.0));
        let p0 = sk.nodes[from].p;
        let t = ((oldest - a.ag) / (oldest - 0.5).max(1.0)).clamp(0.0, 1.0);
        let lift = if a.lang.is_none() {
            -0.02 + 0.06 * t
        } else {
            0.06 + 0.32 * t + 0.12 * (rank / 2) as f64
        };
        let th = if side > 0.0 {
            th_lo + lift
        } else {
            PI - th_lo - lift
        };
        if a.w < 0.02 {
            continue;
        }
        let (er, _) = dome.envelope(th);
        let q = dome.pt(th, er * if a.lang.is_none() { 0.8 } else { 0.84 });
        let full = (q.0, q.1 + if a.lang.is_none() { 12.0 } else { 6.0 });
        let reach = a.w * a.back;
        let to = (
            p0.0 + (full.0 - p0.0) * reach,
            p0.1 + (full.1 - p0.1) * reach,
        );
        let tz = dome.rz * 0.5 * a.zdir * reach;
        let mut yr = Rng::keyed(c.seed, &format!("bdome/low/{}/{}", a.year, key(&a.lang)));
        let len = (to.0 - p0.0).abs();
        // Dropped work dies in place: a dead limb with dead twigs, no leaves.
        let jin = Some(JIN.to_string());
        let wood_lang = if a.dead.is_some() { &jin } else { &a.lang };
        if a.dead.is_some() {
            sk.deadwood.extend(a.lang.clone());
        }
        let n0 = sk.nodes.len();
        limbs.push((n0, side));
        let mut under = limb_to(
            sk,
            from,
            to,
            tz,
            side * len * 0.06,
            2.0 + 2.0 * a.irr,
            1.0 + a.irr,
            yr.range(0.0, math::TAU),
            wood_lang,
        );
        sk.nodes[from].own += a.v * 0.3 * a.w;
        let rx = within(7.0 + 30.0 * (a.vd / FULL_YEAR).sqrt(), 7.0, cell * 0.85) * a.w;
        if let Some(g) = a.dead {
            let key = format!("bdome/fan/low/{}/{}", a.year, key(&a.lang));
            let at = (to.0, to.1 - rx * 0.3);
            let fan_rng = &mut Rng::keyed(c.seed, &key);
            winter_fan(sk, fan_rng, &under, at, rx * 0.8, tz, side, &jin, 1.0 - g);
            bleach_from(sk, n0, g);
            continue;
        }
        under.push(from);
        let bare = c.life.bare(&a.lang);
        if bare > 0.02 {
            let key = format!("bdome/fan/low/{}/{}", a.year, key(&a.lang));
            winter_fan(
                sk,
                &mut Rng::keyed(c.seed, &key),
                &under,
                (to.0, to.1 - rx * 0.3),
                rx * 0.8,
                tz,
                side,
                &a.lang,
                bare,
            );
        }
        if !c.dormant && !c.gone(&a.lang) {
            pad(sk, &mut yr, &a.lang, a.v, to, tz, rx, side, &under, from);
        }
    }
    merge_clouds(sk);
    limbs
}

/// The dome: rim, front, back and crown rows of clouds, with a few front
/// windows; bare twig fans in winter.
fn dome_clouds(
    sk: &mut Skeleton,
    c: &Ctx,
    wood: &Wood,
    dome: &Dome,
    sectors: &Sectors,
    (by_lang, dead_by_lang, dead_w): (
        &BTreeMap<String, Vec<usize>>,
        &BTreeMap<String, Vec<usize>>,
        &BTreeMap<String, f64>,
    ),
    n_arms: usize,
) {
    let (seed, s_a) = (c.seed, c.s_a);
    let Frame {
        big_s, th_lo, cell, ..
    } = c.frame;
    let all_nodes: Vec<usize> = by_lang.values().flatten().copied().collect();
    let mut twigs: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let trunk_tops: Vec<usize> = wood
        .chains
        .iter()
        .flat_map(|ch| ch[ch.len() * 7 / 10..].to_vec())
        .collect();
    let mut rim_angles = vec![th_lo];
    let mut acc = 0.0;
    // Spaced along the dome, so a formal upright's crown has no more clouds.
    let mut last = dome.domed(th_lo, dome.envelope(th_lo).0);
    // A formal upright's clouds are bigger and further apart.
    let spacing = cell * 1.25 * (1.0 + 0.38 * wood.tier);
    for q in 1..=240 {
        let th = th_lo + (PI - 2.0 * th_lo) * f64::from(q) / 240.0;
        let p = dome.domed(th, dome.envelope(th).0);
        acc += dist(p, last);
        last = p;
        if acc >= spacing {
            rim_angles.push(th);
            acc = 0.0;
        }
    }
    if acc > spacing * 0.5 {
        rim_angles.push(PI - th_lo);
    }
    // Every cell keeps its identity (and its randomness) as the dome grows:
    // a new rim cell joins at the end and moves no other.
    // (angle, radius factor, depth factor, size factor, identity)
    let jitter = |id: i64| hash01(seed ^ 0xce11, id) - 0.5;
    let mut cells: Vec<(f64, f64, f64, f64, i64)> = (0_i64..)
        .zip(&rim_angles)
        .map(|(k, th)| (*th, 1.0, 0.3 * jitter(k), 1.0, k))
        .collect();
    // Front and back cells between rim neighbours, about half of them; a
    // formal upright's layers are full down to the lowest, and reach in
    // over the trunk.
    let lo = 0.42 * (1.0 - wood.tier);
    for (k, pair) in (0_i64..).zip(rim_angles.windows(2)) {
        let th = f64::midpoint(pair[0], pair[1]);
        if (lo..=PI - lo).contains(&th) && hash01(seed ^ 0x1ce, k) < 0.8 + 0.2 * wood.tier {
            let rf = 0.62 - 0.1 * wood.tier;
            cells.push((th, rf, 0.55, 0.88, 1000 + k));
            // Mirrored on a formal upright, so both sides match; its full
            // front hides most of the back.
            let back = if th < PI / 2.0 {
                1.0
            } else {
                1.0 - 2.0 * wood.tier
            };
            if hash01(seed ^ 0xbac, k) >= 0.9 * wood.tier {
                cells.push((th + 0.15 * back, 0.6, -0.55, 0.92, 2000 + k));
            }
        }
        // In front of the trunk, every other gap, growing in from nothing.
        if wood.tier > 0.25 && k % 2 == 0 {
            let sz = (wood.tier - 0.25) / 0.75;
            cells.push((th, 0.3, 0.35 + 0.2 * jitter(4000 + k), sz, 4000 + k));
        }
    }
    cells.push((PI / 2.0 + 0.3 * jitter(3000), 0.3, 0.1, 1.1, 3000));
    // Windows: a few front cells left open, where the branches show, and
    // the back cell behind each; more on a big crown, so it never reads as
    // one solid mass.
    let mut skip = vec![false; cells.len()];
    let n_win = if wood.tier > 0.5 {
        0
    } else {
        1 + usize::from(n_arms >= 6) + usize::from(big_s > 125.0)
    };
    let front: Vec<usize> = (0..cells.len()).filter(|&i| cells[i].2 > 0.4).collect();
    // Windows open in the front cells that rank first by a hash of their
    // identity, so they stay put as cells join.
    let mut ranked = front.clone();
    ranked.sort_by(|a, b| {
        hash01(seed ^ 0x3a1d, cells[*a].4).total_cmp(&hash01(seed ^ 0x3a1d, cells[*b].4))
    });
    let mut made = 0;
    for &pick in &ranked {
        if made >= n_win {
            break;
        }
        let i_f = front.iter().position(|x| *x == pick).unwrap_or(0);
        let near = front
            .iter()
            .enumerate()
            .any(|(jf, x)| skip[*x] && jf.abs_diff(i_f) <= 1);
        if !skip[pick] && !near {
            skip[pick] = true;
            made += 1;
        }
    }
    let back: Vec<usize> = (0..cells.len()).filter(|&i| cells[i].2 < -0.4).collect();
    for &i in &front {
        if skip[i]
            && let Some(&b) = back.iter().min_by(|a, b| {
                (cells[**a].0 - cells[i].0)
                    .abs()
                    .total_cmp(&(cells[**b].0 - cells[i].0).abs())
            })
        {
            skip[b] = true;
        }
    }
    // A cell's language, by where it sits on the dome, and the arms of it.
    let lang_of = |th: f64, id: i64| {
        let raw = if s_a < 0.0 {
            (PI - th_lo - th) / (PI - 2.0 * th_lo)
        } else {
            (th - th_lo) / (PI - 2.0 * th_lo)
        };
        sectors.lang_at(raw + (hash01(seed ^ 0x5ec, id) - 0.5) * 0.05)
    };
    let arms_of = |lang: &Lang| {
        by_lang.get(&key(lang)).filter(|v| !v.is_empty()).map_or(
            if all_nodes.is_empty() {
                &trunk_tops
            } else {
                &all_nodes
            },
            |v| v,
        )
    };
    // A bare crown in the wind grows its fans out from its arms, nearest
    // first, so a cell the wind swept far from them hangs from the fans
    // before it, not on a long stem from the arm. Without wind the cells
    // lie near their arms already.
    let windswept = c.dormant && wood.sweep != 0.0;
    let mut order: Vec<usize> = (0..cells.len()).collect();
    if windswept {
        let gap = |ci: usize| {
            let (th, rf, _, _, id) = cells[ci];
            let p = dome.pt(th, dome.envelope(th).0 * rf);
            arms_of(&lang_of(th, id))
                .iter()
                .map(|&i| dist(sk.nodes[i].p, p))
                .fold(f64::INFINITY, f64::min)
        };
        order.sort_by(|a, b| gap(*a).total_cmp(&gap(*b)));
    }
    let mut fans: Vec<P> = Vec::new();
    for ci in order {
        let (th, rf, zf, sz, id) = cells[ci];
        if skip[ci] {
            continue;
        }
        let rng = &mut Rng::keyed(seed, &format!("bdome/cell/{id}"));
        let (er, dens) = dome.envelope(th);
        let rx = cell * sz * (0.82 + 0.18 * dens.min(1.5)) * rng.range(0.92, 1.08);
        // A formal upright's clouds shrink toward its apex, so its edges
        // run straight up to it.
        let up = 1.0 - (2.0 * (th - th_lo) / (PI - 2.0 * th_lo) - 1.0).abs();
        let rx = rx * (1.0 + 0.3 * wood.tier) * (1.0 - 0.4 * wood.tier * up.clamp(0.0, 1.0));
        let a = dome.pt(th, ((er - rx * 0.75) * rf).max(0.0));
        let base = (a.0, a.1 + rx * 0.3);
        let lang = lang_of(th, id);
        let mut under: Vec<usize> = arms_of(&lang).clone();
        // Earlier clouds' twigs are wood too: a new cloud may hang from
        // them, so the twigs branch out instead of fanning from one point.
        under.extend(twigs.get(&key(&lang)).into_iter().flatten());
        // A formal upright's cloud hangs from wood in its own layer, not
        // from the layer above or below.
        if wood.tier > 0.0 {
            let near = cell * 0.9 / wood.tier;
            let mine: Vec<usize> = under
                .iter()
                .copied()
                .filter(|&i| (sk.nodes[i].p.1 - base.1).abs() < near)
                .collect();
            if !mine.is_empty() {
                under = mine;
            }
        }
        let under = &under[..];
        let side = if math::cos(th) >= 0.0 { 1.0 } else { -1.0 };
        let fan_rng = &mut Rng::keyed(seed, &format!("bdome/fan/cell/{id}"));
        if c.gone(&lang) {
            // A dropped language's fine twigs died with its branches: they
            // stay on the dead arms, bleach and break back.
            if let Some(dead) = dead_by_lang.get(&key(&lang)).filter(|v| !v.is_empty()) {
                let g = c.dying(&lang);
                let n0 = sk.nodes.len();
                let jin = Some(JIN.to_string());
                let at = (base.0, base.1 - rx * 0.45);
                let left = dead_w.get(&key(&lang)).copied().unwrap_or(1.0);
                winter_fan(
                    sk,
                    fan_rng,
                    dead,
                    at,
                    rx,
                    zf * dome.rz,
                    side,
                    &jin,
                    (1.0 - g) * left,
                );
                bleach_from(sk, n0, g);
                fans.push(at);
            }
        }
        // As the leaves fall, the cloud's place shows a fan of bare twigs,
        // so the dome still reads as fine ramification. A dropped language's
        // cloud keeps its place, bare, so the crown never closes up at once.
        let bare = c.life.bare(&lang);
        let at = (base.0, base.1 - rx * 0.45);
        // A bare crown has no leaves to fill: a fan crowding one already
        // grown would only tangle with it, so it is left out.
        let crowded = windswept && fans.iter().any(|&f| dist(f, at) < FAN_ROOM * rx);
        if bare > 0.02 && !c.gone(&lang) && !crowded {
            fans.push(at);
            let n0 = sk.nodes.len();
            winter_fan(sk, fan_rng, under, at, rx, zf * dome.rz, side, &lang, bare);
            // A bare crown grows no clouds: its fans are its only twigs,
            // and in the wind the next fan may hang from them.
            if windswept {
                twigs
                    .entry(key(&lang))
                    .or_default()
                    .extend(n0..sk.nodes.len());
            }
        }
        if c.dormant {
            continue;
        }
        let patches = [(1.0, lang.clone()), (1.0, lang.clone())];
        let n0 = sk.nodes.len();
        cloud(
            sk,
            rng,
            &patches,
            base,
            zf * dome.rz,
            rx,
            side,
            under,
            0.0,
            0,
        );
        if let Some(cl) = sk.clouds.last_mut() {
            // A formal upright's layers lie flatter.
            cl.ry = rx * (0.6 - 0.15 * wood.tier);
            cl.rz = rx * 0.85;
        }
        // A dropped language's cloud keeps its place, so the crown never
        // closes up at once, but grows no twigs: its leaves are long gone,
        // and bare twigs hung from whatever living wood is near would read
        // as the teeth of a rake. Its own twigs died with its arms (above).
        if c.gone(&lang) {
            sk.nodes.truncate(n0);
            sk.node_z.truncate(n0);
            continue;
        }
        twigs
            .entry(key(&lang))
            .or_default()
            .extend(n0..sk.nodes.len());
    }
}

/// How close, as a share of its width, a bare crown's fan may stand to one
/// already grown before it is left out.
const FAN_ROOM: f64 = 0.8;

/// Girth: the trunks carry the rest; age thickens them. The arrangement
/// sets where girth goes; history alone sets how much, so the trunk never
/// thins as the tree is restyled.
fn girth(sk: &mut Skeleton, c: &Ctx, wood: &Wood) {
    let (life, age, chains) = (c.life, c.age, &wood.chains);
    let lifetime: f64 = life.years.iter().map(|y| y.total).sum();
    let held: f64 = sk.nodes.iter().skip(1).map(|n| n.own).sum();
    let rest = (lifetime - held).max(0.0) * 0.2;
    let thick = lifetime * 0.25 * math::pow(age / 9.0, 4.0);
    for (k, ch) in chains.iter().enumerate() {
        let part = match (chains.len() > 1, k) {
            (false, _) => 1.0,
            (true, 0) => 0.6,
            // A dead trunk stops thickening as it dies back.
            (true, _) => {
                0.4 * if wood.dead_b {
                    1.0 - 0.85 * wood.dead_g
                } else {
                    1.0
                }
            }
        };
        let own: Vec<usize> = ch.iter().copied().skip(wood.foot).collect();
        let m = own.len().max(1) as f64;
        for &n in &own {
            sk.nodes[n].own += rest * part / m;
        }
        let low = own.len() / 2;
        for &n in own.iter().take(low) {
            sk.nodes[n].own += thick * part / low.max(1) as f64;
        }
    }
    // The root is never thinner than it ever was. A formal upright's trunk
    // carries its steady years on top, laid along the trunk below.
    let (target, f) = girth_target(life);
    let extra = trunk_extra(life, wood.tier, target).max(c.reach.trunk / f);
    let target = (target + extra).max(c.reach.girth / f) - extra;
    // Dead wood keeps its own girth: scaling it with the living tree would
    // swell old jin into horns.
    let dead = |n: &crate::growth::Node| n.language.as_deref() == Some(crate::growth::JIN);
    let mut dead_sum: f64 = sk.nodes.iter().filter(|n| dead(n)).map(|n| n.own).sum();
    // On a small tree the dead wood's minimum girth could outweigh all the
    // living wood; it never takes more than its share.
    let dead_cap = DEAD_SHARE * target;
    if dead_sum > dead_cap {
        let s = dead_cap / dead_sum;
        for n in sk.nodes.iter_mut().filter(|n| dead(n)) {
            n.own *= s;
        }
        dead_sum = dead_cap;
    }
    let sum: f64 = sk.nodes.iter().filter(|n| !dead(n)).map(|n| n.own).sum();
    let k = if sum > 0.0 {
        (target - dead_sum).max(0.0) / sum
    } else {
        0.0
    };
    for n in sk.nodes.iter_mut().filter(|n| !dead(n)) {
        n.own *= k;
    }
    if extra > 0.0 {
        lay_trunk_extra(sk, &chains[0], extra, wood.tier);
    }
    flare(sk, &chains[0].clone());
    limbs_within_trunk(sk, wood);
    for n in &mut sk.nodes {
        n.own *= f;
    }
}

/// Lays a formal upright's `extra` girth along its `main` trunk. Most sits
/// low: the lower trunk swells into a broad nebari. A share ([`TRUNK_UP`])
/// runs evenly up the rest, so the trunk tapers less, as far as the tree
/// stands formal upright now (`tier`): a tree that was once keeps its old
/// girth low, and its leader stays a leader.
fn lay_trunk_extra(sk: &mut Skeleton, main: &[usize], extra: f64, tier: f64) {
    let m = main.len().max(2) as f64;
    let low = 0.35;
    let up = TRUNK_UP * tier.clamp(0.0, 1.0);
    let foot = 0.6 * (1.0 - up);
    for (i, &n) in main.iter().enumerate() {
        let u = i as f64 / (m - 1.0);
        sk.nodes[n].own += if u < low {
            (1.0 - up - foot) * extra * 3.0 * math::sq(1.0 - u / low) / (low * m)
        } else {
            up * extra / ((1.0 - low) * m)
        };
    }
    for (k, w) in [(1usize, 0.55), (2, 0.3), (3, 0.15)] {
        if let Some(&i) = main.get(k) {
            sk.nodes[i].own += foot * extra * w;
        }
    }
}

/// No limb is more than a third of the trunk's radius where it leaves, as
/// on a bonsai: an era far busier than the crown above it would otherwise
/// leave a thin trunk rising out of a lump of limbs. What a limb gives up
/// thickens the lower trunk, so the tree weighs the same.
fn limbs_within_trunk(sk: &mut Skeleton, wood: &Wood) {
    let trunk = &wood.chains[0];
    let on: std::collections::BTreeSet<usize> = wood.chains.iter().flatten().copied().collect();
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); sk.nodes.len()];
    for (i, n) in sk.nodes.iter().enumerate() {
        if let Some(p) = n.parent {
            kids[p].push(i);
        }
    }
    let mut removed = 0.0;
    // Every trunk chain, each from the top down so each cap sees the trunk
    // above already settled; the foot they share is capped once.
    let mut seen = std::collections::BTreeSet::new();
    let order: Vec<usize> = wood
        .chains
        .iter()
        .flat_map(|c| c.iter().rev().copied())
        .filter(|&t| seen.insert(t))
        .collect();
    for t in order {
        let flow = sk.flows();
        let limbs: Vec<usize> = kids[t]
            .iter()
            .copied()
            .filter(|c| !on.contains(c))
            .collect();
        // Radius follows the cube root of flow, so a third of it is 1/27 of
        // the flow; measured on the trunk without its limbs, since cutting a
        // limb thins the trunk it leaves.
        let bare = flow[t] - limbs.iter().map(|&c| flow[c]).sum::<f64>();
        let cap = bare / 26.0;
        for limb in limbs {
            if flow[limb] <= cap {
                continue;
            }
            let s = cap / flow[limb];
            let mut stack = vec![limb];
            while let Some(n) = stack.pop() {
                removed += sk.nodes[n].own * (1.0 - s);
                sk.nodes[n].own *= s;
                stack.extend_from_slice(&kids[n]);
            }
        }
    }
    let low = &trunk[..(trunk.len() / 2).max(1)];
    for &n in low {
        sk.nodes[n].own += removed / low.len() as f64;
    }
}

/// Root flow of a seedling, per lifetime contribution; an older tree never
/// carries less.
pub(crate) const SEEDLING_FLOW: f64 = 1.02;
/// The most of a tree's girth its dead wood may take, so the living trunk is
/// always the heavier one.
const DEAD_SHARE: f64 = 0.35;
/// The seedling's height at the end of its first year.
pub(crate) const SEEDLING_TOP: f64 = 80.0;
/// How much more a whole formal upright's trunk carries after many steady
/// years, as a share of its flow.
const GIRTH_K: f64 = 2.0;
/// What the root flare adds to a tree's flow.
const FLARE: f64 = 1.21;

/// Grows the whole skeleton for `timeline`: a seedling in the first year,
/// the exhibition maple after. `periods` declare languages for private work.
pub fn grow(
    timeline: &Timeline,
    seed: u64,
    leader: &Leader,
    periods: &[(Date, Date, Vec<String>)],
    past: Reach,
    recent: Option<Recent>,
) -> Skeleton {
    let mut sk = root_only();
    let Some(life) = Life::of(timeline, periods) else {
        return sk;
    };
    if life.is_seedling() {
        crate::growth::seedling::seedling(&mut sk, &life, seed, leader.side);
        sk.dormant = life.dormant();
    } else {
        // Every tree grows on the right; one on the left
        // ([`Leader::side`]) is its mirror image.
        tree(&mut sk, &life, seed, &leader.canonical(), past, recent);
        // Grown from the same work, each style comes out a little nearer
        // the others' size.
        sk.scale(crate::style::size_factor(leader));
        if leader.side < 0.0 {
            sk.mirror();
        }
    }
    sk
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::growth::testkit::snap;

    /// Every fixture life old enough for the dome, with its seed.
    fn dome_lives() -> Vec<(String, Life, u64)> {
        crate::fixtures::all()
            .into_iter()
            .filter_map(|f| {
                let life = Life::of(&Timeline::new(&f.snapshot), &[])?;
                (!life.is_seedling()).then(|| (f.name.to_string(), life, f.snapshot.account_id))
            })
            .collect()
    }

    /// The main trunk, and the first strand a language claimed as a second
    /// trunk; dead wood when that language was dropped.
    #[test]
    fn trunk_stage_makes_the_main_trunk_and_the_first_claimed_strand() {
        let leader = Leader::new(crate::config::Style::Moyogi, None, 1.0);
        let mut seen = std::collections::BTreeSet::new();
        for (name, life, seed) in dome_lives() {
            let c = Ctx::new(&life, seed, 0.0);
            let (_, claimed) = crate::growth::trunks::strands(&life);
            let mut sk = root_only();
            let wood = grow_trunks(&mut sk, &c, &leader, &mut Rng::keyed(seed, "bdome"));
            let want = 1 + usize::from(!claimed.is_empty());
            assert_eq!(wood.chains.len(), want, "{name}");
            assert_eq!(wood.sides.len(), want, "{name}");
            assert_eq!(
                wood.dead_b,
                claimed.first().is_some_and(|st| life.dropped(&st.lang)),
                "{name}"
            );
            assert!(wood.chains.iter().all(|ch| ch.len() > wood.foot), "{name}");
            seen.insert(want);
        }
        assert_eq!(
            seen.len(),
            2,
            "the fixtures hold single trunks and claimed strands"
        );
    }

    /// In a full wind a branch that ran out level upwind streams out
    /// downwind, never left standing straight up beside a leaning trunk.
    #[test]
    fn a_level_upwind_branch_streams_out_downwind() {
        let mut sk = root_only();
        let node = |p: P, parent: usize, lang: Option<&str>| crate::growth::Node {
            p,
            parent: Some(parent),
            own: 0.0,
            language: lang.map(str::to_string),
            bleach: 0.0,
            strand: false,
        };
        sk.nodes.push(node((CENTER_X, 380.0), 0, None));
        sk.nodes.push(node((CENTER_X, 360.0), 1, None));
        sk.nodes
            .push(node((CENTER_X - 20.0, 360.0), 2, Some("Rust")));
        sk.nodes
            .push(node((CENTER_X - 40.0, 362.0), 3, Some("Rust")));
        let wood = Wood {
            chains: vec![vec![0, 1, 2]],
            sides: vec![0.0],
            foot: 0,
            s_a: 0.0,
            center: CENTER_X,
            dead_b: false,
            dead_g: 0.0,
            sweep: 1.0,
            peel_b: 0.0,
            months_b: 0,
            free_b: 0,
            cascade: 0.0,
            downwind: 1.0,
            tier: 0.0,
        };
        wind_branches(&mut sk, &wood, &[]);
        let (a, tip) = (sk.nodes[2].p, sk.nodes[4].p);
        let rise = math::atan2(a.1 - tip.1, tip.0 - a.0);
        assert!(tip.0 > a.0, "the branch still points upwind: {tip:?}");
        assert!(
            rise <= STREAM_RISE + 1e-9,
            "the branch stands {:.0} degrees up",
            math::deg(rise)
        );
    }

    /// A cloud out on its own bare arm, well clear of the rest of a windswept
    /// crown, comes in to the crown, its arm with it.
    #[test]
    fn a_lone_tuft_comes_in_to_the_crown() {
        let mut sk = root_only();
        let node = |p: P, parent: usize| crate::growth::Node {
            p,
            parent: Some(parent),
            own: 0.0,
            language: Some("Rust".to_string()),
            bleach: 0.0,
            strand: false,
        };
        // A trunk, a low limb carrying four clouds side by side, and a long
        // bare arm reaching up to one cloud far above them.
        sk.nodes.push(node((CENTER_X, 360.0), 0));
        sk.nodes.push(node((CENTER_X, 330.0), 1));
        for k in 1..=4 {
            sk.nodes.push(node(
                (CENTER_X + 20.0 * f64::from(k), 330.0),
                sk.nodes.len() - 1,
            ));
        }
        let fork = 3;
        let mut at = fork;
        for k in 1..=4 {
            sk.nodes
                .push(node((CENTER_X + 20.0, 330.0 - 20.0 * f64::from(k)), at));
            at = sk.nodes.len() - 1;
        }
        let cloud = |c: P| crate::growth::Cloud {
            c,
            rx: 10.0,
            ry: 6.0,
            ..crate::growth::Cloud::default()
        };
        sk.clouds = (1..=4)
            .map(|k| cloud((CENTER_X + 20.0 * f64::from(k), 326.0)))
            .collect();
        sk.clouds.push(cloud(sk.nodes[at].p));
        let wood = Wood {
            chains: vec![vec![0, 1, 2]],
            sides: vec![0.0],
            foot: 0,
            s_a: 0.0,
            center: CENTER_X,
            dead_b: false,
            dead_g: 0.0,
            sweep: 1.0,
            peel_b: 0.0,
            months_b: 0,
            free_b: 0,
            cascade: 0.0,
            downwind: 1.0,
            tier: 0.0,
        };
        let tip = sk.nodes[at].p;
        no_lone_tuft(&mut sk, &wood);
        let (lone, below) = (&sk.clouds[4], &sk.clouds[0]);
        let gap = (below.c.1 - below.ry) - (lone.c.1 + lone.ry);
        assert!(gap <= LONE_KEEP + 1e-6, "still {gap:.1} above the crown");
        assert!(sk.nodes[at].p.1 > tip.1, "its arm came with it");
        assert_eq!(sk.nodes[fork].p, (CENTER_X + 20.0, 330.0), "the limb stays");
    }

    #[test]
    fn crown_stage_gives_every_crown_language_a_sector() {
        for (name, life, seed) in dome_lives() {
            let c = Ctx::new(&life, seed, 0.0);
            let (arms, _) = crown_arms(&c, &BTreeMap::new());
            let sectors = Sectors::of(&c, &arms);
            let b = &sectors.bounds;
            assert_eq!(b.len(), c.plan.upper.len() + 1, "{name}");
            assert!(
                b[0] == 0.0 && (b[b.len() - 1] - 1.0).abs() < 1e-9,
                "{name}: {b:?}"
            );
            for (i, (_, l)) in c.plan.upper.iter().enumerate() {
                assert!(b[i + 1] > b[i], "{name}: an empty sector");
                assert_eq!(&sectors.lang_at(f64::midpoint(b[i], b[i + 1])), l, "{name}");
            }
            // Every crown arm belongs to a crown language.
            assert!(arms.iter().all(|a| c.sector_of(&a.lang) > 0.0), "{name}");
        }
    }

    /// Years of private work with one public contribution, in a language
    /// later dropped, keep a living limb: private work is never dropped.
    #[test]
    fn a_private_era_with_one_dropped_contribution_stays_alive() {
        use crate::date::Date;
        let d = |s: &str| -> Date { s.parse().unwrap() };
        let mut days: Vec<(Date, u32, &[(&str, u32)])> = Vec::new();
        let mut day = d("2018-04-14");
        while day < d("2024-01-01") {
            // Private only, but for one Python contribution in 2021.
            let langs: &[(&str, u32)] = if day == d("2021-03-02") {
                &[("Python", 1)]
            } else {
                &[]
            };
            days.push((day, 4, langs));
            day = day.add_days(3);
        }
        while day < d("2026-11-17") {
            days.push((day, 9, &[("Rust", 6)]));
            day = day.add_days(1);
        }
        let snapshot = crate::growth::testkit::snap(&days, d("2026-11-17"));
        let timeline = Timeline::new(&snapshot);
        let life = Life::of(&timeline, &[]).unwrap();
        let mut sk = root_only();
        let leader = Leader::new(crate::config::Style::Moyogi, None, 1.0);
        tree(&mut sk, &life, 1, &leader, Reach::default(), None);
        let flow = sk.flows();
        let dead = (0..sk.nodes.len())
            .filter(|&i| sk.nodes[i].language.as_deref() == Some(crate::growth::JIN))
            .map(|i| flow[i])
            .fold(0.0, f64::max);
        assert!(dead < 50.0, "a private era died: dead wood carries {dead}");
    }

    /// No limb is more than a third of the trunk where it leaves, as on a
    /// bonsai, however much heavier its era was than the crown above.
    #[test]
    fn no_limb_outgrows_the_trunk_it_leaves() {
        for (name, life, seed) in dome_lives() {
            let mut sk = root_only();
            let leader = Leader::new(crate::config::Style::Moyogi, None, 1.0);
            let chains = tree(&mut sk, &life, seed, &leader, Reach::default(), None);
            let flow = sk.flows();
            let on: std::collections::BTreeSet<usize> = chains.iter().flatten().copied().collect();
            for &t in &on {
                for (i, n) in sk.nodes.iter().enumerate() {
                    if n.parent == Some(t) && !on.contains(&i) {
                        assert!(
                            flow[i] <= flow[t] / 27.0 + 1e-6,
                            "{name}: a limb carries {:.0} where the trunk carries {:.0}",
                            flow[i],
                            flow[t]
                        );
                    }
                }
            }
        }
    }

    /// A formal upright's triangle stands about as tall as it is wide, never
    /// more than 5% wider.
    #[test]
    fn a_formal_uprights_crown_is_about_as_tall_as_it_is_wide() {
        for big_s in [88.0, 119.0, 188.0] {
            let k = Cone::of(big_s, 40.0, 0.5);
            assert!(
                1.05 * k.h_top >= 2.0 * k.w_b - 1e-9,
                "{big_s}: {:.0} tall, {:.0} wide",
                k.h_top,
                2.0 * k.w_b
            );
        }
    }

    /// A formal upright's extra girth is not all in its nebari: a share of
    /// it runs up the trunk, so the trunk tapers less.
    #[test]
    fn a_formal_uprights_girth_runs_up_its_trunk() {
        let mut sk = root_only();
        let lang: Lang = Some("C".to_string());
        let mut main = vec![0];
        for i in 1..88 {
            let p = *main.last().unwrap_or(&0);
            main.push(crate::growth::wood::add(
                &mut sk,
                (CENTER_X, SOIL_Y - f64::from(i)),
                0.0,
                p,
                &lang,
            ));
        }
        let laid = |tier: f64| {
            let mut sk = sk.clone();
            lay_trunk_extra(&mut sk, &main, 88.0, tier);
            sk.flows()
        };
        let flow = laid(1.0);
        let total = flow[0];
        assert!((total - 88.0).abs() < 3.0, "{total:.1} laid of 88");
        let at = flow[main[53]];
        assert!(
            at >= 8.8,
            "only {at:.1} of 88 carried above 60% of the trunk"
        );
        // A tree no longer formal upright keeps its old girth low: none of
        // it runs up a leader that is not a formal upright's.
        let flow = laid(0.0);
        assert!(
            flow[main[53]] < 1e-9,
            "{:.1} carried up a tree no longer formal",
            flow[main[53]]
        );
    }

    /// A language dropped long ago, a small part of its busy years, still
    /// leaves a bleached twig on its era's limb, fan and all.
    #[test]
    fn a_small_language_dropped_long_ago_leaves_a_dead_twig() {
        use crate::date::Date;
        let d = |s: &str| -> Date { s.parse().unwrap() };
        let mut days: Vec<(Date, u32, &[(&str, u32)])> = Vec::new();
        let mut day = d("2010-01-04");
        while day < d("2014-01-01") {
            days.push((day, 12, &[("JavaScript", 11), ("C", 1)]));
            day = day.add_days(1);
        }
        while day < d("2026-11-17") {
            days.push((day, 8, &[("JavaScript", 4), ("Go", 4)]));
            day = day.add_days(1);
        }
        let snapshot = crate::growth::testkit::snap(&days, d("2026-11-17"));
        let life = Life::of(&Timeline::new(&snapshot), &[]).unwrap();
        let mut sk = root_only();
        let leader = Leader::new(crate::config::Style::Moyogi, None, 1.0);
        let chains = tree(&mut sk, &life, 1, &leader, Reach::default(), None);
        let on: std::collections::BTreeSet<usize> = chains.iter().flatten().copied().collect();
        let jin = |i: usize| sk.nodes[i].language.as_deref() == Some(crate::growth::JIN);
        // A dead twig: dead wood growing from a living limb, not the trunk.
        let twigs: Vec<usize> = (0..sk.nodes.len())
            .filter(|&i| {
                jin(i)
                    && sk.nodes[i]
                        .parent
                        .is_some_and(|p| !jin(p) && !on.contains(&p))
            })
            .collect();
        let fan = |t: usize| {
            (0..sk.nodes.len())
                .filter(|&i| {
                    let mut n = i;
                    while let Some(p) = sk.nodes[n].parent {
                        if p == t {
                            return true;
                        }
                        n = p;
                    }
                    false
                })
                .count()
        };
        assert!(
            twigs.iter().any(|&t| fan(t) >= 4),
            "no dead twig with a fan: {twigs:?}"
        );
    }

    /// A whole era in a dropped language is an unmistakable dead limb, not a
    /// stub; on a single trunk the eras alternate sides.
    #[test]
    fn a_dropped_era_is_a_whole_dead_limb() {
        let snapshot = crate::fixtures::by_name("showcase").unwrap().snapshot;
        let timeline = Timeline::new(&snapshot);
        let life = Life::of(&timeline, &[]).unwrap();
        let mut sk = root_only();
        let leader = Leader::new(crate::config::Style::Moyogi, None, 1.0);
        let chains = tree(
            &mut sk,
            &life,
            snapshot.account_id,
            &leader,
            Reach::default(),
            None,
        );
        let flow = sk.flows();
        let jin: Vec<usize> = (0..sk.nodes.len())
            .filter(|&i| sk.nodes[i].language.as_deref() == Some(crate::growth::JIN))
            .collect();
        let thickest = jin.iter().map(|&i| flow[i]).fold(0.0, f64::max);
        assert!(thickest >= 250.0, "dead limb carries {thickest}");
        let reach = jin
            .iter()
            .map(|&i| (sk.nodes[i].p.0 - CENTER_X).abs())
            .fold(0.0, f64::max);
        assert!(reach >= 45.0, "dead limb reaches {reach}");
        // C# founded the trunk and was dropped long ago: a shari, a strip of
        // dead wood up the living trunk, all but fully bleached.
        assert!(
            sk.shari.is_some_and(|g| g >= 0.9),
            "the dropped founder leaves {:?}",
            sk.shari
        );
        // Era limbs leave the trunk on both sides.
        let trunk: std::collections::BTreeSet<usize> = chains[0].iter().copied().collect();
        let sides: std::collections::BTreeSet<bool> = (0..sk.nodes.len())
            .filter(|&i| {
                sk.nodes[i].parent.is_some_and(|p| trunk.contains(&p)) && !trunk.contains(&i)
            })
            .filter(|&i| SOIL_Y - sk.nodes[i].p.1 < 70.0)
            .map(|i| sk.nodes[i].p.0 > sk.nodes[sk.nodes[i].parent.unwrap()].p.0)
            .collect();
        assert_eq!(sides.len(), 2, "low limbs on both sides");
    }

    #[test]
    fn three_trunk_languages_stay_apart() {
        let start: crate::date::Date = "2021-01-04".parse().unwrap();
        let days: Vec<_> = (0..900)
            .map(|i| {
                let l = ["Go", "Rust", "Python"][usize::try_from(i % 3).unwrap()];
                (start.add_days(i), 3, vec![(l, 3)])
            })
            .collect();
        let days: Vec<_> = days.iter().map(|(d, t, l)| (*d, *t, &l[..])).collect();
        let timeline = Timeline::new(&snap(&days, start.add_days(900)));
        let life = Life::of(&timeline, &[]).unwrap();
        let mut sk = root_only();
        let leader = Leader::new(crate::config::Style::Moyogi, None, 1.0);
        let chains = tree(&mut sk, &life, 1, &leader, Reach::default(), None);
        assert_eq!(chains.len(), 2, "a third trunk language shares a trunk");
        let top = |c: &Vec<usize>| sk.nodes[*c.last().unwrap()].p.0;
        assert!((top(&chains[0]) - top(&chains[1])).abs() > 10.0);
    }

    /// A bare windswept crown branches out twig from twig: its fans never
    /// all hang from one node of an arm the crown streamed away from, which
    /// reads as a comb.
    #[test]
    fn a_bare_windswept_crown_never_fans_from_one_node() {
        let leader = Leader::new(crate::config::Style::Fukinagashi, None, 1.0);
        let mut seen = 0;
        for (name, life, seed) in dome_lives().into_iter().filter(|l| l.1.dormant()) {
            let mut sk = root_only();
            let chains = tree(&mut sk, &life, seed, &leader, Reach::default(), None);
            let trunk: std::collections::BTreeSet<usize> =
                chains.iter().flatten().copied().collect();
            let mut kids = vec![0usize; sk.nodes.len()];
            for n in &sk.nodes {
                if let Some(p) = n.parent.filter(|p| !trunk.contains(p)) {
                    kids[p] += 1;
                }
            }
            let most = kids.iter().max().copied().unwrap_or(0);
            assert!(most <= 5, "{name}: {most} twigs leave one node");
            seen += 1;
        }
        assert!(seen > 0, "the fixtures hold a dormant tree");
    }
}
