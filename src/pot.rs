//! The pot: its size, its drawing and the moss on its soil.
//!
//! Sizing lives apart from drawing, so the rules that pick a pot's width and
//! depth can be tested without painting anything, and a pot's look can
//! change without touching how big it is. The pot is repotted in steps as the
//! tree grows ([`crate::Bonsai::grow`]); here you get the pot of one day.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "geometry on a 512-unit canvas"
)]

use std::fmt::Write as _;

use crate::Bonsai;
use crate::color::Rgb;
use crate::config::Config;
use crate::math;
use crate::paint::Paints;
use crate::render::{Num, PathData};
use crate::rng::Rng;
use crate::scene::{Scene, TILT, project};
use crate::style::{CENTER_X, SOIL_Y};

/// The outline a pot is built on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// An oval: an elliptical rim over a tapering wall.
    Oval,
    /// A deep round bowl with a belly.
    Bowl,
    /// A rectangle seen from a little above.
    Rect,
}

/// The feet a pot stands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feet {
    /// No feet: the pot stands on its base.
    None,
    /// Two small plain feet.
    Plain,
    /// Two carved cloud feet.
    Cloud,
    /// Two bracket feet with a notch.
    Bracket,
    /// Small blocks under the corners.
    Corner,
    /// Small rounded feet.
    Round,
}

/// One pot of the ladder, as data: its shape, glaze and feet.
#[derive(Debug, Clone, Copy)]
pub struct Model {
    /// Its name, as `explain` gives it.
    pub name: &'static str,
    /// The outline it is built on.
    pub shape: Shape,
    /// Its glaze, or its clay when unglazed.
    pub glaze: u32,
    /// Whether it is glazed: only a glazed pot takes `[pot] glaze`.
    pub glazed: bool,
    /// How much deeper than the trunk rule it is drawn.
    pub depth_factor: f64,
    /// What it stands on.
    pub feet: Feet,
    /// Its feet's colour, or `None` for a darker shade of the pot.
    pub feet_color: Option<u32>,
}

const fn pot(
    name: &'static str,
    shape: Shape,
    glaze: u32,
    glazed: bool,
    depth_factor: f64,
    feet: Feet,
    feet_color: Option<u32>,
) -> Model {
    Model {
        name,
        shape,
        glaze,
        glazed,
        depth_factor,
        feet,
        feet_color,
    }
}

/// The ladder: the pot of each level, earned by the longest streak
/// ([`crate::extras::POT_TIERS`]). Round pots first, then the bowl, then
/// the rectangles.
pub const MODELS: [Model; 11] = [
    pot(
        "training pot",
        Shape::Oval,
        0x2e2e2e,
        false,
        1.0,
        Feet::None,
        None,
    ),
    pot(
        "terracotta",
        Shape::Oval,
        0xb5653f,
        false,
        1.0,
        Feet::None,
        None,
    ),
    pot(
        "clay pot on feet",
        Shape::Oval,
        0x6b4e3d,
        false,
        1.0,
        Feet::Plain,
        None,
    ),
    pot(
        "blue glazed oval",
        Shape::Oval,
        0x3e5c76,
        true,
        1.0,
        Feet::Plain,
        Some(0x8b8e91),
    ),
    pot(
        "celadon on cloud feet",
        Shape::Oval,
        0x8fb5a0,
        true,
        1.0,
        Feet::Cloud,
        Some(0xc4a568),
    ),
    pot(
        "crackle with a carved band",
        Shape::Oval,
        0x5f7d86,
        true,
        1.0,
        Feet::Cloud,
        Some(0xddd8cc),
    ),
    pot(
        "antique",
        Shape::Oval,
        0x5a4636,
        true,
        1.0,
        Feet::Cloud,
        Some(0xc9a23a),
    ),
    pot(
        "hobnail bowl",
        Shape::Bowl,
        0x4b2a0f,
        true,
        1.6,
        Feet::None,
        None,
    ),
    pot(
        "vintage clay rectangle",
        Shape::Rect,
        0x7a3f2c,
        false,
        1.5,
        Feet::Cloud,
        None,
    ),
    pot(
        "glossy cobalt rectangle",
        Shape::Rect,
        0x23468f,
        true,
        1.0,
        Feet::Round,
        Some(0x8a4a2e),
    ),
    pot(
        "speckled green rectangle",
        Shape::Rect,
        0x3e8a70,
        true,
        1.0,
        Feet::Bracket,
        None,
    ),
];

/// Gives the pot of `level`; past the top you get the top pot.
pub fn model(level: u8) -> &'static Model {
    &MODELS[usize::from(level).min(MODELS.len() - 1)]
}

/// What a tree asks of its pot.
#[derive(Debug, Clone, Copy)]
pub struct Need {
    /// The pot width its size step calls for.
    pub width: f64,
    /// The trunk's width at its foot when it was repotted, the usual depth.
    pub trunk_depth: f64,
    /// How far it cascades, signed as [`crate::style::Leader::cascade`].
    pub cascade: f64,
    /// How far from the middle its trunk spills over the rim, if it does.
    pub spill: Option<f64>,
    /// The trunk's radius at its foot on the canvas.
    pub base_r: f64,
}

/// A pot's size on the canvas: half-width at the rim, the rim's half-height
/// as seen from a little above, depth, and half-width at the foot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dims {
    /// Half-width at the rim.
    pub hw: f64,
    /// Half-height of the rim as seen: the opening's front-to-back size.
    pub ry: f64,
    /// Depth of the wall.
    pub depth: f64,
    /// Half-width at the foot.
    pub bw: f64,
}

/// How far down a bowl's wall its belly is widest, and by how much.
const BELLY: (f64, f64) = (1.0 / 3.0, 1.07);

impl Model {
    /// Measures the soil this pot holds at `d`, in canvas units cubed.
    ///
    /// Each shape has its own rule: an oval is a frustum of ellipses, a bowl
    /// is summed slice by slice down its belly, a rectangle is a frustum of
    /// rectangles. The opening's front-to-back size is `d.ry`, scaled down to
    /// the foot as the width is. Compare volumes of one shape; across shapes
    /// they are only roughly comparable.
    pub fn volume(&self, d: &Dims) -> f64 {
        let squash = d.ry / d.hw;
        let frustum = |a1: f64, a2: f64| d.depth / 3.0 * (a1 + (a1 * a2).sqrt() + a2);
        match self.shape {
            Shape::Oval => std::f64::consts::PI * frustum(d.hw * d.ry, d.bw * d.bw * squash),
            Shape::Rect => frustum(4.0 * d.hw * d.ry, 4.0 * d.bw * d.bw * squash),
            Shape::Bowl => {
                // The wall's half-width down the bowl: through the rim, the
                // belly and the foot.
                let (tb, wb) = BELLY;
                let r = |t: f64| {
                    let (r0, r1, r2) = (d.hw, d.hw * wb, d.bw);
                    r0 * (t - tb) * (t - 1.0) / (tb * 1.0)
                        + r1 * t * (t - 1.0) / (tb * (tb - 1.0))
                        + r2 * t * (t - tb) / (1.0 - tb)
                };
                let n = 64;
                (0..n)
                    .map(|i| {
                        let w = r((f64::from(i) + 0.5) / f64::from(n));
                        std::f64::consts::PI * w * w * squash
                    })
                    .sum::<f64>()
                    * d.depth
                    / f64::from(n)
            }
        }
    }

    /// The rim's half-height as seen for a pot `hw` half-wide: a rectangle's
    /// opening reads deeper front to back than an oval's.
    fn ry(&self, hw: f64) -> f64 {
        let oval = hw * math::sin(TILT) * 0.55;
        if self.shape == Shape::Rect {
            oval * 1.35
        } else {
            oval
        }
    }

    /// The foot's half-width for a pot `hw` half-wide: an oval's sides grow
    /// straighter the more the tree cascades (`k`, 0 to 1).
    fn bw(&self, hw: f64, k: f64) -> f64 {
        match self.shape {
            Shape::Oval => hw * (0.88 + 0.09 * k),
            Shape::Bowl => hw * 0.69,
            Shape::Rect => hw * 0.88,
        }
    }

    fn dims_at(&self, hw: f64, depth: f64, k: f64) -> Dims {
        Dims {
            hw,
            ry: self.ry(hw),
            depth,
            bw: self.bw(hw, k),
        }
    }
}

/// Gives the size of pot `m` for what a tree needs.
///
/// The width is the tree's size step. The depth is the trunk's width at its
/// foot, at least 20 and at most the larger of 56 and a quarter of the width,
/// then the model's depth factor, at most half the width. A cascade keeps
/// that pot's soil and trades width for depth, up to 0.7 as deep as wide at
/// a full cascade; its rim stays inside the point where the trunk spills
/// over, and if that narrows it, it deepens to keep its soil, never deeper
/// than it is wide.
pub fn dims(need: &Need, m: &Model) -> Dims {
    let k = {
        let w = need.cascade.abs().min(1.0);
        w * w * (3.0 - 2.0 * w)
    };
    let cap = (need.width / 4.0).max(56.0);
    let depth = (need.trunk_depth.clamp(20.0, cap) * m.depth_factor).min(need.width / 2.0);
    let upright = m.dims_at(need.width / 2.0, depth, 0.0);
    if k == 0.0 {
        return upright;
    }
    let soil = m.volume(&upright);
    // The width at which the deeper pot holds the same soil: the volume
    // grows with the width at a fixed depth ratio, so halve the range.
    let ratio = (1.0 - k) * depth / need.width + k * 0.7;
    let mut hw = solve(
        1.0,
        need.width,
        |hw| m.volume(&m.dims_at(hw, ratio * 2.0 * hw, k)),
        soil,
    );
    let mut depth = ratio * 2.0 * hw;
    if let Some(spill) = need.spill {
        let room = spill.max(2.0 * need.base_r);
        if hw > room {
            hw = room;
            depth = solve(depth, 2.0 * hw, |d| m.volume(&m.dims_at(hw, d, k)), soil);
        }
    }
    m.dims_at(hw, depth, k)
}

/// Finds where the growing `f` reaches `target` between `lo` and `hi`; `hi`
/// if it never does.
fn solve(mut lo: f64, mut hi: f64, f: impl Fn(f64) -> f64, target: f64) -> f64 {
    if f(hi) <= target {
        return hi;
    }
    for _ in 0..60 {
        let mid = f64::midpoint(lo, hi);
        if f(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

/// The look of today's drawing for `level`, until each pot draws as its
/// model: the round pots keep theirs, the new ones stand in as the antique.
fn look(level: u8) -> u8 {
    level.min(6)
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
    /// The outline it is built on.
    pub shape: Shape,
    /// Its level on the ladder.
    pub level: u8,
}

/// Sizes the pot for `b`: its width from the tree's pot size, its depth from
/// the trunk it was repotted for, deeper and narrower for a cascade, with the
/// rim inside the point where the trunk spills over. `base_r` is the trunk's
/// radius at its foot on the canvas.
pub fn size(b: &Bonsai, scene: &Scene, base_r: f64, glaze: Rgb) -> Pot {
    let level = b.extras.pot_tier;
    let m = model(level);
    // Where the trunk spills over the rim, for a cascade.
    let spill = (b.leader.cascade != 0.0)
        .then(|| {
            scene.chains.first().and_then(|c| {
                c.0.iter()
                    .map(|p| project(p.0))
                    .find(|p| p.1 > SOIL_Y + 2.0 && (p.0 - CENTER_X).abs() > base_r)
                    .map(|p| (p.0 - CENTER_X).abs() - base_r - 4.0)
            })
        })
        .flatten();
    // Repotted in steps as the tree grows ([`Bonsai::grow`]): as deep as the
    // trunk it was repotted for is wide, the usual rule.
    let d = dims(
        &Need {
            width: b.pot,
            trunk_depth: b.pot_girth * 2.0,
            cascade: b.leader.cascade,
            spill,
            base_r,
        },
        m,
    );
    Pot {
        hw: d.hw,
        ry: d.ry,
        depth: d.depth,
        bw: d.bw,
        glaze,
        shape: m.shape,
        level,
    }
}

/// Gives where a pot's feet start and end, top to bottom: tucked up behind
/// the curve of its foot, peeking out below it.
fn feet_span(pot: &Pot) -> (f64, f64) {
    let top = SOIL_Y + pot.depth + pot.ry * 0.62;
    (top, top + 6.0 + pot.ry * 0.2)
}

/// Draws a pot's two feet as its model has them: in their own colour, or a
/// darker shade of the pot, with a darker outline and the right half in
/// shade, lit from the left. Nothing for a pot without feet.
fn feet(level: u8, pot: &Pot) -> String {
    let m = model(level);
    if m.feet == Feet::None {
        return String::new();
    }
    let color = m.feet_color.map_or(pot.glaze.shift(-0.08, 0.0), Rgb::hex);
    let (y, end) = feet_span(pot);
    let h = end - y;
    let fw = (pot.hw * 0.14).max(6.0);
    let mut s = String::new();
    for (i, side) in [-1.0, 1.0].into_iter().enumerate() {
        let x = CENTER_X + side * pot.bw * 0.68 - fw / 2.0;
        let mut d = PathData::new();
        match m.feet {
            Feet::Cloud => {
                d.m((x, y))
                    .l((x + fw, y))
                    .q((x + fw, y + h), (x + fw * 0.6, y + h * 0.95))
                    .q((x + fw * 0.5, y + h * 0.6), (x + fw * 0.38, y + h * 0.95))
                    .q((x, y + h), (x, y));
            }
            Feet::Bracket => {
                d.m((x, y))
                    .l((x + fw, y))
                    .l((x + fw, y + h * 0.65))
                    .q((x + fw * 0.82, y + h * 0.65), (x + fw * 0.76, y + h))
                    .l((x + fw * 0.24, y + h))
                    .q((x + fw * 0.18, y + h * 0.65), (x, y + h * 0.65));
            }
            Feet::Corner => {
                d.m((x, y)).l((x + fw, y)).l((x + fw, y + h)).l((x, y + h));
            }
            Feet::Round => {
                d.m((x, y))
                    .l((x + fw, y))
                    .q((x + fw, y + h), (x + fw / 2.0, y + h))
                    .q((x, y + h), (x, y));
            }
            Feet::Plain | Feet::None => {
                d.m((x, y))
                    .l((x + fw, y))
                    .l((x + fw * 0.9, y + h))
                    .l((x + fw * 0.1, y + h));
            }
        }
        d.z();
        let _ = write!(
            s,
            r#"<clipPath id="pf{i}"><path d="{d}"/></clipPath><path d="{d}" fill="{color}"/><rect x="{}" y="{}" width="{}" height="{}" fill="{}" clip-path="url(#pf{i})"/><path d="{d}" fill="none" stroke="{}" stroke-width=".7" stroke-linejoin="round"/>"#,
            Num(x + fw * 0.55),
            Num(y - 1.0),
            Num(fw),
            Num(h + 2.0),
            color.shift(-0.07, 0.0),
            color.shift(-0.2, 0.0),
            d = d.as_str(),
        );
    }
    s
}

/// Draws the pot's top as seen from a little above: a lighter rim ring, the
/// dark inside of the wall at the back, and the soil set a little lower.
pub fn top(pot: &Pot, soil: Rgb) -> String {
    format!(
        r#"<ellipse cx="{CENTER_X}" cy="{SOIL_Y}" rx="{}" ry="{}" fill="{}"/><ellipse cx="{CENTER_X}" cy="{}" rx="{}" ry="{}" fill="{}"/><ellipse cx="{CENTER_X}" cy="{}" rx="{}" ry="{}" fill="{soil}"/>"#,
        Num(pot.hw),
        Num(pot.ry),
        pot.glaze.shift(0.08, 0.0),
        Num(SOIL_Y - 0.15),
        Num(pot.hw - 2.4),
        Num((pot.ry - 0.9).max(1.0)),
        pot.glaze.shift(-0.2, 0.0),
        Num(SOIL_Y + 0.6),
        Num(pot.hw - 4.0),
        Num((pot.ry - 2.0).max(1.0)),
    )
}

/// Gives the pot's glaze for `level`: the model's, or the configured glaze
/// on a glazed pot; lifted a little on the dark theme, except the plastic
/// training pot, which takes the theme's own plastic.
pub fn glaze(level: u8, config: &Config, paints: &Paints) -> Rgb {
    let m = model(level);
    let body = if level == 0 {
        paints.plastic
    } else if m.glazed {
        config.pot.glaze.unwrap_or(Rgb::hex(m.glaze))
    } else {
        Rgb::hex(m.glaze)
    };
    if level > 0 && paints.dark {
        body.shift(0.05, 0.0)
    } else {
        body
    }
}

/// Moss on the soil while a weekly streak runs.
pub fn moss(b: &Bonsai, paints: &Paints, pot: &Pot) -> (String, String) {
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
        // A cushion: two overlapping lobes, the second a little to one side.
        let side = if r.unit() < 0.5 { -1.0 } else { 1.0 };
        let _ = write!(
            dark[near],
            r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}"/><ellipse cx="{}" cy="{}" rx="{}" ry="{}"/>"#,
            Num(x),
            Num(y),
            Num(rx),
            Num(ry),
            Num(x + side * rx * 0.6),
            Num(y + ry * 0.25),
            Num(rx * 0.7),
            Num(ry * 0.85)
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

/// The pot's front wall, lit from the left, with its tier's details: a left
/// tree's is mirrored.
pub fn front(b: &Bonsai, pot: &Pot) -> String {
    let Pot {
        hw,
        ry,
        depth,
        bw,
        glaze,
        ..
    } = *pot;
    let tier = look(b.extras.pot_tier);
    let bottom = SOIL_Y + depth;
    let dark = glaze.shift(-0.08, 0.0);
    let mut s = String::new();
    // Feet first, so the wall overlaps their tops.
    s.push_str(&feet(b.extras.pot_tier, pot));
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
pub fn rim(pot: &Pot) -> String {
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
pub fn marks(b: &Bonsai, pot: &Pot) -> String {
    let Pot {
        hw,
        ry,
        depth,
        bw,
        glaze,
        ..
    } = *pot;
    let tier = look(b.extras.pot_tier);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Eight round pots first, then the bowl, then three rectangles, each
    /// earned by a longer streak.
    #[test]
    fn the_ladder_runs_round_then_the_bowl_then_rectangles() {
        let shapes: Vec<Shape> = (0..11).map(|l| model(l).shape).collect();
        assert!(shapes[..7].iter().all(|s| *s == Shape::Oval), "{shapes:?}");
        assert_eq!(shapes[7], Shape::Bowl);
        assert!(shapes[8..].iter().all(|s| *s == Shape::Rect), "{shapes:?}");
        assert_eq!(model(10).name, "speckled green rectangle");
        assert_eq!(
            crate::extras::POT_TIERS,
            [4, 13, 26, 39, 52, 78, 104, 136, 174, 216]
        );
    }

    fn need(width: f64, trunk_depth: f64, cascade: f64, spill: Option<f64>, base_r: f64) -> Need {
        Need {
            width,
            trunk_depth,
            cascade,
            spill,
            base_r,
        }
    }

    /// Each shape measures its own soil: an oval with straight sides is a
    /// frustum of ellipses, a bowl's belly and a rectangle's corners hold more.
    #[test]
    fn each_shape_measures_its_own_soil() {
        let d = Dims {
            hw: 100.0,
            ry: 20.0,
            depth: 40.0,
            bw: 80.0,
        };
        let oval = model(3).volume(&d);
        let bowl = model(7).volume(&d);
        let rect = model(8).volume(&d);
        assert!(
            oval > 0.0 && bowl > oval && rect > oval,
            "{oval} {bowl} {rect}"
        );
        let (a1, a2) = (100.0 * 20.0, 80.0 * 16.0);
        let frustum = std::f64::consts::PI * 40.0 / 3.0 * (a1 + f64::sqrt(a1 * a2) + a2);
        assert!((oval - frustum).abs() / frustum < 1e-9, "{oval} {frustum}");
    }

    /// A cascade's pot holds the soil of the pot it would get standing
    /// upright: narrower and deeper, 0.7 as deep as it is wide.
    #[test]
    fn a_cascade_keeps_the_upright_pots_soil() {
        for level in 0..11u8 {
            let m = model(level);
            let up = dims(&need(195.0, 36.0, 0.0, None, 14.0), m);
            let down = dims(&need(195.0, 36.0, 1.0, None, 14.0), m);
            let (a, b) = (m.volume(&up), m.volume(&down));
            assert!((a - b).abs() / a < 0.01, "level {level}: {a} vs {b}");
            assert!(
                (down.depth / (2.0 * down.hw) - 0.7).abs() < 0.02,
                "level {level}: {} deep, {} wide",
                down.depth,
                2.0 * down.hw
            );
            assert!(down.hw < up.hw, "level {level}");
        }
    }

    /// A trunk spilling over close to the middle keeps the rim inside it:
    /// the pot gets deeper to keep its soil, never deeper than it is wide.
    #[test]
    fn a_spill_close_in_makes_the_pot_deeper_not_smaller() {
        let m = model(3);
        let free = dims(&need(195.0, 36.0, 1.0, None, 14.0), m);
        // Free, the pot is about 121 wide; a spill 55 from the middle holds it to 110.
        let held = dims(&need(195.0, 36.0, 1.0, Some(55.0), 14.0), m);
        assert!(held.hw <= 55.0 + 1e-9, "{}", held.hw);
        assert!((m.volume(&held) - m.volume(&free)).abs() / m.volume(&free) < 0.01);
        assert!(held.depth > free.depth, "{} vs {}", held.depth, free.depth);
        assert!(held.depth <= 2.0 * held.hw + 1e-9);
    }

    /// The depth cap grows with the pot: a big pot can be a quarter of its
    /// width deep; small and middle pots keep the trunk rule.
    #[test]
    fn big_pots_go_deeper_and_small_ones_stay() {
        let big = dims(&need(305.0, 133.0, 0.0, None, 66.0), model(3));
        assert!((big.depth - 76.25).abs() < 0.5, "{}", big.depth);
        let small = dims(&need(125.0, 30.0, 0.0, None, 10.0), model(3));
        assert!((small.depth - 30.0).abs() < 1e-9, "{}", small.depth);
        let bowl = dims(&need(305.0, 133.0, 0.0, None, 66.0), model(7));
        assert!(
            bowl.depth > big.depth && bowl.depth <= 305.0 / 2.0 + 1e-9,
            "{}",
            bowl.depth
        );
    }

    /// A pot's feet tuck up behind its curved foot and peek out below it,
    /// at the smallest pot and the largest.
    #[test]
    fn feet_tuck_under_the_pot_and_peek_out() {
        for hw in [40.0, 200.0] {
            let pot = Pot {
                hw,
                ry: hw * 0.17,
                depth: 30.0,
                bw: hw * 0.88,
                glaze: Rgb::hex(0x3e5c76),
                shape: Shape::Oval,
                level: 3,
            };
            let (top, bottom) = feet_span(&pot);
            // Where the foot's front edge is, at the feet: 0.6 of the way out.
            let edge = SOIL_Y + pot.depth + pot.ry * 0.8;
            assert!(
                top < edge,
                "{hw}: the feet start at {top}, below the pot's edge {edge}"
            );
            assert!(
                bottom > edge + 2.0,
                "{hw}: the feet end at {bottom}, hidden behind {edge}"
            );
        }
    }

    /// Every level draws at the smallest pot without a broken number.
    #[test]
    fn every_level_draws_at_the_smallest_pot() {
        let mut b = crate::Bonsai::grow(
            &crate::fixtures::by_name("steady").unwrap().snapshot,
            &Config::default(),
            None,
        );
        for level in 0..11u8 {
            b.extras.pot_tier = level;
            let pot = Pot {
                hw: 40.0,
                ry: 6.8,
                depth: 20.0,
                bw: 35.0,
                glaze: Rgb::hex(model(level).glaze),
                shape: model(level).shape,
                level,
            };
            let svg = front(&b, &pot) + &rim(&pot) + &top(&pot, Rgb::hex(0x5a4632));
            assert!(
                !svg.contains("NaN") && !svg.contains("inf"),
                "level {level}"
            );
        }
    }

    /// Only glazed pots take a configured glaze.
    #[test]
    fn unglazed_pots_keep_their_clay() {
        let glazed: Vec<u8> = (0..11).filter(|&l| model(l).glazed).collect();
        assert_eq!(glazed, vec![3, 4, 5, 6, 7, 9, 10]);
    }
}
