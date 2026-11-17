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

use crate::config::Style;
use crate::date::Date;
use crate::growth::clouds::{cloud, merge_clouds, pad};
use crate::growth::life::{FULL_YEAR, Lang, Life, Month, Year, decay, irr, months_cv, shares_of};
use crate::growth::trunks::{Trunk, trunks};
use crate::growth::wood::{
    Limb, PI, at_frac, at_height, dead_limb, dead_trunk, flare, jin_limb, limb_to, root_only,
    trunk3, twin, winter_fan, zof,
};
use crate::growth::{GOLDEN, P, Skeleton};
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
struct YearPlan {
    upper: Vec<(usize, Lang)>,
    crown: Vec<usize>,
    eras: Vec<Vec<usize>>,
    pause_m: i32,
}

fn plan_years(life: &Life, trunks: &[Trunk]) -> YearPlan {
    let ny = life.years.len();
    let mut upper: Vec<(usize, Lang)> = trunks
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.dropped)
        .map(|(k, t)| (k, t.lang.clone()))
        .collect();
    if upper.is_empty() {
        upper.push((0, trunks[0].lang.clone()));
    }
    upper.truncate(3);
    let up_share = |upper: &[(usize, Lang)], y: &Year| -> f64 {
        shares_of(&y.months, y.total)
            .iter()
            .filter(|s| upper.iter().any(|u| u.1 == s.0))
            .map(|s| s.1)
            .sum::<f64>()
            / y.total.max(1e-9)
    };

    // A stray day or two is not a year of growth.
    let act: Vec<usize> = {
        let a: Vec<usize> = (0..ny).filter(|&i| life.is_active_year(i)).collect();
        if a.is_empty() {
            (0..ny).filter(|&i| life.years[i].days > 0).collect()
        } else {
            a
        }
    };
    // The crown is the newest run of years led by the crown languages...
    let mut d0 = act.len() - 1;
    while d0 > 0 {
        let (a, b) = (&life.years[act[d0 - 1]], &life.years[act[d0]]);
        if b.y - a.y > 1 || up_share(&upper, a) < 0.5 {
            break;
        }
        d0 -= 1;
    }
    // ...at most the newest six years, so a long steady life still shows its
    // age below...
    if act.len() - d0 > 6 {
        d0 = act.len() - 6;
    }
    // ...and at least the last three years of a long life.
    while d0 > 0 && act.len() - d0 < 3 && act.len() > 4 {
        let (a, b) = (&life.years[act[d0 - 1]], &life.years[act[d0]]);
        if b.y - a.y > 1 {
            break;
        }
        d0 -= 1;
    }
    let crown: Vec<usize> = act[d0..].to_vec();
    // When the trunk languages have left the crown, its own lead language
    // crowns the main trunk.
    let mass: f64 = crown
        .iter()
        .map(|&i| life.years[i].total)
        .sum::<f64>()
        .max(1e-9);
    let up_mass: f64 = crown
        .iter()
        .map(|&i| up_share(&upper, &life.years[i]) * life.years[i].total)
        .sum();
    // The crown holds living languages only: a dropped one is deadwood.
    let gone = |l: &Lang| l.as_deref().is_some_and(|l| life.dropped(l));
    let (ms, _) = months_of(life, &crown);
    let living: Vec<(Lang, f64)> = shares_of(&ms, mass)
        .into_iter()
        .filter(|(l, _)| l.is_some() && !gone(l))
        .collect();
    if up_mass / mass < 0.3
        && let Some((l, _)) = living.first().cloned()
        && !upper.iter().any(|u| u.1 == l)
    {
        upper.insert(0, (0, l));
        upper.truncate(3);
    }
    // A language with 18% or more of the crown years owns a sector of the
    // crown too, riding the main trunk.
    for (l, v) in living {
        if upper.len() >= 3 {
            break;
        }
        if v / mass >= 0.18 && !upper.iter().any(|u| u.1 == l) {
            upper.push((0, l));
        }
    }
    // Every trunk dropped: a living crown language takes the main trunk.
    if upper.iter().any(|u| !gone(&u.1)) {
        upper.retain(|u| !gone(&u.1));
    }
    let old: Vec<usize> = act[..d0].to_vec();
    let pause_m = old.last().map_or(0, |&i| {
        let a = span_of(&life.years[i]).1;
        let b = span_of(&life.years[crown[0]]).0;
        (b - a - 1).max(0)
    });
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
    // A run too small to carry a limb joins its neighbour.
    let all_total: f64 = life.years.iter().map(|y| y.total).sum::<f64>().max(1.0);
    while eras.len() > 1 {
        let Some(i) = eras.iter().position(|e| {
            let t: f64 = e.iter().map(|&i| life.years[i].total).sum();
            let d: usize = e.iter().map(|&i| life.years[i].days).sum();
            t < 0.01 * all_total && d < 12
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
}

impl Frame {
    fn of(life: &Life, plan: &YearPlan) -> Self {
        let age = life.age();
        let ny = life.years.len();
        let y_last = life.years[ny - 1].y;
        let recent: f64 = plan
            .crown
            .iter()
            .map(|&i| life.years[i].total * decay(f64::from(y_last - life.years[i].y)))
            .sum();
        let fill = wt(recent / 3.0).min(1.2);
        let big_s = (40.0 + 29.0 * math::ln(1.0 + ny as f64))
            * (0.86 + 0.16 * fill)
            * (0.86 + 0.014 * age.min(18.0));
        let cell = (big_s * 0.2).clamp(13.0, 31.0);
        let n_old = plan.eras.len();
        // Many eras sit closer together: the tree never grows tall from them.
        let gap_h = if n_old > 1 {
            (30.0 / (n_old - 1) as f64).min(15.0)
        } else {
            15.0
        };
        let window = if n_old > 0 {
            14.0 + 22.0 * (f64::from(plan.pause_m) / 12.0).min(1.0)
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
        };
        if n_old > 0 {
            frame.h_vis = h_clear.max(frame.limb_h(n_old - 1) + 12.0 + window + cell * 0.55);
        }
        frame
    }

    /// Height above the soil of era limb `i`, oldest lowest.
    fn limb_h(&self, i: usize) -> f64 {
        28.0 + self.gap_h * i as f64
    }
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
    trunks: Vec<Trunk>,
    plan: YearPlan,
    frame: Frame,
    age: f64,
    y_first: i32,
    y_last: i32,
    /// The main trunk's side: -1 or 1.
    s_a: f64,
    dormant: bool,
    /// The dome's flat bottom.
    y_b: f64,
}

impl<'a> Ctx<'a> {
    fn new(life: &'a Life, seed: u64) -> Self {
        let trunks = trunks(life);
        let plan = plan_years(life, &trunks);
        let frame = Frame::of(life, &plan);
        let ny = life.years.len();
        Self {
            life,
            seed,
            age: life.age(),
            y_first: life.years[0].y,
            y_last: life.years[ny - 1].y,
            s_a: Rng::keyed(seed, "bdome/side").sign(),
            dormant: life.dormant(),
            y_b: SOIL_Y - frame.h_vis,
            trunks,
            plan,
            frame,
        }
    }

    /// Whether `l` is a crown language.
    fn is_upper(&self, l: &Lang) -> bool {
        self.plan.upper.iter().any(|u| &u.1 == l)
    }

    /// Whether `l` was dropped.
    fn gone(&self, l: &Lang) -> bool {
        l.as_deref().is_some_and(|l| self.life.dropped(l))
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
}

impl Wood {
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
                    .map(|a| a.vd)
                    .sum::<f64>()
            })
            .collect();
        let mut widths: Vec<f64> = mass.iter().map(|m| math::pow(m.max(1.0), 0.6)).collect();
        let wsum: f64 = widths.iter().sum::<f64>().max(1e-9);
        for w in &mut widths {
            *w = (*w / wsum).max(if upper.len() > 1 { 0.3 } else { 0.0 });
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
}

impl Dome {
    /// The dome's radius at angle `th` and how densely arms crowd there.
    fn envelope(&self, th: f64) -> (f64, f64) {
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

    /// The point at angle `th` and radius `r` on the dome's face.
    fn pt(&self, th: f64, r: f64) -> P {
        (
            self.center + r * math::cos(th),
            self.y_b - self.vv * r * math::sin(th),
        )
    }
}

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
pub fn tree(sk: &mut Skeleton, life: &Life, seed: u64, leader: &Leader) -> Vec<Vec<usize>> {
    let c = Ctx::new(life, seed);
    let mut rng = Rng::keyed(seed, "bdome");
    sk.dormant = c.dormant;
    let wood = grow_trunks(sk, &c, leader, &mut rng);
    grow_eras(sk, &c, &wood);
    let (arms, lower, jins) = crown_arms(&c);
    let sectors = Sectors::of(&c, &arms);
    let (placed, dome) = place_arms(&c, &wood, &arms, &sectors);
    let by_lang = grow_arms(sk, &c, &wood, &dome, &arms, &placed);
    grow_skirt(sk, &c, &wood, &dome, &lower);
    crown_deadwood(sk, &c, &wood, &mut rng, &jins);
    dome_clouds(
        sk,
        &c,
        &wood,
        &dome,
        &sectors,
        &by_lang,
        arms.len(),
        &mut rng,
    );
    girth(sk, &c, &wood);
    wood.chains
}

/// Trunks: one nebari, a V of two; dropped trunk languages as snags,
/// dropped languages as stubs; the apex and height.
fn grow_trunks(sk: &mut Skeleton, c: &Ctx, leader: &Leader, rng: &mut Rng) -> Wood {
    let (life, s_a, upper) = (c.life, c.s_a, &c.plan.upper);
    let Frame { big_s, vv, .. } = c.frame;
    let h_vis = c.frame.h_vis;
    let downwind = if leader.x_at(150.0) < 0.0 { -1.0 } else { 1.0 };
    let (amp_k, lean) = match leader.style {
        Style::Chokkan => (0.4, 0.0),
        Style::Moyogi => (1.0, 0.0),
        Style::Shakan => (1.0, 0.25 * downwind),
        Style::Fukinagashi => (1.0, 0.35 * downwind),
    };
    let c_irr = irr(life.life_cv);
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
    let living_twin = {
        let mut ks: Vec<usize> = upper.iter().map(|u| u.0).collect();
        ks.dedup();
        ks.len() > 1
    };
    let h_a = h_vis + vv * big_s * 0.32;
    let mut chains: Vec<Vec<usize>> = Vec::new();
    let mut sides = vec![s_a];
    if living_twin {
        let d = big_s * (0.42 + 0.022 * c.age.min(16.0));
        let ub = upper
            .iter()
            .find(|u| u.0 != upper[0].0)
            .cloned()
            .unwrap_or_else(|| upper[0].clone());
        let share_b = c.trunks[ub.0].share / c.trunks[upper[0].0].share;
        let h_b = h_a * (0.84 + 0.12 * share_b.sqrt().min(1.0));
        let mut a = foot_a.clone();
        a.extend(
            twin(
                sk,
                base,
                h_a - 6.0,
                s_a,
                3.0,
                d * 0.45,
                amp,
                freq,
                ph,
                0.0,
                &None,
            )
            .into_iter()
            .skip(1),
        );
        let mut b = foot_a.clone();
        b.extend(
            twin(
                sk,
                base,
                h_b - 6.0,
                -s_a,
                3.0,
                d * 0.55,
                amp * 0.85,
                freq * 1.1,
                ph + 1.3,
                -6.0,
                &ub.1,
            )
            .into_iter()
            .skip(1),
        );
        chains.push(a);
        chains.push(b);
        sides.push(-s_a);
    } else {
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
            )
            .into_iter()
            .skip(1),
        );
        chains.push(a);
    }
    // Dropped trunk languages stand as bleached snags beside the trunk.
    for (k, t) in c.trunks.iter().enumerate() {
        if t.dropped && !upper.iter().any(|u| u.0 == k) {
            let from = at_frac(&chains[0], 0.06);
            let w = Limb::new(from, if s_a > 0.0 { PI } else { 0.0 }, 0.55);
            dead_trunk(sk, rng, w, h_a * 1.25, t.share * life.total * 0.5);
        }
    }
    // Every dropped language (1% or more of the lifetime) leaves a jin stub
    // on the trunk.
    let dropped: Vec<(String, f64)> = life
        .lang
        .iter()
        .filter(|(l, v)| life.dropped(l) && v / life.total >= 0.01)
        .take(3)
        .cloned()
        .collect();
    for (j, (_, v)) in dropped.iter().enumerate() {
        let side = if j % 2 == 0 { -s_a } else { s_a };
        let k = usize::from(sides.len() > 1 && side != s_a);
        let from = at_height(
            sk,
            &chains[k],
            14.0 + 11.0 * j as f64 + 6.0 * (j % 2) as f64,
        );
        let el = if j % 3 == 1 { 0.6 } else { -0.4 };
        let w = Limb::new(from, math::atan2(el, side), 0.5 + 0.1 * j as f64);
        jin_limb(sk, rng, w, 9.0 + 14.0 * (v / life.total).sqrt(), v * 0.06);
    }
    let tops: Vec<P> = chains
        .iter()
        .map(|ch| sk.nodes[*ch.last().unwrap_or(&0)].p)
        .collect();
    let center = tops.iter().map(|p| p.0).sum::<f64>() / tops.len() as f64;
    sk.apex = (center, c.y_b - vv * big_s);
    sk.height = h_vis + vv * big_s;
    Wood {
        chains,
        sides,
        foot: foot_a.len(),
        s_a,
        center,
    }
}

/// Old eras: low limbs, oldest lowest, sides alternating.
fn grow_eras(sk: &mut Skeleton, c: &Ctx, wood: &Wood) {
    let (life, seed, s_a, upper) = (c.life, c.seed, c.s_a, &c.plan.upper);
    let Frame { big_s, cell, .. } = c.frame;
    let n_old = c.plan.eras.len();
    let era_side = |ei: usize, era: &[usize]| -> f64 {
        // On twin trunks an era led by a living trunk language keeps to that
        // trunk's side; on a single trunk the eras alternate.
        let (ms, t) = months_of(life, era);
        let (l, _) = lead(&ms, t);
        if wood.sides.len() > 1
            && let Some(u) = upper.iter().position(|u| u.1 == l && l.is_some())
        {
            return if u == 0 { s_a } else { -s_a };
        }
        if (n_old - 1 - ei).is_multiple_of(2) {
            s_a
        } else {
            -s_a
        }
    };
    for (ei, era) in c.plan.eras.iter().enumerate() {
        let side = era_side(ei, era);
        let k = wood.chain_on(side);
        let from = at_height(sk, &wood.chains[k], c.frame.limb_h(ei));
        let p0 = sk.nodes[from].p;
        let y_end = life.years[*era.last().unwrap_or(&0)].y;
        let ag = f64::from(c.y_last - y_end);
        let era_total: f64 = era.iter().map(|&i| life.years[i].total).sum();
        let named: f64 = era.iter().flat_map(|&i| life.years[i].langs.values()).sum();
        let gone_v: f64 = era
            .iter()
            .flat_map(|&i| life.years[i].langs.iter())
            .filter(|(l, _)| life.dropped(l))
            .map(|(_, v)| v)
            .sum();
        let grown: f64 = era
            .iter()
            .map(|&i| 5.0 + 6.0 * (life.years[i].days as f64 / 60.0).sqrt().min(1.6))
            .sum();
        let len = (16.0 + grown) * (1.0 + 0.08 * ag.min(14.0));
        let len = within(len, 40.0, big_s * 1.05);
        let mut er = Rng::keyed(seed, &format!("bdome/era/{ei}"));
        let ecv = irr(months_cv(&months_of(life, era).0));
        sk.nodes[from].own += era_total * 0.03 * ag.min(12.0);
        let ez = er.range(-8.0, 8.0);
        if named > 0.0 && gone_v / named >= 0.8 {
            let az = if side > 0.0 { 0.25 } else { PI - 0.25 };
            dead_limb(
                sk,
                &mut er,
                Limb::new(from, az, 0.42),
                len * 0.7 + 10.0,
                era_total * 0.25,
            );
            continue;
        }
        let end = (p0.0 + side * len, p0.1 + 0.55 * ag.min(14.0) - 2.0);
        let gnarl = 0.004 * ag.min(14.0);
        let limb = limb_to(
            sk,
            from,
            end,
            ez,
            side * len * 0.05,
            len * (0.03 + 0.05 * ecv + gnarl),
            1.5 + ecv + 2.0 * gnarl / 0.056,
            er.range(0.0, math::TAU),
            &None,
        );
        // Old wood keeps thickening: the limb carries its years.
        if let Some(&mid) = limb.get(limb.len() * 6 / 10) {
            sk.nodes[mid].own +=
                era_total * 0.1 * ag.min(12.0) + 130.0 * era.len() as f64 * ag.min(14.0);
        }
        // The era's dropped languages: bleached limbs rising off it.
        if gone_v / named.max(1e-9) >= 0.15 {
            let w = Limb::new(limb[limb.len() * 2 / 10], math::atan2(0.15, side), 0.85);
            jin_limb(sk, &mut er, w, len * 0.3 + 12.0, gone_v * 0.06);
            let w2 = Limb::new(limb[limb.len() * 55 / 100], math::atan2(-0.3, side), 0.7);
            jin_limb(sk, &mut er, w2, len * 0.15 + 8.0, gone_v * 0.02);
        }
        let n = era.len();
        for (j, &i) in era.iter().enumerate() {
            let y = &life.years[i];
            let yag = f64::from(c.y_last - y.y);
            let t = 0.42 + 0.58 * (j + 1) as f64 / n as f64;
            let at = limb[((limb.len() - 1) as f64 * t) as usize];
            let ap = sk.nodes[at].p;
            let az0 = zof(sk, at);
            let shares = shares_of(&y.months, y.total);
            let ys = y.total.max(1e-9);
            let tw_len = 6.0 + 8.0 * (y.days as f64 / 250.0).sqrt().min(1.2);
            let mut yr = Rng::keyed(seed, &format!("bdome/old/{}", y.y));
            for (slot, (l, v)) in shares
                .iter()
                .filter(|s| s.1 / ys >= 0.12)
                .take(3)
                .enumerate()
            {
                let f = match slot {
                    0 => 0.0,
                    s if s % 2 == 1 => -1.0,
                    _ => 1.0,
                };
                let to = (
                    ap.0 + side * tw_len * (0.5 + 0.2 * f) + yr.range(-2.0, 2.0),
                    ap.1 - tw_len * (0.9 - 0.25 * f64::abs(f)),
                );
                let tz = az0 + f * 9.0 + yr.range(-3.0, 3.0);
                if c.gone(l) {
                    continue;
                }
                let twig = limb_to(
                    sk,
                    at,
                    to,
                    tz,
                    0.0,
                    1.5 + 1.5 * irr(y.cv()),
                    1.2,
                    yr.range(0.0, math::TAU),
                    l,
                );
                let vd = v * decay(yag);
                let rx = within(8.5 + 22.0 * (vd / FULL_YEAR).sqrt(), 8.5, cell * 0.62);
                let mut under = twig;
                under.push(at);
                if c.dormant {
                    winter_fan(
                        sk,
                        &mut yr,
                        &under,
                        (to.0, to.1 - rx * 0.3),
                        rx * 0.8,
                        tz,
                        side,
                        l,
                    );
                } else {
                    pad(sk, &mut yr, l, *v, to, tz, rx, side, &under, from);
                }
                sk.nodes[at].own += v * 0.05 * yag.min(12.0);
            }
        }
    }
}

/// The crown years as arms, one per language and year: the crown
/// languages' arms, the minor languages' (at most two a year) for the skirt,
/// and each year's dropped share for deadwood.
fn crown_arms(c: &Ctx) -> (Vec<Arm>, Vec<Arm>, Vec<f64>) {
    let life = c.life;
    let phi0 = Rng::keyed(c.seed, "bdome/phi").range(0.0, math::TAU);
    let mut arms: Vec<Arm> = Vec::new();
    let mut lower: Vec<Arm> = Vec::new();
    let mut jins: Vec<f64> = Vec::new();
    for &i in &c.plan.crown {
        let y = &life.years[i];
        let ag = f64::from(c.y_last - y.y);
        let az = phi0 + math::rad(f64::from(y.y - c.y_first) * GOLDEN);
        let ys = y.total.max(1e-9);
        let mut gone_v = 0.0;
        let mut n_low = 0;
        for (l, v) in &shares_of(&y.months, y.total) {
            if c.gone(l) {
                gone_v += v;
                continue;
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
            };
            if c.is_upper(l) {
                arms.push(a);
            } else if v / ys >= 0.04 && n_low < 2 {
                n_low += 1;
                lower.push(a);
            }
        }
        if gone_v > 0.15 * y.total {
            jins.push(gone_v);
        }
    }
    (arms, lower, jins)
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
        let y0 = mine
            .iter()
            .map(|&i| arms[i].year)
            .min()
            .unwrap_or(c.y_first);
        let y1 = mine.iter().map(|&i| arms[i].year).max().unwrap_or(c.y_last);
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
        let t_of = |y: i32| {
            if y1 > y0 {
                f64::from(y - y0) / f64::from(y1 - y0)
            } else {
                1.0
            }
        };
        let k = if ui == 0 { 0 } else { wood.chain_on(-s_a) };
        for &i in &mine {
            let a = &arms[i];
            let th = sectors.theta(at_t(t_of(a.year), a.side));
            let r = reach_of(a.days);
            placed.push((i, th, r, k));
            env.push((th, r, a.vd.sqrt() + 5.0));
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
) -> BTreeMap<String, Vec<usize>> {
    let rz = dome.rz;
    let mut by_lang: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let forks = (c.age / 5.0).floor().clamp(0.0, 3.0) as usize;
    for &(i, th, r, k) in placed {
        let a = arms[i].clone();
        let f_lo = wood.rim_frac(k, sk, c.y_b);
        let from = at_frac(
            &wood.chains[k],
            f_lo + (0.98 - f_lo) * math::sin(th).clamp(0.0, 1.0) * 0.9,
        );
        let (er, _) = dome.envelope(th);
        let to = dome.pt(th, er.min(r) * 0.66);
        let tz = rz * 0.42 * a.zdir;
        let p0 = sk.nodes[from].p;
        let len = dist(to, p0);
        let sgn = if to.0 >= p0.0 { 1.0 } else { -1.0 };
        let mut yr = Rng::keyed(c.seed, &format!("bdome/arm/{}/{}", a.year, key(&a.lang)));
        let nodes = limb_to(
            sk,
            from,
            to,
            tz,
            sgn * len * 0.08,
            len * (0.02 + 0.06 * a.irr),
            1.0 + 1.5 * a.irr,
            yr.range(0.0, math::TAU),
            &a.lang,
        );
        if let Some(&tip) = nodes.last() {
            sk.nodes[tip].own += a.v * 0.55;
        }
        sk.nodes[from].own += a.v * 0.2 + a.total * 0.05 * a.ag.min(12.0) * 0.5;
        by_lang
            .entry(key(&a.lang))
            .or_default()
            .extend_from_slice(&nodes[nodes.len() * 45 / 100..]);
        // Ramification: an older tree's arm has forked more often.
        for f in 0..forks {
            let at = nodes[nodes.len() * (40 + 15 * f) / 100];
            let ap = sk.nodes[at].p;
            let dth = if f % 2 == 0 { 0.32 } else { -0.32 } * (1.0 + 0.3 * f as f64);
            let (er2, _) = dome.envelope(th + dth);
            let tgt = dome.pt(th + dth, er2.min(r) * yr.range(0.5, 0.68));
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
                &a.lang,
            );
            if let Some(&tip) = tw.last() {
                sk.nodes[tip].own += a.v * 0.06;
            }
            by_lang
                .entry(key(&a.lang))
                .or_default()
                .extend_from_slice(&tw[tw.len() / 2..]);
        }
    }
    by_lang
}

/// The dome's skirt: minor languages and private work on short limbs just
/// under the rim, each keeping to one side.
fn grow_skirt(sk: &mut Skeleton, c: &Ctx, wood: &Wood, dome: &Dome, lower: &[Arm]) {
    let (s_a, th_lo, cell) = (c.s_a, c.frame.th_lo, c.frame.cell);
    let mut lmass: BTreeMap<String, f64> = BTreeMap::new();
    for a in lower {
        *lmass.entry(key(&a.lang)).or_default() += a.vd;
    }
    let mut lorder: Vec<(String, f64)> = lmass.into_iter().collect();
    lorder.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let ly0 = lower.iter().map(|a| a.year).min().unwrap_or(c.y_last);
    for a in lower {
        let rank = lorder.iter().position(|x| x.0 == key(&a.lang)).unwrap_or(0);
        let side = if rank % 2 == 0 { -s_a } else { s_a };
        let k = wood.chain_on(side);
        let f_lo = wood.rim_frac(k, sk, c.y_b);
        let from = at_frac(&wood.chains[k], (f_lo - 0.02).max(0.0));
        let p0 = sk.nodes[from].p;
        let t = f64::from(a.year - ly0) / f64::from((c.y_last - ly0).max(1));
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
        let (er, _) = dome.envelope(th);
        let q = dome.pt(th, er * if a.lang.is_none() { 0.8 } else { 0.84 });
        let to = (q.0, q.1 + if a.lang.is_none() { 12.0 } else { 6.0 });
        let tz = dome.rz * 0.5 * a.zdir;
        let mut yr = Rng::keyed(c.seed, &format!("bdome/low/{}/{}", a.year, key(&a.lang)));
        let len = (to.0 - p0.0).abs();
        let mut under = limb_to(
            sk,
            from,
            to,
            tz,
            side * len * 0.06,
            2.0 + 2.0 * a.irr,
            1.0 + a.irr,
            yr.range(0.0, math::TAU),
            &a.lang,
        );
        sk.nodes[from].own += a.v * 0.3;
        let rx = within(7.0 + 30.0 * (a.vd / FULL_YEAR).sqrt(), 7.0, cell * 0.85);
        under.push(from);
        if c.dormant {
            winter_fan(
                sk,
                &mut yr,
                &under,
                (to.0, to.1 - rx * 0.3),
                rx * 0.8,
                tz,
                side,
                &a.lang,
            );
        } else {
            pad(sk, &mut yr, &a.lang, a.v, to, tz, rx, side, &under, from);
        }
    }
    merge_clouds(sk);
}

/// Deadwood of the crown years: a jin limb for each year that dropped
/// languages.
fn crown_deadwood(sk: &mut Skeleton, c: &Ctx, wood: &Wood, rng: &mut Rng, jins: &[f64]) {
    for (j, amount) in jins.iter().enumerate() {
        let side = if j % 2 == 0 { -c.s_a } else { c.s_a };
        let k = wood.chain_on(side);
        let f_lo = wood.rim_frac(k, sk, c.y_b);
        let from = at_frac(&wood.chains[k], f_lo * (0.9 - 0.1 * j as f64));
        let w = Limb::new(from, math::atan2(0.35, side), 0.2 + 0.12 * j as f64);
        let len = (18.0 + 8.0 * (j as f64).sqrt()).min(c.frame.big_s * 0.4);
        jin_limb(sk, rng, w, len, amount * 0.15);
    }
}

/// The dome: rim, front, back and crown rows of clouds, with a few front
/// windows; bare twig fans in winter.
fn dome_clouds(
    sk: &mut Skeleton,
    c: &Ctx,
    wood: &Wood,
    dome: &Dome,
    sectors: &Sectors,
    by_lang: &BTreeMap<String, Vec<usize>>,
    n_arms: usize,
    rng: &mut Rng,
) {
    let (seed, s_a) = (c.seed, c.s_a);
    let Frame {
        big_s, th_lo, cell, ..
    } = c.frame;
    let all_nodes: Vec<usize> = by_lang.values().flatten().copied().collect();
    let trunk_tops: Vec<usize> = wood
        .chains
        .iter()
        .flat_map(|ch| ch[ch.len() * 7 / 10..].to_vec())
        .collect();
    let mut rim_angles = vec![th_lo];
    let mut acc = 0.0;
    let mut last = dome.pt(th_lo, dome.envelope(th_lo).0);
    let spacing = cell * 1.25;
    for q in 1..=240 {
        let th = th_lo + (PI - 2.0 * th_lo) * f64::from(q) / 240.0;
        let p = dome.pt(th, dome.envelope(th).0);
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
    // (angle, radius factor, depth factor, size factor)
    let mut cells: Vec<(f64, f64, f64, f64)> = rim_angles
        .iter()
        .map(|th| (*th, 1.0, rng.range(-0.15, 0.15), 1.0))
        .collect();
    let nf = ((rim_angles.len() as f64) * 0.55).round().max(3.0) as usize;
    for q in 0..nf {
        let th = 0.42 + (PI - 0.84) * (q as f64 + 0.5) / nf as f64;
        cells.push((th, 0.62, 0.55, 0.88));
        cells.push((th + 0.15, 0.6, -0.55, 0.92));
    }
    cells.push((PI / 2.0 + rng.range(-0.15, 0.15), 0.3, 0.1, 1.1));
    // Windows: a few front cells left open, where the branches show, and
    // the back cell behind each; more on a big crown, so it never reads as
    // one solid mass.
    let mut skip = vec![false; cells.len()];
    let n_win = 1 + usize::from(n_arms >= 6) + usize::from(big_s > 125.0);
    let front: Vec<usize> = (0..cells.len()).filter(|&i| cells[i].2 > 0.4).collect();
    let mut made = 0;
    let mut q = 0i64;
    while made < n_win && q < 40 {
        let pick = front[(hash01(seed ^ 0x3a1d, q) * front.len() as f64) as usize % front.len()];
        q += 1;
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
    for (ci, &(th, rf, zf, sz)) in cells.iter().enumerate() {
        if skip[ci] {
            continue;
        }
        let (er, dens) = dome.envelope(th);
        let rx = cell * sz * (0.82 + 0.18 * dens.min(1.5)) * rng.range(0.92, 1.08);
        let a = dome.pt(th, ((er - rx * 0.75) * rf).max(0.0));
        let base = (a.0, a.1 + rx * 0.3);
        let u = {
            let raw = if s_a < 0.0 {
                (PI - th_lo - th) / (PI - 2.0 * th_lo)
            } else {
                (th - th_lo) / (PI - 2.0 * th_lo)
            };
            raw + (hash01(seed ^ 0x5ec, ci as i64) - 0.5) * 0.05
        };
        let lang = sectors.lang_at(u);
        let under: &[usize] = by_lang.get(&key(&lang)).filter(|v| !v.is_empty()).map_or(
            if all_nodes.is_empty() {
                &trunk_tops
            } else {
                &all_nodes
            },
            |v| v,
        );
        let side = if math::cos(th) >= 0.0 { 1.0 } else { -1.0 };
        if c.dormant {
            // Winter: the cloud's place is a fan of bare twigs, so the dome
            // still reads as fine ramification.
            winter_fan(
                sk,
                rng,
                under,
                (base.0, base.1 - rx * 0.45),
                rx,
                zf * dome.rz,
                side,
                &lang,
            );
            continue;
        }
        let patches = [(1.0, lang.clone()), (1.0, lang.clone())];
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
            cl.ry = rx * 0.6;
            cl.rz = rx * 0.85;
        }
    }
}

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
            (true, _) => 0.4,
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
    let f = math::pow((age / 14.0).clamp(0.0, 1.0), 1.2).max(0.35);
    let target =
        lifetime * (SEEDLING_FLOW / (FLARE * f)).max(1.5 + 0.25 * math::pow(age / 9.0, 4.0));
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
    flare(sk, &chains[0].clone());
    for n in &mut sk.nodes {
        n.own *= f;
    }
}

/// Root flow of a seedling, per lifetime contribution; an older tree never
/// carries less.
pub(crate) const SEEDLING_FLOW: f64 = 1.02;
/// The most of a tree's girth its dead wood may take, so the living trunk is
/// always the heavier one.
const DEAD_SHARE: f64 = 0.35;
/// What the root flare adds to a tree's flow.
const FLARE: f64 = 1.21;

/// Grows the whole skeleton for `timeline`: a seedling in the first year,
/// the exhibition maple after. `periods` declare languages for private work.
pub fn grow(
    timeline: &Timeline,
    seed: u64,
    leader: &Leader,
    periods: &[(Date, Date, Vec<String>)],
) -> Skeleton {
    let mut sk = root_only();
    let Some(life) = Life::of(timeline, periods) else {
        return sk;
    };
    if life.span_months < 12 {
        crate::growth::seedling::seedling(&mut sk, &life, seed);
        sk.dormant = life.dormant();
    } else {
        tree(&mut sk, &life, seed, leader);
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
                (life.span_months >= 12).then(|| (f.name.to_string(), life, f.snapshot.account_id))
            })
            .collect()
    }

    #[test]
    fn trunk_stage_makes_one_chain_per_living_trunk() {
        let leader = Leader::new(crate::config::Style::Moyogi, 1, None);
        let mut seen = std::collections::BTreeSet::new();
        for (name, life, seed) in dome_lives() {
            let c = Ctx::new(&life, seed);
            let mut living: Vec<usize> = c.plan.upper.iter().map(|u| u.0).collect();
            living.dedup();
            let mut sk = root_only();
            let wood = grow_trunks(&mut sk, &c, &leader, &mut Rng::keyed(seed, "bdome"));
            let want = if living.len() > 1 { 2 } else { 1 };
            assert_eq!(wood.chains.len(), want, "{name}");
            assert_eq!(wood.sides.len(), want, "{name}");
            assert!(wood.chains.iter().all(|ch| ch.len() > wood.foot), "{name}");
            seen.insert(want);
        }
        assert_eq!(seen.len(), 2, "the fixtures hold single and twin trunks");
    }

    #[test]
    fn crown_stage_gives_every_crown_language_a_sector() {
        for (name, life, seed) in dome_lives() {
            let c = Ctx::new(&life, seed);
            let (arms, _, _) = crown_arms(&c);
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
            assert!(arms.iter().all(|a| c.is_upper(&a.lang)), "{name}");
        }
    }

    /// A whole era in a dropped language is an unmistakable dead limb, not a
    /// stub; on a single trunk the eras alternate sides.
    #[test]
    fn a_dropped_era_is_a_whole_dead_limb() {
        let snapshot = crate::fixtures::by_name("showcase").unwrap().snapshot;
        let timeline = Timeline::new(&snapshot);
        let life = Life::of(&timeline, &[]).unwrap();
        let mut sk = root_only();
        let leader = Leader::new(crate::config::Style::Moyogi, 1, None);
        let chains = tree(&mut sk, &life, snapshot.account_id, &leader);
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
        // The dead C# trunk (the first dead wood grown) rises high, and its
        // broken top shows clear of the leaves.
        let first = jin[0];
        let snag: Vec<usize> = (first..sk.nodes.len())
            .take_while(|&i| sk.nodes[i].language.as_deref() == Some(crate::growth::JIN))
            .collect();
        let top = snag
            .iter()
            .map(|&i| SOIL_Y - sk.nodes[i].p.1)
            .fold(0.0, f64::max);
        assert!(top >= 80.0, "the dead trunk reaches only {top}");
        let shows = snag
            .iter()
            .filter(|&&i| {
                let p = sk.nodes[i].p;
                // A cloud is a dome on a flat bottom at `c`: only what is
                // above that bottom and inside the dome is hidden.
                SOIL_Y - p.1 > 80.0
                    && sk.clouds.iter().all(|c| {
                        let dx = (p.0 - c.c.0) / (c.rx + 4.0);
                        let dy = (p.1 - c.c.1) / (c.ry + 4.0);
                        p.1 > c.c.1 + 4.0 || dx * dx + dy * dy > 1.0
                    })
            })
            .count();
        assert!(
            shows >= 2,
            "only {shows} nodes of the dead trunk's top show"
        );
        assert_eq!(chains.len(), 1);
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
        let leader = Leader::new(crate::config::Style::Moyogi, 1, None);
        let chains = tree(&mut sk, &life, 1, &leader);
        assert_eq!(chains.len(), 2, "a third trunk language shares a trunk");
        let top = |c: &Vec<usize>| sk.nodes[*c.last().unwrap()].p.0;
        assert!((top(&chains[0]) - top(&chains[1])).abs() > 10.0);
    }
}
