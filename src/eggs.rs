//! Achievement easter eggs and kintsugi.
//!
//! Each GitHub achievement adds one small, fitting object; the object grows
//! with the tier (x1 to x4). The common one-click achievements (Quickdraw,
//! YOLO, Heart On Your Sleeve, Open Sourcerer) are left out, so the tree stays
//! calm. Every egg has a fixed spot, so they never collide:
//!
//! | Achievement | x1 | x2 | x3 | x4 |
//! |---|---|---|---|---|
//! | Pull Shark | small stone fish | goldfish pebble | stone tuna | stone shark |
//! | Starstruck | paper star | clear-glass furin | blue furin painted with stars | golden furin, strip swaying |
//! | Pair Extraordinaire | (nothing) | two pebbles | yin-yang pebbles | red-crowned cranes |
//! | Galaxy Brain | glowworm | firefly | firefly + crescent moon | firefly + full moon |
//! | Public Sponsor | a 5-yen coin (go-en, "good connection") in the pot | | | |
//! | Arctic Code Vault | snow on the rim | | | |
//! | Mars 2020 | a taketombo (bamboo copter) on the soil | | | |
//!
//! Kintsugi: every archived repository is a break in the pot, mended with gold.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "positions on a 512-unit canvas"
)]

use std::fmt::Write as _;

use crate::Bonsai;
use crate::color::Rgb;
use crate::config::Legend;
use crate::growth::GOLDEN;
use crate::math;
use crate::noise::{fbm, hash01, noise1};
use crate::paint::{Frame, Paints, Pot, norm};
use crate::render::{Num, PathData};
use crate::rng::{Rng, siphash13};
use crate::scene::{Scene, WORLD, project};
use crate::style::{CENTER_X, SOIL_Y};

/// At most this many archived repositories become kintsugi breaks; about half
/// of them face the viewer.
pub const MAX_SEAMS: usize = 19;

/// Achievements drawn as easter eggs; the rest are too common to show.
pub const DRAWN: [&str; 7] = [
    "pull-shark",
    "starstruck",
    "pair-extraordinaire",
    "galaxy-brain",
    "public-sponsor",
    "arctic-code-vault-contributor",
    "mars-2020-contributor",
];

/// Egg markup by layer.
#[derive(Debug, Clone, Default)]
pub struct Eggs {
    /// Behind the pot's back rim.
    pub behind: String,
    /// On the soil, behind the tree.
    pub back: String,
    /// In front of the tree.
    pub front: String,
    /// On top of everything: rim and sky.
    pub top: String,
    /// Animated elements.
    pub animated: usize,
}

/// Branch points in the upper tree, least crowded by leaves first.
fn branch_spots(scene: &Scene) -> Vec<(usize, f64, f64)> {
    let leaves: Vec<(f64, f64)> = scene
        .tufts
        .iter()
        .map(|t| {
            let (x, y, _) = project(t.p);
            (x, y)
        })
        .collect();
    let reach = 14.0 * WORLD;
    let (mut upper, mut lower) = (Vec::new(), Vec::new());
    for (chain, _) in scene.chains.iter().skip(1) {
        if chain.len() < 4 {
            continue;
        }
        for (k, (p, _)) in chain.iter().enumerate() {
            let f = k as f64 / chain.len() as f64;
            if !(0.35..=0.85).contains(&f) || k % 2 == 1 {
                continue;
            }
            let (x, y, _) = project(*p);
            if !(50.0..=462.0).contains(&x) || y > SOIL_Y - 40.0 || y < 50.0 {
                continue;
            }
            let crowd = leaves
                .iter()
                .filter(|l| (l.0 - x).powi(2) + (l.1 - y).powi(2) < reach * reach)
                .count();
            if y < SOIL_Y - 90.0 {
                upper.push((crowd, x, y));
            } else {
                lower.push((crowd, x, y));
            }
        }
    }
    upper.sort_by(|a, c| {
        a.0.cmp(&c.0)
            .then(a.2.total_cmp(&c.2))
            .then(a.1.total_cmp(&c.1))
    });
    lower.sort_by(|a, c| {
        a.0.cmp(&c.0)
            .then(a.2.total_cmp(&c.2))
            .then(a.1.total_cmp(&c.1))
    });
    upper.extend(lower);
    upper
}

/// The first spot at least `gap` from every spot already taken.
fn take_spot(
    spots: &[(usize, f64, f64)],
    taken: &mut Vec<(f64, f64)>,
    gap: f64,
) -> Option<(f64, f64)> {
    let found = spots
        .iter()
        .find(|s| {
            taken
                .iter()
                .all(|t| (t.0 - s.1).powi(2) + (t.1 - s.2).powi(2) > gap * gap)
        })
        .map(|s| (s.1, s.2));
    if let Some(p) = found {
        taken.push(p);
    }
    found
}

/// The easter eggs of `b`'s achievements.
/// Where the moon hangs: the clearer of the frame's top corners. When both
/// are crowded, the frame grows upward until the moon's disc stands clear.
/// `k` is the moon's size relative to a full-grown frame. With a legend, the
/// corner away from it wins whenever it is clear.
fn moon_spot(scene: &Scene, frame: &mut Frame, k: f64, legend: Legend) -> (f64, f64) {
    let inset = 34.0 * k;
    let clear = 30.0 * k;
    let mut pts: Vec<(f64, f64)> = scene
        .tufts
        .iter()
        .map(|t| {
            let (x, y, _) = project(t.p);
            (x, y)
        })
        .collect();
    for (chain, _) in &scene.chains {
        pts.extend(chain.iter().map(|(p, _)| {
            let (x, y, _) = project(*p);
            (x, y)
        }));
    }
    let clearance = |c: (f64, f64)| {
        pts.iter()
            .map(|p| ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt())
            .fold(f64::INFINITY, f64::min)
    };
    let top = frame.1 + inset;
    let left = (frame.0 + inset, top);
    let right = (frame.0 + frame.2 - inset, top);
    let away = match legend {
        Legend::Right => Some(left),
        Legend::Left => Some(right),
        Legend::Off => None,
    };
    let (spot, room) = match away {
        Some(c) if clearance(c) >= clear => (c, clearance(c)),
        _ if clearance(left) > clearance(right) => (left, clearance(left)),
        _ => (right, clearance(right)),
    };
    if room >= clear {
        return spot;
    }
    let lift = ((clear - room) / 8.0).ceil() * 8.0;
    frame.1 -= lift;
    frame.3 += lift;
    (spot.0, spot.1 - lift)
}

/// The easter eggs of `b`'s achievements. The moon may grow `frame` upward,
/// and keeps clear of the legend's side.
pub fn eggs(
    b: &Bonsai,
    scene: &Scene,
    paints: &Paints,
    pot: &Pot,
    frame: &mut Frame,
    legend: Legend,
) -> Eggs {
    let mut e = Eggs::default();
    let tier = |slug: &str| b.achievements.get(slug).copied();
    if !DRAWN.iter().any(|slug| tier(slug).is_some()) {
        return e;
    }
    let (hw, ry) = (pot.hw, pot.ry);
    let w = WORLD;
    let dark = paints.dark;
    let shadow = paints.soil.shift(-0.15, 0.0);
    let spots = branch_spots(scene);
    let mut taken: Vec<(f64, f64)> = Vec::new();

    // ---- back soil ----
    if tier("galaxy-brain") == Some(1) {
        let _ = write!(
            e.back,
            r##"<g transform="translate({} {}) scale({})"><use href="#glowworm"/></g>"##,
            Num(CENTER_X - hw * 0.45),
            Num(SOIL_Y + ry * 0.15),
            Num(w * 1.1)
        );
        e.animated += 1;
    }
    if tier("mars-2020-contributor").is_some() {
        let _ = write!(
            e.back,
            r##"<use href="#taketombo" transform="translate({} {}) scale({}) rotate(-12)"/>"##,
            Num(CENTER_X + hw * 0.22),
            Num(SOIL_Y - ry * 0.3),
            Num(w * 1.5)
        );
    }
    if tier("arctic-code-vault-contributor").is_some() {
        // Snow along the back of the rim, before the tree is drawn over it.
        let mut snow = PathData::new();
        snow.m((CENTER_X - hw + 2.0, SOIL_Y - 1.0)).a(
            hw - 2.0,
            ry - 1.0,
            true,
            (CENTER_X + hw - 2.0, SOIL_Y - 1.0),
        );
        let _ = write!(
            e.back,
            r##"<path d="{}" fill="none" stroke="#f4f7fb" stroke-width="2.6" stroke-dasharray="14 5 22 7 9 4" stroke-linecap="round"/>"##,
            snow.as_str()
        );
    }

    // ---- front soil ----
    if let Some(t) = tier("pull-shark") {
        let (symbol, half) = match t {
            1 => ("fish1", 5.5),
            2 => ("fish2", 6.5),
            3 => ("fish3", 9.0),
            _ => ("fish4", 12.0),
        };
        // The goldfish is a pebble painted orange-gold; the others stay plain stone.
        let paint = if t == 2 {
            if dark {
                Rgb::hex(0xe59a45)
            } else {
                Rgb::hex(0xd9822f)
            }
        } else {
            paints.stone
        };
        let _ = write!(
            e.front,
            r##"<g transform="translate({} {}) scale({})"><ellipse cy="1.4" rx="{}" ry="1.6" fill="{shadow}" opacity=".45"/><use href="#{symbol}" fill="{paint}" stroke="{}" stroke-width=".3"/><path d="M-2-2.2Q.4-3.2 2.6-2.2" fill="none" stroke="#fff" stroke-width=".55" opacity=".45"/><g fill="{}" opacity=".3"><circle cx="-2" cy="-.6" r=".35"/><circle cx="1" cy="-1.4" r=".28"/><circle cx="-.4" cy=".4" r=".3"/></g></g>"##,
            Num(CENTER_X + hw * 0.5),
            Num(SOIL_Y + ry * 0.35),
            Num(w * 1.3),
            Num(half),
            paint.shift(-0.18, 0.0),
            paint.shift(-0.2, 0.0)
        );
    }
    if let Some(t) = tier("pair-extraordinaire")
        && t >= 2
    {
        let symbol = match t {
            2 => "pebbles",
            3 => "yinyang",
            _ => "cranes",
        };
        let _ = write!(
            e.front,
            r##"<g transform="translate({} {}) scale({})"><ellipse cy=".6" rx="8" ry="1.4" fill="{shadow}" opacity=".45"/><use href="#{symbol}"/></g>"##,
            Num(CENTER_X - hw * 0.3),
            Num(SOIL_Y + ry * 0.45),
            Num(w * 1.5)
        );
    }
    if tier("public-sponsor").is_some() {
        let _ = write!(
            e.front,
            r##"<g transform="translate({} {}) scale({})"><ellipse cy=".7" rx="4.4" ry="1.5" fill="{shadow}" opacity=".45"/><use href="#goen"/></g>"##,
            Num(CENTER_X + hw * 0.16),
            Num(SOIL_Y + ry * 0.62),
            Num(w * 1.5)
        );
    }

    // ---- branches ----
    if let Some(t) = tier("starstruck")
        && let Some((x, y)) = take_spot(&spots, &mut taken, 30.0)
    {
        let scale = if t == 1 { 1.25 } else { 1.5 };
        let mut g = format!(
            r#"<g transform="translate({} {}) scale({})">"#,
            Num(x),
            Num(y),
            Num(w * scale)
        );
        if t == 1 {
            let star = if dark { "#f6efd2" } else { "#efe3b8" };
            let _ = write!(
                g,
                r##"<path d="M0 0V7" stroke="#8a7a5a" stroke-width=".35"/><use href="#star5" y="10.6" fill="{star}" stroke="#b39a5a" stroke-width=".35"/><circle class="tw" cx="1.2" cy="9" r=".7" fill="#fff"/>"##
            );
            e.animated += 1;
        } else {
            let outline = if dark { "#cfd8e0" } else { "#8fa3b3" };
            let (glass, opacity, edge, strip) = match t {
                2 => (
                    if dark { "#dfe9f2" } else { "#eef5fb" },
                    ".35",
                    outline,
                    "#f4f1e6",
                ),
                3 => ("#2f4f93", ".82", "#22396b", "#cfe3f5"),
                _ => ("#e0b34a", "1", "#a97f2a", "#c0392b"),
            };
            let _ = write!(
                g,
                r##"<path d="M0 0V3.2" stroke="#8a7a5a" stroke-width=".35"/><path d="M-4.2 9.4Q-4.4 3.4 0 3.2Q4.4 3.4 4.2 9.4Z" fill="{glass}" fill-opacity="{opacity}" stroke="{edge}" stroke-width=".4"/><path d="M-4.4 9.4h8.8" stroke="{edge}" stroke-width=".45"/><path d="M-2.4 4.8Q-3.2 6.4-3 8.2" fill="none" stroke="#fff" stroke-width=".45" opacity=".7"/>"##
            );
            if t == 3 {
                // Stars painted on the blue glass.
                g.push_str(r##"<g fill="#fff3c4"><use href="#star5" transform="translate(1.4 6) scale(.32)"/><use href="#star5" transform="translate(-.6 7.8) scale(.24)"/><use href="#star5" transform="translate(2.6 8.2) scale(.2)"/><circle cx="-1.6" cy="5.6" r=".22"/><circle cx="3" cy="5.2" r=".18"/></g>"##);
            }
            // The clapper and the paper strip that catches the wind.
            let sway = if t >= 4 { r#" class="fs""# } else { "" };
            e.animated += usize::from(t >= 4);
            let _ = write!(
                g,
                r##"<g{sway}><path d="M0 4.2V12" stroke="#8a7a5a" stroke-width=".3"/><circle cy="8.6" r=".7" fill="{edge}"/><rect x="-1.2" y="12" width="2.4" height="7.4" fill="{strip}" stroke="#00000022" stroke-width=".2"/></g>"##
            );
        }
        g.push_str("</g>");
        e.front.push_str(&g);
    }
    if let Some(t) = tier("galaxy-brain") {
        // The firefly stays from x2 on; the moon joins it at x3 (crescent) and x4 (full).
        if t >= 2
            && let Some((x, y)) = take_spot(&spots, &mut taken, 30.0)
        {
            let _ = write!(
                e.front,
                r##"<g class="ff"><g transform="translate({} {}) scale({})"><use href="#firefly"/></g></g>"##,
                Num(x + 18.0),
                Num(y - 16.0),
                Num(w)
            );
            e.animated += 2;
        }
        if t >= 3 {
            // Sized to the frame, so it never looms over a young tree.
            let k = (frame.2 / 480.0).clamp(0.5, 1.0);
            let at = moon_spot(scene, frame, k, legend);
            // Golden with a halo on light backgrounds, pale on dark: visible on both.
            let (body, halo, crater) = if dark {
                ("#f3e3a3", "#f6e9b2", "#d9c47e")
            } else {
                ("#e9b949", "#f4d27a", "#c9962e")
            };
            let glow = if dark { ".07" } else { ".16" };
            let moon = if t == 3 {
                format!(
                    r#"<circle r="14" fill="{halo}" opacity="{glow}"/><path d="M0-9A9 9 0 0 0 0 9A11.5 11.5 0 0 1 0-9Z" fill="{body}" transform="rotate(-25)"/>"#
                )
            } else {
                format!(
                    r#"<circle r="16" fill="{halo}" opacity="{glow}"/><circle r="9" fill="{body}"/><circle cx="-2.6" cy="-2" r="1.8" fill="{crater}" opacity=".6"/><circle cx="3" cy="1.4" r="1.3" fill="{crater}" opacity=".6"/><circle cx="-.6" cy="4" r="1" fill="{crater}" opacity=".5"/>"#
                )
            };
            let _ = write!(
                e.top,
                r#"<g transform="translate({} {}) scale({})">{moon}</g>"#,
                Num(at.0),
                Num(at.1),
                Num(w * 1.5 * k)
            );
        }
    }

    // ---- rim ----
    if tier("arctic-code-vault-contributor").is_some() {
        let mut snow = PathData::new();
        snow.m((CENTER_X - hw + 1.0, SOIL_Y))
            .a(hw - 1.0, ry, false, (CENTER_X + hw - 1.0, SOIL_Y));
        let _ = write!(
            e.top,
            r##"<path d="{}" fill="none" stroke="#f4f7fb" stroke-width="2.8" stroke-dasharray="18 6 10 4 26 8" stroke-linecap="round"/>"##,
            snow.as_str()
        );
    }
    e
}

/// Kintsugi: every archived repository is a break in the pot, mended with
/// gold. A break starts at the rim and runs down past the foot (clipped by the
/// pot, so it continues underneath), meandering on two scales with a width
/// that swells and thins like poured lacquer. Breaks sit at golden-angle steps
/// around the pot in archive order, so archiving another repository never
/// moves an old one; only those on the front half are visible.
pub fn kintsugi(archived: &[String], seed: u64, pot: &Pot) -> String {
    let Pot {
        hw,
        ry,
        depth,
        bw,
        glaze,
    } = *pot;
    let at = |a: f64, t: f64| {
        let rad = hw + (bw - hw) * t;
        (
            CENTER_X + rad * math::cos(a),
            SOIL_Y + depth * t + ry * math::sin(a),
        )
    };
    let base = hash01(seed, 0x6b69) * 360.0;
    let mut veins = String::new();
    for (i, name) in archived.iter().take(MAX_SEAMS).enumerate() {
        let theta = math::rad(base + i as f64 * GOLDEN);
        if math::sin(theta) < 0.2 || math::cos(theta).abs() > 0.9 {
            continue;
        }
        let key = siphash13(name.as_bytes());
        let slant = Rng::keyed(seed ^ key, "seam").range(-0.16, 0.16);
        let mut jag = Rng::keyed(seed ^ key, "jag");
        let steps = 36;
        let pts: Vec<(f64, f64, f64)> = (0..=steps)
            .map(|k| {
                let f = f64::from(k) / f64::from(steps);
                let t = -0.04 + 1.29 * f;
                let a = theta + slant * f + 0.09 * fbm(key ^ 1, f * 2.4);
                let (x, y) = at(a, t);
                let j = if k == 0 { 0.0 } else { jag.range(-0.75, 0.75) };
                let width = 0.7 + 1.5 * (0.5 + 0.5 * noise1(key ^ 0x76, f * 5.0));
                (x + j, y, width)
            })
            .collect();
        let n = pts.len();
        let side = |i: usize, sign: f64, extra: f64| {
            let (pb, nb) = (pts[i.saturating_sub(1)], pts[(i + 1).min(n - 1)]);
            let t = norm((nb.0 - pb.0, nb.1 - pb.1));
            let w = pts[i].2 / 2.0 + extra;
            (pts[i].0 - t.1 * w * sign, pts[i].1 + t.0 * w * sign)
        };
        let outline = |extra: f64| {
            let mut d = PathData::coarse();
            d.m(side(0, 1.0, extra));
            for i in 1..n {
                d.l(side(i, 1.0, extra));
            }
            for i in (0..n).rev() {
                d.l(side(i, -1.0, extra));
            }
            d.z();
            d
        };
        let mut highlight = PathData::coarse();
        for i in 0..n {
            let (pb, nb) = (pts[i.saturating_sub(1)], pts[(i + 1).min(n - 1)]);
            let t = norm((nb.0 - pb.0, nb.1 - pb.1));
            let q = (
                pts[i].0 + t.1 * pts[i].2 * 0.2,
                pts[i].1 - t.0 * pts[i].2 * 0.2,
            );
            if i == 0 {
                highlight.m(q)
            } else {
                highlight.l(q)
            };
        }
        let _ = write!(
            veins,
            r##"<path d="{}" fill="{}" opacity=".5"/><path d="{}" fill="#c9972f"/><path d="{}" fill="none" stroke="#f6dc8a" stroke-width=".35" opacity=".85"/>"##,
            outline(0.55).as_str(),
            glaze.shift(-0.16, 0.0),
            outline(0.0).as_str(),
            highlight.as_str()
        );
    }
    if veins.is_empty() {
        return String::new();
    }
    let mut wall = PathData::new();
    wall.m((CENTER_X - hw, SOIL_Y))
        .a(hw, ry, false, (CENTER_X + hw, SOIL_Y))
        .l((CENTER_X + bw, SOIL_Y + depth))
        .a(bw, ry, true, (CENTER_X - bw, SOIL_Y + depth))
        .z();
    format!(
        r#"<clipPath id="pc"><path d="{}"/></clipPath><g clip-path="url(#pc)">{veins}</g>"#,
        wall.as_str()
    )
}
