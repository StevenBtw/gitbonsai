//! The scene: the grown skeleton lifted into 3D, ready to be lit and drawn.
//!
//! Wood becomes chains of `(point, radius)` (each chain follows the child
//! carrying the most flow, Murray's law sets the girth). Living pads become
//! ellipsoids whose upper shell is covered in leaf tufts, sampled on a
//! Fibonacci sphere, each with a normal for lighting and an occlusion value.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "geometry over a few thousand nodes and tufts on a 512-unit canvas"
)]

use crate::Bonsai;
use crate::color::Rgb;
use crate::foliage::{LanguageState, density};
use crate::growth::{GOLDEN, P};
use crate::math;
use crate::noise::hash01;
use crate::style::{CENTER_X, SOIL_Y};

/// A 3D vector: x right, y up, z toward the viewer.
pub type V3 = [f64; 3];

/// `a + b`.
pub fn v_add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// `a * s`.
pub fn v_scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// The dot product.
pub fn v_dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `a` scaled to unit length.
pub fn v_norm(a: V3) -> V3 {
    v_scale(a, 1.0 / v_dot(a, a).sqrt().max(1e-9))
}

/// The light: upper left, slightly toward the viewer.
pub const LIGHT: V3 = [-0.55, 0.75, 0.38];
/// The camera looks down a little, so pad tops and the pot rim show.
pub const TILT: f64 = 0.32;
/// One fixed scale from world units to the canvas: big enough to fill a
/// README, and fixed so a tree never zooms between weeks.
pub const WORLD: f64 = 1.4;
/// The most leaf tufts drawn; the most hidden (least lit) go first.
pub const TUFT_BUDGET: usize = 900;

/// A world point on the canvas: `(x, y, depth)`, depth growing toward the viewer.
pub fn project(p: V3) -> (f64, f64, f64) {
    let (s, c) = (math::sin(TILT), math::cos(TILT));
    let p = v_scale(p, WORLD);
    (
        CENTER_X + p[0],
        SOIL_Y - (p[1] * c - p[2] * s),
        p[2] * c + p[1] * s,
    )
}

/// Branch radius for a flow of contributions. Murray's law: `r^3` is additive
/// at every fork, so girth follows the cube root of what a branch carries,
/// and it never shrinks because flow never does.
pub fn radius(flow: f64) -> f64 {
    0.5 + 0.42 * math::cbrt(flow.max(0.0))
}

/// A tuft of maple leaves.
#[derive(Debug, Clone)]
pub struct Tuft {
    /// Position.
    pub p: V3,
    /// Outward normal, for lighting.
    pub n: V3,
    /// Exposure, 0 (inside the pad) to 1 (the lit top).
    pub q: f64,
    /// Leaf color before lighting.
    pub color: Rgb,
    /// New growth from last week: smaller and bronze.
    pub fresh: bool,
    /// Carries a pair of samaras.
    pub samara: bool,
    /// Size factor.
    pub size: f64,
}

/// A chain of wood: points with radii, and whether it is deadwood.
pub type Chain = (Vec<(V3, f64)>, bool);

/// Everything to draw, in world space.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    /// Wood.
    pub chains: Vec<Chain>,
    /// Leaves.
    pub tufts: Vec<Tuft>,
    /// Visible pads: center, half-width, half-height.
    pub pads: Vec<(V3, f64, f64)>,
    /// Level 11: tubing from the soil out to every pad.
    pub drip: Vec<Vec<(V3, f64)>>,
}

/// The share of leaf tufts carrying samaras at a lifetime level: none before
/// level 3 (maples only seed when mature), more with every level after.
pub fn samara_share(level: u8) -> f64 {
    if level < 3 {
        0.0
    } else {
        (0.018 * f64::from(level - 2)).min(0.18)
    }
}

impl Scene {
    /// The scene of a grown tree.
    pub fn build(b: &Bonsai) -> Self {
        let sk = &b.skeleton;
        let seed = b.account_id;
        let n = sk.nodes.len();
        let mut scene = Self::default();
        if n == 0 {
            return scene;
        }
        let flow = sk.flows();
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
        for i in (0..n).rev() {
            if let Some(p) = sk.nodes[i].parent {
                children[p].push(i);
            }
        }
        let lift = |p: P| -> V3 { [p.0 - CENTER_X, SOIL_Y - p.1, 0.0] };

        // Chains: follow the child carrying the most flow; every other child
        // starts a chain of its own.
        let mut starts = vec![0usize];
        while let Some(s0) = starts.pop() {
            let mut chain = Vec::new();
            if let Some(p) = sk.nodes[s0].parent {
                chain.push(p);
            }
            let mut cur = s0;
            loop {
                chain.push(cur);
                let mut kids = children[cur].clone();
                if kids.is_empty() {
                    break;
                }
                kids.sort_by(|a, c| flow[*c].total_cmp(&flow[*a]).then(a.cmp(c)));
                starts.extend_from_slice(&kids[1..]);
                cur = kids[0];
            }
            let last = chain[chain.len() - 1];
            // The trunk never becomes jin, whichever branch its heaviest path leads into.
            let jin =
                chain[0] != 0 && b.state(sk.nodes[last].language.as_deref()) == LanguageState::Jin;
            scene.chains.push((
                chain
                    .iter()
                    .map(|&i| (lift(sk.nodes[i].p), radius(flow[i])))
                    .collect(),
                jin,
            ));
        }

        let seeds = samara_share(b.extras.level);
        let fresh_share = (b.last_week_total / (b.window.total.max(1.0) * 6.0)).min(0.15);
        let mut pads: Vec<(P, f64, f64, Option<&str>)> = sk
            .pads
            .iter()
            .filter(|p| !p.retired)
            .map(|p| (p.c, p.rx, p.ry, p.language.as_deref()))
            .collect();
        if b.window.total > 0.0 && !b.dormant() {
            let crown = 11.0 + 9.0 * density(b.window.total);
            pads.push((sk.apex, crown, crown * 0.55, None));
        }
        let mut tufts = Vec::new();
        let mut tubed = std::collections::BTreeSet::new();
        for (pi, &(c, prx, pry, language)) in pads.iter().enumerate() {
            if b.state(language) != LanguageState::Alive {
                continue;
            }
            // Leaves only where this pad's own branches arrived: centered on
            // their ends and just covering them, so no leaf ever floats.
            let arrived: Vec<P> = sk
                .nodes
                .iter()
                // Any branch wood counts; the leader only feeds the crown.
                .filter(|and| and.language.is_some() == language.is_some())
                .filter(|and| {
                    ((and.p.0 - c.0) / prx).powi(2) + ((and.p.1 - c.1) / pry).powi(2) < 1.0
                })
                .map(|and| and.p)
                .collect();
            if arrived.is_empty() {
                continue;
            }
            let count = arrived.len() as f64;
            let centroid = (
                arrived.iter().map(|p| p.0).sum::<f64>() / count,
                arrived.iter().map(|p| p.1).sum::<f64>() / count,
            );
            let spread_x = arrived
                .iter()
                .map(|p| (p.0 - centroid.0).abs())
                .fold(0.0, f64::max);
            let spread_y = arrived
                .iter()
                .map(|p| (p.1 - centroid.1).abs())
                .fold(0.0, f64::max);
            let c = (centroid.0, centroid.1 - 2.0);
            let d = density(b.window.of_language(language));
            let c3 = lift(c);
            let rx = (prx * (0.75 + 0.25 * d)).min(spread_x + 9.0);
            let ry = (pry * 1.2).min(spread_y + 7.0);
            let rz = rx;
            scene.pads.push((c3, rx, ry));
            if b.extras.drip {
                // Tubing follows the wood from the pad's nearest node down to the soil.
                let near = (0..n)
                    .min_by(|&a, &e| {
                        let da = (sk.nodes[a].p.0 - c.0).powi(2) + (sk.nodes[a].p.1 - c.1).powi(2);
                        let de = (sk.nodes[e].p.0 - c.0).powi(2) + (sk.nodes[e].p.1 - c.1).powi(2);
                        da.total_cmp(&de)
                    })
                    .unwrap_or(0);
                // Walk down only until the tubing joins a line already laid.
                let mut line = Vec::new();
                let mut cur = Some(near);
                while let Some(i) = cur {
                    line.push((lift(sk.nodes[i].p), radius(flow[i])));
                    if !tubed.insert(i) {
                        break;
                    }
                    cur = sk.nodes[i].parent;
                }
                line.reverse();
                scene.drip.push(line);
            }
            // A Fibonacci sphere over the pad's upper shell.
            let k = (40.0 + 90.0 * d) as usize;
            let phase = hash01(seed, pi as i64) * math::TAU;
            for i in 0..k {
                let y = 1.0 - 2.0 * (i as f64 + 0.5) / k as f64;
                if y < -0.35 {
                    continue;
                }
                let r = (1.0 - y * y).sqrt();
                let th = math::rad(i as f64 * GOLDEN) + phase;
                let u = [math::cos(th) * r, y, math::sin(th) * r];
                let p = v_add(c3, [u[0] * rx, u[1] * ry, u[2] * rz]);
                let normal = v_norm([u[0] / rx, u[1] / ry, u[2] / rz]);
                let salt = (pi * 1000 + i) as i64;
                let h = hash01(seed ^ 0x51, salt);
                tufts.push(Tuft {
                    p,
                    n: normal,
                    q: 0.35 + 0.65 * (y + 1.0) / 2.0,
                    color: b.maple.leaf(language, hash01(seed ^ 0xc0, salt)),
                    fresh: y > 0.55 && h < fresh_share * 2.5,
                    samara: language.is_some()
                        && (-0.3..0.35).contains(&y)
                        && h > 1.0 - seeds * 1.4,
                    size: 1.05 + 0.55 * d,
                });
            }
        }
        // Keep within budget: drop the most hidden leaves first.
        if tufts.len() > TUFT_BUDGET {
            let mut order: Vec<usize> = (0..tufts.len()).collect();
            order.sort_by(|&a, &c| tufts[c].q.total_cmp(&tufts[a].q).then(a.cmp(&c)));
            let mut keep = vec![false; tufts.len()];
            for &i in &order[..TUFT_BUDGET] {
                keep[i] = true;
            }
            tufts = tufts
                .into_iter()
                .zip(keep)
                .filter_map(|(t, k)| k.then_some(t))
                .collect();
        }
        scene.tufts = tufts;
        scene
    }
}
