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
use crate::math;
use crate::noise::hash01;
use crate::render::{Num, PathData, Theme};
use crate::rng::Rng;
use crate::scene::{LIGHT, Scene, TILT, V3, WORLD, project, v_add, v_dot, v_norm, v_scale};
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
                jin: Rgb::hex(0xd9d4c7),
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
}

/// A unit vector in 2D.
pub fn norm((x, y): (f64, f64)) -> (f64, f64) {
    let l = (x * x + y * y).sqrt().max(1e-9);
    (x / l, y / l)
}

/// A tube with a round end cap along `(x, y, r)` samples.
pub fn tube(points: &[(f64, f64, f64)]) -> PathData {
    let n = points.len();
    let mut d = PathData::coarse();
    if n < 2 {
        return d;
    }
    let tangent = |i: usize| {
        let (a, b) = (points[i.saturating_sub(1)], points[(i + 1).min(n - 1)]);
        norm((b.0 - a.0, b.1 - a.1))
    };
    let side = |i: usize, s: f64| {
        let (tx, ty) = tangent(i);
        let (x, y, r) = points[i];
        (x - ty * r * s, y + tx * r * s)
    };
    d.m(side(0, -1.0));
    for i in 1..n {
        d.l(side(i, -1.0));
    }
    let (tx, ty) = tangent(n - 1);
    let r = points[n - 1].2;
    let (a, b) = (side(n - 1, -1.0), side(n - 1, 1.0));
    d.c(
        (a.0 + tx * r * 1.33, a.1 + ty * r * 1.33),
        (b.0 + tx * r * 1.33, b.1 + ty * r * 1.33),
        b,
    );
    for i in (0..n - 1).rev() {
        d.l(side(i, 1.0));
    }
    d.z();
    d
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
    let light = v_norm(LIGHT);
    // The light's direction on screen, for which side of a branch is lit.
    let (lx, ly, _) = project(v_scale(light, 100.0));
    let ls = norm((lx - CENTER_X, ly - SOIL_Y));
    let haze = |c: Rgb, depth: f64| {
        let back = (-depth / 140.0).clamp(0.0, 1.0);
        c.shift(0.05 * back, -0.03 * back)
    };
    let mut out = Painted::default();
    let mut items: Vec<(f64, String)> = Vec::new();
    let mut extent = (f64::MAX, f64::MIN, SOIL_Y);

    // ---- wood, in pieces so leaves can pass in front of and behind it ----
    let mut twigs = [String::new(), String::new()];
    for (ci, (chain, jin)) in scene.chains.iter().enumerate() {
        let pts: Vec<(f64, f64, f64, f64)> = chain
            .iter()
            .map(|(p, r)| {
                let (x, y, z) = project(*p);
                (x, y, *r * WORLD, z)
            })
            .collect();
        // Relax the growth zigzag, then subdivide; thin twigs need less.
        let relaxed = relax(&relax(&pts));
        let thick = pts.iter().any(|p| p.2 > 3.2);
        let fine = simplify(
            &if thick {
                chaikin(&chaikin(&relaxed))
            } else {
                chaikin(&relaxed)
            },
            0.6,
        );
        for p in &fine {
            extent = (
                extent.0.min(p.0 - p.2),
                extent.1.max(p.0 + p.2),
                extent.2.min(p.1 - p.2),
            );
        }
        // Thin twigs all share one path per color, drawn behind the leaves.
        if !thick {
            let seg: Vec<(f64, f64, f64)> = fine.iter().map(|p| (p.0, p.1, p.2)).collect();
            twigs[usize::from(*jin)].push_str(tube(&seg).as_str());
            continue;
        }
        let piece = 20;
        let mut start = 0;
        while start + 1 < fine.len() {
            let end = (start + piece).min(fine.len() - 1);
            let seg: Vec<(f64, f64, f64)> =
                fine[start..=end].iter().map(|p| (p.0, p.1, p.2)).collect();
            let depth =
                fine[start..=end].iter().map(|p| p.3).sum::<f64>() / (end - start + 1) as f64;
            let base = haze(if *jin { paints.jin } else { paints.bark }, depth);
            let mut s = format!(r#"<path d="{}" fill="{base}"/>"#, tube(&seg).as_str());
            if seg.iter().any(|p| p.2 > 4.5) {
                // The shadow falls on whichever side faces away from the light.
                let m = seg.len();
                let away = |i: usize| {
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
                    let mut d = PathData::coarse();
                    for i in 0..m {
                        let n = away(i);
                        let q = (
                            seg[i].0 + n.0 * seg[i].2 * from,
                            seg[i].1 + n.1 * seg[i].2 * from,
                        );
                        if i == 0 {
                            d.m(q)
                        } else {
                            d.l(q)
                        };
                    }
                    for i in (0..m).rev() {
                        let n = away(i);
                        d.l((
                            seg[i].0 + n.0 * seg[i].2 * to,
                            seg[i].1 + n.1 * seg[i].2 * to,
                        ));
                    }
                    d.z();
                    d
                };
                let _ = write!(
                    s,
                    r#"<path d="{}" fill="{}"/>"#,
                    strip(0.05, 0.98).as_str(),
                    base.shift(-0.09, 0.0)
                );
                let _ = write!(
                    s,
                    r#"<path d="{}" fill="{}"/>"#,
                    strip(-0.35, -0.72).as_str(),
                    base.shift(0.07, 0.0)
                );
                if ci == 0 && seg.iter().any(|p| p.2 > 6.0) {
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
            items.push((depth, s));
            start = end;
        }
    }

    // Twiglets from the wood out to leaves that sit clear of it: one hairline
    // path behind the leaves.
    let mut twiglets = PathData::coarse();
    for t in &scene.tufts {
        if let Some(q) = t.stem {
            let (x0, y0, _) = project(q);
            let (x1, y1, _) = project(t.p);
            twiglets.m((x0, y0));
            twiglets.l((x1, y1));
        }
    }
    if !twiglets.is_empty() {
        items.push((
            f64::MIN,
            format!(
                r#"<path d="{}" fill="none" stroke="{}" stroke-width="{}" stroke-linecap="round"/>"#,
                twiglets.as_str(),
                paints.bark,
                Num::coarse(0.8 * WORLD)
            ),
        ));
    }
    for (k, d) in twigs.iter().enumerate() {
        if !d.is_empty() {
            let color = if k == 1 { paints.jin } else { paints.bark };
            items.push((f64::MIN, format!(r#"<path d="{d}" fill="{color}"/>"#)));
        }
    }

    // ---- leaves: lit by n.L, darkened by occlusion; bronze flush; samaras ----
    let mut fallen: Vec<(f64, f64, f64)> = Vec::new();
    for (j, t) in scene.tufts.iter().enumerate() {
        let (x, y, depth) = project(t.p);
        let diffuse = v_dot(t.n, light).max(0.0);
        let shade = (0.25 + 0.75 * diffuse) * (0.55 + 0.45 * t.q);
        let mut c = t.color.shift(
            paints.leaf_shift + (shade - 0.55) * 0.38,
            (shade - 0.5) * 0.04,
        );
        // Light warms, shadow cools.
        c = if shade > 0.55 {
            c.mix(Rgb::hex(0xe8d27a), (shade - 0.55) * 0.25)
        } else {
            c.mix(Rgb::hex(0x24384a), (0.55 - shade) * 0.35)
        };
        if t.fresh {
            // Japanese maples flush bronze-red before the leaves settle.
            c = c.mix(Rgb::hex(0xc8553d), 0.45).shift(0.06, 0.0);
        }
        c = haze(c, depth);
        let h = hash01(seed, j as i64 + 7);
        let (sx, sy, _) = project(v_add(t.p, t.n));
        let angle = math::deg(math::atan2(sy - y, sx - x)) + 90.0 + (h - 0.5) * 50.0;
        let symbol = if h < 0.5 { "t" } else { "u" };
        let scale = WORLD
            * t.size
            * (0.85 + 0.3 * hash01(seed, j as i64 + 11))
            * if t.fresh { 0.75 } else { 1.0 };
        extent = (
            extent.0.min(x - 8.0),
            extent.1.max(x + 8.0),
            extent.2.min(y - 8.0),
        );
        let mut s = format!(
            r##"<use href="#{symbol}" transform="translate({} {})rotate({})scale({})" fill="{c}"/>"##,
            Num::coarse(x),
            Num::coarse(y),
            Num::coarse(angle),
            Num::coarse(scale)
        );
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
        items.push((depth + 0.5, s));
        if animate
            && b.extras.bloom.is_some()
            && fallen.len() < 9
            && t.q > 0.5
            && hash01(seed ^ 0xfa, j as i64) < 0.03
        {
            fallen.push((x, y, h));
        }
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

    // ---- the pot, sized to the tree, seen from a little above ----
    let size = ((SOIL_Y - extent.2).max(extent.1 - extent.0) * 0.62).clamp(80.0, 400.0);
    let hw = size / 2.0;
    let base_r = WORLD
        * scene
            .chains
            .first()
            .and_then(|c| c.0.first())
            .map_or(1.0, |p| p.1);
    let depth = (base_r * 1.6).clamp(20.0, 56.0);
    let pot = Pot {
        hw,
        ry: hw * math::sin(TILT) * 0.55,
        depth,
        bw: hw * 0.88,
        glaze: pot_glaze(b.extras.pot_tier, config, &paints),
    };
    let egg = eggs::eggs(b, &scene, &paints, &pot);

    let body = &mut out.body;
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
    let _ = write!(
        body,
        r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="{}" opacity=".35"/>"#,
        Num(canopy_x.clamp(CENTER_X - hw * 0.5, CENTER_X + hw * 0.5)),
        Num(SOIL_Y + 0.6),
        Num((hw - 8.0) * 0.75),
        Num((pot.ry - 3.0).max(1.0) * 0.7),
        paints.soil.shift(-0.12, 0.0)
    );
    body.push_str(&moss(b, &paints, &pot));
    body.push_str(&egg.back);
    body.push_str(&nebari(b, &paints, base_r));
    for (_, s) in &items {
        body.push_str(s);
    }
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
    body.push_str(&pot_front(b, &pot));
    body.push_str(&eggs::kintsugi(&b.archived, seed, &pot));
    let mut rim = PathData::new();
    rim.m((CENTER_X - hw, SOIL_Y))
        .a(hw, pot.ry, false, (CENTER_X + hw, SOIL_Y));
    let _ = write!(
        body,
        r#"<path d="{}" fill="none" stroke="{}" stroke-width="2.4"/>"#,
        rim.as_str(),
        pot.glaze.shift(0.08, 0.0)
    );
    body.push_str(&egg.top);
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

/// Surface roots, spreading with age.
fn nebari(b: &Bonsai, paints: &Paints, base_r: f64) -> String {
    let mut roots = String::new();
    let spread = 1.3 + 1.1 * b.extras.nebari.min(1.0);
    for i in 0..b.extras.roots.max(3) {
        let mut r = Rng::keyed(b.account_id, &format!("root/{i}"));
        let az = r.range(0.0, math::TAU);
        let reach = base_r * r.range(spread, spread + 1.0);
        let pts: Vec<(f64, f64, f64)> = (0..=6)
            .map(|j| {
                let t = f64::from(j) / 6.0;
                let p3: V3 = [
                    math::cos(az) * reach * t / WORLD,
                    base_r / WORLD * 0.3 * (1.0 - t).powi(2),
                    math::sin(az) * reach * t / WORLD,
                ];
                let (x, y, _) = project(p3);
                (x, y, base_r * 0.42 * (1.0 - t) + 0.5)
            })
            .collect();
        roots.push_str(tube(&pts).as_str());
    }
    format!(
        r#"<path d="{roots}" fill="{}"/>"#,
        paints.bark.shift(-0.04, 0.0)
    )
}

/// Moss on the soil while a weekly streak runs.
fn moss(b: &Bonsai, paints: &Paints, pot: &Pot) -> String {
    let clumps = (b.extras.moss * 34.0).round() as usize;
    if clumps == 0 {
        return String::new();
    }
    let (mut dark, mut light) = (String::new(), String::new());
    for i in 0..clumps {
        let mut r = Rng::keyed(b.account_id, &format!("moss/{i}"));
        let a = r.range(0.0, math::TAU);
        let d = r.range(0.25, 0.95).sqrt();
        let x = CENTER_X + math::cos(a) * d * (pot.hw - 10.0);
        let y = SOIL_Y + math::sin(a) * d * (pot.ry - 2.5).max(1.0);
        let rx = r.range(3.0, 7.0);
        let ry = rx * r.range(0.25, 0.4);
        let _ = write!(
            dark,
            r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}"/>"#,
            Num(x),
            Num(y),
            Num(rx),
            Num(ry)
        );
        let _ = write!(
            light,
            r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}"/>"#,
            Num(x - rx * 0.15),
            Num(y - ry * 0.35),
            Num(rx * 0.6),
            Num(ry * 0.55)
        );
    }
    format!(
        r#"<g fill="{}">{dark}</g><g fill="{}">{light}</g>"#,
        paints.moss[0], paints.moss[1]
    )
}

/// The pot's front wall, lit from the left, with its tier's details.
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
    let mut shade = PathData::new();
    shade
        .m((CENTER_X + hw * 0.35, SOIL_Y + ry * 0.93))
        .a(hw, ry, false, (CENTER_X + hw, SOIL_Y))
        .l((CENTER_X + bw, bottom))
        .a(bw, ry, true, (CENTER_X + bw * 0.35, bottom + ry * 0.93))
        .z();
    let _ = write!(
        s,
        r#"<path d="{}" fill="{}"/>"#,
        shade.as_str(),
        glaze.shift(-0.07, 0.0)
    );
    let mut rng = Rng::keyed(b.account_id, "pot");
    match tier {
        0 => {
            // The training pot's moulded ribs.
            let mut ribs = PathData::new();
            for k in 0..7 {
                let x = CENTER_X + hw * 0.8 * (f64::from(k) / 3.0 - 1.0);
                let y = SOIL_Y + ry * (1.0 - (x - CENTER_X).powi(2) / hw.powi(2)).max(0.0).sqrt();
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
    if tier == 5 {
        let mut crackle = PathData::new();
        for _ in 0..10 {
            let mut p = (
                CENTER_X + rng.range(-0.75, 0.75) * bw,
                SOIL_Y + rng.range(0.5, 0.9) * depth + ry * 0.6,
            );
            crackle.m(p);
            for _ in 0..3 {
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
    s
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
