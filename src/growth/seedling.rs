//! A first-year seedling: one thin stem with a few small tufts.
//!
//! An account active for under a year is a single tall, thin stem. Each
//! language with 5% or more of its work (up to four, private work counting as
//! one) is a small tuft: the largest on top, the rest alternating down the
//! stem. The stem grows with active months.

use std::collections::BTreeMap;

use crate::growth::Skeleton;
use crate::growth::clouds::pad;
use crate::growth::dome::SEEDLING_TOP;
use crate::growth::life::{Lang, Life, Month, irr, shares_of};
use crate::growth::wood::{at_frac, limb_to};
use crate::math;
use crate::rng::Rng;
use crate::style::{CENTER_X, SOIL_Y, SPROUT};

/// Grows the seedling into `sk` (which holds only the root), leaning to
/// `side` ([`crate::style::side`]).
pub fn seedling(sk: &mut Skeleton, life: &Life, seed: u64, side: f64) {
    let mut rng = Rng::keyed(seed, "bdome/sprout");
    let mut all: BTreeMap<u32, Month> = BTreeMap::new();
    let mut total = 0.0;
    for (i, m) in (0u32..).zip(life.years.iter().flat_map(|y| y.months.values())) {
        all.insert(i, m.clone());
        total += m.total;
    }
    let shares = shares_of(&all, total);
    let r = irr(life.life_cv);
    // Tall from the start, and no taller than the young tree it becomes.
    let h = SEEDLING_TOP - (12 - all.len().min(12)) as f64 / 3.0;
    // The stem leans and bows to the tree's side, as far as the sprout has
    // picked it: a tiny one stands nearly straight, so a sprout changing
    // side only shifts a little, and from the 60th contribution it is full.
    let lifetime: f64 = life.years.iter().map(|y| y.total).sum();
    let to_side = side * (lifetime / f64::from(SPROUT)).min(1.0);
    let lean = to_side * rng.range(0.02, 0.045) * h;
    let top = (CENTER_X + lean, SOIL_Y - h);
    // The bow's coin, still drawn so the rest of the stream stays.
    let _ = rng.sign();
    let stem = limb_to(
        sk,
        0,
        top,
        0.0,
        to_side * h * 0.05,
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
    let mut side = rng.sign() * side;
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

#[cfg(test)]
mod tests {
    use crate::config::Style;
    use crate::date::Date;
    use crate::growth::dome::{Reach, grow};
    use crate::growth::testkit::snap;
    use crate::style::{CENTER_X, Leader};
    use crate::timeline::Timeline;

    /// How far the seedling's top stands right of its foot, for `n` days of
    /// one contribution each, on `side`.
    fn top_off(n: i32, side: f64, seed: u64) -> f64 {
        let d0 = Date::from_ymd(2026, 1, 5).unwrap();
        let days: Vec<_> = (0..n)
            .map(|i| (d0.add_days(i), 1, &[("Rust", 1)][..]))
            .collect();
        let mut s = snap(&days, d0.add_days(n + 1));
        s.account_id = seed;
        let leader = Leader::new(Style::Chokkan, None, side);
        let sk = grow(
            &Timeline::new(&s),
            seed,
            &leader,
            &[],
            Reach::default(),
            None,
        );
        sk.apex.0 - CENTER_X
    }

    #[test]
    fn the_stem_leans_to_the_side_as_the_sprout_grows() {
        for seed in 1..40 {
            for side in [1.0, -1.0] {
                let (young, grown, older) = (
                    top_off(6, side, seed),
                    top_off(60, side, seed),
                    top_off(120, side, seed),
                );
                assert!(grown * side > 1.2, "{seed} {side}: {grown}");
                // A tiny sprout stands nearly straight; from 60 on the lean is full.
                assert!(
                    young.abs() < 0.2 * grown.abs(),
                    "{seed} {side}: {young} {grown}"
                );
                assert!(
                    (older - grown).abs() < 0.2 * grown.abs(),
                    "{seed}: {older} {grown}"
                );
            }
        }
    }
}
