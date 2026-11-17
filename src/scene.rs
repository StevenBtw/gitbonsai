//! The scene: the grown skeleton lifted into 3D, ready to be lit and drawn.
//!
//! Wood becomes chains of `(point, radius)` (each chain follows the child
//! carrying the most flow, Murray's law sets the girth); deadwood is marked
//! in the skeleton. Every cloud becomes a solid mass and flat-bottomed domes
//! whose shell is covered in leaf tufts, sampled on a Fibonacci sphere, each
//! with a normal for lighting and an exposure value. A resting tree has no
//! leaves.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "geometry over a few thousand nodes and tufts on a 512-unit canvas"
)]

use crate::Bonsai;
use crate::color::Rgb;
use crate::growth::{Cloud, GOLDEN, JIN, P};
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
    /// Private work: drawn in the private tone.
    pub private: bool,
    /// A grey leaf's tip in a public language's color.
    pub tip: Option<Rgb>,
}

/// A chain of wood: points with radii, and whether it is deadwood.
pub type Chain = (Vec<(V3, f64)>, Option<f64>);

/// Everything to draw, in world space.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    /// Wood.
    pub chains: Vec<Chain>,
    /// Leaves.
    pub tufts: Vec<Tuft>,
    /// Visible clouds: center, half-width, half-height.
    pub pads: Vec<(V3, f64, f64)>,
    /// Level 11: tubing from the soil out to every pad.
    pub drip: Vec<Vec<(V3, f64)>>,
    /// Solid cloud masses behind the leaves: center, rx, ry, rz, color,
    /// and whether it is private work.
    pub masses: Vec<(V3, f64, f64, f64, Rgb, bool)>,
    /// Chains that are strands along the trunk, by index into `chains`.
    pub strands: std::collections::BTreeSet<usize>,
    /// Side chains by index into `chains`: the girth of the wood they leave
    /// and the fork they leave it at, on its axis.
    pub joins: std::collections::BTreeMap<usize, (f64, V3)>,
    /// A shari up the trunk, bleached this far ([`crate::growth::Skeleton::shari`]).
    pub shari: Option<f64>,
}

/// How many of the side leaves (the lower flank of each cloud) carry
/// samaras, per unit of [`samara_share`]: clouds are dense, so a level shows
/// without spotting the crown.
pub const SAMARA_RATE: f64 = 0.4;

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
        let n = sk.nodes.len();
        let mut scene = Self {
            shari: b.skeleton.shari,
            ..Self::default()
        };
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
        let nz = |i: usize| sk.node_z.get(i).copied().unwrap_or(0.0);

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
                // The wood runs on into the child that carries the most, but
                // straight on weighs half again: a heavy side limb must not cut
                // the trunk off above it. Trunk runs on into trunk, strand into
                // strand: a heavy strand never takes the trunk's line.
                let p = sk.nodes[cur].p;
                let back = sk.nodes[cur]
                    .parent
                    .map_or((p.0, p.1 + 1.0), |q| sk.nodes[q].p);
                let along = |k: usize| {
                    let (a, b) = (
                        (p.0 - back.0, p.1 - back.1),
                        (sk.nodes[k].p.0 - p.0, sk.nodes[k].p.1 - p.1),
                    );
                    let n = ((a.0 * a.0 + a.1 * a.1) * (b.0 * b.0 + b.1 * b.1))
                        .sqrt()
                        .max(1e-9);
                    let cos = (a.0 * b.0 + a.1 * b.1) / n;
                    let kind = if sk.nodes[k].strand == sk.nodes[cur].strand {
                        1.0
                    } else {
                        0.1
                    };
                    flow[k] * (2.0 + cos) * kind
                };
                kids.sort_by(|a, c| along(*c).total_cmp(&along(*a)).then(a.cmp(c)));
                starts.extend_from_slice(&kids[1..]);
                cur = kids[0];
            }
            // Deadwood is marked in the skeleton; the trunk never is.
            let last = chain[chain.len() - 1];
            let jin = (chain[0] != 0 && sk.nodes[last].language.as_deref() == Some(JIN))
                .then_some(sk.nodes[last].bleach);
            let mut wood: Vec<(V3, f64)> = chain
                .iter()
                .map(|&i| {
                    let mut v = lift(sk.nodes[i].p);
                    v[2] = nz(i);
                    (v, radius(flow[i]))
                })
                .collect();
            if chain[0] == 0 {
                // Root flare.
                for (j, w) in wood.iter_mut().enumerate() {
                    w.1 *= 1.0 + 0.35 * math::exp(-(j as f64) / 1.2);
                }
            }
            // A strand by its own wood: a limb forking from a strand is not one.
            let own = if chain[0] == s0 {
                &chain[..]
            } else {
                &chain[1..]
            };
            let strand = own.iter().any(|&i| sk.nodes[i].strand);
            // A side branch starts on its parent's surface and reaches back
            // inside a fixed short way, at most half the parent's girth: so
            // it never leaves a gap, and never shows through the far side.
            // It starts at its own girth, a little flared, not the parent's:
            // else short stubs become thorns.
            if chain[0] != s0 && wood.len() > 1 {
                let (fork, r) = wood[0];
                let own = wood[1].1;
                if !strand {
                    let inset = r - own.min(0.5 * r);
                    let off = |p: V3| (math::sq(p[0] - fork[0]) + math::sq(p[1] - fork[1])).sqrt();
                    // Wholly inside its parent, a branch doesn't show at all.
                    let Some(j) = (1..wood.len()).find(|&j| off(wood[j].0) >= inset) else {
                        continue;
                    };
                    let (a, b) = (wood[j - 1], wood[j]);
                    let at = |t: f64| -> V3 {
                        [
                            a.0[0] + (b.0[0] - a.0[0]) * t,
                            a.0[1] + (b.0[1] - a.0[1]) * t,
                            a.0[2] + (b.0[2] - a.0[2]) * t,
                        ]
                    };
                    // Where the segment crosses `inset`, found by halving.
                    let (mut lo, mut hi) = (0.0, 1.0);
                    for _ in 0..40 {
                        let mid = f64::midpoint(lo, hi);
                        if off(at(mid)) < inset {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    let start = at(hi);
                    wood.splice(..j, [(start, b.1)]);
                }
                scene.joins.insert(scene.chains.len(), (r, fork));
                wood[0].1 = wood[0].1.min(own * 1.3);
            }
            // Wood thins gradually above a fork, never in one step: where a
            // heavy limb leaves, the trunk tapers over a short stretch.
            for i in 1..wood.len() {
                let (a, b) = (wood[i - 1], wood[i]);
                let run = (math::sq(b.0[0] - a.0[0]) + math::sq(b.0[1] - a.0[1])).sqrt();
                wood[i].1 = b.1.max(a.1 - 0.3 * run);
            }
            if strand {
                scene.strands.insert(scene.chains.len());
            }
            scene.chains.push((wood, jin));
        }
        // A resting tree stands bare for the winter.
        if !sk.dormant {
            cloud_leaves(b, &mut scene);
        }
        scene
    }
}

/// A leaf's color: its language's, or silver for work without one.
/// A public language for a grey leaf: drawn with odds `odds` against
/// public work's share of the recent window, then by each language's part.
fn pluck_lang(b: &Bonsai, odds: f64, which: f64) -> Option<&str> {
    let named: f64 = b.window.by_language.values().sum();
    if b.window.total <= 0.0 || odds >= named / b.window.total {
        return None;
    }
    let mut at = which * named;
    for (l, v) in &b.window.by_language {
        if at < *v {
            return Some(l.as_str());
        }
        at -= v;
    }
    b.window.by_language.keys().last().map(String::as_str)
}

fn leaf_color(b: &Bonsai, lang: Option<&str>, pick: f64) -> Rgb {
    match lang {
        Some(_) => b.maple.leaf(lang, pick),
        None => b.maple.private().shift(0.05 * (pick - 0.5), 0.0),
    }
}

/// Leaf tufts over every cloud of the tree.
const CLOUD_TUFTS: f64 = 900.0;
/// The cloud area that earns the full tuft budget.
const FULL_AREA: f64 = 6500.0;
/// A quarter turn.
const PI_HALF: f64 = math::TAU / 4.0;

/// Leaves on the clouds: each cloud a flat-bottomed dome (big ones several
/// domes side by side, like cumulus), densely covered, every tuft colored by
/// the nearest color patch. A small tree gets fewer leaves, not denser ones.
fn cloud_leaves(b: &Bonsai, scene: &mut Scene) {
    let sk = &b.skeleton;
    let seed = b.account_id;
    let seeds = samara_share(b.extras.level);
    let fresh_share = (b.last_week_total / (b.window.total.max(1.0) * 6.0)).min(0.15);
    let (s, c) = (math::sin(TILT), math::cos(TILT));
    let area = |cl: &Cloud| PI_HALF * cl.rx * (cl.ry * c + cl.rz * s) + PI_HALF * cl.rx * cl.rz * s;
    let mut lobes: Vec<(usize, Cloud)> = Vec::new();
    for (ci, cl) in sk.clouds.iter().enumerate() {
        let count = ((cl.rx / 17.0).floor() as usize).clamp(1, 3);
        if count == 1 {
            lobes.push((ci, cl.clone()));
            continue;
        }
        let lrx = cl.rx / count as f64 * 1.45;
        for j in 0..count {
            let f = j as f64 / (count - 1) as f64 * 2.0 - 1.0;
            let hj = hash01(seed ^ 0x10b, (ci * 10 + j) as i64);
            let mut l = cl.clone();
            l.c.0 += f * (cl.rx - lrx);
            l.c.1 += hash01(seed ^ 0x10c, (ci * 10 + j) as i64) * 4.0 - 2.0;
            l.z += (hj - 0.5) * cl.rz * 0.5;
            l.rx = lrx;
            l.rz = cl.rz * (0.8 + 0.25 * hj);
            // The middle dome stands highest.
            l.ry = cl.ry
                * (1.0 - 0.3 * f.abs())
                * (0.85 + 0.3 * hj).max(0.5)
                * (lrx / cl.rx).sqrt()
                * 1.25;
            lobes.push((ci, l));
        }
    }
    let total_area: f64 = lobes.iter().map(|l| area(&l.1)).sum::<f64>().max(1.0);
    let total = (CLOUD_TUFTS * (total_area / FULL_AREA).min(1.0)).max(90.0);
    let flow = sk.flows();
    let mut tubed = std::collections::BTreeSet::new();
    let mut tufts = Vec::new();
    for (li, (ci, cl)) in lobes.iter().enumerate() {
        let c3 = [cl.c.0 - CENTER_X, SOIL_Y - cl.c.1, cl.z];
        let a = area(cl);
        let k = (total * a / total_area).max(14.0);
        // Tufts sized so k of them cover the cloud about twice over.
        let size = (a * 2.8 / (k * 30.0)).sqrt().clamp(0.9, 2.6);
        let dom = cl
            .patches
            .iter()
            .map(|p| (p.3 / (1.0 + (p.0.0 - cl.c.0).abs() / cl.rx), p))
            .max_by(|x, y| x.0.total_cmp(&y.0))
            .and_then(|p| p.1.2.clone());
        // A dropped language's cloud keeps its place in the crown, bare: no
        // leaves, nothing to water.
        if dom.is_some() && b.state(dom.as_deref()) == crate::foliage::LanguageState::Jin {
            continue;
        }
        // The mass behind the leaves shrinks as they fall.
        let left = b.fullness(dom.as_deref()).sqrt();
        if left > 0.02 {
            scene.masses.push((
                c3,
                cl.rx * left,
                cl.ry * left,
                cl.rz * left,
                leaf_color(b, dom.as_deref(), 0.5),
                dom.is_none(),
            ));
        }
        scene.pads.push((c3, cl.rx, cl.ry));
        if b.extras.drip && (li == 0 || lobes[li - 1].0 != *ci) {
            // Tubing follows the wood from the cloud's nearest node down to
            // the soil, until it joins a line already laid.
            let d = |i: usize| {
                let p = sk.nodes[i].p;
                (p.0 - cl.c.0) * (p.0 - cl.c.0) + (p.1 - cl.c.1) * (p.1 - cl.c.1)
            };
            let near = (0..sk.nodes.len())
                .min_by(|&x, &y| d(x).total_cmp(&d(y)))
                .unwrap_or(0);
            let mut line = Vec::new();
            let mut cur = Some(near);
            while let Some(i) = cur {
                let p = sk.nodes[i].p;
                let z = sk.node_z.get(i).copied().unwrap_or(0.0);
                line.push(([p.0 - CENTER_X, SOIL_Y - p.1, z], radius(flow[i])));
                if !tubed.insert(i) {
                    break;
                }
                cur = sk.nodes[i].parent;
            }
            line.reverse();
            scene.drip.push(line);
        }
        let anchors: Vec<(V3, Option<&str>, f64)> = cl
            .patches
            .iter()
            .map(|(p, z, l, w)| ([p.0 - CENTER_X, SOIL_Y - p.1, *z], l.as_deref(), *w))
            .collect();
        // Leaves at 1.5 times 0.2's count, at the same size: a full crown.
        let nn = (k * 1.5 / 0.36) as usize;
        let phase = hash01(seed, li as i64) * math::TAU;
        let ph2 = hash01(seed ^ 0x77, li as i64) * math::TAU;
        for i in 0..nn {
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / nn as f64;
            if y < -0.04 {
                continue;
            }
            let r = (1.0 - y * y).sqrt();
            let th = math::rad(i as f64 * GOLDEN) + phase;
            let u = [math::cos(th) * r, y.max(0.0), math::sin(th) * r];
            if u[2] < -0.3 {
                continue;
            }
            let salt = (li * 1000 + i) as i64;
            let h = hash01(seed ^ 0x51, salt);
            let bump =
                1.0 + 0.1 * math::sin(3.0 * th + ph2) + 0.06 * math::sin(5.0 * th + 2.0 * ph2);
            let lift = 0.85 + 0.3 * hash01(seed ^ 0x3c, salt);
            let p = v_add(
                c3,
                [
                    u[0] * cl.rx * bump,
                    u[1] * cl.ry * bump * lift,
                    u[2] * cl.rz * bump,
                ],
            );
            let normal = v_norm([u[0] / cl.rx, u[1] / cl.ry, u[2] / cl.rz]);
            let jit = hash01(seed ^ 0x99, salt);
            let lang = anchors
                .iter()
                .map(|(a, l, w)| {
                    let d = (p[0] - a[0]) * (p[0] - a[0]) + 0.5 * (p[2] - a[2]) * (p[2] - a[2]);
                    (d * (0.7 + 0.6 * jit) / math::pow(w.max(1.0), 0.2), *l)
                })
                .min_by(|x, y| x.0.total_cmp(&y.0))
                .and_then(|x| x.1);
            // A silent language's leaves fall one by one, never all at once.
            if hash01(seed ^ 0xfa11, salt) >= b.fullness(lang) {
                continue;
            }
            // A grey leaf's tip turns a public language's color: on as many
            // grey leaves as named public work's share of the recent window.
            let tip = lang
                .is_none()
                .then(|| pluck_lang(b, hash01(seed ^ 0x91c, salt), hash01(seed ^ 0x92d, salt)))
                .flatten()
                .map(|l| leaf_color(b, Some(l), hash01(seed ^ 0xc1, salt)));
            tufts.push(Tuft {
                tip,
                p,
                n: normal,
                q: 0.35 + 0.65 * u[1],
                color: leaf_color(b, lang, hash01(seed ^ 0xc0, salt)),
                fresh: u[1] > 0.55 && h < fresh_share * 2.5,
                samara: lang.is_some()
                    && u[1] > 0.1
                    && u[1] < 0.35
                    && h > 1.0 - seeds * SAMARA_RATE,
                size,
                private: lang.is_none(),
            });
        }
    }
    scene.tufts = tufts;
}
