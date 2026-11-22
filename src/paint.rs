//! Painting the scene: one light, depth-sorted wood and leaves, the pot.
//!
//! Volume comes from light, not outlines. Each leaf tuft is shaded by how its
//! normal faces the light and by occlusion, warmed where lit and cooled in
//! shadow; wood gets a lit side and a shadow side; everything is sorted back
//! to front so leaves pass in front of and behind branches. Nothing here uses
//! filters or gradients, so the SVG renders the same in every browser, behind
//! GitHub's image proxy, and in resvg.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "geometry over a few thousand nodes and tufts on a 512-unit canvas"
)]

use std::fmt::Write as _;

use crate::Bonsai;
use crate::color::Rgb;
use crate::config::Config;
use crate::eggs;
use crate::maple::private_tone;
use crate::math;
use crate::noise::hash01;
use crate::render::{Num, PathData, Theme};
use crate::rng::Rng;
use crate::scene::{Chain, Scene, TILT, V3, WORLD, light, project, v_add, v_dot, v_norm, v_scale};
use crate::style::{CENTER_X, SOIL_Y};

/// The colors that don't come from the data.
#[derive(Debug, Clone, Copy)]
pub struct Paints {
    /// Whether this is the dark variant.
    pub dark: bool,
    /// Live wood.
    pub bark: Rgb,
    /// Deadwood.
    pub jin: Rgb,
    /// The inked edge around deadwood, so bleached wood reads on any page.
    pub jin_ink: Rgb,
    /// The soil surface.
    pub soil: Rgb,
    /// Two moss tones.
    pub moss: [Rgb; 2],
    /// Drip tubing.
    pub tubing: Rgb,
    /// Drip drops.
    pub water: Rgb,
    /// Carved stone figures.
    pub stone: Rgb,
    /// Samaras and momiji.
    pub crimson: Rgb,
    /// Leaf lightness shift: lighter on dark.
    pub leaf_shift: f64,
    /// The tier-0 training pot.
    pub plastic: Rgb,
}

impl Paints {
    /// The paints of `theme`.
    pub fn of(theme: Theme) -> Self {
        match theme {
            Theme::Light => Self {
                dark: false,
                bark: Rgb::hex(0x6e4c36),
                jin: Rgb::hex(0xcdc7ba),
                jin_ink: Rgb::hex(0x9a9182),
                soil: Rgb::hex(0x5a4632),
                moss: [Rgb::hex(0x5f7f35), Rgb::hex(0x84a34a)],
                tubing: Rgb::hex(0x22272b),
                water: Rgb::hex(0x3d97d6),
                stone: Rgb::hex(0x8f918a),
                crimson: Rgb::hex(0xb8352a),
                leaf_shift: 0.0,
                plastic: Rgb::hex(0x2e2e2e),
            },
            Theme::Dark => Self {
                dark: true,
                bark: Rgb::hex(0x8a6a50),
                jin: Rgb::hex(0xc4beb0),
                jin_ink: Rgb::hex(0x4e4941),
                soil: Rgb::hex(0x4d3c2c),
                moss: [Rgb::hex(0x6d8f3c), Rgb::hex(0x93b356)],
                tubing: Rgb::hex(0xb9c0c7),
                water: Rgb::hex(0x79bdf0),
                stone: Rgb::hex(0xa4a69d),
                crimson: Rgb::hex(0xd9503f),
                leaf_shift: 0.05,
                plastic: Rgb::hex(0x55555a),
            },
        }
    }
}

/// The pot, as drawn: half-width, rim half-height, depth, foot half-width.
#[derive(Debug, Clone, Copy)]
pub struct Pot {
    /// Half-width at the rim.
    pub hw: f64,
    /// Half-height of the rim ellipse.
    pub ry: f64,
    /// Depth of the pot wall.
    pub depth: f64,
    /// Half-width at the foot.
    pub bw: f64,
    /// Glaze color.
    pub glaze: Rgb,
}

/// The drawing, with its element counts for the budget.
#[derive(Debug, Clone, Default)]
pub struct Painted {
    /// SVG markup, back to front.
    pub body: String,
    /// Animated elements.
    pub animated: usize,
    /// The part of the canvas worth showing.
    pub frame: Frame,
}

/// A rectangle of the canvas: `(x, y, width, height)`.
pub type Frame = (f64, f64, f64, f64);

/// The canvas cropped to the tree, the pot and its shadow, plus a margin and
/// at least 320 units wide. It sits on an 8-unit grid, so it only moves when
/// the tree outgrows it, and its aspect stays between square and 2:1, which
/// reads well both in a wide desktop column and on a phone.
pub fn frame(extent: (f64, f64, f64), pot: &Pot) -> Frame {
    const MARGIN: f64 = 10.0;
    const GRID: f64 = 8.0;
    // Never closer than this, so a seedling isn't all pot.
    const MIN_WIDTH: f64 = 320.0;
    let down = |v: f64| (v / GRID).floor() * GRID;
    let up = |v: f64| (v / GRID).ceil() * GRID;
    let mut x0 = down(extent.0.min(CENTER_X - pot.hw) - MARGIN);
    let mut x1 = up(extent.1.max(CENTER_X + pot.hw * 1.2) + MARGIN);
    let mut y0 = down(extent.2.min(SOIL_Y - 40.0) - MARGIN);
    let y1 = up(SOIL_Y + pot.depth + pot.ry + 8.0 + MARGIN);
    if x1 - x0 < MIN_WIDTH {
        let pad = up((MIN_WIDTH - (x1 - x0)) / 2.0);
        x0 -= pad;
        x1 += pad;
    }
    let (w, h) = (x1 - x0, y1 - y0);
    if w < h {
        let pad = up((h - w) / 2.0);
        x0 -= pad;
        x1 += pad;
    } else if w > 2.0 * h {
        y0 -= up(w / 2.0 - h);
    }
    (x0, y0, x1 - x0, y1 - y0)
}

/// A unit vector in 2D.
pub fn norm((x, y): (f64, f64)) -> (f64, f64) {
    let l = (x * x + y * y).sqrt().max(1e-9);
    (x / l, y / l)
}

/// Continues `d` from `pts[0]` (the current point) through the rest of `pts`
/// on one smooth curve: Catmull-Rom, written as cubic Béziers. Interior
/// control points mirror each other, so after the first segment every one is
/// a short `s`.
pub fn smooth_through(d: &mut PathData, pts: &[(f64, f64)]) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    if n == 2 {
        // Two points make a straight curve: a line says the same, shorter.
        d.l(pts[1]);
        return;
    }
    let at = |i: isize| -> (f64, f64) {
        if i < 0 {
            (2.0 * pts[0].0 - pts[1].0, 2.0 * pts[0].1 - pts[1].1)
        } else if i as usize >= n {
            let (a, b) = (pts[n - 1], pts[n - 2]);
            (2.0 * a.0 - b.0, 2.0 * a.1 - b.1)
        } else {
            pts[i as usize]
        }
    };
    for i in 0..n - 1 {
        let k = i as isize;
        let (p0, p1, p2, p3) = (at(k - 1), at(k), at(k + 1), at(k + 2));
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        if i == 0 {
            let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
            d.c(c1, c2, p2);
        } else {
            d.s(c2, p2);
        }
    }
}

/// A tube along `(x, y, r)` samples, ending in a round cap when `cap` and cut
/// flat otherwise (for a piece that the next one overlaps). Its sides are
/// curves when `smooth`; thin twigs don't need them, and straight sides are
/// shorter to write. Its flat start is left open: the fill closes it, but an
/// outline never draws a line across the cut.
pub fn tube(points: &[(f64, f64, f64)], cap: bool, smooth: bool) -> PathData {
    tube_from(points, false, cap, smooth)
}

/// A tube as [`tube`], with a round start too when `round_start`: a side
/// branch's start, inside its parent, never shows a square corner.
pub fn tube_from(
    points: &[(f64, f64, f64)],
    round_start: bool,
    cap: bool,
    smooth: bool,
) -> PathData {
    let n = points.len();
    let mut d = PathData::coarse();
    if n < 2 {
        return d;
    }
    // Tangents over two neighbours each way, so sides don't wobble.
    let tangent = |i: usize| {
        let (a, b) = (points[i.saturating_sub(2)], points[(i + 2).min(n - 1)]);
        norm((b.0 - a.0, b.1 - a.1))
    };
    let side = |i: usize, s: f64| {
        let (tx, ty) = tangent(i);
        let (x, y, r) = points[i];
        (x - ty * r * s, y + tx * r * s)
    };
    let left: Vec<(f64, f64)> = (0..n).map(|i| side(i, -1.0)).collect();
    let right: Vec<(f64, f64)> = (0..n).rev().map(|i| side(i, 1.0)).collect();
    let side_line = |d: &mut PathData, pts: &[(f64, f64)]| {
        if smooth {
            smooth_through(d, pts);
        } else {
            for &p in &pts[1..] {
                d.l(p);
            }
        }
    };
    d.m(left[0]);
    side_line(&mut d, &left);
    let (a, b) = (left[n - 1], right[0]);
    if cap {
        let (tx, ty) = tangent(n - 1);
        let r = points[n - 1].2;
        d.c(
            (a.0 + tx * r * 1.33, a.1 + ty * r * 1.33),
            (b.0 + tx * r * 1.33, b.1 + ty * r * 1.33),
            b,
        );
    } else {
        d.l(b);
    }
    side_line(&mut d, &right);
    if round_start {
        let (tx, ty) = tangent(0);
        let r = points[0].2;
        let (b, a) = (right[n - 1], left[0]);
        d.c(
            (b.0 - tx * r * 1.33, b.1 - ty * r * 1.33),
            (a.0 - tx * r * 1.33, a.1 - ty * r * 1.33),
            a,
        );
        d.z();
    }
    d
}

/// A thin twig, for the order twigs are drawn in: its centre line on the
/// canvas with depth, `(x, y, z)`, its mean depth, its widest radius, and
/// the twig it grows from, by index, when that is a twig too.
#[derive(Debug, Clone)]
struct Twig {
    line: Vec<(f64, f64, f64)>,
    z: f64,
    w: f64,
    parent: Option<usize>,
}

/// Orders `twigs` for drawing, back to front: you get every index once.
///
/// Where two cross, the one in front at the crossing comes later, so the
/// right outline shows there even when a long twig is in front on average.
/// A twig comes before the twig it grows from, so it comes out from behind
/// it. Otherwise, and to break the rare cycle, the one further back on
/// average comes first.
fn layer(twigs: &[Twig]) -> Vec<usize> {
    let n = twigs.len();
    // `later[i]`: the twigs that must be drawn after twig `i`.
    let mut later: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut waits = vec![0usize; n];
    let mut must = |before: usize, after: usize| {
        later[before].push(after);
        waits[after] += 1;
    };
    for (i, t) in twigs.iter().enumerate() {
        if let Some(p) = t.parent.filter(|&p| p < n) {
            must(i, p);
        }
    }
    let boxes: Vec<Bounds> = twigs.iter().map(|t| bounds(&t.line)).collect();
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (boxes[i], boxes[j]);
            let related = twigs[i].parent == Some(j) || twigs[j].parent == Some(i);
            if related || !overlaps(a, b) {
                continue;
            }
            // Where a twig starts it is still coming out of wood, maybe the
            // other's: no crossing there.
            let near = 1.5 * (twigs[i].w + twigs[j].w) + 1.0;
            let starting = |t: &Twig, (x, y): (f64, f64)| {
                t.line
                    .first()
                    .is_some_and(|s| math::sq(s.0 - x) + math::sq(s.1 - y) < near * near)
            };
            if let Some((at, zi, zj)) = crossing(&twigs[i].line, &twigs[j].line) {
                if starting(&twigs[i], at) || starting(&twigs[j], at) {
                    continue;
                }
                if zi > zj {
                    must(j, i);
                } else {
                    must(i, j);
                }
            }
        }
    }
    // Back to front: of the twigs free to go, the one furthest back; when
    // none is free, a cycle, the one furthest back of those left.
    let mut order = Vec::with_capacity(n);
    let mut done = vec![false; n];
    let back = |a: &usize, b: &usize| twigs[*a].z.total_cmp(&twigs[*b].z).then(a.cmp(b));
    while order.len() < n {
        let next = (0..n)
            .filter(|&i| !done[i] && waits[i] == 0)
            .min_by(back)
            .or_else(|| (0..n).filter(|&i| !done[i]).min_by(back));
        let Some(i) = next else { break };
        done[i] = true;
        order.push(i);
        for &k in &later[i] {
            waits[k] = waits[k].saturating_sub(1);
        }
    }
    order
}

/// Finds where the centre lines `a` and `b` first cross: the point, then
/// each one's depth there. You get `None` when they never cross.
///
/// Lines meeting at less than about 20 degrees lie along each other rather
/// than cross, and don't count.
fn crossing(a: &[(f64, f64, f64)], b: &[(f64, f64, f64)]) -> Option<((f64, f64), f64, f64)> {
    // Only segments whose boxes meet can cross: a crown's many twigs stay
    // quick to layer.
    let whole = bounds(b);
    for s in a.windows(2) {
        let sb = bounds(s);
        if !overlaps(sb, whole) {
            continue;
        }
        for t in b.windows(2) {
            if !overlaps(sb, bounds(t)) {
                continue;
            }
            let (p, r) = ((s[0].0, s[0].1), (s[1].0 - s[0].0, s[1].1 - s[0].1));
            let (q, w) = ((t[0].0, t[0].1), (t[1].0 - t[0].0, t[1].1 - t[0].1));
            let den = r.0 * w.1 - r.1 * w.0;
            // `den / len` is the sine of the angle they meet at.
            let len =
                (math::sq(r.0) + math::sq(r.1)).sqrt() * (math::sq(w.0) + math::sq(w.1)).sqrt();
            if den.abs() < 0.34 * len || len < 1e-12 {
                continue;
            }
            let (dx, dy) = (q.0 - p.0, q.1 - p.1);
            let u = (dx * w.1 - dy * w.0) / den;
            let v = (dx * r.1 - dy * r.0) / den;
            if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                return Some((
                    (p.0 + u * r.0, p.1 + u * r.1),
                    s[0].2 + u * (s[1].2 - s[0].2),
                    t[0].2 + v * (t[1].2 - t[0].2),
                ));
            }
        }
    }
    None
}

/// A box on the canvas: least x and y, most x and y.
type Bounds = (f64, f64, f64, f64);

/// Gives the box around a line's points.
fn bounds(line: &[(f64, f64, f64)]) -> Bounds {
    line.iter()
        .fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, p| {
            (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1))
        })
}

/// Tells whether two boxes meet; touching edges count.
fn overlaps(a: Bounds, b: Bounds) -> bool {
    a.0 <= b.2 && b.0 <= a.2 && a.1 <= b.3 && b.1 <= a.3
}

/// Widens a strand's piece `seg` into the soft groove of shadow it lies in:
/// 1.45 times its width.
///
/// On the strand's first piece (`from_start`) the groove widens from the
/// strand's own width, so its start stays hidden in the trunk the strand
/// leaves instead of poking out beside it.
fn groove(seg: &[(f64, f64, f64)], from_start: bool) -> Vec<(f64, f64, f64)> {
    let n = seg.len().max(2) as f64;
    seg.iter()
        .enumerate()
        .map(|(i, p)| {
            let k = if from_start {
                (2.0 * i as f64 / n).min(1.0)
            } else {
                1.0
            };
            (p.0, p.1, p.2 * (1.0 + 0.45 * k))
        })
        .collect()
}

/// Starts `pts` in a point `reach` back along its first step: dead wood runs
/// out of the wood it leaves as a narrowing tongue, the way jin runs into a
/// shari, where a round end reads as a cut. Fewer than two points are left as
/// they are.
pub fn point_start(pts: &mut Vec<(f64, f64, f64, f64)>, reach: f64) {
    if let [a, b, ..] = pts[..] {
        let (tx, ty) = norm((b.0 - a.0, b.1 - a.1));
        pts.insert(0, (a.0 - tx * reach, a.1 - ty * reach, 0.0, a.3));
    }
}

/// How far, on the canvas, dead wood may reach past the crown.
const WHIP: f64 = 12.0;

/// Cuts `pts` before its first point that no disc `(x, y, r)` of `near`
/// reaches over: none within `reach` of it across, and up to `reach` below
/// it or higher. Wood under the crown is left whole; wood that rises above
/// it or leaves its sides is cut there. It keeps at least its first stretch.
fn cut_near(pts: &mut Vec<(f64, f64, f64, f64)>, near: &[(f64, f64, f64)], reach: f64) {
    let out = |p: &(f64, f64, f64, f64)| {
        !near
            .iter()
            .any(|q| (q.0 - p.0).abs() <= q.2 + reach && q.1 - q.2 <= p.1 + reach)
    };
    if let Some(k) = pts.iter().position(out) {
        pts.truncate(k.max(2));
    }
}

/// Tells how high a shari climbs, up to `top`: it ends where the run of its
/// points `(x, y, r, t)` (on the strip, `r` the trunk's girth there, `t` how
/// far up it) that reaches its top begins, when each of them comes within
/// half that girth of a limb of living wood drawn in front of the trunk, its
/// points `(x, y, r)`. A strip that ends beside that wood reads as part of it; one that
/// only passes it, and climbs on, reads as its own. You get `top` when the
/// strip climbs clear, and the first point's `t` when none of it is clear.
fn shari_end(at: &[(f64, f64, f64, f64)], front: &[&[(f64, f64, f64)]], top: f64) -> f64 {
    let clear = |a: &(f64, f64, f64, f64)| {
        let near = |q: (f64, f64, f64)| {
            math::sq(a.0 - q.0) + math::sq(a.1 - q.1) < math::sq(q.2 + 0.5 * a.2)
        };
        // The wood between a limb's points counts: the nearest point on each
        // stretch, with its girth there.
        !front.iter().any(|l| {
            (l.len() == 1 && near(l[0]))
                || l.windows(2).any(|w| {
                    let (p, q) = (w[0], w[1]);
                    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
                    let len = dx * dx + dy * dy;
                    let u = if len > 0.0 {
                        (((a.0 - p.0) * dx + (a.1 - p.1) * dy) / len).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    near((p.0 + dx * u, p.1 + dy * u, p.2 + (q.2 - p.2) * u))
                })
        })
    };
    let from = at.iter().rposition(clear).map_or(0, |k| k + 1);
    at.get(from).map_or(top, |a| a.3.min(top))
}

/// How long, in world units, a fork's side may be and still count as a stub
/// rather than a branch.
const STUB: f64 = 4.0;

/// Tells, for each chain, whether it is part of a bare dead whip: dead wood
/// that, from where it leaves living wood, never forks into two branches
/// longer than a stub. A strand's way on along the trunk is not a branch. A
/// branched dead top is not a whip, so you get false for all of its chains,
/// and for all living wood.
fn whips(
    chains: &[Chain],
    joins: &std::collections::BTreeMap<usize, (f64, V3, usize)>,
    strands: &std::collections::BTreeSet<usize>,
) -> Vec<bool> {
    let n = chains.len();
    let dead = |i: usize| i < n && chains[i].1.is_some();
    let gap = |a: V3, b: V3| {
        (math::sq(a[0] - b[0]) + math::sq(a[1] - b[1]) + math::sq(a[2] - b[2])).sqrt()
    };
    let length = |pts: &[(V3, f64)]| pts.windows(2).map(|w| gap(w[0].0, w[1].0)).sum::<f64>();
    // The chain where each piece of dead wood leaves living wood. A chain
    // always comes after the chain it grows from.
    let mut root: Vec<usize> = (0..n).collect();
    for (&k, j) in joins {
        if dead(k) && dead(j.2) {
            root[k] = root[j.2];
        }
    }
    // Branches longer than a stub at each fork of dead wood, by parent and
    // node: the parent's own way on counts as one.
    let mut sides: std::collections::BTreeMap<(usize, usize), usize> =
        std::collections::BTreeMap::new();
    for (&k, j) in joins {
        if !(dead(k) && dead(j.2)) || length(&chains[k].0) <= STUB {
            continue;
        }
        let wood = &chains[j.2].0;
        let at = (0..wood.len())
            .min_by(|&a, &b| gap(wood[a].0, j.1).total_cmp(&gap(wood[b].0, j.1)))
            .unwrap_or(0);
        let on = usize::from(!strands.contains(&j.2) && length(&wood[at..]) > STUB);
        *sides.entry((j.2, at)).or_insert(on) += 1;
    }
    let mut forked = vec![false; n];
    for (&(p, _), &count) in &sides {
        if count > 1 {
            forked[root[p]] = true;
        }
    }
    (0..n).map(|i| dead(i) && !forked[root[i]]).collect()
}

/// Runs wood that leaves the trunk low, within its own width of the foot at
/// `foot_y`, on straight down into the soil as the trunk does: the soil cut
/// then squares its start, which otherwise shows as a round end on the
/// trunk's face whenever the wood sorts in front there. Whether it did.
fn into_soil(seg: &mut Vec<(f64, f64, f64)>, foot_y: f64) -> bool {
    let Some(&(x, y, r)) = seg.first() else {
        return false;
    };
    // Too high to reach the soil, or in it already.
    if y + 2.0 * r < foot_y || y >= foot_y {
        return false;
    }
    seg.splice(0..0, [(x, y + 2.0 * r, r), (x, y + r, r)]);
    true
}

/// Dead wood narrows toward its end the more it has bleached, to a point
/// when white.
pub fn weather_tip(pts: &mut [(f64, f64, f64, f64)], bleach: f64) {
    let Some(&(tx, ty, tr, _)) = pts.last() else {
        return;
    };
    let length: f64 = pts
        .windows(2)
        .map(|w| (math::sq(w[1].0 - w[0].0) + math::sq(w[1].1 - w[0].1)).sqrt())
        .sum();
    let reach = (tr * 5.0).min(length * 0.6).max(1e-9);
    let least = 1.0 - 0.9 * bleach;
    for p in pts.iter_mut().rev() {
        let d = (math::sq(p.0 - tx) + math::sq(p.1 - ty)).sqrt();
        if d >= reach {
            break;
        }
        p.2 *= least + (1.0 - least) * d / reach;
    }
}

/// Narrows the last stretch of a branch, about three tip-widths long, so it
/// ends in a slim rounded point instead of a blunt stub. Short stubs keep
/// their round end: tapered, they would read as thorns.
pub fn taper_tip(pts: &mut [(f64, f64, f64, f64)]) {
    let Some(&(tx, ty, tr, _)) = pts.last() else {
        return;
    };
    let length: f64 = pts
        .windows(2)
        .map(|w| (math::sq(w[1].0 - w[0].0) + math::sq(w[1].1 - w[0].1)).sqrt())
        .sum();
    if length < tr * 10.0 {
        return;
    }
    let reach = (tr * 3.0).max(1e-9);
    for p in pts.iter_mut().rev() {
        let d = (math::sq(p.0 - tx) + math::sq(p.1 - ty)).sqrt();
        if d >= reach {
            break;
        }
        p.2 *= 0.6 + 0.4 * d / reach;
    }
}

/// The point of polyline `line` nearest to `p`.
pub fn nearest_on(line: &[(f64, f64)], p: (f64, f64)) -> (f64, f64) {
    let mut best = (f64::MAX, p);
    for (i, &a) in line.iter().enumerate() {
        let b = line.get(i + 1).copied().unwrap_or(a);
        let (ux, uy) = (b.0 - a.0, b.1 - a.1);
        let t = (((p.0 - a.0) * ux + (p.1 - a.1) * uy) / (ux * ux + uy * uy).max(1e-12))
            .clamp(0.0, 1.0);
        let q = (a.0 + ux * t, a.1 + uy * t);
        let d = math::sq(q.0 - p.0) + math::sq(q.1 - p.1);
        if d < best.0 {
            best = (d, q);
        }
    }
    best.1
}

/// Moves a branch's start by `d`, less further along, its tip not at all.
pub fn follow(pts: &mut [(f64, f64, f64, f64)], d: (f64, f64)) {
    let mut arc = vec![0.0; pts.len()];
    for k in 1..pts.len() {
        arc[k] = arc[k - 1]
            + (math::sq(pts[k].0 - pts[k - 1].0) + math::sq(pts[k].1 - pts[k - 1].1)).sqrt();
    }
    let total = arc.last().copied().unwrap_or(0.0).max(1e-9);
    for (p, s) in pts.iter_mut().zip(arc) {
        let f = 1.0 - s / total;
        p.0 += d.0 * f;
        p.1 += d.1 * f;
    }
}

/// A five-point moving average; the ends stay put so chains still meet.
pub fn relax(pts: &[(f64, f64, f64, f64)]) -> Vec<(f64, f64, f64, f64)> {
    let n = pts.len();
    if n < 5 {
        return pts.to_vec();
    }
    (0..n)
        .map(|i| {
            if i == 0 || i == n - 1 {
                return pts[i];
            }
            let (a, b) = (i.saturating_sub(2), (i + 2).min(n - 1));
            let k = (b - a + 1) as f64;
            let mut s = (0.0, 0.0, 0.0, 0.0);
            for p in &pts[a..=b] {
                s = (s.0 + p.0, s.1 + p.1, s.2 + p.2, s.3 + p.3);
            }
            (s.0 / k, s.1 / k, pts[i].2.max(s.2 / k), s.3 / k)
        })
        .collect()
}

/// Ramer-Douglas-Peucker: drops points that lie within `tolerance` of the
/// line through their neighbours, keeping the ends.
pub fn simplify(pts: &[(f64, f64, f64, f64)], tolerance: f64) -> Vec<(f64, f64, f64, f64)> {
    fn keep(
        pts: &[(f64, f64, f64, f64)],
        tolerance: f64,
        out: &mut Vec<bool>,
        lo: usize,
        hi: usize,
    ) {
        if hi <= lo + 1 {
            return;
        }
        let (a, b) = (pts[lo], pts[hi]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt().max(1e-9);
        let mut worst = (0.0, lo);
        for (i, p) in pts.iter().enumerate().take(hi).skip(lo + 1) {
            // Distance from the chord, and a change in radius counts too.
            let d = ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len;
            let t = (i - lo) as f64 / (hi - lo) as f64;
            let dr = (p.2 - (a.2 + (b.2 - a.2) * t)).abs();
            if d.max(dr) > worst.0 {
                worst = (d.max(dr), i);
            }
        }
        if worst.0 > tolerance {
            out[worst.1] = true;
            keep(pts, tolerance, out, lo, worst.1);
            keep(pts, tolerance, out, worst.1, hi);
        }
    }
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = vec![false; pts.len()];
    out[0] = true;
    out[pts.len() - 1] = true;
    keep(pts, tolerance, &mut out, 0, pts.len() - 1);
    pts.iter()
        .zip(out)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

/// Chaikin's corner cutting, once.
pub fn chaikin(pts: &[(f64, f64, f64, f64)]) -> Vec<(f64, f64, f64, f64)> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let mix = |t: f64| {
            (
                a.0 + (b.0 - a.0) * t,
                a.1 + (b.1 - a.1) * t,
                a.2 + (b.2 - a.2) * t,
                a.3 + (b.3 - a.3) * t,
            )
        };
        out.push(mix(0.25));
        out.push(mix(0.75));
    }
    out.push(pts[pts.len() - 1]);
    out
}

/// A seven-lobed palmate leaf (Acer palmatum), pointing up from its stalk at
/// the origin: slender pointed lobes with real width, deep sinuses between.
pub fn maple_leaf_path() -> String {
    let lobes = [
        (-120.0, 0.5),
        (-80.0, 0.78),
        (-40.0, 0.96),
        (0.0, 1.05),
        (40.0, 0.96),
        (80.0, 0.78),
        (120.0, 0.5),
    ];
    let len = 4.8;
    let pt = |deg: f64, r: f64| {
        let a = math::rad(deg - 90.0);
        (math::cos(a) * r, math::sin(a) * r - 0.4)
    };
    let mut d = PathData::new();
    d.m(pt(-140.0, 1.1));
    for (deg, l) in lobes {
        let l = l * len;
        d.c(
            pt(deg - 17.0, 0.45 * l),
            pt(deg - 8.0, 0.88 * l),
            pt(deg, l),
        );
        d.c(
            pt(deg + 8.0, 0.88 * l),
            pt(deg + 17.0, 0.45 * l),
            pt(deg + 20.0, 1.25),
        );
    }
    d.l(pt(140.0, 1.1)).z();
    // The stalk.
    d.m((-0.22, 0.4))
        .l((0.22, 0.4))
        .l((0.12, 2.6))
        .l((-0.12, 2.6))
        .z();
    d.as_str().to_string()
}

/// Paints the tree, back to front.
pub fn paint(b: &Bonsai, config: &Config, theme: Theme) -> Painted {
    let paints = Paints::of(theme);
    let scene = Scene::build(b);
    let animate = config.render.animate;
    let seed = b.account_id;
    // A tree is lit from its open side; a row of trees by one light.
    let lit = config.render.light.unwrap_or(b.leader.side);
    let light = v_norm(light(lit));
    // The light's direction on screen, for which side of a branch is lit.
    let (lx, ly, _) = project(v_scale(light, 100.0));
    let ls = norm((lx - CENTER_X, ly - SOIL_Y));
    let haze = |c: Rgb, depth: f64| {
        let back = (-depth / 140.0).clamp(0.0, 1.0);
        c.shift(0.05 * back, -0.03 * back)
    };
    // Leaves and clouds at the back fade into the air; front ones darken.
    let air = if paints.dark {
        Rgb::hex(0x3a4250)
    } else {
        Rgb::hex(0xb9c3cf)
    };
    let mut out = Painted::default();
    let mut items: Vec<(f64, String)> = Vec::new();
    // The trunk's foot on the canvas, where it goes into the soil.
    let (fx, foot_y) =
        scene
            .chains
            .first()
            .and_then(|c| c.0.first())
            .map_or((CENTER_X, SOIL_Y), |p| {
                let (x, y, _) = project(p.0);
                (x, y)
            });
    let mut extent = (f64::MAX, f64::MIN, SOIL_Y);
    // The lowest point drawn: a cascade hangs below the pot.
    let mut low = SOIL_Y;

    // ---- wood, in pieces so leaves can pass in front of and behind it ----
    // Thin twigs, living and dead, with their outlines and whether dead:
    // drawn one by one, back to front, both kinds in one order ([`layer`]).
    let mut twigs: Vec<(Twig, String, bool)> = Vec::new();
    // Where each chain's twig is, by chain.
    let mut twig_at: Vec<Option<usize>> = vec![None; scene.chains.len()];
    // Each chain's line as drawn, by index.
    let mut lines: Vec<Vec<(f64, f64)>> = Vec::with_capacity(scene.chains.len());
    // Each chain's drawn line with its radius, for what must touch it.
    let mut drawn: Vec<Vec<(f64, f64, f64)>> = Vec::with_capacity(scene.chains.len());
    // Dead wood left out, by chain: what grows from it is left out too.
    let mut dropped = vec![false; scene.chains.len()];
    // Bare dead whips, and those cut short at the crown, by chain: a
    // branched dead top is kept as it grew.
    let whip = whips(&scene.chains, &scene.joins, &scene.strands);
    let mut cut = vec![false; scene.chains.len()];
    // Trunk pieces as drawn: (item, outline points, dead). A dead main trunk,
    // or one with a shari, always shows: a strand crossing it passes behind.
    let mut trunk_pieces: Vec<(usize, Vec<(f64, f64, f64)>, bool)> = Vec::new();
    // A low strand's run into the soil, (item, where in its paint, path):
    // added only if the strand ends up in front of the trunk's foot, where
    // it shows, so a tree that never shows it keeps every byte for leaves.
    let mut soil_runs: Vec<(usize, usize, String)> = Vec::new();
    // The main trunk's lowest piece, by item.
    let mut trunk_foot: Option<usize> = None;
    // Each shari as measured on the trunk: (item, where in its paint, strip
    // points, how high it reaches, fill and ink).
    let mut sharis: Vec<(
        usize,
        usize,
        Vec<(f64, f64, f64, (f64, f64), f64)>,
        f64,
        String,
    )> = Vec::new();
    // Living limbs and strands as drawn, (item, outline): a shari ends below
    // those in front of the trunk.
    let mut living: Vec<(usize, Vec<(f64, f64, f64)>)> = Vec::new();
    // Every thick piece of wood as drawn, and every twig, for what peeks out
    // from behind them.
    let mut wood_pieces: Vec<Vec<(f64, f64, f64)>> = Vec::new();
    let mut twig_pieces: Vec<Vec<(f64, f64, f64)>> = Vec::new();
    // A cascade's wood is slender.
    let slim = {
        let w = b.leader.cascade.abs();
        1.0 - 0.4 * w * w * (3.0 - 2.0 * w)
    };
    // The crown on the canvas, `(x, y, r)`: the trunk, living wood and leaves.
    let crown: Vec<(f64, f64, f64)> = scene
        .chains
        .iter()
        .enumerate()
        .filter(|(ci, c)| *ci == 0 || c.1.is_none())
        .flat_map(|(_, c)| c.0.iter().map(|q| (q.0, q.1 * WORLD)))
        .chain(scene.tufts.iter().map(|t| (t.p, 0.0)))
        .map(|(p, r)| {
            let (x, y, _) = project(p);
            (x, y, r)
        })
        .collect();
    for (ci, (chain, dead)) in scene.chains.iter().enumerate() {
        // Dead wood greys from bark to bleached; past half way it is drawn
        // the way jin is, inked and weathered.
        let bleach = dead.unwrap_or(0.0);
        let jin = &(dead.is_some() && bleach >= 0.5);
        let pts: Vec<(f64, f64, f64, f64)> = chain
            .iter()
            .map(|(p, r)| {
                let (x, y, z) = project(*p);
                (x, y, *r * WORLD * slim, z)
            })
            .collect();
        // Relax the growth zigzag, then subdivide; thin twigs need less. The
        // outline is drawn as curves, so few points carry a smooth branch.
        // Smoothed hard before simplifying: a smoother line needs fewer
        // points, so the file shrinks as the wood rounds out.
        let mut relaxed = pts.clone();
        for _ in 0..8 {
            relaxed = relax(&relaxed);
        }
        let thick = pts.iter().any(|p| p.2 > 3.2);
        let mut fine = simplify(&chaikin(&relaxed), if thick { 1.3 } else { 1.2 });
        // Dead wood ends snapped while fresh and weathers to a point as it
        // bleaches.
        let snapped = dead.is_some();
        // A bare dead whip, a thin limb or a strand with only stubs off it,
        // ends where the crown does: past it, it would stretch the frame for
        // wood that is gone.
        if whip[ci] && scene.joins.contains_key(&ci) && (!thick || scene.strands.contains(&ci)) {
            let whole = fine.len();
            cut_near(&mut fine, &crown, WHIP);
            cut[ci] = fine.len() < whole;
        }
        if snapped {
            weather_tip(&mut fine, bleach);
        } else {
            taper_tip(&mut fine);
        }
        // Smoothing pulls a branch off its nodes, so a side branch moves with
        // its parent's drawn line: it leaves the wood, not the air beside it.
        let fork = scene.joins.get(&ci).map(|r| {
            let (x0, y0, z0) = project(r.1);
            let q = lines.get(r.2).map_or((x0, y0), |l| nearest_on(l, (x0, y0)));
            (q.0, q.1, z0, q.0 - x0, q.1 - y0)
        });
        if let Some((.., dx, dy)) = fork {
            follow(&mut fine, (dx, dy));
        }
        lines.push(fine.iter().map(|p| (p.0, p.1)).collect());
        drawn.push(fine.iter().map(|p| (p.0, p.1, p.2)).collect());
        // A limb flares where it leaves its parent, a collar that runs the two
        // woods together instead of one cutting into the other.
        if let Some((r, (x0, y0, ..))) = scene
            .joins
            .get(&ci)
            .zip(fork)
            .filter(|_| thick && !scene.strands.contains(&ci))
        {
            let r = r.0 * WORLD;
            let own = fine.get(fine.len() / 3).map_or(0.0, |p| p.2);
            let reach = (own * 3.0).max(1e-9);
            // Widest where it leaves the parent's surface, back to its own
            // girth inside and out, and never wider than the parent.
            for p in &mut fine {
                let d = (math::sq(p.0 - x0) + math::sq(p.1 - y0)).sqrt() - r;
                // The collar acts only near the join; past it a limb keeps its
                // own girth and may thin to its tip.
                if d >= reach {
                    continue;
                }
                let k = if d < 0.0 {
                    1.0 + d / (0.5 * r).max(1e-9)
                } else {
                    1.0 - d / reach
                }
                .clamp(0.0, 1.0);
                p.2 = p.2.max((own * (1.0 + 0.7 * k * k)).min(0.9 * r));
            }
        }
        let strand = scene.strands.contains(&ci);
        // What grows from dead wood left out goes with it: no twig hangs in
        // the air. A chain always comes after the chain it grows from. What
        // grew past a cut would reach back to it across the air.
        let past_cut = |r: &(f64, V3, usize)| {
            cut[r.2] && fork.is_some_and(|(.., dx, dy)| math::sq(dx) + math::sq(dy) > 36.0)
        };
        if scene
            .joins
            .get(&ci)
            .is_some_and(|r| dropped[r.2] || past_cut(r))
        {
            dropped[ci] = true;
            continue;
        }
        // A thin dead strand winds behind the trunk and shows only as a stray
        // spike beside it: it is left out.
        if *jin && strand && !thick && pts.iter().all(|p| p.2 < 1.2) {
            dropped[ci] = true;
            continue;
        }
        // A dead twig that has come away from the wood it grew from would
        // float in the air, a spike with nothing under it: it is left out.
        if *jin && !thick && !strand {
            let off = scene.joins.get(&ci).zip(fine.first()).and_then(|(r, p)| {
                drawn
                    .get(r.2)?
                    .iter()
                    .map(|q| (math::sq(q.0 - p.0) + math::sq(q.1 - p.1)).sqrt() - q.2)
                    .reduce(f64::min)
            });
            if off.is_some_and(|d| d > 6.0) {
                dropped[ci] = true;
                continue;
            }
        }
        // Only wood that is drawn sets the frame.
        for p in &fine {
            extent = (
                extent.0.min(p.0 - p.2),
                extent.1.max(p.0 + p.2),
                extent.2.min(p.1 - p.2),
            );
            low = low.max(p.1 + p.2);
        }
        // Thin twigs are drawn behind the leaves.
        if !thick && !strand {
            // A dead twig starts at the centre of the wood it grows from,
            // hidden there: smoothing can pull that wood's edge off the twig's
            // start, and its pale cut end would stand in the air beside it.
            if let (true, Some(l), Some(p)) = (
                *jin,
                scene.joins.get(&ci).and_then(|j| lines.get(j.2)),
                fine.first_mut(),
            ) {
                (p.0, p.1) = nearest_on(l, (p.0, p.1));
            }
            let seg: Vec<(f64, f64, f64)> = fine.iter().map(|p| (p.0, p.1, p.2)).collect();
            // Its place among the twigs is set when they are drawn
            // ([`layer`]): by depth, and behind the twig it grows from, so its
            // flat start never draws a line across it.
            let parent = scene.joins.get(&ci).and_then(|r| twig_at[r.2]);
            let twig = Twig {
                line: fine.iter().map(|p| (p.0, p.1, p.3)).collect(),
                z: fine.iter().map(|p| p.3).sum::<f64>() / fine.len().max(1) as f64,
                w: fine.iter().map(|p| p.2).fold(0.0, f64::max),
                parent,
            };
            twig_at[ci] = Some(twigs.len());
            twigs.push((twig, tube(&seg, true, false).as_str().to_string(), *jin));
            twig_pieces.push(seg);
            continue;
        }
        // Dead wood leaving other wood starts in a point back inside it, and
        // is drawn whole at its own depth: the narrowing start lies on that
        // wood where the limb crosses it, and hides inside it otherwise.
        let tongue = scene.joins.get(&ci).filter(|_| *jin).map(|r| r.0 * WORLD);
        if let Some(r) = tongue {
            point_start(&mut fine, 0.8 * r);
        }
        let piece = 36;
        let mut start = 0;
        while start + 1 < fine.len() {
            let end = (start + piece).min(fine.len() - 1);
            // A side limb's first stretch, inside the wood it leaves, lies
            // behind that wood: every limb comes out from behind its parent.
            // Only a limb that would otherwise paint in front needs the cut.
            let ahead =
                fine[..=end].iter().map(|p| p.3).sum::<f64>() / (end + 1) as f64 > fine[0].3 - 5.0;
            let join = (start == 0 && !strand && ahead && tongue.is_none())
                .then(|| scene.joins.get(&ci).zip(fork))
                .flatten()
                .map(|(r, (x0, y0, z0, ..))| {
                    let k = (1..end)
                        .find(|&k| {
                            (math::sq(fine[k].0 - x0) + math::sq(fine[k].1 - y0)).sqrt()
                                > r.0 * WORLD
                        })
                        .unwrap_or(end);
                    (k, z0.min(fine[0].3) - 5.0)
                });
            let end = join.map_or(end, |j| j.0);
            // Each piece runs two points into the next, which covers the seam,
            // so only the branch's real tip gets a cap.
            let last = end == fine.len() - 1;
            let reach = if last {
                end
            } else {
                (end + 2).min(fine.len() - 1)
            };
            let mut seg: Vec<(f64, f64, f64)> = fine[start..=reach]
                .iter()
                .map(|p| (p.0, p.1, p.2))
                .collect();
            // Wood from the foot runs straight down into the soil: cut square
            // across a steep lean, one corner of the cut would stand above it.
            if start == 0 && !scene.joins.contains_key(&ci) {
                let (x, y, r) = seg[0];
                seg.splice(0..0, [(x, y + 2.0 * r, r), (x, y + r, r)]);
            }
            // So does a living second trunk leaving it low, as a piece of
            // its own under the strand, which keeps its shape (dead wood
            // starts where it was cut).
            let soil = (start == 0 && strand && !*jin && scene.joins.contains_key(&ci))
                .then(|| {
                    let mut run = vec![seg[0]];
                    into_soil(&mut run, foot_y).then(|| tube(&run, false, false))
                })
                .flatten();
            let depth = join.map_or(
                fine[start..=end].iter().map(|p| p.3).sum::<f64>() / (end - start + 1) as f64,
                |j| j.1,
            );

            // Living wood under the crown lies in its shadow, so limbs that
            // cross there read apart from each other and from the lit trunk.
            let shade = if dead.is_some() {
                0.0
            } else {
                let at = |k: usize| chain[k * (chain.len() - 1) / (fine.len() - 1).max(1)].0;
                (start..=end)
                    .map(|k| crown_shadow(&scene.pads, at(k)))
                    .sum::<f64>()
                    / (end - start + 1) as f64
            };
            let base = haze(
                paints
                    .bark
                    .mix(paints.jin, bleach)
                    .shift(-0.13 * shade, 0.0),
                depth,
            );
            let splinter = snapped && last && bleach < 0.8;
            let d = tube_from(
                &seg,
                start == 0 && scene.joins.contains_key(&ci) && tongue.is_none(),
                last && !splinter,
                true,
            );
            // Deadwood gets an inked edge, the way jin is drawn: bleached
            // wood alone is invisible on a white page.
            let mut s = if *jin {
                format!(
                    r#"<path d="{}" fill="{base}" stroke="{}" stroke-width="0.8" stroke-linejoin="round"/>"#,
                    d.as_str(),
                    paints.jin_ink
                )
            } else if strand {
                // A strand lies in a soft groove of its own shadow: it reads
                // apart from the trunk without an outline.
                let wide = groove(&seg, start == 0);
                let shadow = format!(
                    r#"<path d="{}" fill="{}"/>"#,
                    tube_from(
                        &wide,
                        start == 0 && scene.joins.contains_key(&ci),
                        last,
                        true
                    )
                    .as_str(),
                    base.shift(-0.1, 0.0),
                );
                // The run into the soil goes between the groove and the
                // strand, in the strand's color.
                if let Some(run) = &soil {
                    soil_runs.push((
                        items.len(),
                        shadow.len(),
                        format!(
                            r#"<path d="{}" fill="{}"/>"#,
                            run.as_str(),
                            base.shift(0.03, 0.0)
                        ),
                    ));
                }
                format!(
                    r#"{shadow}<path d="{}" fill="{}"/>"#,
                    d.as_str(),
                    base.shift(0.03, 0.0)
                )
            } else {
                format!(r#"<path d="{}" fill="{base}"/>"#, d.as_str())
            };
            if splinter && seg.len() > 1 {
                // Torn fibres across the broken end, shorter as it weathers.
                let m = seg.len();
                let (x, y, r) = seg[m - 1];
                let (tx, ty) = norm((x - seg[m - 2].0, y - seg[m - 2].1));
                let (nx, ny) = (-ty, tx);
                let at =
                    |u: f64, f: f64| (x + nx * r * u + tx * r * f, y + ny * r * u + ty * r * f);
                let mut d = PathData::coarse();
                d.m(at(-1.0, -0.6));
                for k in 0..4 {
                    let u0 = -1.0 + 0.5 * k as f64;
                    let h = ((ci * 31 + k * 17) % 7) as f64 / 6.0;
                    d.l(at(u0, 0.0));
                    d.l(at(
                        0.8 * (u0 + 0.25),
                        (0.4 + 0.9 * h) * 1.6 * (1.0 - bleach),
                    ));
                }
                d.l(at(1.0, 0.0));
                d.l(at(1.0, -0.6));
                d.z();
                let ink = if *jin {
                    format!(
                        r#" stroke="{}" stroke-width="0.8" stroke-linejoin="round""#,
                        paints.jin_ink
                    )
                } else {
                    String::new()
                };
                let _ = write!(s, r#"<path d="{}" fill="{base}"{ink}/>"#, d.as_str());
            }
            let thick = seg.iter().any(|p| p.2 > 6.5);
            // A young tree is framed close, so its trunk is drawn as large as
            // an old one: it keeps a lit side, though its shadow stays soft.
            let young = !thick && ci == 0 && seg.iter().any(|p| p.2 > 3.2);
            if thick || seg.iter().any(|p| p.2 > 1.2) {
                // The shadow falls on whichever side faces away from the light.
                let m = seg.len();
                let away = |seg: &[(f64, f64, f64)], i: usize| {
                    let m = seg.len();
                    let (nb, pb) = (seg[(i + 1).min(m - 1)], seg[i.saturating_sub(1)]);
                    let t = norm((nb.0 - pb.0, nb.1 - pb.1));
                    let n = (-t.1, t.0);
                    if n.0 * ls.0 + n.1 * ls.1 > 0.0 {
                        (-n.0, -n.1)
                    } else {
                        n
                    }
                };
                let strip = |from: f64, to: f64| {
                    let offset = |i: usize, k: f64| {
                        let n = away(&seg, i);
                        (seg[i].0 + n.0 * seg[i].2 * k, seg[i].1 + n.1 * seg[i].2 * k)
                    };
                    // How far the line from point `a` to point `b` passes
                    // from point `i`.
                    let off = |a: usize, b: usize, i: usize| {
                        let (p, q, s) = (seg[a], seg[b], seg[i]);
                        let t = norm((q.0 - p.0, q.1 - p.1));
                        ((s.0 - p.0) * t.1 - (s.1 - p.1) * t.0).abs()
                    };
                    // Strips are soft bands inside the wood: a point every
                    // dozen units or so carries them on a smooth curve.
                    let mut idx = vec![0];
                    let mut run = 0.0;
                    for i in 1..m {
                        run += (math::sq(seg[i].0 - seg[i - 1].0)
                            + math::sq(seg[i].1 - seg[i - 1].1))
                        .sqrt();
                        // A bend the band would cut across, out of thin wood,
                        // keeps its point.
                        let bend = i + 1 < m && off(idx[idx.len() - 1], i + 1, i) > 0.5 * seg[i].2;
                        if run >= 14.0 || i == m - 1 || bend {
                            idx.push(i);
                            run = 0.0;
                        }
                    }
                    let there: Vec<(f64, f64)> = idx.iter().map(|&i| offset(i, from)).collect();
                    let back: Vec<(f64, f64)> = idx.iter().rev().map(|&i| offset(i, to)).collect();
                    let mut d = PathData::coarse();
                    d.m(there[0]);
                    smooth_through(&mut d, &there);
                    d.l(back[0]);
                    smooth_through(&mut d, &back);
                    d.z();
                    d
                };
                // Jin is bleached: push its shadow and light further apart.
                let (shade, lit) = if *jin {
                    (base.shift(-0.14, -0.01), base.shift(0.10, 0.0))
                } else {
                    (base.shift(-0.09, 0.0), base.shift(0.07, 0.0))
                };
                if thick {
                    let _ = write!(
                        s,
                        r#"<path d="{}" fill="{shade}"/>"#,
                        strip(0.05, 0.98).as_str(),
                    );
                }
                if thick || young {
                    let _ = write!(
                        s,
                        r#"<path d="{}" fill="{lit}"/>"#,
                        strip(-0.35, -0.72).as_str(),
                    );
                }
                if !*jin {
                    // Every branch has a soft shadow underneath, so branches
                    // crossing in one bark color still read one by one.
                    let _ = write!(
                        s,
                        r#"<path d="{}" fill="{}"/>"#,
                        strip(0.7, 0.97).as_str(),
                        base.shift(-0.1, 0.0)
                    );
                }
                if let Some(g) = scene.shari.filter(|_| ci == 0 && start == 0) {
                    // A shari: the founding language's dead wood, a strip up
                    // the living trunk that narrows to a point as it climbs and
                    // reaches higher as it bleaches. It is measured along the
                    // whole trunk and drawn once, with its first piece.
                    let top = 0.2 + 0.25 * g;
                    let mut seg: Vec<(f64, f64, f64)> =
                        fine.iter().map(|p| (p.0, p.1, p.2)).collect();
                    if !scene.joins.contains_key(&ci) {
                        let (x, y, r) = seg[0];
                        seg.splice(0..0, [(x, y + 2.0 * r, r), (x, y + r, r)]);
                    }
                    let m = seg.len();
                    // How far along the trunk, 0 at the foot to 1 at the
                    // tip: by length, so a trunk that cascades below its
                    // foot measures the same way.
                    let lens: Vec<f64> = (0..m.saturating_sub(1))
                        .map(|i| {
                            (math::sq(seg[i + 1].0 - seg[i].0) + math::sq(seg[i + 1].1 - seg[i].1))
                                .sqrt()
                        })
                        .collect();
                    let total = lens.iter().sum::<f64>().max(1e-9);
                    // The outline is simplified to few points: the strip takes
                    // its own, every few units along the wood.
                    let mut at: Vec<(f64, f64, f64, (f64, f64), f64)> = Vec::new();
                    let mut walked = 0.0;
                    for i in 0..m.saturating_sub(1) {
                        let (p, q) = (seg[i], seg[i + 1]);
                        let len = lens[i];
                        let n = (len / 3.0).ceil().max(1.0) as usize;
                        for j in 0..n {
                            let u = j as f64 / n as f64;
                            let t = (walked + len * u) / total;
                            if t <= top {
                                let (x, y) = (p.0 + (q.0 - p.0) * u, p.1 + (q.1 - p.1) * u);
                                at.push((x, y, p.2 + (q.2 - p.2) * u, away(&seg, i), t));
                            }
                        }
                        walked += len;
                    }
                    // Drawn once every piece of wood has its depth, here in
                    // the trunk's paint: it ends below living wood in front.
                    let ink = if g >= 0.5 {
                        format!(r#" stroke="{}" stroke-width=".6""#, paints.jin_ink)
                    } else {
                        String::new()
                    };
                    let fill = haze(paints.bark.mix(paints.jin, 0.8 * g), depth).shift(0.04, 0.0);
                    sharis.push((
                        items.len(),
                        s.len(),
                        at,
                        top,
                        format!(r#"fill="{fill}"{ink}"#),
                    ));
                }
                if ci == 0 && seg.iter().any(|p| p.2 > 3.2) {
                    // Bark on the trunk: a long line that follows the wood.
                    let mut grain = PathData::coarse();
                    for off in [-0.2] {
                        for (i, p) in seg.iter().enumerate() {
                            let (nb, pb) = (seg[(i + 1).min(m - 1)], seg[i.saturating_sub(1)]);
                            let t = norm((nb.0 - pb.0, nb.1 - pb.1));
                            let q = (p.0 - t.1 * p.2 * off, p.1 + t.0 * p.2 * off);
                            if i == 0 {
                                grain.m(q)
                            } else {
                                grain.l(q)
                            };
                        }
                    }
                    let _ = write!(
                        s,
                        r#"<path d="{}" fill="none" stroke="{}" stroke-width=".45" opacity=".3"/>"#,
                        grain.as_str(),
                        base.shift(-0.12, 0.0)
                    );
                }
            }
            if *jin && seg.iter().any(|p| p.2 > 1.4) {
                // Weathered grain that spirals along the dead wood.
                let m = seg.len();
                for (k, off) in [(0.0, -0.32), (2.1, 0.28)] {
                    let mut grain = PathData::coarse();
                    for (i, p) in seg.iter().enumerate() {
                        let (nb, pb) = (seg[(i + 1).min(m - 1)], seg[i.saturating_sub(1)]);
                        let t = norm((nb.0 - pb.0, nb.1 - pb.1));
                        let o = off + 0.22 * math::sin(i as f64 * 0.7 + k);
                        let q = (p.0 - t.1 * p.2 * o, p.1 + t.0 * p.2 * o);
                        if i == 0 {
                            grain.m(q)
                        } else {
                            grain.l(q)
                        };
                    }
                    let _ = write!(
                        s,
                        r#"<path d="{}" fill="none" stroke="{}" stroke-width=".4" stroke-linecap="round" opacity=".55"/>"#,
                        grain.as_str(),
                        paints.jin_ink
                    );
                }
            }
            let outline: Vec<(f64, f64, f64)> =
                fine[start..=end].iter().map(|p| (p.0, p.1, p.2)).collect();
            if ci == 0 || strand {
                // Only the main trunk's dead wood comes to the front: a dead
                // strand keeps winding round the trunk, behind it as it turns.
                let dead = ci == 0 && (dead.is_some() || scene.shari.is_some());
                trunk_pieces.push((items.len(), outline.clone(), dead));
            }
            if ci != 0 && dead.is_none() {
                living.push((items.len(), outline.clone()));
            }
            wood_pieces.push(outline);
            if ci == 0 && start == 0 {
                trunk_foot = Some(items.len());
            }
            items.push((depth, s));
            start = end;
        }
    }
    for (live, outline, _) in trunk_pieces.iter().filter(|t| !t.2) {
        let crosses = |other: &Vec<(f64, f64, f64)>| {
            outline.iter().any(|p| {
                other
                    .iter()
                    .any(|q| math::sq(p.0 - q.0) + math::sq(p.1 - q.1) < math::sq(p.2 + q.2))
            })
        };
        let behind = trunk_pieces
            .iter()
            .filter(|t| t.2 && crosses(&t.1))
            .map(|t| items[t.0].0 - 1.0)
            .fold(items[*live].0, f64::min);
        items[*live].0 = behind;
    }
    for (item, at_byte, run) in soil_runs {
        if trunk_foot.is_none_or(|t| items[item].0 > items[t].0) {
            items[item].1.insert_str(at_byte, &run);
        }
    }
    // Beside living wood in front of the trunk the pale strip would read as
    // part of that wood: the shari narrows to its point below it.
    for (item, at_byte, at, top, paint) in sharis {
        let wander = |t: f64| 0.1 + 0.22 * math::sin(t / top * 5.0);
        let mid: Vec<(f64, f64, f64, f64)> = at
            .iter()
            .map(|&(x, y, r, n, t)| (x + n.0 * r * wander(t), y + n.1 * r * wander(t), r, t))
            .collect();
        let front: Vec<&[(f64, f64, f64)]> = living
            .iter()
            .filter(|l| items[l.0].0 > items[item].0)
            .map(|l| l.1.as_slice())
            .collect();
        let end = shari_end(&mid, &front, top);
        let at: Vec<_> = at.into_iter().filter(|a| a.4 <= end).collect();
        if at.len() > 2 {
            // It wanders with the grain, and its edges are torn.
            let edge = |&(x, y, r, n, t): &(f64, f64, f64, (f64, f64), f64), side: f64| {
                let w = 0.26
                    * (1.0 - t / end).max(0.0).sqrt()
                    * (1.0 + 0.3 * math::sin(t * 90.0 + 2.0 * side));
                let k = wander(t) + side * w;
                (x + n.0 * r * k, y + n.1 * r * k)
            };
            let there: Vec<(f64, f64)> = at.iter().map(|a| edge(a, -1.0)).collect();
            let back: Vec<(f64, f64)> = at.iter().rev().map(|a| edge(a, 1.0)).collect();
            let mut d = PathData::coarse();
            d.m(there[0]);
            smooth_through(&mut d, &there);
            d.l(back[0]);
            smooth_through(&mut d, &back);
            d.z();
            items[item]
                .1
                .insert_str(at_byte, &format!(r#"<path d="{}" {paint}/>"#, d.as_str()));
        }
    }

    // Each twig outlined, drawn back to front: the twig in front hides the
    // one behind it, its outline too, so crossing twigs read as one over the
    // other, not one flat shape. Living twigs get a faint darker outline;
    // dead ones their ink, or they vanish on a white page.
    // Living and dead twigs are layered together, so a dead twig passes
    // behind a living one as well as in front of it. The living twigs'
    // paint is the group's; each run of dead twigs, their ink, in a group
    // of its own: short, so the file stays small.
    if !twigs.is_empty() {
        let shapes: Vec<Twig> = twigs.iter().map(|t| t.0.clone()).collect();
        let mut s = format!(
            r#"<g fill="{}" stroke="{}" stroke-width="0.35" stroke-linejoin="round">"#,
            paints.bark,
            paints.bark.shift(-0.08, 0.0)
        );
        let mut dead_run = false;
        for i in layer(&shapes) {
            let dead = twigs[i].2;
            if dead != dead_run {
                if dead {
                    let _ = write!(
                        s,
                        r#"<g fill="{}" stroke="{}" stroke-width=".7">"#,
                        paints.jin, paints.jin_ink
                    );
                } else {
                    s.push_str("</g>");
                }
                dead_run = dead;
            }
            let _ = write!(s, r#"<path d="{}"/>"#, twigs[i].1);
        }
        if dead_run {
            s.push_str("</g>");
        }
        s.push_str("</g>");
        items.push((f64::MIN, s));
    }

    // ---- leaves: each pad lit as one smooth ball in three soft tones, the
    // way painted backgrounds shade foliage; bronze flush; samaras ----
    let smoothstep = |e0: f64, e1: f64, v: f64| {
        let t = ((v - e0) / (e1 - e0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let mut fallen: Vec<(f64, f64, f64)> = Vec::new();
    for (j, t) in scene.tufts.iter().enumerate() {
        let (x, y, depth) = project(t.p);
        let diffuse = v_dot(t.n, light).max(0.0);
        let shade = (0.25 + 0.75 * diffuse) * (0.55 + 0.45 * t.q);
        // Light from the pad's ball normal and height in the pad, then a
        // ramp: cool shadow, the leaf color, warm light, with soft steps.
        let s = 0.62 * diffuse + 0.38 * t.q;
        // A lifted leaf is colored for where it is drawn: in front, not
        // faded into the air behind.
        let z = t.p[2] + t.lift;
        let back = (-z / 70.0).clamp(0.0, 1.0);
        let front = (z / 70.0).clamp(0.0, 1.0);
        let seen = depth + t.lift;
        let lighting = |base: Rgb| {
            let dark = base.shift(-0.15, -0.01).mix(Rgb::hex(0x2f3d63), 0.28);
            let lit = base.shift(0.09, -0.005).mix(Rgb::hex(0xfff0c2), 0.22);
            let mut c = base
                .mix(dark, 1.0 - smoothstep(0.33, 0.39, s))
                .mix(lit, smoothstep(0.66, 0.72, s));
            if t.fresh {
                // Japanese maples flush bronze-red before the leaves settle.
                c = c.mix(Rgb::hex(0xc8553d), 0.45).shift(0.06, 0.0);
            }
            haze(c, seen)
                .mix(air, 0.34 * back)
                .shift(-0.04 * front, 0.0)
        };
        let base = if t.private {
            private_tone(t.color, paints.dark)
        } else {
            t.color.shift(paints.leaf_shift, 0.0)
        };
        let c = lighting(base);
        let h = hash01(seed, j as i64 + 7);
        let (sx, sy, _) = project(v_add(t.p, t.n));
        // Turned the mirror way on a left tree.
        let angle =
            math::deg(math::atan2(sy - y, sx - x)) + 90.0 + b.leader.side * (h - 0.5) * 50.0;
        let symbol = if h < 0.5 { "t" } else { "u" };
        let scale = WORLD
            * t.size
            * (0.85 + 0.3 * hash01(seed, j as i64 + 11))
            * if t.fresh { 0.75 } else { 1.0 };
        // Larger leaves on a thinned crown reach further.
        let reach = 8.0 / b.leaf_share.sqrt();
        extent = (
            extent.0.min(x - reach),
            extent.1.max(x + reach),
            extent.2.min(y - reach),
        );
        low = low.max(y + reach);
        let mut s = format!(
            r##"<use href="#{symbol}" transform="translate({} {})rotate({})scale({})" fill="{c}"/>"##,
            Num::coarse(x),
            Num::coarse(y),
            Num::coarse(angle),
            Num::coarse(scale)
        );
        if let Some(tip) = t.tip {
            // A grey leaf's tip in a public language's color: the same leaf,
            // smaller, toward the tip.
            let a = math::rad(angle);
            let k = 2.6 * scale;
            let _ = write!(
                s,
                r##"<use href="#{symbol}" transform="translate({} {})rotate({})scale({})" fill="{}"/>"##,
                Num::coarse(x + math::sin(a) * k),
                Num::coarse(y - math::cos(a) * k),
                Num::coarse(angle),
                Num::coarse(scale * 0.5),
                lighting(tip.shift(paints.leaf_shift, 0.0))
            );
        }
        if t.samara {
            // Samaras: a pair of winged seeds hanging below the leaves.
            let red = paints.crimson.shift((shade - 0.6) * 0.18, 0.0);
            let _ = write!(
                s,
                r##"<use href="#sm" transform="translate({} {})rotate({})scale({})" fill="{red}"/>"##,
                Num::coarse(x + (h - 0.5) * 5.0),
                Num::coarse(y + 2.0),
                Num::coarse((h - 0.5) * 40.0),
                Num(WORLD * (1.15 + 0.3 * h))
            );
        }
        items.push((seen + 0.5, s));
        if animate
            && b.extras.bloom.is_some()
            && fallen.len() < 9
            && t.q > 0.5
            && hash01(seed ^ 0xfa, j as i64) < 0.03
        {
            fallen.push((x, y, h));
        }
    }

    // ---- solid cloud masses behind each cloud's leaves ----
    for (mi, (c3, rx, ry, rz, color, private)) in scene.masses.iter().enumerate() {
        let (cx, cy, depth) = project(*c3);
        let (s, c) = (math::sin(TILT), math::cos(TILT));
        let up = ((ry * c) * (ry * c) + (rz * s) * (rz * s)).sqrt() * WORLD * 0.95;
        let down = rz * s * WORLD * 0.9;
        let hw = rx * WORLD * 0.97;
        let ph = hash01(seed ^ 0xb1, mi as i64) * math::TAU;
        let n = 40;
        let pts: Vec<(f64, f64)> = (0..=n)
            .map(|i| {
                let a = math::TAU * f64::from(i % n) / f64::from(n);
                let lobe =
                    1.0 + 0.07 * math::sin(7.0 * a + ph) + 0.05 * math::sin(11.0 * a + 2.0 * ph);
                let (ca, sa) = (math::cos(a), math::sin(a));
                let v = if sa < 0.0 { up } else { down };
                (cx + ca * hw * lobe, cy + sa * v * lobe)
            })
            .collect();
        let mut d = PathData::coarse();
        d.m(pts[0]);
        smooth_through(&mut d, &pts);
        d.z();
        let base = if *private {
            private_tone(*color, paints.dark)
        } else {
            color.shift(paints.leaf_shift, 0.0)
        };
        let dark = base.shift(-0.15, -0.01).mix(Rgb::hex(0x2f3d63), 0.28);
        let back = (-c3[2] / 70.0).clamp(0.0, 1.0);
        let fill = haze(base.mix(dark, 0.55), depth).mix(air, 0.34 * back);
        extent = (
            extent.0.min(cx - hw),
            extent.1.max(cx + hw),
            extent.2.min(cy - up),
        );
        low = low.max(cy + up);
        items.push((
            depth - 0.3 * rz * c - 0.5,
            format!(r#"<path d="{}" fill="{fill}"/>"#, d.as_str()),
        ));
    }

    // ---- level 11: drip tubing along the wood ----
    for line in &scene.drip {
        let pts: Vec<(f64, f64, f64, f64)> = line
            .iter()
            .map(|(p, r)| {
                let (x, y, z) = project(*p);
                (x, y, r * WORLD, z)
            })
            .collect();
        let fine = chaikin(&relax(&relax(&pts)));
        let m = fine.len();
        let mut start = 0;
        while start + 1 < m {
            let end = (start + 20).min(m - 1);
            let mut d = PathData::coarse();
            for i in start..=end {
                let (nb, pb) = (fine[(i + 1).min(m - 1)], fine[i.saturating_sub(1)]);
                let t = norm((nb.0 - pb.0, nb.1 - pb.1));
                // Offset toward the viewer's right of the wood, so the tube shows.
                let side = if -t.1 >= 0.0 { 1.0 } else { -1.0 };
                let q = (
                    fine[i].0 - t.1 * fine[i].2 * 0.55 * side,
                    fine[i].1 + t.0 * fine[i].2 * 0.55 * side,
                );
                if i == start {
                    d.m(q)
                } else {
                    d.l(q)
                };
            }
            let depth =
                fine[start..=end].iter().map(|p| p.3).sum::<f64>() / (end - start + 1) as f64;
            items.push((
                depth + 400.0,
                format!(
                    r#"<path d="{}" fill="none" stroke="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"#,
                    d.as_str(),
                    paints.tubing,
                    Num(2.4 * WORLD)
                ),
            ));
            start = end;
        }
    }
    items.sort_by(|a, c| a.0.total_cmp(&c.0));
    // A sprout is drawn leaf by leaf in place of the seedling, which still
    // sizes the pot and frame, so the hand-over doesn't move them, and fades
    // in over the sprout's last contributions.
    let sprout = crate::sprout::draw(b, &scene, &paints, &items, animate);
    if let Some((s, moving)) = &sprout {
        items = vec![(0.0, s.clone())];
        fallen.clear();
        out.animated += moving;
    }

    // ---- the pot, sized to the tree, seen from a little above ----
    // Repotted in steps as the tree grows ([`Bonsai::grow`]).
    let size = b.pot;
    let hw = size / 2.0;
    let base_r = WORLD
        * scene
            .chains
            .first()
            .and_then(|c| c.0.first())
            .map_or(1.0, |p| p.1);
    // As deep as the trunk it was repotted for is wide, the usual rule: the
    // depth changes only at a repotting, as the size does.
    let depth = (b.pot_girth * 2.0).clamp(20.0, 56.0);
    // A semi-cascade stands in a deeper, narrower pot, its rim inside the
    // point where the trunk spills over it.
    let k = {
        let w = b.leader.cascade.abs();
        w * w * (3.0 - 2.0 * w)
    };
    let (hw, depth) = if k > 0.0 {
        let spill = scene.chains.first().and_then(|c| {
            c.0.iter()
                .map(|p| project(p.0))
                .find(|p| p.1 > SOIL_Y + 2.0 && (p.0 - CENTER_X).abs() > base_r)
                .map(|p| (p.0 - CENTER_X).abs() - base_r - 4.0)
        });
        let narrow = hw * (1.0 - 0.35 * k);
        let hw = spill.map_or(narrow, |s| narrow.min(s.max(base_r * 2.0)));
        (hw, depth + k * (hw * 1.2 - depth).max(0.0))
    } else {
        (hw, depth)
    };
    let pot = Pot {
        hw,
        ry: hw * math::sin(TILT) * 0.55,
        depth,
        // Straight-sided for a cascade.
        bw: hw * (0.88 + 0.09 * k),
        glaze: pot_glaze(b.extras.pot_tier, config, &paints),
    };
    // A sprout stands centered in its frame: the seedling to come sizes it,
    // but its sides even out around the pot until the hand-over, so the
    // little plant never sits in a corner of empty sky.
    let extent = if sprout.is_some() {
        let even = 1.0 - crate::sprout::handover(crate::sprout::count(b));
        let half = (CENTER_X - extent.0).max(extent.1 - CENTER_X);
        (
            extent.0 - even * (extent.0 - (CENTER_X - half)),
            extent.1 + even * ((CENTER_X + half) - extent.1),
            extent.2,
        )
    } else {
        extent
    };
    out.frame = frame(extent, &pot);
    // Room below the pot for what hangs there, kept no taller than wide.
    let bottom = ((low + 10.0) / 8.0).ceil() * 8.0;
    if bottom > out.frame.1 + out.frame.3 {
        out.frame.3 = bottom - out.frame.1;
        if out.frame.2 < out.frame.3 {
            let pad = ((out.frame.3 - out.frame.2) / 16.0).ceil() * 8.0;
            out.frame.0 -= pad;
            out.frame.2 += 2.0 * pad;
        }
    }
    // The eggs keep their places on a right tree; a left tree's are their
    // mirror image, drawings and all, as its own light is. In a row lit as
    // one they stay so: where they sit follows the tree's shape, not the
    // row's light.
    let left = b.leader.side < 0.0;
    let flip = |f: &mut Frame| f.0 = 2.0 * CENTER_X - f.0 - f.2;
    // The roots, before the eggs: what peeks out behind the trunk keeps
    // clear of them.
    let (roots_back, roots_front, root_hulls) = nebari(
        b,
        &paints,
        base_r,
        &pot,
        scene.chains.first().map_or(&[][..], |c| &c.0[..]),
        ls,
    );
    let flip_x = |x: f64| if left { 2.0 * CENTER_X - x } else { x };
    let canonical =
        |pieces: &mut dyn Iterator<Item = &Vec<(f64, f64, f64)>>| -> Vec<Vec<(f64, f64, f64)>> {
            pieces
                .map(|t| t.iter().map(|&(x, y, r)| (flip_x(x), y, r)).collect())
                .collect()
        };
    let trunk = canonical(&mut trunk_pieces.iter().map(|t| &t.1));
    let wood = canonical(&mut wood_pieces.iter());
    let twiggy = canonical(&mut twig_pieces.iter());
    let roots: Vec<Vec<(f64, f64)>> = root_hulls
        .iter()
        .map(|h| h.iter().map(|&(x, y)| (flip_x(x), y)).collect())
        .collect();
    // Eggs hang only from living branches that are drawn: a strand winds
    // round the trunk and dead wood weathers, so neither is drawn where
    // its points lie.
    let mut shown = scene.clone();
    for (ci, (chain, gone)) in shown.chains.iter_mut().zip(&dropped).enumerate() {
        if *gone || chain.1.is_some() || scene.strands.contains(&ci) {
            chain.0.clear();
        }
    }
    let egg = if left {
        flip(&mut out.frame);
        let mut e = eggs::eggs(
            b,
            &shown.mirrored(),
            &paints,
            &pot,
            &out.frame,
            (&trunk, &wood, &twiggy, &roots),
        );
        flip(&mut out.frame);
        for layer in [&mut e.behind, &mut e.back, &mut e.front, &mut e.top] {
            *layer = mirror(layer);
        }
        e
    } else {
        eggs::eggs(
            b,
            &shown,
            &paints,
            &pot,
            &out.frame,
            (&trunk, &wood, &twiggy, &roots),
        )
    };

    let body = &mut out.body;
    // The pot's own shadow on the table, soft-edged, the light from the upper
    // left: the pot sits down instead of floating.
    let foot = SOIL_Y + pot.depth;
    let shade = if paints.dark { "#000" } else { "#3b3226" };
    let _ = write!(
        body,
        r#"<ellipse cx="{}" cy="{}" rx="{}" ry="4" fill="{shade}" opacity=".16"/><ellipse cx="{}" cy="{}" rx="{}" ry="2.4" fill="{shade}" opacity=".26"/>"#,
        Num(CENTER_X + 5.0 * lit),
        Num(foot + 1.2),
        Num(pot.bw * 1.18),
        Num(CENTER_X + 3.5 * lit),
        Num(foot + 0.8),
        Num(pot.bw * 1.02),
    );
    body.push_str(&egg.behind);
    let _ = write!(
        body,
        r#"<ellipse cx="{CENTER_X}" cy="{SOIL_Y}" rx="{}" ry="{}" fill="{}"/><ellipse cx="{CENTER_X}" cy="{}" rx="{}" ry="{}" fill="{}"/>"#,
        Num(hw),
        Num(pot.ry),
        pot.glaze.shift(-0.2, 0.0),
        Num(SOIL_Y + 0.6),
        Num(hw - 4.0),
        Num((pot.ry - 2.0).max(1.0)),
        paints.soil
    );
    // The canopy's soft shadow on the soil.
    let canopy_x = f64::midpoint(extent.0, extent.1) + 10.0;
    let cx = canopy_x.clamp(CENTER_X - hw * 0.5, CENTER_X + hw * 0.5);
    // On the soil only: never past the rim.
    let rx = ((hw - 8.0) * 0.75)
        .min(hw - 6.0 - (cx - CENTER_X).abs())
        .max(4.0);
    let _ = write!(
        body,
        r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="{}" opacity=".35"/>"#,
        Num(cx),
        Num(SOIL_Y + 0.6),
        Num(rx),
        Num((pot.ry - 3.0).max(1.0) * 0.7),
        paints.soil.shift(-0.12, 0.0)
    );
    let (moss_back, moss_front) = moss(b, &paints, &pot);
    body.push_str(&moss_back);
    body.push_str(&egg.back);

    body.push_str(&roots_back);
    // The trunk goes into the soil: across its footprint nothing of it shows
    // below its foot, so no flare, strand end or wood run down into the soil
    // lies on the soil under the roots.
    let fy = foot_y;
    let fr = base_r * 1.5;
    let mut cut = PathData::coarse();
    cut.m((fx - 4000.0, fy - 4000.0))
        .l((fx + 4000.0, fy - 4000.0))
        .l((fx + 4000.0, fy + 4000.0))
        .l((fx + fr, fy + 4000.0))
        .l((fx + fr, fy))
        .l((fx - fr, fy))
        .l((fx - fr, fy + 4000.0))
        .l((fx - 4000.0, fy + 4000.0))
        .z();
    let _ = write!(
        body,
        r#"<clipPath id="soil"><path d="{}"/></clipPath><g clip-path="url(#soil)">"#,
        cut.as_str()
    );
    for (_, s) in &items {
        body.push_str(s);
    }
    body.push_str("</g>");
    body.push_str(&roots_front);
    body.push_str(&moss_front);
    // Momiji: in a milestone week a few leaves turn crimson and drift down.
    for (k, (x, y, h)) in fallen.iter().enumerate() {
        let duration = 7.0 + 5.0 * h;
        let _ = write!(
            body,
            r##"<g transform="translate({} {})"><use href="#ml" class="mf" fill="{}" style="animation-duration:{}s;animation-delay:-{}s"/></g>"##,
            Num(*x),
            Num(*y),
            paints.crimson.shift(0.05 * (k % 3) as f64, 0.0),
            Num(duration),
            Num(duration * hash01(seed ^ 0xde, k as i64))
        );
        out.animated += 1;
    }
    if b.is_empty() {
        // A seedling: a thin stem and its first two maple leaves.
        let green = crate::maple::MAPLE_GREEN.shift(paints.leaf_shift, 0.0);
        let _ = write!(
            body,
            r##"<path d="M256 404Q254 396 257 389" fill="none" stroke="{}" stroke-width="1.6" stroke-linecap="round"/><use href="#ml" transform="translate(257 389)rotate(-55)scale(1.3)" fill="{green}"/><use href="#ml" transform="translate(257 389)rotate(50)scale(1.1)" fill="{}"/>"##,
            green.shift(-0.1, 0.0),
            green.shift(0.05, 0.0)
        );
    }
    body.push_str(&egg.front);
    out.animated += egg.animated;
    if !scene.drip.is_empty() {
        out.animated += usize::from(animate);
        body.push_str(&drip_ends(&scene, &paints, &pot, animate));
    }
    // The pot is lit as the tree is.
    let front = pot_front(b, &pot);
    body.push_str(&if lit < 0.0 { mirror(&front) } else { front });
    body.push_str(&eggs::kintsugi(&b.archived, seed, &pot));
    // The maker's seal sits on the pot's shaded half: mirrored with it.
    let marks = pot_marks(b, &pot);
    body.push_str(&if lit < 0.0 { mirror(&marks) } else { marks });
    let rim = pot_rim(&pot);
    body.push_str(&if lit < 0.0 { mirror(&rim) } else { rim });
    body.push_str(&egg.top);
    let signs = crate::legend::Signs {
        fresh: (sprout.is_none() || crate::sprout::handover(crate::sprout::count(b)) > 0.5)
            && scene.tufts.iter().any(|t| t.fresh),
        falling: !fallen.is_empty(),
        glaze: Some(pot.glaze),
    };
    let legend = crate::legend::legend(b, &paints, signs, config.render.legend, &mut out.frame);
    out.body.push_str(&legend);
    out
}

/// The glaze for the pot tier; the config glaze applies from tier 3.
fn pot_glaze(tier: u8, config: &Config, paints: &Paints) -> Rgb {
    let glaze = config.pot.glaze;
    let body = match tier {
        0 => paints.plastic,
        1 => Rgb::hex(0xb5653f),
        2 => Rgb::hex(0x6b4e3d),
        3 => glaze.unwrap_or(Rgb::hex(0x3e5c76)),
        4 => glaze.unwrap_or(Rgb::hex(0x8fb5a0)),
        5 => glaze.unwrap_or(Rgb::hex(0x5f7d86)),
        _ => glaze.unwrap_or(Rgb::hex(0x5a4636)),
    };
    if tier > 0 && paints.dark {
        body.shift(0.05, 0.0)
    } else {
        body
    }
}

/// How far a surface root at azimuth `az` reaches: longer with a stout
/// trunk and old nebari, shorter toward the viewer, and always inside the pot
/// rim (half-width `hw`) from a foot `off` sideways of the pot's center.
/// `u` in `[0, 1)` varies it; `brace` (0 to 1) lengthens a root that holds
/// the tree against the wind.
fn root_reach(base_r: f64, nebari: f64, hw: f64, az: f64, u: f64, brace: f64, off: f64) -> f64 {
    // At least 1.6 trunk radii, so a root shows past the trunk's foot,
    // unless the pot rim needs it shorter.
    let reach = (base_r * (1.35 + u * (0.25 + 0.5 * nebari.clamp(0.0, 1.0)))).max(base_r * 1.6)
        * (1.0 + 0.45 * brace);
    // From the foot to the rim along the root, on the soil.
    let (c, s, rim) = (math::cos(az), math::sin(az), hw * 0.78);
    let to_rim = (-off * c + (math::sq(rim) - math::sq(off * s)).max(0.0).sqrt()).max(0.0);
    reach.min(to_rim * (0.45 + 0.55 * c.abs()))
}

/// Measures how deep wood at `e` lies in the crown's shadow, 0 in the open to
/// 1 right under a cloud's middle: under a pad of `pads` and at most three of
/// its half-heights below its center. The deepest pad counts.
fn crown_shadow(pads: &[(V3, f64, f64)], e: V3) -> f64 {
    pads.iter()
        .map(|(c, rx, ry)| {
            let across = 1.0 - math::sq((e[0] - c[0]) / rx);
            let below = (c[1] - e[1]) / ry;
            if across <= 0.0 || !(0.0..3.0).contains(&below) {
                return 0.0;
            }
            across * (1.0 - below / 3.0)
        })
        .fold(0.0, f64::max)
}

/// A root's azimuth `az` on a tree on `side`: a left tree's roots lie as the
/// right one's mirror image.
fn mirror_az(az: f64, side: f64) -> f64 {
    if side < 0.0 { math::TAU / 2.0 - az } else { az }
}

/// Convex hull of points, counter-clockwise (monotone chain).
fn hull(mut p: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    p.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    let mut h: Vec<(f64, f64)> = Vec::new();
    for pass in 0..2 {
        let start = h.len();
        let iter: Box<dyn Iterator<Item = &(f64, f64)>> = if pass == 0 {
            Box::new(p.iter())
        } else {
            Box::new(p.iter().rev())
        };
        for &q in iter {
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    h
}

/// Surface roots, after a real maple's nebari: rounded ridges lying on the
/// soil, each with a half-elliptic cross-section that widens into the trunk
/// and flattens toward its tip. More roots with age (3 to 7), reaching
/// further and thicker; those facing us are drawn over the trunk. Returns
/// what goes behind the trunk and what goes in front of it.
fn nebari(
    b: &Bonsai,
    paints: &Paints,
    base_r: f64,
    pot: &Pot,
    trunk: &[(V3, f64)],
    ls: (f64, f64),
) -> (String, String, Vec<Vec<(f64, f64)>>) {
    let bark = paints.bark;
    let n = b.extras.roots.max(3);
    let age = b.extras.nebari.clamp(0.0, 1.0);
    let (x0, y0) = trunk.first().map_or((CENTER_X, SOIL_Y), |p| {
        let (x, y, _) = project(p.0);
        (x, y)
    });
    let tilt = pot.ry / pot.hw;
    let lit = if ls.0 < 0.0 { -1.0 } else { 1.0 };
    let mut roots: Vec<(f64, String)> = Vec::new();
    let mut hulls = Vec::new();
    for i in 0..n {
        let mut r = Rng::keyed(b.account_id, &format!("root/{i}"));
        let az = mirror_az(
            0.9 + f64::from(i) * 2.399_963 + r.range(-0.2, 0.2),
            b.leader.side,
        );
        // Roots facing into the wind brace the tree: longer and thicker.
        // And those on the uphill side of a leaning trunk.
        let uphill = -b.leader.tilt.signum() * (b.leader.tilt.abs() / 0.55).min(1.0);
        let brace = (-b.leader.sweep * math::cos(az))
            .max(uphill * math::cos(az))
            .max(0.0);
        let reach = root_reach(
            base_r,
            b.extras.nebari,
            pot.hw,
            az,
            r.unit(),
            brace,
            x0 - CENTER_X,
        );
        let bend = b.leader.side * r.range(-0.3, 0.3);
        // Front roots are seen across their width: slimmer, so each stays
        // its own root.
        let front_k = 1.0 - 0.3 * math::sin(az).max(0.0);
        let w0 =
            base_r * (0.26 + 0.14 * age) * r.range(0.85, 1.15) * front_k * (1.0 + 0.35 * brace);
        let h0 = base_r * (0.3 + 0.2 * age) * r.range(0.85, 1.15) * (1.0 + 0.3 * brace);
        let from = base_r * 0.55;
        // Ground point, ground direction and screen offset of a ground vector.
        let ground = |a: f64, d: f64| (x0 + math::cos(a) * d, y0 + math::sin(a) * d * tilt);
        let k = 20;
        let sec = |t: f64| {
            let a = az + bend * t * t;
            let d = from + (reach - from) * t;
            let g = ground(a, d);
            let (px, pz) = (-math::sin(a), math::cos(a));
            // It widens into the trunk where it leaves it, like a fillet.
            let w = w0 * (1.0 - 0.5 * t) * (1.0 + 0.35 * math::sq(math::sq(1.0 - t))) + 0.35;
            // Rounded where it leaves the trunk, then low along the soil,
            // and blunt where it goes under.
            let h = (h0 * math::pow(1.0 - t, 1.2) + w0 * 0.25 * (1.0 - t * t))
                * (1.0 - 0.25 * math::pow(1.0 - t, 6.0));
            (g, (px * w, pz * w * tilt), h)
        };
        // Cross-section curve at t: half-ellipse from one side over the top.
        let curve = |t: f64| -> Vec<(f64, f64)> {
            let (g, p, h) = sec(t);
            (0..=10)
                .map(|j| {
                    let th = std::f64::consts::PI * f64::from(j) / 10.0;
                    (
                        g.0 + p.0 * math::cos(th),
                        g.1 + p.1 * math::cos(th) - h * math::sin(th),
                    )
                })
                .collect()
        };
        // A root is nearly convex: one outline round all its cross-sections.
        let mut all: Vec<(f64, f64)> = Vec::new();
        for j in 0..=k {
            all.extend(curve(f64::from(j) / f64::from(k)));
        }
        let h = hull(all);
        let mut body = PathData::new();
        body.m(h[0]);
        for &q in &h[1..] {
            body.l(q);
        }
        body.z();
        // The ridge, a little toward the light, and the shaded flank.
        let line = |side: f64, up: f64| {
            let mut d = PathData::new();
            for j in 0..=k {
                let t = f64::from(j) / f64::from(k);
                let (g, p, h) = sec(t);
                let q = (g.0 + p.0 * side, g.1 + p.1 * side - h * up);
                if j == 0 {
                    d.m(q);
                } else {
                    d.l(q);
                }
            }
            d.as_str().to_string()
        };
        let id = format!("rh{i}");
        let ww = w0 + h0 * 0.3;
        let s = format!(
            r#"<clipPath id="{id}"><path d="{o}"/></clipPath><path d="{o}" fill="{base}"/><g clip-path="url(#{id})" fill="none" stroke-linecap="round"><path d="{shade}" stroke="{dark}" stroke-width="{sw}"/><path d="{ridge}" stroke="{light}" stroke-width="{rw}"/></g>"#,
            o = body.as_str(),
            base = bark.shift(-0.03, 0.0),
            shade = line(-lit * 0.7, 0.35),
            dark = bark.shift(-0.09, 0.0),
            sw = Num(ww * 0.6),
            ridge = line(lit * 0.25, 0.9),
            light = bark.shift(0.05, 0.0),
            rw = Num(ww * 0.18),
        );
        roots.push((math::sin(az), s));
        hulls.push(h);
    }
    roots.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Deep shade where the roots leave the trunk: the valleys between them.
    let valley = format!(
        r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="{}"/>"#,
        Num(x0),
        Num(y0 + base_r * tilt * 0.35),
        Num(base_r * 0.85),
        Num(base_r * tilt * 0.6 + 0.6),
        bark.shift(-0.16, 0.0)
    );
    let (mut back, mut front) = (String::new(), valley);
    for (z, s) in roots {
        if z > 0.1 {
            front.push_str(&s);
        } else {
            back.push_str(&s);
        }
    }
    (back, front, hulls)
}

/// Moss on the soil while a weekly streak runs.
fn moss(b: &Bonsai, paints: &Paints, pot: &Pot) -> (String, String) {
    let clumps = (b.extras.moss * 34.0).round() as usize;
    if clumps == 0 {
        return (String::new(), String::new());
    }
    // Clumps on the near half of the soil grow over the roots; the rest
    // stay behind the trunk.
    let (mut dark, mut light) = (
        [String::new(), String::new()],
        [String::new(), String::new()],
    );
    for i in 0..clumps {
        let mut r = Rng::keyed(b.account_id, &format!("moss/{i}"));
        let a = r.range(0.0, math::TAU);
        let d = r.range(0.25, 0.95).sqrt();
        let x = CENTER_X + math::cos(a) * d * (pot.hw - 10.0);
        let y = SOIL_Y + math::sin(a) * d * (pot.ry - 2.5).max(1.0);
        let rx = r.range(3.0, 7.0);
        let ry = rx * r.range(0.25, 0.4);
        let near = usize::from(y > SOIL_Y);
        let _ = write!(
            dark[near],
            r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}"/>"#,
            Num(x),
            Num(y),
            Num(rx),
            Num(ry)
        );
        let _ = write!(
            light[near],
            r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}"/>"#,
            Num(x - rx * 0.15),
            Num(y - ry * 0.35),
            Num(rx * 0.6),
            Num(ry * 0.55)
        );
    }
    let group = |k: usize| {
        if dark[k].is_empty() {
            return String::new();
        }
        format!(
            r#"<g fill="{}">{}</g><g fill="{}">{}</g>"#,
            paints.moss[0], dark[k], paints.moss[1], light[k]
        )
    };
    (group(0), group(1))
}

/// `svg` mirrored about the pot's center line; empty stays empty.
fn mirror(svg: &str) -> String {
    if svg.is_empty() {
        return String::new();
    }
    format!(
        r#"<g transform="matrix(-1 0 0 1 {} 0)">{svg}</g>"#,
        Num(2.0 * CENTER_X)
    )
}

/// The pot's front wall, lit from the left, with its tier's details: a left
/// tree's is mirrored.
fn pot_front(b: &Bonsai, pot: &Pot) -> String {
    let Pot {
        hw,
        ry,
        depth,
        bw,
        glaze,
    } = *pot;
    let tier = b.extras.pot_tier;
    let bottom = SOIL_Y + depth;
    let dark = glaze.shift(-0.08, 0.0);
    let mut s = String::new();
    // Feet first, so the wall overlaps their tops; cloud feet from tier 4.
    if tier >= 2 {
        let fw = (hw * 0.14).max(6.0);
        let _ = write!(s, r#"<g fill="{dark}">"#);
        for side in [-1.0, 1.0] {
            let x = CENTER_X + side * hw * 0.6 - fw / 2.0;
            let y = bottom + ry * 0.75 - 2.0;
            let mut d = PathData::new();
            if tier >= 4 {
                d.m((x, y))
                    .l((x + fw, y))
                    .q((x + fw, y + 7.0), (x + fw * 0.55, y + 6.5))
                    .q((x + fw * 0.5, y + 4.0), (x + fw * 0.3, y + 6.5))
                    .q((x, y + 7.0), (x, y))
                    .z();
            } else {
                d.m((x, y))
                    .l((x + fw, y))
                    .l((x + fw * 0.9, y + 5.5))
                    .l((x + fw * 0.1, y + 5.5))
                    .z();
            }
            let _ = write!(s, r#"<path d="{}"/>"#, d.as_str());
        }
        s.push_str("</g>");
    }
    let mut wall = PathData::new();
    wall.m((CENTER_X - hw, SOIL_Y))
        .a(hw, ry, false, (CENTER_X + hw, SOIL_Y))
        .l((CENTER_X + bw, bottom))
        .a(bw, ry, true, (CENTER_X - bw, bottom))
        .z();
    let _ = write!(s, r#"<path d="{}" fill="{glaze}"/>"#, wall.as_str());
    for (f, tone) in SHADE {
        // On the rim and foot ellipses exactly, so the rim's steps meet these.
        let k = (1.0 - f * f).sqrt();
        let mut shade = PathData::new();
        shade
            .m((CENTER_X + hw * f, SOIL_Y + ry * k))
            .a(hw, ry, false, (CENTER_X + hw, SOIL_Y))
            .l((CENTER_X + bw, bottom))
            .a(bw, ry, true, (CENTER_X + bw * f, bottom + ry * k))
            .z();
        let _ = write!(
            s,
            r#"<path d="{}" fill="{}"/>"#,
            shade.as_str(),
            glaze.shift(tone, 0.0)
        );
    }
    // The tier's details stay on the wall: a random walk or a band's end
    // would otherwise run off it.
    let _ = write!(
        s,
        r#"<clipPath id="pw"><path d="{}"/></clipPath><g clip-path="url(#pw)">"#,
        wall.as_str()
    );
    let mut rng = Rng::keyed(b.account_id, "pot");
    match tier {
        0 => {
            // The training pot's moulded ribs.
            let mut ribs = PathData::new();
            for k in 0..7 {
                let x = CENTER_X + hw * 0.8 * (f64::from(k) / 3.0 - 1.0);
                let y = SOIL_Y
                    + ry * (1.0 - math::sq(x - CENTER_X) / math::sq(hw))
                        .max(0.0)
                        .sqrt();
                ribs.m((x, y + 3.0))
                    .l((x * 0.98 + CENTER_X * 0.02, bottom + ry * 0.6));
            }
            let _ = write!(
                s,
                r#"<path d="{}" stroke="{}" stroke-width=".8"/>"#,
                ribs.as_str(),
                glaze.shift(0.08, 0.0)
            );
        }
        3.. => {
            let _ = write!(
                s,
                r##"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="#fff" opacity=".12"/>"##,
                Num(CENTER_X - hw * 0.6),
                Num(SOIL_Y + depth * 0.55),
                Num(hw * 0.08),
                Num(depth * 0.3)
            );
        }
        _ => {}
    }
    if tier == 5 {
        let mut crackle = PathData::new();
        for _ in 0..10 {
            let mut p = (
                CENTER_X + rng.range(-0.75, 0.75) * bw,
                SOIL_Y + rng.range(0.5, 0.9) * depth + ry * 0.6,
            );
            crackle.m(p);
            for _ in 0..8 {
                p = (p.0 + rng.range(-5.0, 5.0), p.1 + rng.range(-3.0, 3.0));
                crackle.l(p);
            }
        }
        let _ = write!(
            s,
            r#"<path d="{}" fill="none" stroke="{dark}" stroke-width=".5"/>"#,
            crackle.as_str()
        );
    }
    if tier >= 6 {
        // Antique: worn patina and a maker's seal.
        let _ = write!(
            s,
            r#"<g fill="{}" opacity=".35">"#,
            glaze.mix(Rgb::hex(0x7f9a6a), 0.5).shift(0.08, 0.0)
        );
        for _ in 0..4 {
            let _ = write!(
                s,
                r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}"/>"#,
                Num(CENTER_X + rng.range(-0.7, 0.7) * bw),
                Num(SOIL_Y + rng.range(0.5, 0.85) * depth + ry * 0.6),
                Num(rng.range(5.0, 12.0)),
                Num(rng.range(2.0, 4.0))
            );
        }
        s.push_str("</g>");
    }
    s.push_str("</g>");
    s
}

/// Where the pot turns from the light, as shares of its half-width right of
/// center, and how much darker it is from there: two steps, no gradient.
const SHADE: [(f64, f64); 2] = [(0.2, -0.035), (0.38, -0.07)];

/// Draws the rim's lip: a band astride the rim's front edge, 2.4 deep at
/// the front and tapering to nothing at the sides, stepped into shade where
/// the wall is, edge to edge with it. Lit from the left: mirror it for a
/// left tree.
fn pot_rim(pot: &Pot) -> String {
    let Pot { hw, ry, glaze, .. } = *pot;
    // Half over the soil's edge, half down the wall, as thick as a rim reads.
    let (top, lip) = (ry - 1.2, ry + 1.2);
    let mut s = String::new();
    for (f, tone) in [(-1.0, 0.0)].into_iter().chain(SHADE) {
        let k = (1.0 - f * f).sqrt();
        let mut d = PathData::new();
        d.m((CENTER_X + hw * f, SOIL_Y + top * k))
            .a(hw, top, false, (CENTER_X + hw, SOIL_Y))
            .a(hw, lip, true, (CENTER_X + hw * f, SOIL_Y + lip * k))
            .z();
        let _ = write!(
            s,
            r#"<path d="{}" fill="{}"/>"#,
            d.as_str(),
            glaze.shift(0.08 + tone, 0.0)
        );
    }
    s
}

/// The pot's carved bands (tier 5 and up) and maker's seal (tier 6 and up),
/// drawn after the kintsugi: the gold mends the clay under them.
fn pot_marks(b: &Bonsai, pot: &Pot) -> String {
    let Pot {
        hw,
        ry,
        depth,
        bw,
        glaze,
    } = *pot;
    let tier = b.extras.pot_tier;
    let dark = glaze.shift(-0.08, 0.0);
    let mut s = String::new();
    if tier >= 5 {
        // A carved band.
        let mut band = PathData::new();
        for share in [0.3, 0.4] {
            let y = SOIL_Y + depth * share;
            let w = hw + (bw - hw) * share;
            band.m((CENTER_X - w, y)).a(w, ry, false, (CENTER_X + w, y));
        }
        let _ = write!(
            s,
            r#"<path d="{}" fill="none" stroke="{dark}" stroke-width=".9"/>"#,
            band.as_str()
        );
    }
    if tier >= 6 {
        let (sx, sy) = (CENTER_X + hw * 0.5, SOIL_Y + depth * 0.5 + ry * 0.5);
        let _ = write!(
            s,
            r##"<rect x="{}" y="{}" width="6" height="6" rx=".8" fill="#7a2e22"/><rect x="{}" y="{}" width="3" height="3" fill="none" stroke="#c98a6a" stroke-width=".6"/>"##,
            Num(sx),
            Num(sy),
            Num(sx + 1.5),
            Num(sy + 1.5)
        );
    }
    if s.is_empty() {
        return s;
    }
    // On the wall, as clipped in [`pot_front`]: a band's ends curve out past it.
    format!(r#"<g clip-path="url(#pw)">{s}</g>"#)
}

/// Level 11: a timer box on the rim, an emitter under every pad, and drops
/// falling from all of them at the same moment.
fn drip_ends(scene: &Scene, paints: &Paints, pot: &Pot, animate: bool) -> String {
    let mut emitters = String::new();
    let mut drops = String::new();
    for (c, _, ry_pad) in &scene.pads {
        let (x, y, _) = project(v_add(*c, [0.0, -ry_pad * 0.9, 0.0]));
        let _ = write!(
            emitters,
            r#"<circle cx="{}" cy="{}" r="{}"/>"#,
            Num(x),
            Num(y),
            Num(2.4 * WORLD)
        );
        let _ = write!(
            drops,
            r#"<path d="M{} {}q-1.9 2.6 0 4.2q1.9-1.6 0-4.2z"/>"#,
            Num(x),
            Num(y + 2.6 * WORLD)
        );
    }
    let mut s = format!(r#"<g fill="{}">{emitters}</g>"#, paints.tubing);
    if animate {
        // One group for every drop: they fall at the same moment, on purpose.
        let _ = write!(s, r#"<g class="dp" fill="{}">{drops}</g>"#, paints.water);
    }
    let _ = write!(
        s,
        r##"<g transform="translate({} {}) scale({})"><path d="M0-6Q-14-6-22-14" fill="none" stroke="{t}" stroke-width="2.4"/><rect x="-7" y="-12" width="14" height="11" rx="2" fill="{t}"/><rect x="-5" y="-10" width="7" height="4" rx=".8" fill="#7ed37e"/><circle cx="4" cy="-8" r="1.3" fill="#e05a4f"/></g>"##,
        Num(CENTER_X + pot.hw * 0.74),
        Num(SOIL_Y),
        Num(WORLD),
        t = paints.tubing
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dead wood leaving a parent starts in a point, back along its line.
    #[test]
    fn dead_wood_starts_in_a_point_back_inside_its_parent() {
        let mut limb = vec![(10.0, 0.0, 3.0, 1.0), (20.0, 0.0, 3.0, 2.0)];
        point_start(&mut limb, 6.0);
        assert_eq!(limb.len(), 3);
        assert_eq!(limb[0], (4.0, 0.0, 0.0, 1.0));
    }

    /// A shari climbs to its top unless living wood in front of the trunk
    /// lies over or beside it up to there; then it ends below that wood.
    /// Wood it only passes keeps it whole.
    #[test]
    fn a_shari_ends_below_living_wood_in_front() {
        let strip: Vec<(f64, f64, f64, f64)> = (0..=10)
            .map(|i| (0.0, -10.0 * f64::from(i), 8.0, f64::from(i) / 10.0))
            .collect();
        assert_eq!(shari_end(&strip, &[], 0.8), 0.8);
        let far = [(30.0, -50.0, 3.0)];
        assert_eq!(shari_end(&strip, &[&far], 0.8), 0.8);
        let beside: Vec<(f64, f64, f64)> =
            (5..=10).map(|i| (6.0, -10.0 * f64::from(i), 3.0)).collect();
        assert_eq!(shari_end(&strip, &[&beside], 0.8), 0.5);
        let passed = [(6.0, -20.0, 3.0)];
        assert_eq!(shari_end(&strip, &[&passed], 0.8), 0.8);
        // A straight limb is simplified to its two ends: the wood between
        // them counts too.
        let straight = [(6.0, -50.0, 3.0), (6.0, -100.0, 3.0)];
        assert_eq!(shari_end(&strip, &[&straight], 0.8), 0.5);
    }

    /// A dead whip ends before it rises above the crown or leaves its sides;
    /// under the crown it is left whole, and it keeps its first stretch.
    #[test]
    fn dead_wood_is_cut_where_it_leaves_the_crown() {
        let crown = [(0.0, 0.0, 0.0), (10.0, 0.0, 0.0), (20.0, 0.0, 0.0)];
        let mut under = vec![(0.0, 10.0, 1.0, 0.0), (10.0, 30.0, 1.0, 0.0)];
        under.push((20.0, 30.0, 1.0, 0.0));
        cut_near(&mut under, &crown, 5.0);
        assert_eq!(under.len(), 3);
        let mut whip = vec![(0.0, 10.0, 1.0, 0.0), (10.0, -4.0, 1.0, 0.0)];
        whip.extend([(16.0, -10.0, 1.0, 0.0), (40.0, -4.0, 1.0, 0.0)]);
        cut_near(&mut whip, &crown, 5.0);
        assert_eq!(whip.len(), 2);
        let mut aside = vec![(0.0, 10.0, 1.0, 0.0), (20.0, 10.0, 1.0, 0.0)];
        aside.push((40.0, 10.0, 1.0, 0.0));
        cut_near(&mut aside, &crown, 5.0);
        assert_eq!(aside.len(), 2);
        let mut long = vec![(0.0, 1.0, 1.0, 0.0), (0.0, -30.0, 1.0, 0.0)];
        cut_near(&mut long, &crown, 5.0);
        assert_eq!(long.len(), 2);
    }

    /// A dead line with only stubs off it is a whip, however many chains
    /// it is drawn in; a dead top that forks into two branches is not.
    #[test]
    fn a_branched_dead_top_is_never_capped() {
        let line = |x: f64, y0: f64, y1: f64| -> Vec<(V3, f64)> {
            (0..=10)
                .map(|i| ([x, y0 + (y1 - y0) * f64::from(i) / 10.0, 0.0], 1.0))
                .collect()
        };
        let chains: Vec<Chain> = vec![
            (line(0.0, 0.0, 50.0), None),
            // A whip in two chains, with a stub off its middle.
            (line(0.0, 20.0, 40.0), Some(1.0)),
            (line(0.0, 40.0, 70.0), Some(1.0)),
            (
                vec![([0.0, 30.0, 0.0], 1.0), ([1.0, 31.0, 0.0], 1.0)],
                Some(1.0),
            ),
            // A dead top that forks half way.
            (line(0.0, 50.0, 80.0), Some(1.0)),
            (line(0.0, 65.0, 90.0), Some(1.0)),
            // A spike off the foot of a dead strand up the trunk.
            (line(1.0, 0.0, 50.0), Some(1.0)),
            (line(5.0, 5.0, 30.0), Some(1.0)),
        ];
        let mut joins = std::collections::BTreeMap::new();
        joins.insert(1, (1.0, [0.0, 20.0, 0.0], 0));
        joins.insert(2, (1.0, [0.0, 40.0, 0.0], 1));
        joins.insert(3, (1.0, [0.0, 30.0, 0.0], 1));
        joins.insert(4, (1.0, [0.0, 50.0, 0.0], 0));
        joins.insert(5, (1.0, [0.0, 65.0, 0.0], 4));
        joins.insert(6, (1.0, [1.0, 0.0, 0.0], 0));
        joins.insert(7, (1.0, [1.0, 5.0, 0.0], 6));
        let strands = std::collections::BTreeSet::from([6]);
        assert_eq!(
            whips(&chains, &joins, &strands),
            vec![false, true, true, true, false, false, true, true]
        );
    }

    /// A flat start is left open, so an inked outline never draws a line
    /// across the cut; a round start is closed.
    #[test]
    fn a_flat_start_is_left_open() {
        let line = [(0.0, 0.0, 2.0), (0.0, -10.0, 2.0), (0.0, -20.0, 1.0)];
        assert!(!tube(&line, true, false).as_str().ends_with('Z'));
        assert!(tube_from(&line, true, true, false).as_str().ends_with('Z'));
    }

    /// Wood just under a cloud is in its shadow; wood beside it, above it
    /// or far below it is in the light.
    #[test]
    fn wood_under_the_crown_lies_in_its_shadow() {
        let pads = [([0.0, 88.0, 0.0], 19.0, 8.8)];
        let under = crown_shadow(&pads, [0.0, 80.0, 0.0]);
        assert!(under > 0.6, "{under}");
        for (at, why) in [
            ([30.0, 80.0, 0.0], "beside"),
            ([0.0, 99.0, 0.0], "above"),
            ([0.0, 30.0, 0.0], "far below"),
        ] {
            let shade = crown_shadow(&pads, at);
            assert!(shade < 0.05, "{why}: {shade}");
        }
    }

    /// A second trunk leaving the trunk low runs on down into the soil, so
    /// its start is cut at the soil, never a round end on the trunk's face;
    /// one leaving higher up keeps its start.
    #[test]
    fn a_strand_leaving_the_trunk_low_runs_into_the_soil() {
        let mut low = vec![(254.0, 396.5, 7.4), (254.0, 390.0, 7.0)];
        assert!(into_soil(&mut low, 404.0));
        assert!(
            low[0].1 > 404.0,
            "starts at {:.1}, above the soil",
            low[0].1
        );
        assert!(low.iter().all(|p| (p.0 - 254.0).abs() < 1e-9));
        let mut high = vec![(254.0, 396.5, 0.9), (254.0, 390.0, 0.9)];
        assert!(!into_soil(&mut high, 404.0));
        assert_eq!(high.len(), 2);
        // Wood already starting in the soil is cut there as it is.
        let mut sunk = vec![(257.3, 405.3, 0.2), (250.0, 400.0, 0.2)];
        assert!(!into_soil(&mut sunk, 404.0));
        assert_eq!(sunk.len(), 2);
    }

    /// A strand's groove starts as wide as the strand, then widens: at the
    /// start it never pokes out of the trunk the strand leaves.
    #[test]
    fn a_strands_groove_starts_at_its_own_width() {
        let seg: Vec<(f64, f64, f64)> = (0..19).map(|i| (0.0, -f64::from(i), 3.0)).collect();
        let g = groove(&seg, true);
        assert!((g[0].2 - 3.0).abs() < 1e-9, "starts {:.2} wide", g[0].2);
        assert!((g[18].2 - 4.35).abs() < 1e-9, "ends {:.2} wide", g[18].2);
        // A later piece is a full groove all along.
        let full = groove(&seg, false);
        assert!(full.iter().all(|p| (p.2 - 4.35).abs() < 1e-9), "{full:?}");
    }

    /// Where two twigs cross, the one in front there is drawn later, even
    /// when the other is in front on average.
    #[test]
    fn crossing_twigs_layer_by_their_depth_where_they_cross() {
        // A runs left to right, coming toward the viewer: in front on
        // average, but far behind where B crosses it, near its right end.
        let a = Twig {
            line: vec![(0.0, 0.0, 5.0), (10.0, 0.0, -5.0)],
            z: 0.0,
            w: 0.6,
            parent: None,
        };
        let b = Twig {
            line: vec![(8.0, -5.0, -1.0), (8.0, 5.0, -1.0)],
            z: -1.0,
            w: 0.6,
            parent: None,
        };
        assert_eq!(layer(&[a.clone(), b]), vec![0, 1]);
        // A twig coming out of another's wood, where it starts, does not
        // cross it there: it stays behind, as its depth on average says.
        let out = Twig {
            line: vec![(6.0, 0.5, 4.0), (6.0, -8.0, -6.0)],
            z: -1.0,
            w: 0.6,
            parent: None,
        };
        assert_eq!(layer(&[a.clone(), out]), vec![1, 0]);
        // Twigs lying nearly along each other do not cross: by average.
        let along = Twig {
            // In front of A where it meets it, behind it on average.
            line: vec![(1.0, -1.0, 0.0), (9.0, 0.6, 0.0)],
            z: -1.0,
            w: 0.6,
            parent: None,
        };
        assert_eq!(layer(&[a, along]), vec![1, 0]);
    }

    /// A twig comes out from behind the twig it grows from, wherever it
    /// lies on average.
    #[test]
    fn a_twig_is_drawn_before_the_twig_it_grows_from() {
        let parent = Twig {
            line: vec![(0.0, 0.0, -2.0), (10.0, 0.0, -2.0)],
            z: -2.0,
            w: 0.6,
            parent: None,
        };
        let child = Twig {
            line: vec![(5.0, 0.0, 3.0), (5.0, -6.0, 3.0)],
            z: 3.0,
            w: 0.6,
            parent: Some(0),
        };
        assert_eq!(layer(&[parent, child]), vec![1, 0]);
        // Apart, twigs keep their order by depth.
        let far = |x: f64, z: f64| Twig {
            line: vec![(x, 0.0, z), (x, -5.0, z)],
            z,
            w: 0.6,
            parent: None,
        };
        assert_eq!(
            layer(&[far(0.0, 2.0), far(20.0, -1.0), far(40.0, 0.5)]),
            vec![1, 2, 0]
        );
    }

    #[test]
    fn follow_moves_the_start_and_keeps_the_tip_even_on_a_tiny_twig() {
        let mut twig = [(100.0, 100.0, 0.5, 0.0), (100.01, 100.0, 0.5, 0.0)];
        follow(&mut twig, (5.0, -3.0));
        assert_eq!((twig[0].0, twig[0].1), (105.0, 97.0));
        assert!(
            (twig[1].0 - 100.01).abs() < 1e-9 && (twig[1].1 - 100.0).abs() < 1e-9,
            "{twig:?}"
        );
        let mut limb = [
            (0.0, 0.0, 1.0, 0.0),
            (10.0, 0.0, 1.0, 0.0),
            (20.0, 0.0, 1.0, 0.0),
        ];
        follow(&mut limb, (0.0, 4.0));
        assert!((limb[1].1 - 2.0).abs() < 1e-9, "{limb:?}");
    }

    #[test]
    fn nearest_on_finds_the_closest_point_of_a_polyline() {
        let line = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        assert_eq!(nearest_on(&line, (4.0, 3.0)), (4.0, 0.0));
        assert_eq!(nearest_on(&line, (13.0, 6.0)), (10.0, 6.0));
        assert_eq!(nearest_on(&line, (-5.0, -1.0)), (0.0, 0.0));
        assert_eq!(nearest_on(&[(2.0, 2.0)], (9.0, 9.0)), (2.0, 2.0));
    }

    #[test]
    fn a_hull_keeps_the_corners_and_drops_what_is_inside() {
        let h = hull(vec![
            (0.0, 0.0),
            (2.0, 0.0),
            (1.0, 1.0),
            (2.0, 2.0),
            (0.0, 2.0),
            (1.0, 0.5),
        ]);
        assert_eq!(h.len(), 4, "{h:?}");
        for c in [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)] {
            assert!(h.contains(&c), "{c:?} in {h:?}");
        }
    }

    #[test]
    fn roots_stay_inside_the_pot() {
        for base_r in [4.0, 12.0, 30.0] {
            for nebari in [0.0, 0.5, 1.0] {
                for hw in [60.0, 100.0, 140.0] {
                    for i in 0..16 {
                        let az = f64::from(i) / 16.0 * math::TAU;
                        for (u, brace) in [(0.0, 0.0), (0.5, 0.5), (0.999, 1.0)] {
                            // A trunk off center, as a cascade's is, too.
                            // On either side: a left cascade's foot is the mirror image.
                            for off in [0.0, -0.3 * hw, 0.2 * hw, 0.3 * hw, -0.2 * hw] {
                                let reach = root_reach(base_r, nebari, hw, az, u, brace, off);
                                assert!(
                                    (off + reach * math::cos(az)).abs() <= hw * 0.78 + 1e-9,
                                    "{base_r} {nebari} {hw} {az} {off}: {reach}"
                                );
                                let mirrored = root_reach(
                                    base_r,
                                    nebari,
                                    hw,
                                    mirror_az(az, -1.0),
                                    u,
                                    brace,
                                    -off,
                                );
                                assert!((mirrored - reach).abs() < 1e-9, "{az} {off}");
                            }
                        }
                    }
                }
            }
        }
    }
}
