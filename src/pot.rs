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
        0x36_2612,
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

/// The rim's half-height as seen for a pot `hw` half-wide: seen from the
/// same angle, every shape's opening is as deep front to back as an oval's.
fn rim_ry(hw: f64) -> f64 {
    hw * math::sin(TILT) * 0.55
}

/// How far below its rim the bowl's soil lies, as a share of its depth.
const BOWL_INSET: f64 = 0.09;
/// How deep the bowl is for its width: one shape at every size.
const BOWL_DEPTH: f64 = 0.38;

/// How far a rectangle's top face is drawn in at its back edge, on each
/// side, in detail units: a touch of depth, the back a little narrower than
/// the front.
const RECT_BACK_TAPER: f64 = 3.0;

/// How far below its rim frame a rectangle's soil lies, as a share of its
/// depth: enough for a thin line of inner wall.
const RECT_INSET: f64 = 0.035;

/// The bowl's wall is one curve from the rim to the foot: it leaves the rim
/// toward this share of the rim's half-width, this share of the depth down
/// (its shoulder)...
const SHOULDER: (f64, f64) = (1.07, 0.42);

/// ...and comes into the foot from this share of the foot's half-width, so
/// it swells a little below the rim and rounds in to a small foot.
const FOOT_SWELL: f64 = 1.22;

/// Gives a bowl's half-width a fraction `t` of the way down, on the curve
/// its wall is drawn with: from the rim `hw` to the foot `bw`.
fn belly(hw: f64, bw: f64, t: f64) -> f64 {
    let (sx, sy) = SHOULDER;
    // The curve's height grows along it: find where it is `t` down.
    let at = |u: f64, a: f64, b: f64, c: f64, d: f64| {
        let v = 1.0 - u;
        v * v * v * a + 3.0 * v * v * u * b + 3.0 * v * u * u * c + u * u * u * d
    };
    // At the rim and the foot the curve is where it starts and ends.
    if t <= 0.0 {
        return hw;
    }
    if t >= 1.0 {
        return bw;
    }
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..50 {
        let mid = f64::midpoint(lo, hi);
        if at(mid, 0.0, sy, 1.0, 1.0) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    at(hi, hw, hw * sx, bw * FOOT_SWELL, bw)
}

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
                let n = 64;
                (0..n)
                    .map(|i| {
                        let w = belly(d.hw, d.bw, (f64::from(i) + 0.5) / f64::from(n));
                        std::f64::consts::PI * w * w * squash
                    })
                    .sum::<f64>()
                    * d.depth
                    / f64::from(n)
            }
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
            ry: rim_ry(hw),
            depth,
            bw: self.bw(hw, k),
        }
    }
}

/// Finds how far from the pot's middle a cascade's trunk spills over the
/// rim, less the trunk's radius and a little room: the nearest point where
/// any of its wood drops below the rim, since the hanging trunk is a chain
/// of its own. `None` for a tree that doesn't cascade.
fn spill_of(b: &Bonsai, scene: &Scene, base_r: f64) -> Option<f64> {
    if b.leader.cascade == 0.0 {
        return None;
    }
    scene
        .chains
        .iter()
        .filter_map(|c| {
            c.0.iter()
                .map(|p| project(p.0))
                .find(|p| p.1 > SOIL_Y + 2.0 && (p.0 - CENTER_X).abs() > base_r)
                .map(|p| (p.0 - CENTER_X).abs() - base_r - 4.0)
        })
        .reduce(f64::min)
}

/// Gives the size of pot `m` for what a tree needs.
///
/// The width is the tree's size step. The depth is the trunk's width at its
/// foot, at least 20 and at most the larger of 56 and a quarter of the width,
/// then the model's depth factor, at most half the width, or 0.27 of it for a
/// rectangle (just deeper than its mockup). A cascade keeps that pot's soil
/// and trades width for depth, up to 0.7 as deep as wide at a full cascade;
/// its rim stays inside the point where the trunk spills over, and if that
/// narrows it, it deepens to keep its soil, never deeper than it is wide.
///
/// The bowl is one shape at every size, [`BOWL_DEPTH`] as deep as it is wide:
/// a cascade gets the same bowl, and a spill close in makes it smaller.
pub fn dims(need: &Need, m: &Model) -> Dims {
    if m.shape == Shape::Bowl {
        let mut hw = need.width / 2.0;
        if let Some(spill) = need.spill {
            hw = hw.min(spill.max(2.0 * need.base_r));
        }
        return m.dims_at(hw, BOWL_DEPTH * 2.0 * hw, 0.0);
    }
    let k = {
        let w = need.cascade.abs().min(1.0);
        w * w * (3.0 - 2.0 * w)
    };
    let cap = (need.width / 4.0).max(56.0);
    let deepest = match m.shape {
        Shape::Oval => 0.5,
        Shape::Bowl => BOWL_DEPTH,
        Shape::Rect => 0.27,
    };
    let depth = (need.trunk_depth.clamp(20.0, cap) * m.depth_factor).min(need.width * deepest);
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
    // A rectangle's lip reaches past its wall, a share of its width: its
    // wall stays that much further in.
    let spill = spill_of(b, scene, base_r).map(|s| {
        if m.shape == Shape::Rect {
            let probe = Pot {
                hw: 100.0,
                ry: 0.0,
                depth: 0.0,
                bw: 0.0,
                glaze,
                shape: Shape::Rect,
                level,
            };
            s / (1.0 + lip_of(&probe).0 / 100.0)
        } else {
            s
        }
    });
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
    let top = if pot.shape == Shape::Rect {
        SOIL_Y + pot.depth + pot.ry - 2.0
    } else {
        SOIL_Y + pot.depth + pot.ry * 0.62
    };
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
    match pot.shape {
        Shape::Rect => return top_rect(pot, soil),
        Shape::Bowl => return top_bowl(pot, soil),
        Shape::Oval => {}
    }
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

/// The bowl seen from above: its rim ring, the glossy inner wall a shade
/// darker than outside, and the soil lying [`Pot::inset`] lower, a little
/// narrower, never past the inner wall.
fn top_bowl(pot: &Pot, soil: Rgb) -> String {
    let (hw, ry, rim) = (pot.hw, pot.ry, pot.rim_y());
    let (iw, iry) = (hw - 2.4, (ry - 0.9).max(1.0));
    format!(
        r#"<ellipse cx="{CENTER_X}" cy="{}" rx="{}" ry="{}" fill="{}"/><clipPath id="po"><ellipse cx="{CENTER_X}" cy="{o}" rx="{}" ry="{}"/></clipPath><ellipse cx="{CENTER_X}" cy="{o}" rx="{}" ry="{}" fill="{}"/><g clip-path="url(#po)"><ellipse cx="{CENTER_X}" cy="{}" rx="{}" ry="{}" fill="{soil}"/></g>"#,
        Num(rim),
        Num(hw),
        Num(ry),
        pot.glaze.shift(0.08, 0.0),
        Num(iw),
        Num(iry),
        Num(iw),
        Num(iry),
        pot.glaze.shift(-0.1, 0.0),
        Num(SOIL_Y + 0.6),
        Num((hw - 4.0) * 0.95),
        Num((ry - 2.0).max(1.0) * 0.95),
        o = Num(rim - 0.15),
    )
}

/// Draws a pot's soft shadow on the table, offset away from the light
/// (`lit` is 1 lit from the left, -1 from the right), so the pot sits down
/// instead of floating.
pub fn shadow(pot: &Pot, lit: f64, dark: bool) -> String {
    let shade = if dark { "#000" } else { "#3b3226" };
    // The bowl and a rectangle: a tight, soft contact shadow where the
    // saucer or the feet meet the table, as wide as the foot.
    if pot.shape != Shape::Oval {
        let k = detail(pot);
        let (foot, width) = if pot.shape == Shape::Rect {
            (feet_span(pot).1 - pot.inset(), pot.bw * 1.04)
        } else {
            let sy = pot.ry * pot.bw / pot.hw * 0.8;
            (pot.rim_y() + pot.depth + 4.0 * k + sy * 0.6, pot.bw * 1.32)
        };
        return format!(
            r#"<ellipse cx="{}" cy="{}" rx="{}" ry="3.4" fill="{shade}" opacity=".1"/><ellipse cx="{}" cy="{}" rx="{}" ry="1.8" fill="{shade}" opacity=".2"/>"#,
            Num(CENTER_X + 3.0 * lit),
            Num(foot + 0.6),
            Num(width * 1.1),
            Num(CENTER_X + 2.0 * lit),
            Num(foot + 0.3),
            Num(width * 0.98),
        );
    }
    let foot = SOIL_Y + pot.depth;
    format!(
        r#"<ellipse cx="{}" cy="{}" rx="{}" ry="4" fill="{shade}" opacity=".16"/><ellipse cx="{}" cy="{}" rx="{}" ry="2.4" fill="{shade}" opacity=".26"/>"#,
        Num(CENTER_X + 5.0 * lit),
        Num(foot + 1.2),
        Num(pot.bw * 1.18),
        Num(CENTER_X + 3.5 * lit),
        Num(foot + 0.8),
        Num(pot.bw * 1.02),
    )
}

impl Pot {
    /// Gives how far below the rim the soil lies, a share of the depth: the
    /// bowl's sits visibly down inside, a rectangle's just below its rim
    /// frame, an oval is filled to the brim. The pot stands higher by as
    /// much, and the tree stays.
    pub fn inset(&self) -> f64 {
        self.depth
            * match self.shape {
                Shape::Oval => 0.0,
                Shape::Bowl => BOWL_INSET,
                Shape::Rect => RECT_INSET,
            }
    }

    /// Gives where the rim's top lies: the soil's level, raised by
    /// [`Self::inset`].
    pub fn rim_y(&self) -> f64 {
        SOIL_Y - self.inset()
    }

    /// Gives the wall's half-width a fraction `t` of the way down, from the
    /// rim at 0 to the foot at 1.
    pub fn half_width(&self, t: f64) -> f64 {
        if self.shape == Shape::Bowl {
            belly(self.hw, self.bw, t)
        } else {
            self.hw + (self.bw - self.hw) * t
        }
    }

    /// Gives where the rim's front edge is at `x`: on an oval or a bowl the
    /// ellipse just inside the lip, highest at the sides; on a rectangle the
    /// straight front edge of its opening. Past the sides you get the rim's
    /// height there.
    pub fn rim_front_y(&self, x: f64) -> f64 {
        if self.shape == Shape::Rect {
            return self.rim_y() + self.ry;
        }
        let across = ((x - CENTER_X) / self.hw).clamp(-1.0, 1.0);
        self.rim_y() + (self.ry - 1.2) * (1.0 - across * across).sqrt()
    }

    /// Gives the pot's wall as drawn, the outline the details on it and the
    /// kintsugi clip to: an oval's from its rim to its foot, a bowl's round
    /// belly, a rectangle's face under its lip.
    pub fn wall(&self) -> PathData {
        let (hw, ry, d, bw) = (self.hw, self.ry, self.depth, self.bw);
        let cx = CENTER_X;
        match self.shape {
            Shape::Oval => {
                let mut wall = PathData::new();
                wall.m((cx - hw, SOIL_Y))
                    .a(hw, ry, false, (cx + hw, SOIL_Y))
                    .l((cx + bw, SOIL_Y + d))
                    .a(bw, ry, true, (cx - bw, SOIL_Y + d))
                    .z();
                wall
            }
            Shape::Bowl => {
                let (rim, by) = (SOIL_Y, SOIL_Y + d);
                let mut wall = PathData::new();
                wall.m((cx - hw, rim))
                    .a(hw, ry, false, (cx + hw, rim))
                    .c(
                        (cx + hw * SHOULDER.0, rim + d * SHOULDER.1),
                        (cx + bw * FOOT_SWELL, by),
                        (cx + bw, by),
                    )
                    .a(bw, ry * bw / hw, true, (cx - bw, by))
                    .c(
                        (cx - bw * FOOT_SWELL, by),
                        (cx - hw * SHOULDER.0, rim + d * SHOULDER.1),
                        (cx - hw, rim),
                    )
                    .z();
                wall
            }
            Shape::Rect => {
                let k = detail(self);
                let (_, lip_h) = lip_of(self);
                let (y0, by) = (SOIL_Y + ry + lip_h, SOIL_Y + ry + d);
                let pot = self;
                let mut face = PathData::new();
                match pot.level {
                    // Gently bulging sides, widest a third of the way down.
                    9 => {
                        face.m((cx - hw, y0))
                            .l((cx + hw, y0))
                            .c(
                                (cx + hw * 1.03, y0 + (by - y0) * 0.3),
                                (cx + bw * 1.02, by - 4.0 * k),
                                (cx + bw, by),
                            )
                            .l((cx - bw, by))
                            .c(
                                (cx - bw * 1.02, by - 4.0 * k),
                                (cx - hw * 1.03, y0 + (by - y0) * 0.3),
                                (cx - hw, y0),
                            );
                    }
                    // Tapering to the foot, with rounded corners.
                    10 => {
                        let cr = 9.0 * k;
                        face.m((cx - hw, y0))
                            .l((cx + hw, y0))
                            .l((cx + bw + cr * 0.3, by - cr))
                            .q((cx + bw, by), (cx + bw - cr, by))
                            .l((cx - bw + cr, by))
                            .q((cx - bw, by), (cx - bw - cr * 0.3, by - cr));
                    }
                    _ => {
                        face.m((cx - hw, y0))
                            .l((cx + hw, y0))
                            .l((cx + bw, by))
                            .l((cx - bw, by));
                    }
                }
                face.z();
                face
            }
        }
    }
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
/// tree's is mirrored. `dark` lifts colours a glaze does not set, as the
/// glaze itself is lifted on a dark page.
pub fn front(b: &Bonsai, pot: &Pot, dark: bool) -> String {
    match pot.shape {
        Shape::Bowl => return lift(pot, front_bowl(b, pot, dark)),
        Shape::Rect => return lift(pot, front_rect(b, pot)),
        Shape::Oval => {}
    }
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
    let wall = pot.wall();
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
pub const SHADE: [(f64, f64); 2] = [(0.2, -0.035), (0.38, -0.07)];

/// Draws the rim's lip: a band astride the rim's front edge, 2.4 deep at
/// the front and tapering to nothing at the sides, stepped into shade where
/// the wall is, edge to edge with it. Lit from the left: mirror it for a
/// left tree.
pub fn rim(pot: &Pot) -> String {
    match pot.shape {
        Shape::Rect => lift(pot, rim_rect(pot)),
        Shape::Bowl => lift(pot, rim_round(pot)),
        Shape::Oval => rim_round(pot),
    }
}

/// Raises what is drawn of `pot` at the soil's level to where its rim
/// stands ([`Pot::inset`]); an oval's stays.
pub fn lift(pot: &Pot, svg: String) -> String {
    if pot.inset() == 0.0 || svg.is_empty() {
        return svg;
    }
    format!(
        r#"<g transform="translate(0 -{})">{svg}</g>"#,
        Num(pot.inset())
    )
}

/// The rim of a round pot or the bowl, at the soil's level.
fn rim_round(pot: &Pot) -> String {
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
    if pot.shape != Shape::Oval {
        return String::new();
    }
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

/// The scale of a pot's small details: 1 at a pot 200 wide.
fn detail(pot: &Pot) -> f64 {
    (pot.hw / 100.0).clamp(0.4, 2.0)
}

/// Writes the shade steps over a wall already drawn in its glaze: lit from
/// the left, two darker steps to the right.
fn shade_steps(s: &mut String, pot: &Pot, top: f64, bottom: f64) {
    for (f, tone) in SHADE {
        let _ = write!(
            s,
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
            Num(CENTER_X + pot.hw * f),
            Num(top),
            Num(pot.hw * 1.3),
            Num(bottom - top + 2.0),
            pot.glaze.shift(tone, 0.0)
        );
    }
}

/// Draws the hobnail bowl: a round belly, raised amber nubs in staggered
/// rows on its glaze, a green glaze running down in drips, a plain rim band
/// and a gloss streak, on a wooden saucer.
fn front_bowl(b: &Bonsai, pot: &Pot, dark: bool) -> String {
    let (hw, ry, d, bw) = (pot.hw, pot.ry, pot.depth, pot.bw);
    let (cx, rim) = (CENTER_X, SOIL_Y);
    let by = rim + d;
    let k = detail(pot);
    // The green glaze sits a little lighter on a dark page, as the body does.
    let lift = |hex: u32| {
        let c = Rgb::hex(hex);
        if dark { c.shift(0.05, 0.0) } else { c }
    };
    let mut s = String::new();
    // The bamboo saucer, behind and under the bowl.
    let (sx, sy) = (bw * 1.32, ry * bw / hw * 0.8);
    let _ = write!(
        s,
        r##"<ellipse cx="{cx}" cy="{}" rx="{}" ry="{}" fill="#b5843f"/><ellipse cx="{cx}" cy="{}" rx="{}" ry="{}" fill="#e7b45e"/><g fill="none" stroke="#c39047" stroke-width=".7" opacity=".7"><ellipse cx="{cx}" cy="{}" rx="{}" ry="{}"/><ellipse cx="{cx}" cy="{}" rx="{}" ry="{}"/></g>"##,
        Num(by + 4.0 * k),
        Num(sx),
        Num(sy),
        Num(by + 2.0 * k),
        Num(sx),
        Num(sy),
        Num(by + 2.0 * k),
        Num(sx * 0.86),
        Num(sy * 0.86),
        Num(by + 2.0 * k),
        Num(sx * 0.95),
        Num(sy * 0.95),
    );
    let wall = pot.wall();
    let _ = write!(
        s,
        r#"<clipPath id="pw"><path d="{w}"/></clipPath><path d="{w}" fill="{}"/><g clip-path="url(#pw)">"#,
        pot.glaze,
        w = wall.as_str()
    );
    shade_steps(&mut s, pot, rim, by + ry);
    // The olive glaze over the lower third, running up into the brown in
    // narrow streaks that taper to a point: most stay low, a few reach high
    // between the nub rows. Two thin half-tone fringes soften its edge, and
    // a few short streaks of olive sit loose above it.
    let mut r = Rng::keyed(b.account_id, "pot/bowl");
    let edge_y = rim + d * 0.72;
    let mut run = PathData::coarse();
    run.m((cx - hw * 1.2, edge_y));
    let mut x = cx - hw * 1.2;
    while x < cx + hw * 1.2 {
        // A stretch of the base line, gently uneven, then a streak.
        let gap = r.range(3.0, 11.0) * k;
        run.q(
            (x + gap / 2.0, edge_y + r.range(-1.5, 1.5) * k),
            (x + gap, edge_y),
        );
        x += gap;
        let w = r.range(3.0, 7.0) * k;
        let h = math::pow(r.unit(), 2.6) * d * 0.36 + r.range(1.0, 3.0) * k;
        let tip = x + w / 2.0 + r.range(-0.2, 0.2) * w;
        // Steep sides, a narrow rounded tip.
        run.c(
            (x + w * 0.15, edge_y - h * 0.6),
            (tip - w * 0.25, edge_y - h),
            (tip, edge_y - h),
        )
        .c(
            (tip + w * 0.25, edge_y - h),
            (x + w * 0.85, edge_y - h * 0.6),
            (x + w, edge_y),
        );
        x += w;
    }
    run.l((cx + hw * 1.3, by + ry * 2.0))
        .l((cx - hw * 1.3, by + ry * 2.0))
        .z();
    let (green, edge_green) = (lift(0x5e_5418), lift(0x7a_6a22));
    let _ = write!(
        s,
        r##"<defs><path id="ol" d="{r}"/></defs><use href="#ol" fill="none" stroke="{edge_green}" stroke-width="{}" stroke-linejoin="round" opacity=".3"/><use href="#ol" fill="{green}" stroke="{edge_green}" stroke-width="{}" stroke-linejoin="round"/><g fill="{edge_green}" opacity=".45">"##,
        Num(4.0 * k),
        Num(1.6 * k),
        r = run.as_str()
    );
    for _ in 0..9 {
        let _ = write!(
            s,
            r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}"/>"#,
            Num::coarse(cx + r.range(-0.9, 0.9) * hw),
            Num::coarse(edge_y - r.range(2.0, 12.0) * k),
            Num::coarse(r.range(0.6, 1.3) * k),
            Num::coarse(r.range(2.0, 5.0) * k)
        );
    }
    s.push_str("</g>");
    // Dense raised nubs, small glossy drops in staggered rows that follow
    // the curve: amber on the brown, green-gold where the olive runs. One nub
    // and two rows (the second staggered) are drawn once; each row is reused
    // at its depth, scaled to the wall there, and the whole field once more
    // in green-gold, clipped to the olive. Nubs foreshortened at the sides
    // are left out.
    let (rows, across) = (12, 21);
    let band = 5.5 * k;
    let step = (d - band) / f64::from(rows);
    let z = (step * 0.36).min(2.6 * k);
    let _ = write!(
        s,
        r##"<defs><g id="nub"><ellipse cy="{}" rx="{}" ry="{}" fill="#1e0f04" opacity=".5"/><path d="M0 {}C{} {} {} {} 0 {}C{} {} {} {} 0 {}Z"/><ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="#fff3c4" opacity=".85"/></g>"##,
        Num(0.5 * z),
        Num(0.85 * z),
        Num(0.95 * z),
        Num(-z),
        Num(0.6 * z),
        Num(-0.4 * z),
        Num(0.75 * z),
        Num(0.25 * z),
        Num(0.8 * z),
        Num(-0.75 * z),
        Num(0.25 * z),
        Num(-0.6 * z),
        Num(-0.4 * z),
        Num(-z),
        Num(-0.25 * z),
        Num(-0.35 * z),
        Num(0.22 * z),
        Num(0.3 * z)
    );
    for (id, stagger) in [("nr0", 0.0), ("nr1", 0.5)] {
        let _ = write!(s, r#"<g id="{id}">"#);
        for i in 0..across {
            let t = (f64::from(i) + stagger + 0.25) / f64::from(across);
            let a = std::f64::consts::PI * (1.0 - t);
            if math::cos(a).abs() > 0.93 {
                continue;
            }
            let _ = write!(
                s,
                r##"<use href="#nub" x="{}" y="{}"/>"##,
                Num::coarse(hw * math::cos(a) * 0.97),
                Num::coarse(ry * math::sin(a))
            );
        }
        s.push_str("</g>");
    }
    s.push_str(r#"<g id="nubs">"#);
    for row in 0..rows {
        let down = band + (d - band) * (f64::from(row) + 0.5) / f64::from(rows);
        let h = pot.half_width(down / d) / hw;
        let _ = write!(
            s,
            r##"<use href="#nr{}" transform="translate({cx} {})scale({})"/>"##,
            row % 2,
            Num::coarse(rim + down),
            Num(h)
        );
    }
    let _ = write!(
        s,
        r##"</g><clipPath id="pg"><use href="#ol"/></clipPath></defs><use href="#nubs" fill="#d9a03c"/><g clip-path="url(#pg)"><use href="#nubs" fill="#b0a03c"/></g>"##
    );
    // The plain glossy rim band above the nubs, its groove, and a gloss
    // streak on the lit side.
    let _ = write!(
        s,
        r##"<path d="M{l} {rim}A{hw} {ry} 0 0 0 {r} {rim}L{r} {b}A{hw} {ry} 0 0 1 {l} {b}Z" fill="{}"/><path d="M{l} {b}A{hw} {ry} 0 0 0 {r} {b}" fill="none" stroke="#24130a" stroke-width="1.2" opacity=".7"/><path d="M{} {}Q{} {} {} {}" fill="none" stroke="#fff" stroke-opacity=".22" stroke-width="{}" stroke-linecap="round"/>"##,
        lift(0x57_4930),
        Num(cx - hw * 0.78),
        Num(rim + d * 0.16),
        Num(cx - hw * 0.9),
        Num(rim + d * 0.45),
        Num(cx - hw * 0.66),
        Num(rim + d * 0.78),
        Num(3.0 * k),
        l = Num(cx - hw * 0.99),
        r = Num(cx + hw * 0.99),
        hw = Num(hw * 0.99),
        ry = Num(ry),
        rim = Num(rim),
        b = Num(rim + band),
    );
    s.push_str("</g>");
    s
}

/// How far a rectangle's lip reaches past its wall on each side, and how
/// deep the lip band is.
fn lip_of(pot: &Pot) -> (f64, f64) {
    let k = detail(pot);
    match pot.level {
        10 => (12.0 * k, 8.0 * k),
        9 => (5.0 * k, 7.0 * k),
        _ => (4.0 * k, 7.0 * k),
    }
}

/// Writes `n` dots in one group, all in `fill`, scattered over the face of a
/// rectangle between `y0` and `y1`.
#[allow(clippy::too_many_arguments, reason = "one call per kind of speckle")]
fn speckle(
    s: &mut String,
    r: &mut Rng,
    pot: &Pot,
    (y0, y1): (f64, f64),
    n: usize,
    fill: &str,
    opacity: &str,
    size: (f64, f64),
) {
    let k = detail(pot);
    let _ = write!(s, r#"<g fill="{fill}" opacity="{opacity}">"#);
    for _ in 0..n {
        let _ = write!(
            s,
            r#"<circle cx="{}" cy="{}" r="{}"/>"#,
            Num::coarse(CENTER_X + r.range(-1.0, 1.0) * pot.hw),
            Num::coarse(r.range(y0, y1)),
            Num::coarse(r.range(size.0, size.1) * k)
        );
    }
    s.push_str("</g>");
}

/// Draws a rectangle's front: its feet, then its face under the lip,
/// tapering to the foot, with each model's finish.
fn front_rect(b: &Bonsai, pot: &Pot) -> String {
    let (hw, ry, d, bw) = (pot.hw, pot.ry, pot.depth, pot.bw);
    let cx = CENTER_X;
    let k = detail(pot);
    let (_, lip_h) = lip_of(pot);
    let y0 = SOIL_Y + ry + lip_h;
    let by = SOIL_Y + ry + d;
    let mut s = feet(pot.level, pot);
    let face = pot.wall();
    let _ = write!(
        s,
        r#"<clipPath id="pw"><path d="{f}"/></clipPath><path d="{f}" fill="{}"/><g clip-path="url(#pw)">"#,
        pot.glaze,
        f = face.as_str()
    );
    shade_steps(&mut s, pot, y0, by);
    let mut r = Rng::keyed(b.account_id, "pot/rect");
    match pot.level {
        // Vintage clay: fine grain, a faint mineral bloom near the foot, a
        // carved panel, the maker's chop pressed into the clay.
        8 => {
            speckle(
                &mut s,
                &mut r,
                pot,
                (y0, by),
                45,
                "#4a2116",
                ".45",
                (0.3, 0.7),
            );
            speckle(
                &mut s,
                &mut r,
                pot,
                (y0, by),
                25,
                "#b07a5c",
                ".4",
                (0.3, 0.7),
            );
            s.push_str(r##"<g fill="#e8dccb" opacity=".13">"##);
            for _ in 0..5 {
                let _ = write!(
                    s,
                    r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}"/>"#,
                    Num::coarse(cx + r.range(-0.8, 0.8) * bw),
                    Num::coarse(by - r.range(2.0, 9.0) * k),
                    Num::coarse(r.range(8.0, 18.0) * k),
                    Num::coarse(r.range(1.5, 3.0) * k)
                );
            }
            s.push_str("</g>");
            let (px0, px1) = (
                cx - f64::midpoint(hw, bw) + 8.0 * k,
                cx + f64::midpoint(hw, bw) - 8.0 * k,
            );
            let (py0, py1) = (y0 + 6.0 * k, by - 8.0 * k);
            let _ = write!(
                s,
                r##"<path d="M{} {}H{}L{} {}H{}Z" fill="none" stroke="#3d1a10" stroke-width="{}" opacity=".75"/><path d="M{} {}H{}" stroke="#b4795a" stroke-width=".8" opacity=".6"/>"##,
                Num(px0),
                Num(py0),
                Num(px1),
                Num(px1 - 2.0 * k),
                Num(py1),
                Num(px0 + 2.0 * k),
                Num(1.3 * k),
                Num(px0 + k),
                Num(py0 + 1.3 * k),
                Num(px1 - k)
            );
            let _ = write!(
                s,
                r##"<g class="chop" transform="translate({} {})scale({})"><rect width="8" height="8" rx=".6" fill="#3f1c12" opacity=".8"/><path d="M2 2h4M2 4h4M4 2v4" stroke="#9c6449" stroke-width=".6"/></g>"##,
                Num(px1 - 13.0 * k),
                Num(py1 - 12.0 * k),
                Num(k)
            );
        }
        // Glossy cobalt: long glassy highlights, a darker foot.
        9 => {
            let _ = write!(
                s,
                r##"<g class="gloss" stroke="#fff" stroke-linecap="round"><path d="M{} {}H{}" stroke-opacity=".28" stroke-width="{}"/><path d="M{} {}H{}" stroke-opacity=".16" stroke-width="{}"/></g><rect x="{}" y="{}" width="{}" height="{}" fill="{}" opacity=".6"/>"##,
                Num(cx - hw * 0.85),
                Num(y0 + (by - y0) * 0.22),
                Num(cx + hw * 0.15),
                Num(2.2 * k),
                Num(cx - hw * 0.7),
                Num(y0 + (by - y0) * 0.32),
                Num(cx - hw * 0.1),
                Num(1.4 * k),
                Num(cx - hw * 1.1),
                Num(by - 9.0 * k),
                Num(hw * 2.2),
                Num(12.0 * k),
                pot.glaze.shift(-0.2, 0.0)
            );
        }
        // Speckled green: dark and light speckles, a raised band, a line
        // above the foot.
        _ => {
            speckle(
                &mut s,
                &mut r,
                pot,
                (y0, by),
                90,
                "#173a2e",
                ".7",
                (0.35, 1.0),
            );
            speckle(
                &mut s,
                &mut r,
                pot,
                (y0, by),
                35,
                "#b9d8c4",
                ".6",
                (0.35, 1.0),
            );
            let band = y0 + (by - y0) * 0.46;
            let strip = |y: f64, h: f64, fill: Rgb| {
                format!(
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{fill}"/>"#,
                    Num(cx - hw * 1.1),
                    Num(y),
                    Num(hw * 2.2),
                    Num(h)
                )
            };
            s.push_str(&strip(band, 3.2 * k, pot.glaze.shift(-0.15, 0.0)));
            s.push_str(&strip(band - 1.4 * k, 1.4 * k, pot.glaze.shift(0.1, 0.0)));
            s.push_str(&strip(by - 7.0 * k, 2.0 * k, pot.glaze.shift(-0.14, 0.0)));
        }
    }
    s.push_str("</g>");
    s
}

/// Draws a rectangle's lip band across its front, lit from the left: the
/// speckled green's flares out and curves back under, the vintage clay's
/// edges are worn lighter.
fn rim_rect(pot: &Pot) -> String {
    let (hw, ry) = (pot.hw, pot.ry);
    let cx = CENTER_X;
    let k = detail(pot);
    let (over, lip_h) = lip_of(pot);
    let lipw = hw + over;
    let (y, y1) = (SOIL_Y + ry, SOIL_Y + ry + lip_h);
    let band = |from: f64, fill: Rgb| {
        let mut p = PathData::new();
        p.m((from, y)).l((cx + lipw, y));
        if pot.level == 10 {
            p.q((cx + lipw, y1), (cx + hw, y1));
        } else {
            p.l((cx + lipw - k, y1));
        }
        if from < cx - hw {
            if pot.level == 10 {
                p.l((cx - hw, y1)).q((cx - lipw, y1), (cx - lipw, y));
            } else {
                p.l((cx - lipw + k, y1));
            }
        } else {
            p.l((from, y1));
        }
        p.z();
        format!(r#"<path d="{}" fill="{fill}"/>"#, p.as_str())
    };
    let mut s = band(cx - lipw, pot.glaze.shift(0.08, 0.0));
    s.push_str(&band(cx + hw * 0.2, pot.glaze));
    let edge = if pot.level == 8 { "#c99476" } else { "#fff" };
    let _ = write!(
        s,
        r#"<path d="M{} {}H{}" stroke="{edge}" stroke-opacity=".35" stroke-width="{}" stroke-linecap="round"/>"#,
        Num(cx - lipw + 5.0 * k),
        Num(y + 1.4 * k),
        Num(cx - lipw * 0.2),
        Num(k)
    );
    s
}

/// Draws a rectangle's top from a little above: a lighter rim frame, the
/// dark inside of the wall at the back, the soil set a little lower.
fn top_rect(pot: &Pot, soil: Rgb) -> String {
    let (hw, ry, rim) = (pot.hw, pot.ry, pot.rim_y());
    let cx = CENTER_X;
    let k = detail(pot);
    let (over, _) = lip_of(pot);
    let lipw = hw + over;
    let tp = RECT_BACK_TAPER * k;
    // A face from its front edge (`l`..`r` at `yf`) to its back edge at
    // `yb`, drawn in by `tp` on each side: a touch of depth.
    let trap = |l: f64, r: f64, yb: f64, yf: f64, fill: &str| {
        format!(
            r#"<path d="M{} {}H{}L{} {}H{}Z"{fill}/>"#,
            Num(l + tp),
            Num(yb),
            Num(r - tp),
            Num(r),
            Num(yf),
            Num(l)
        )
    };
    // The opening inside the rim frame, its back wall a shade darker than
    // the outside; the soil lies [`Pot::inset`] lower and, as the walls
    // lean in toward the foot, a little narrower, so the short side walls
    // show as slanted strips.
    let (l0, r0) = (cx - lipw + 4.0 * k, cx + lipw - 4.0 * k);
    let (b0, f0) = (rim - ry + 2.4 * k, rim + ry - 2.4 * k);
    let drop = pot.inset();
    let lean = ((pot.hw - pot.bw) / pot.depth * drop).max(1.2 * k);
    let (l1, r1) = (l0 + lean, r0 - lean);
    let (b1, f1) = (b0 + drop + 0.6 * k, f0 + drop);
    let strip = |x0: f64, x1: f64, back: f64| {
        format!(
            r#"<path d="M{} {}L{} {}L{} {}L{} {}Z" fill="{}"/>"#,
            Num(x0 + back),
            Num(b0),
            Num(x1 + back),
            Num(b1),
            Num(x1),
            Num(f1),
            Num(x0),
            Num(f0),
            pot.glaze.shift(-0.18, 0.0)
        )
    };
    let mut s = trap(
        cx - lipw,
        cx + lipw,
        rim - ry,
        rim + ry,
        &format!(r#" fill="{}""#, pot.glaze.shift(0.12, 0.0)),
    );
    let opening = trap(l0, r0, b0, f0, "");
    let _ = write!(
        s,
        r#"<clipPath id="po">{opening}</clipPath>{}<g clip-path="url(#po)">{}{}{}</g>"#,
        trap(
            l0,
            r0,
            b0,
            f0,
            &format!(r#" fill="{}""#, pot.glaze.shift(-0.12, 0.0))
        ),
        strip(l0, l1, tp),
        strip(r0, r1, -tp),
        trap(l1, r1, b1, f1, &format!(r#" fill="{soil}""#)),
    );
    s
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
            [4, 13, 26, 39, 52, 78, 104, 156, 207, 259]
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
    /// upright: narrower and deeper, 0.7 as deep as it is wide. The bowl
    /// keeps its one shape instead ([`the_bowl_keeps_its_shape`]).
    #[test]
    fn a_cascade_keeps_the_upright_pots_soil() {
        for level in (0..11u8).filter(|l| model(*l).shape != Shape::Bowl) {
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

    /// On a thick trunk the bowl and the rectangles keep close to their
    /// mockups' proportions instead of turning into cauldrons and crates; a
    /// cascade still goes 0.7 deep.
    #[test]
    fn square_pots_and_the_bowl_stay_shallow_on_big_trunks() {
        for width in [150.0, 200.0, 305.0] {
            for level in 7..11u8 {
                let m = model(level);
                let up = dims(&need(width, 133.0, 0.0, None, 40.0), m);
                let most = if level == 7 { 0.38 } else { 0.27 };
                assert!(
                    up.depth <= most * width + 1e-9,
                    "level {level} at {width}: {} deep",
                    up.depth
                );
            }
        }
        let down = dims(&need(200.0, 133.0, 1.0, None, 40.0), model(8));
        assert!((down.depth / (2.0 * down.hw) - 0.7).abs() < 0.02);
    }

    /// The bowl is one shape at every size: 0.38 as deep as it is wide,
    /// whatever its trunk, a cascade the same bowl as an upright tree, and a
    /// spill close in makes it smaller, not deeper.
    #[test]
    fn the_bowl_keeps_its_shape() {
        let m = model(7);
        for width in [125.0, 195.0, 305.0] {
            for trunk in [20.0, 46.0, 133.0] {
                for cascade in [0.0, 0.5, 1.0] {
                    for spill in [None, Some(40.0)] {
                        let d = dims(&need(width, trunk, cascade, spill, 10.0), m);
                        assert!(
                            (d.depth / (2.0 * d.hw) - BOWL_DEPTH).abs() < 1e-9,
                            "{width} wide, trunk {trunk}, cascade {cascade}, spill {spill:?}: {} deep, {} half-wide",
                            d.depth,
                            d.hw
                        );
                    }
                }
                let free = dims(&need(width, trunk, 1.0, None, 10.0), m);
                assert!(
                    (free.hw - width / 2.0).abs() < 1e-9,
                    "a cascade's bowl is full width"
                );
            }
        }
    }

    /// The wall's half-width down the pot: straight on an oval and a
    /// rectangle, and on a bowl the curve its wall is drawn with, widest a
    /// sixth of the way down.
    #[test]
    fn a_bowls_wall_swells_at_its_belly() {
        let mut pot = Pot {
            hw: 100.0,
            ry: 20.0,
            depth: 60.0,
            bw: 69.0,
            glaze: Rgb::hex(0),
            shape: Shape::Bowl,
            level: 7,
        };
        assert!((pot.half_width(0.0) - 100.0).abs() < 1e-9);
        // The drawn wall's own numbers (hw 100, bw 69).
        for (t, w) in [(1.0 / 6.0, 101.301), (0.5, 97.293), (0.75, 89.306)] {
            assert!(
                (pot.half_width(t) - w).abs() < 0.01,
                "{t}: {}",
                pot.half_width(t)
            );
        }
        assert!((pot.half_width(1.0) - 69.0).abs() < 1e-9);
        pot.shape = Shape::Rect;
        assert!((pot.half_width(0.5) - 84.5).abs() < 1e-9);
    }

    /// Each pot casts its shadow at its own foot: an oval where it always
    /// did; the bowl and a rectangle a tight, soft contact shadow where the
    /// saucer or the feet meet the table, as wide as the foot.
    #[test]
    fn each_pot_casts_a_shadow_at_its_foot() {
        let attr = |s: &str, tag: &str, name: &str| -> f64 {
            let at = s.find(tag).expect(tag);
            let rest = &s[at..];
            let key = format!(" {name}=\"");
            let from = rest.find(&key).expect(name) + key.len();
            rest[from..from + rest[from..].find('"').unwrap_or(0)]
                .parse()
                .unwrap_or(f64::NAN)
        };
        let mut pot = Pot {
            hw: 100.0,
            ry: 14.0,
            depth: 50.0,
            bw: 88.0,
            glaze: Rgb::hex(0),
            shape: Shape::Rect,
            level: 8,
        };
        let s = shadow(&pot, 1.0, false);
        let foot = feet_span(&pot).1 - pot.inset();
        assert!(
            (attr(&s, "<ellipse", "cy") - (foot + 0.6)).abs() < 0.01,
            "{s}"
        );
        assert!(
            (attr(&s, "<ellipse", "rx") - pot.bw * 1.04 * 1.1).abs() < 0.01,
            "{s}"
        );
        pot.shape = Shape::Bowl;
        pot.level = 7;
        let s = shadow(&pot, 1.0, false);
        let saucer = pot.rim_y() + pot.depth + 4.0 * detail(&pot);
        let cy = attr(&s, "<ellipse", "cy");
        assert!(cy > saucer && cy < saucer + pot.ry, "{s}");
        assert!(
            (attr(&s, "<ellipse", "rx") - pot.bw * 1.32 * 1.1).abs() < 0.01,
            "{s}"
        );
        pot.shape = Shape::Oval;
        pot.level = 3;
        let s = shadow(&pot, 1.0, false);
        assert!(
            (attr(&s, "<ellipse", "cy") - (SOIL_Y + 51.2)).abs() < 1e-9,
            "{s}"
        );
    }

    /// The bowl and the rectangles hold their soil below the rim, the bowl
    /// 9% of its depth and a rectangle 3.5%: the pot stands that much
    /// higher, its inner wall shows above the soil, and the soil stays inside
    /// the opening. An oval is filled to the brim.
    #[test]
    fn the_bowl_and_rectangles_hold_their_soil_below_the_rim() {
        let b = crate::Bonsai::grow(
            &crate::fixtures::by_name("steady").unwrap().snapshot,
            &Config::default(),
            None,
        );
        for (shape, level) in [(Shape::Bowl, 7), (Shape::Rect, 8), (Shape::Rect, 10)] {
            let pot = Pot {
                hw: 100.0,
                ry: 17.0,
                depth: 60.0,
                bw: 80.0,
                glaze: Rgb::hex(0x36_2612),
                shape,
                level,
            };
            let inset = if shape == Shape::Bowl { 5.4 } else { 2.1 };
            assert!((pot.inset() - inset).abs() < 1e-9, "{level}");
            assert!((pot.rim_y() - (SOIL_Y - inset)).abs() < 1e-9, "{level}");
            let top = top(&pot, Rgb::hex(0x4a_3a2a));
            assert!(top.contains(r#"clip-path="url(#po)""#), "{level}: {top}");
            assert!(
                front(&b, &pot, false)
                    .starts_with(&format!(r#"<g transform="translate(0 -{inset})">"#)),
                "{level}"
            );
            assert!(pot.rim_front_y(CENTER_X) < SOIL_Y + pot.ry - 1.0, "{level}");
        }
        let oval = Pot {
            hw: 100.0,
            ry: 17.0,
            depth: 60.0,
            bw: 80.0,
            glaze: Rgb::hex(0x3e_5c76),
            shape: Shape::Oval,
            level: 3,
        };
        assert_eq!(oval.inset(), 0.0);
        assert_eq!(oval.rim_y(), SOIL_Y);
        // The bowl reads deepest, a rectangle just below its frame.
        const { assert!(BOWL_INSET > RECT_INSET && RECT_INSET > 0.0) };
    }

    /// A cascade's pot keeps inside the point where its trunk spills over
    /// the rim, a rectangle's lip included, so the hanging wood never runs
    /// behind the rim.
    #[test]
    fn a_cascade_pot_keeps_inside_where_the_trunk_spills() {
        for name in ["veteran", "legacy"] {
            let mut config = Config::default();
            config.tree.style = Some(crate::config::Style::Kengai);
            let mut b = Bonsai::grow(
                &crate::fixtures::by_name(name).unwrap().snapshot,
                &config,
                None,
            );
            let scene = Scene::build(&b);
            let base_r = b.foot_girth();
            let spill = spill_of(&b, &scene, base_r).unwrap_or_else(|| panic!("{name}: no spill"));
            for level in [3, 9, 10] {
                b.extras.pot_tier = level;
                let pot = size(&b, &scene, base_r, Rgb::hex(0));
                let over = if pot.shape == Shape::Rect {
                    lip_of(&pot).0
                } else {
                    0.0
                };
                assert!(
                    pot.hw + over <= spill.max(2.0 * base_r) + 1e-6,
                    "{name} {level}: {} + {over} past {spill}",
                    pot.hw
                );
            }
        }
    }

    /// A rectangle's top face narrows a little toward the back, a touch of
    /// depth: its rim frame's back edge is 3 detail units in on each side.
    #[test]
    fn a_rectangles_top_narrows_toward_the_back() {
        let pot = Pot {
            hw: 100.0,
            ry: 17.0,
            depth: 50.0,
            bw: 88.0,
            glaze: Rgb::hex(0x7a_3f2c),
            shape: Shape::Rect,
            level: 8,
        };
        let s = top(&pot, Rgb::hex(0x4a_3a2a));
        let d = s
            .split(r#"d=""#)
            .nth(1)
            .and_then(|r| r.split('"').next())
            .unwrap();
        let n: Vec<f64> = d
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .filter(|t| !t.is_empty())
            .map(|t| t.parse().unwrap())
            .collect();
        // M back-left y H back-right L front-right y H front-left Z
        let (back, front) = (n[2] - n[0], n[3] - n[5]);
        let k = detail(&pot);
        assert!((front - back - 6.0 * k).abs() < 0.05, "{d}");
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
            let svg = front(&b, &pot, false) + &rim(&pot) + &top(&pot, Rgb::hex(0x5a4632));
            assert!(
                !svg.contains("NaN") && !svg.contains("inf"),
                "level {level}"
            );
        }
    }

    /// The new pots stay cheap, so a heavy crown keeps its leaves under the
    /// SVG budget: under 9 KB each at a 244 pot.
    #[test]
    fn new_pots_stay_cheap() {
        let mut b = crate::Bonsai::grow(
            &crate::fixtures::by_name("steady").unwrap().snapshot,
            &Config::default(),
            None,
        );
        for level in 7..11u8 {
            b.extras.pot_tier = level;
            let m = model(level);
            let d = dims(
                &Need {
                    width: 244.0,
                    trunk_depth: 40.0,
                    cascade: 0.0,
                    spill: None,
                    base_r: 20.0,
                },
                m,
            );
            let pot = Pot {
                hw: d.hw,
                ry: d.ry,
                depth: d.depth,
                bw: d.bw,
                glaze: Rgb::hex(m.glaze),
                shape: m.shape,
                level,
            };
            let bytes = front(&b, &pot, true).len()
                + rim(&pot).len()
                + marks(&b, &pot).len()
                + top(&pot, Rgb::hex(0x5a4632)).len();
            assert!(bytes < 9_000, "level {level}: {bytes} bytes");
        }
    }

    /// The rim's front edge: on an oval it follows the ellipse, highest at
    /// the sides; on a rectangle it runs straight across, at its rim.
    #[test]
    fn the_rims_front_edge_follows_the_shape() {
        let oval = Pot {
            hw: 100.0,
            ry: 20.0,
            depth: 30.0,
            bw: 88.0,
            glaze: Rgb::hex(0),
            shape: Shape::Oval,
            level: 3,
        };
        assert!((oval.rim_front_y(CENTER_X) - (SOIL_Y + 18.8)).abs() < 1e-9);
        assert!((oval.rim_front_y(CENTER_X + 100.0) - SOIL_Y).abs() < 1e-9);
        let rect = Pot {
            shape: Shape::Rect,
            level: 8,
            ..oval
        };
        // Its rim stands above the soil by its inset.
        assert!((rect.rim_front_y(CENTER_X + 80.0) - (SOIL_Y - rect.inset() + 20.0)).abs() < 1e-9);
    }

    /// Only glazed pots take a configured glaze.
    #[test]
    fn unglazed_pots_keep_their_clay() {
        let glazed: Vec<u8> = (0..11).filter(|&l| model(l).glazed).collect();
        assert_eq!(glazed, vec![3, 4, 5, 6, 7, 9, 10]);
    }
}
