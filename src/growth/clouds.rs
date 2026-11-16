//! Domed foliage clouds and their twigs.
//!
//! A cloud rests on a flat bottom; each color patch hangs on a twig from the
//! nearest wood, and the twig carries part of the patch's contributions so
//! the limb under a busy cloud thickens.

use std::collections::BTreeMap;

use crate::growth::life::Lang;
use crate::growth::wood::limb_to;
use crate::growth::{Cloud, P, Skeleton};
use crate::math;
use crate::rng::Rng;

/// A cloud resting on `base`, one color patch per entry of `patches`
/// (weight, language), each on a twig from the nearest of `under`. `side`
/// orders patches: earliest toward the trunk, latest outward. Twig tips carry
/// `share` of the weight; `rest_to` carries the remainder.
#[allow(clippy::too_many_arguments, reason = "the shape of a cloud")]
pub fn cloud(
    sk: &mut Skeleton,
    rng: &mut Rng,
    patches: &[(f64, Lang)],
    base: P,
    z: f64,
    rx: f64,
    side: f64,
    under: &[usize],
    share: f64,
    rest_to: usize,
) {
    let ry = rx * 0.5;
    let rz = rx * 0.75;
    let n = patches.len().max(1);
    let mut out = Vec::new();
    for (j, (w, lang)) in patches.iter().enumerate() {
        let f = if n == 1 {
            0.0
        } else {
            j as f64 / (n - 1) as f64 * 2.0 - 1.0
        };
        let ax = base.0 + side * f * rx * 0.72 + rng.range(-3.0, 3.0);
        let az = z + rng.range(-0.45, 0.45) * rz;
        let ay = base.1 - rng.range(0.1, 0.3) * ry;
        let from = under
            .iter()
            .copied()
            .min_by(|a, b| {
                let d =
                    |i: usize| (sk.nodes[i].p.0 - ax).abs() + 0.6 * (sk.nodes[i].p.1 - ay).abs();
                d(*a).total_cmp(&d(*b)).then(a.cmp(b))
            })
            .unwrap_or(rest_to);
        let tw = limb_to(
            sk,
            from,
            (ax, ay),
            az,
            0.0,
            1.5,
            1.0,
            rng.range(0.0, math::TAU),
            lang,
        );
        if let Some(&tip) = tw.last() {
            sk.nodes[tip].own += w * share;
        }
        sk.nodes[rest_to].own += w * (1.0 - share);
        out.push(((ax, ay), az, lang.clone(), *w));
    }
    sk.clouds.push(Cloud {
        c: base,
        z,
        rx,
        ry,
        rz,
        patches: out,
    });
}

/// A single-language cloud resting on `base`, on two twigs from `under`.
#[allow(clippy::too_many_arguments, reason = "the shape of a cloud")]
pub fn pad(
    sk: &mut Skeleton,
    rng: &mut Rng,
    lang: &Lang,
    w: f64,
    base: P,
    z: f64,
    rx: f64,
    side: f64,
    under: &[usize],
    rest_to: usize,
) {
    let patches = [(w * 0.5, lang.clone()), (w * 0.5, lang.clone())];
    cloud(sk, rng, &patches, base, z, rx, side, under, 0.25, rest_to);
    if let Some(c) = sk.clouds.last_mut() {
        c.ry = rx * 0.56;
        c.rz = rx * 0.8;
    }
}

/// The language that colors most of a cloud.
pub fn lead_of(c: &Cloud) -> Lang {
    let mut by: BTreeMap<Lang, f64> = BTreeMap::new();
    for p in &c.patches {
        *by.entry(p.2.clone()).or_default() += p.3;
    }
    by.into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
        .and_then(|x| x.0)
}

/// Nearby clouds of one language become one mass: fewer, larger clouds.
pub fn merge_clouds(sk: &mut Skeleton) {
    loop {
        let n = sk.clouds.len();
        let mut found = None;
        'scan: for i in 0..n {
            for j in i + 1..n {
                let (a, b) = (&sk.clouds[i], &sk.clouds[j]);
                if lead_of(a) != lead_of(b) || (a.z - b.z).abs() > 30.0 {
                    continue;
                }
                let gap = (a.c.0 - b.c.0).abs() - (a.rx + b.rx);
                let dy = (a.c.1 - b.c.1).abs();
                let span = a.rx.max(b.rx) * 2.0 + (a.c.0 - b.c.0).abs();
                if gap < 0.15 * (a.rx + b.rx) && dy < 0.7 * (a.ry + b.ry) && span < 100.0 {
                    found = Some((i, j));
                    break 'scan;
                }
            }
        }
        let Some((i, j)) = found else { break };
        let b = sk.clouds.remove(j);
        let a = &mut sk.clouds[i];
        let wa = a.rx * a.rx;
        let wb = b.rx * b.rx;
        let lo = (a.c.0 - a.rx).min(b.c.0 - b.rx);
        let hi = (a.c.0 + a.rx).max(b.c.0 + b.rx);
        // Area kept roughly: the union's width, a little tucked in.
        let rx = ((hi - lo) * 0.5 * 0.92).min((wa + wb).sqrt() * 1.25);
        a.c = (f64::midpoint(lo, hi), (a.c.1 * wa + b.c.1 * wb) / (wa + wb));
        a.z = (a.z * wa + b.z * wb) / (wa + wb);
        a.ry = a.ry.max(b.ry) * 1.05;
        a.rz = a.rz.max(b.rz);
        a.rx = rx;
        a.patches.extend(b.patches);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::growth::wood::root_only;

    #[test]
    fn merged_clouds_keep_their_languages() {
        let rust = Some("Rust".to_string());
        let go = Some("Go".to_string());
        let mut rng = Rng::keyed(1, "t");
        let mut sk = root_only();
        pad(
            &mut sk,
            &mut rng,
            &rust,
            10.0,
            (200.0, 200.0),
            0.0,
            20.0,
            1.0,
            &[0],
            0,
        );
        pad(
            &mut sk,
            &mut rng,
            &rust,
            10.0,
            (225.0, 202.0),
            0.0,
            20.0,
            1.0,
            &[0],
            0,
        );
        merge_clouds(&mut sk);
        assert_eq!(sk.clouds.len(), 1, "same language, overlapping: one mass");

        let mut sk = root_only();
        pad(
            &mut sk,
            &mut rng,
            &rust,
            10.0,
            (200.0, 200.0),
            0.0,
            20.0,
            1.0,
            &[0],
            0,
        );
        pad(
            &mut sk,
            &mut rng,
            &go,
            10.0,
            (225.0, 202.0),
            0.0,
            20.0,
            1.0,
            &[0],
            0,
        );
        merge_clouds(&mut sk);
        assert_eq!(sk.clouds.len(), 2, "different languages stay apart");
    }
}
