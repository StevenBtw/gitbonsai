//! A first-year seedling: one thin stem with a few small tufts.
//!
//! An account active for under a year is a single tall, thin stem. Each
//! language with 5% or more of its work (up to four, private work counting as
//! one) is a small tuft: the largest on top, the rest alternating down the
//! stem. The stem grows with active months and days.

use std::collections::BTreeMap;

use crate::growth::Skeleton;
use crate::growth::clouds::pad;
use crate::growth::life::{Lang, Life, Month, irr, shares_of};
use crate::growth::wood::{at_frac, limb_to};
use crate::math;
use crate::rng::Rng;
use crate::style::{CENTER_X, SOIL_Y};

/// Grows the seedling into `sk` (which holds only the root).
pub fn seedling(sk: &mut Skeleton, life: &Life, seed: u64) {
    let mut rng = Rng::keyed(seed, "bdome/sprout");
    let mut all: BTreeMap<u32, Month> = BTreeMap::new();
    let mut total = 0.0;
    let mut days = 0usize;
    for (i, m) in (0u32..).zip(life.years.iter().flat_map(|y| y.months.values())) {
        all.insert(i, m.clone());
        total += m.total;
        days += m.days;
    }
    let shares = shares_of(&all, total);
    let r = irr(life.life_cv);
    let h = 70.0 + 3.0 * all.len() as f64 + 0.04 * days as f64;
    let lean = rng.range(-0.05, 0.05) * h;
    let top = (CENTER_X + lean, SOIL_Y - h);
    let stem = limb_to(
        sk,
        0,
        top,
        0.0,
        rng.sign() * h * 0.07,
        h * (0.02 + 0.04 * r),
        1.0 + r,
        rng.range(0.0, math::TAU),
        &None,
    );
    let mut chain = vec![0];
    chain.extend(&stem);
    sk.apex = top;
    sk.height = h;
    let t = total.max(1e-9);
    let named: Vec<&(Lang, f64)> = shares.iter().filter(|s| s.1 / t >= 0.05).take(4).collect();
    let mut side = rng.sign();
    let lows = named.len().saturating_sub(1).max(1);
    for (q, (lang, w)) in named.iter().enumerate() {
        let rx = (5.0 + 7.0 * (w / t).sqrt()).clamp(5.0, 9.5);
        if q == 0 {
            let at = *chain.last().unwrap_or(&0);
            let under: Vec<usize> = chain[chain.len() * 88 / 100..].to_vec();
            pad(
                sk,
                &mut rng,
                lang,
                *w,
                (top.0, top.1 + 2.0),
                0.0,
                rx * 1.1,
                side,
                &under,
                at,
            );
            continue;
        }
        let frac = 0.8 - 0.26 * (q - 1) as f64 / lows as f64;
        let from = at_frac(&chain, frac);
        let a = sk.nodes[from].p;
        let len = 6.0 + rx * 0.9;
        let to = (a.0 + side * len, a.1 - 5.0 - 0.25 * len);
        let tz = rng.range(-5.0, 5.0);
        let twig = limb_to(
            sk,
            from,
            to,
            tz,
            side * len * 0.12,
            1.4 + 1.2 * r,
            1.5,
            rng.range(0.0, math::TAU),
            lang,
        );
        pad(
            sk,
            &mut rng,
            lang,
            *w,
            (to.0 + side * rx * 0.2, to.1 - 2.0),
            tz,
            rx,
            side,
            &twig,
            from,
        );
        side = -side;
    }
    // The seedling carries its lifetime, so the young tree that follows it
    // never carries less.
    let lifetime: f64 = life.years.iter().map(|y| y.total).sum();
    let sum: f64 = sk.nodes.iter().map(|n| n.own).sum();
    if sum > 0.0 {
        let k = lifetime * crate::growth::dome::SEEDLING_FLOW / sum;
        for n in &mut sk.nodes {
            n.own *= k;
        }
    } else if let Some(n) = sk.nodes.get_mut(1) {
        n.own = lifetime * crate::growth::dome::SEEDLING_FLOW;
    }
}
