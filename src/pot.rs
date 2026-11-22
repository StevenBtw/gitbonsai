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
}

/// Sizes the pot for `b`: its width from the tree's pot size, its depth from
/// the trunk it was repotted for, deeper and narrower for a cascade, with the
/// rim inside the point where the trunk spills over. `base_r` is the trunk's
/// radius at its foot on the canvas.
pub fn size(b: &Bonsai, scene: &Scene, base_r: f64, glaze: Rgb) -> Pot {
    // Repotted in steps as the tree grows ([`Bonsai::grow`]).
    let size = b.pot;
    let hw = size / 2.0;
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
    Pot {
        hw,
        ry: hw * math::sin(TILT) * 0.55,
        depth,
        // Straight-sided for a cascade.
        bw: hw * (0.88 + 0.09 * k),
        glaze,
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

/// The pot's front wall, lit from the left, with its tier's details: a left
/// tree's is mirrored.
pub fn front(b: &Bonsai, pot: &Pot) -> String {
    let Pot {
        hw,
        ry,
        depth,
        bw,
        glaze,
    } = *pot;
    let tier = look(b.extras.pot_tier);
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

    /// Only glazed pots take a configured glaze.
    #[test]
    fn unglazed_pots_keep_their_clay() {
        let glazed: Vec<u8> = (0..11).filter(|&l| model(l).glazed).collect();
        assert_eq!(glazed, vec![3, 4, 5, 6, 7, 9, 10]);
    }
}
