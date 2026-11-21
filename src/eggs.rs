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
//! | Pair Extraordinaire (spring) | (nothing) | two pebbles | yin-yang pebbles | red-crowned cranes |
//! | Galaxy Brain | glowworm (summer) | firefly (summer) | firefly + a galaxy begonia leaf behind the trunk | firefly + a night sky petunia behind the trunk |
//! | Public Sponsor | a 5-yen coin (go-en, "good connection") in the pot | | | |
//! | Arctic Code Vault | a frosted seed on the soil (winter) | | | |
//! | Mars 2020 | a fallen samara, the maple's helicopter seed, on the ground (autumn) | | | |
//!
//! Seasons come from the observed day, in whole months, and the tree's
//! hemisphere. Eggs for free or common badges come and go with them; eggs
//! earned through work stay all year.
//!
//! One egg comes from the work itself: 3/7 of the window's contributions on
//! the weekend fold a red paper kabuto (samurai helmet) on the soil, and 4/7
//! gives it a gold crest.
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
use crate::config::Hemisphere;
use crate::growth::GOLDEN;
use crate::math;
use crate::noise::{fbm, hash01, noise1};
use crate::paint::{Frame, Paints, Pot, norm};
use crate::render::{Num, PathData};
use crate::rng::{Rng, siphash13};
use crate::scene::{Scene, TILT, WORLD, project};
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
            let mut crowd = leaves
                .iter()
                .filter(|l| math::sq(l.0 - x) + math::sq(l.1 - y) < reach * reach)
                .count();
            // Nothing hangs over or in front of a cloud, so small accent
            // clouds stay in view.
            let (s, c) = (math::sin(TILT), math::cos(TILT));
            for (c3, rx, ry, rz, _, _) in &scene.masses {
                let (cx, cy, _) = project(*c3);
                let hw = rx * WORLD + 10.0;
                let up = ((ry * c) * (ry * c) + (rz * s) * (rz * s)).sqrt() * WORLD + 8.0;
                if (x - cx).abs() < hw && y > cy - up - 34.0 && y < cy + rz * s * WORLD + 6.0 {
                    crowd += COVERED;
                }
            }
            if y < SOIL_Y - 90.0 {
                upper.push((crowd, x, y));
            } else {
                lower.push((crowd, x, y));
            }
        }
    }
    rank(upper, lower)
}

/// What a cloud adds to a spot's crowding: more than any count of leaves.
const COVERED: usize = 1000;

/// Spots `(crowd, x, y)` in the order eggs take them: every clear spot
/// before any under a cloud, upper before lower, then least crowded, highest
/// and leftmost first.
fn rank(upper: Vec<(usize, f64, f64)>, lower: Vec<(usize, f64, f64)>) -> Vec<(usize, f64, f64)> {
    let mut all: Vec<(bool, bool, (usize, f64, f64))> = upper
        .into_iter()
        .map(|s| (s.0 >= COVERED, false, s))
        .chain(lower.into_iter().map(|s| (s.0 >= COVERED, true, s)))
        .collect();
    all.sort_by(|a, c| {
        (a.0, a.1, a.2.0)
            .cmp(&(c.0, c.1, c.2.0))
            .then(a.2.2.total_cmp(&c.2.2))
            .then(a.2.1.total_cmp(&c.2.1))
    });
    all.into_iter().map(|(_, _, s)| s).collect()
}

/// A meteorological season: whole months.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Season {
    Winter,
    Spring,
    Summer,
    Autumn,
}

/// The season on the day the tree shows, in its hemisphere: December to
/// February is winter in the north, June to August in the south.
fn season(b: &Bonsai) -> Season {
    let (_, m, _) = b.observed_on.ymd();
    let m = if b.hemisphere == Hemisphere::South {
        (m + 5) % 12 + 1
    } else {
        m
    };
    match m {
        12 | 1 | 2 => Season::Winter,
        3..=5 => Season::Spring,
        6..=8 => Season::Summer,
        _ => Season::Autumn,
    }
}

/// Where a fallen samara lies, its seed at the returned point and its wing
/// to the right: on the ground under the lowest leaves overhanging the pot,
/// clear of its foot and inside the frame; without any, beside the left foot.
/// `k` is its size against a full-size pot's.
fn samara_spot(scene: &Scene, pot: &Pot, frame: &Frame, k: f64) -> (f64, f64) {
    let (len, gap) = (16.0 * WORLD * k, 2.0 * WORLD);
    let over = scene
        .tufts
        .iter()
        .map(|t| project(t.p))
        .filter(|(x, ..)| (x - CENTER_X).abs() > pot.hw)
        .max_by(|a, b| a.1.total_cmp(&b.1));
    let x = match over {
        Some((x, ..)) if x > CENTER_X => x.max(CENTER_X + pot.bw + gap),
        Some((x, ..)) => x.min(CENTER_X - pot.bw - len - gap),
        None => CENTER_X - pot.bw - len - gap,
    };
    let x = x.clamp(
        frame.0 + gap,
        (frame.0 + frame.2 - len - gap).max(frame.0 + gap),
    );
    (x, SOIL_Y + pot.depth - 0.5)
}

/// Where the firefly hovers: in open air close to the leaves, never inside
/// a cloud, preferably in the shade just under the crown. Its own drift
/// stays clear of the leaves.
fn firefly_spot(scene: &Scene, taken: &mut Vec<(f64, f64)>) -> Option<(f64, f64)> {
    let leaves: Vec<(f64, f64)> = scene
        .tufts
        .iter()
        .map(|t| {
            let (x, y, _) = project(t.p);
            (x, y)
        })
        .collect();
    if leaves.is_empty() {
        return None;
    }
    let (s, c) = (math::sin(TILT), math::cos(TILT));
    let clouds: Vec<(f64, f64, f64, f64, f64)> = scene
        .masses
        .iter()
        .map(|(c3, rx, ry, rz, _, _)| {
            let (cx, cy, _) = project(*c3);
            let up = ((ry * c) * (ry * c) + (rz * s) * (rz * s)).sqrt() * WORLD;
            (cx, cy, rx * WORLD, up, rz * s * WORLD)
        })
        .collect();
    let in_cloud = |x: f64, y: f64| {
        clouds.iter().any(|&(cx, cy, hw, up, down)| {
            let v = if y < cy { up } else { down } + 9.0;
            let dx = (x - cx) / (hw + 9.0);
            let dy = (y - cy) / v;
            dx * dx + dy * dy < 1.0
        })
    };
    // The crown's underside: the lowest cloud bottoms, on average.
    let under = clouds.iter().map(|q| q.1 + q.4).sum::<f64>() / clouds.len().max(1) as f64;
    let (x0, x1) = leaves
        .iter()
        .fold((f64::MAX, f64::MIN), |a, l| (a.0.min(l.0), a.1.max(l.0)));
    let y0 = leaves.iter().map(|l| l.1).fold(f64::MAX, f64::min);
    let mut best: Option<(f64, f64, f64)> = None;
    let mut y = y0;
    while y <= SOIL_Y - 34.0 {
        let mut x = x0;
        while x <= x1 {
            let clear = leaves
                .iter()
                .map(|l| ((l.0 - x) * (l.0 - x) + (l.1 - y) * (l.1 - y)).sqrt())
                .fold(f64::MAX, f64::min);
            let free = taken
                .iter()
                .all(|t| (t.0 - x) * (t.0 - x) + (t.1 - y) * (t.1 - y) > 900.0);
            if (16.0..=40.0).contains(&clear) && free && !in_cloud(x, y) {
                let score = (clear - 20.0).abs()
                    + 0.08 * (y - under - 18.0).abs()
                    + 0.02 * (x - CENTER_X).abs();
                if best.is_none_or(|b| score < b.0) {
                    best = Some((score, x, y));
                }
            }
            x += 6.0;
        }
        y += 6.0;
    }
    let (_, x, y) = best?;
    taken.push((x, y));
    Some((x, y))
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
                .all(|t| math::sq(t.0 - s.1) + math::sq(t.1 - s.2) > gap * gap)
        })
        .map(|s| (s.1, s.2));
    if let Some(p) = found {
        taken.push(p);
    }
    found
}

/// The pot half-width at which the soil eggs are drawn full size; on a
/// smaller pot they shrink with it.
const POT_FULL: f64 = 110.0;

/// Places a soil egg of radius `r` at `at` if it is clear of the eggs in
/// `soil`, else at the nearest clear spot sideways along the soil, within the
/// pot, and takes that spot. With no clear spot left it gets none: the egg
/// is left out rather than laid on another.
fn settle(soil: &mut Vec<(f64, f64, f64)>, at: (f64, f64), r: f64, hw: f64) -> Option<(f64, f64)> {
    let clear = |x: f64| {
        soil.iter()
            .all(|&(a, b, q)| math::sq(a - x) + math::sq(b - at.1) >= math::sq(r + q))
    };
    let room = (0.85 * hw - r).max(0.0);
    let step = (0.5 * r).max(1.0);
    let x = (0..60)
        .map(|i| {
            let d = step * f64::from((i + 1) / 2);
            if i % 2 == 1 { at.0 + d } else { at.0 - d }
        })
        .filter(|x| (x - CENTER_X).abs() <= room)
        .find(|&x| clear(x))?;
    soil.push((x, at.1, r));
    Some((x, at.1))
}

/// How high above the soil a galaxy plant's stem meets the trunk's side.
const STEM_FOOT: f64 = 6.0;

/// The trunks' lit edge on the canvas, `h` units above the soil: the
/// outermost left side, where a right tree's light falls, of the trunks as
/// drawn (`trunk`: pieces of outline, points with their radius), between
/// points where a piece crosses that height. `None` where none does.
fn trunk_edge(trunk: &[Vec<(f64, f64, f64)>], h: f64) -> Option<f64> {
    let y = SOIL_Y - h;
    trunk
        .iter()
        .flat_map(|piece| piece.windows(2))
        .filter(|s| (s[0].1 - y) * (s[1].1 - y) <= 0.0 && s[0].1 != s[1].1)
        .map(|s| {
            let t = (y - s[0].1) / (s[1].1 - s[0].1);
            let at = |a: f64, b: f64| a + (b - a) * t;
            at(s[0].0, s[1].0) - at(s[0].2, s[1].2)
        })
        .reduce(f64::min)
}

/// The girth at which a galaxy plant is drawn at full size: a trunk this
/// thick (drawn radius on the canvas) or thicker.
const PLANT_GIRTH: f64 = 12.0;

/// The thickest a trunk is drawn, as a radius on the canvas, `h` units above
/// the soil; 0 where none passes that height.
fn trunk_girth(trunk: &[Vec<(f64, f64, f64)>], h: f64) -> f64 {
    let y = SOIL_Y - h;
    trunk
        .iter()
        .flat_map(|piece| piece.windows(2))
        .filter(|s| (s[0].1 - y) * (s[1].1 - y) <= 0.0 && s[0].1 != s[1].1)
        .map(|s| s[0].2 + (s[1].2 - s[0].2) * (y - s[0].1) / (s[1].1 - s[0].1))
        .fold(0.0, f64::max)
}

/// Whether wood or a root drawn in front covers canvas point `p`: within a
/// piece of wood's radius of its line, or inside a root's outline.
fn covered(p: (f64, f64), wood: &[Vec<(f64, f64, f64)>], roots: &[Vec<(f64, f64)>]) -> bool {
    let in_wood = wood.iter().flat_map(|w| w.windows(2)).any(|s| {
        let (a, b) = (s[0], s[1]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let t =
            (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy).max(1e-9)).clamp(0.0, 1.0);
        let r = a.2 + (b.2 - a.2) * t;
        math::sq(p.0 - a.0 - t * dx) + math::sq(p.1 - a.1 - t * dy) < r * r
    });
    in_wood
        || roots.iter().any(|h| {
            let mut c = false;
            for (i, a) in h.iter().enumerate() {
                let b = h[(i + 1) % h.len()];
                if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0
                {
                    c = !c;
                }
            }
            c
        })
}

/// The easter eggs of `b`'s achievements, as on a right tree. `trunk` is the
/// trunks' drawn outline, in pieces of points with their radius, `wood` all
/// thick wood's the same way, `twigs` the thin wood's, and `roots` the
/// roots' outlines.
pub fn eggs(
    b: &Bonsai,
    scene: &Scene,
    paints: &Paints,
    pot: &Pot,
    frame: &Frame,
    (trunk, wood, twigs, roots): (
        &[Vec<(f64, f64, f64)>],
        &[Vec<(f64, f64, f64)>],
        &[Vec<(f64, f64, f64)>],
        &[Vec<(f64, f64)>],
    ),
) -> Eggs {
    let mut e = Eggs::default();
    let tier = |slug: &str| b.achievements.get(slug).copied();
    if !DRAWN.iter().any(|slug| tier(slug).is_some()) && b.extras.weekend == 0 {
        return e;
    }
    let (hw, ry) = (pot.hw, pot.ry);
    let w = WORLD;
    let season = season(b);
    let dark = paints.dark;
    let shadow = paints.soil.shift(-0.15, 0.0);
    let spots = branch_spots(scene);
    let mut taken: Vec<(f64, f64)> = Vec::new();

    // The soil eggs keep to the pot's size, and each to a spot of its own:
    // on a small pot they shrink, and one that would touch another moves
    // along the soil.
    let k = (hw / POT_FULL).clamp(0.55, 1.0);
    let mut soil: Vec<(f64, f64, f64)> = Vec::new();

    // ---- back soil ----
    if tier("galaxy-brain") == Some(1) && season == Season::Summer {
        let at = (CENTER_X - hw * 0.45, SOIL_Y + ry * 0.15);
        if let Some((x, y)) = settle(&mut soil, at, 4.0 * w * 1.1 * k, hw) {
            let _ = write!(
                e.back,
                r##"<g transform="translate({} {}) scale({})"><use href="#glowworm"/></g>"##,
                Num(x),
                Num(y),
                Num(w * 1.1 * k)
            );
            e.animated += 1;
        }
    }
    if tier("mars-2020-contributor").is_some() && season == Season::Autumn {
        // A fallen samara, the maple's own helicopter seed, in the leaf color
        // of the main trunk's language; a faint light edge keeps its shape on
        // a dark page.
        let main = crate::growth::life::Life::of(&b.timeline, &b.periods)
            .and_then(|life| crate::growth::trunks::trunks(&life).into_iter().next())
            .and_then(|t| t.lang);
        let fill = main
            .as_deref()
            .map_or(b.maple.private(), |l| b.maple.leaf(Some(l), 0.5));
        let (x, y) = samara_spot(scene, pot, frame, k);
        let _ = write!(
            e.behind,
            r##"<g transform="translate({} {}) scale({})"><ellipse cx="4.6" cy="1.1" rx="8.4" ry="1.1" fill="#000" opacity=".2"/><g transform="scale(1.45 .8) rotate(-14)"><path d="M.6-1C4-2.9 9.2-2.5 10.8-.5C9.6 1.1 4.2 1.7.7 1Z" fill="{fill}" opacity=".92"/><path d="M.6-1C4-2.9 9.2-2.5 10.8-.5" fill="none" stroke="#fff" stroke-width=".3" opacity=".22"/><path d="M1.2-.3C4-1.4 7.5-1.5 10.2-.7M1.2 0C4.2-.4 7.6-.2 10 .3M1.2.3C4 .7 6.8 1 8.6 1" fill="none" stroke="#000" stroke-width=".25" opacity=".25"/><ellipse rx="1.6" ry="1.35" fill="{fill}"/><ellipse rx="1.6" ry="1.35" fill="#000" opacity=".3"/></g></g>"##,
            Num(x),
            Num(y),
            Num(w * k)
        );
    }
    // ---- front soil ----
    if tier("arctic-code-vault-contributor").is_some() && season == Season::Winter {
        // A frosted seed, fallen at the front left: a samara pair tipped
        // over, paled by frost and rimed, a third of it behind the rim, clear
        // of the kabuto further in.
        let f: f64 = 0.82;
        let at = (
            CENTER_X - hw * f,
            SOIL_Y + ry * (1.0 - f * f).sqrt() - 2.0 * w * k,
        );
        soil.push((at.0, at.1, 4.6 * w * k));
        let _ = write!(
            e.front,
            r##"<g transform="translate({} {}) scale({})"><ellipse cy="1.2" rx="4.6" ry=".9" fill="#261708" opacity=".35"/><g transform="scale(1.05 .72) rotate(-58)"><use href="#sm" fill="#d9a6a2"/><use href="#sm" fill="none" stroke="#eef5fa" stroke-width=".55" opacity=".9"/></g><g fill="#fff"><circle cx="-1.6" cy="-1.1" r=".45" opacity=".9"/><circle cx="2.9" cy=".2" r=".3" opacity=".8"/></g></g>"##,
            Num(at.0),
            Num(at.1),
            Num(w * k)
        );
    }
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
        let at = (CENTER_X + hw * 0.5, SOIL_Y + ry * 0.35);
        if let Some((x, y)) = settle(&mut soil, at, half * w * 1.3 * k, hw) {
            let _ = write!(
                e.front,
                r##"<g transform="translate({} {}) scale({})"><ellipse cy="1.4" rx="{}" ry="1.6" fill="{shadow}" opacity=".45"/><use href="#{symbol}" fill="{paint}" stroke="{}" stroke-width=".3"/><path d="M-2-2.2Q.4-3.2 2.6-2.2" fill="none" stroke="#fff" stroke-width=".55" opacity=".45"/><g fill="{}" opacity=".3"><circle cx="-2" cy="-.6" r=".35"/><circle cx="1" cy="-1.4" r=".28"/><circle cx="-.4" cy=".4" r=".3"/></g></g>"##,
                Num(x),
                Num(y),
                Num(w * 1.3 * k),
                Num(half),
                paint.shift(-0.18, 0.0),
                paint.shift(-0.2, 0.0)
            );
        }
    }
    // A pair is a spring thing: the pebbles and the cranes' courtship.
    let pair = tier("pair-extraordinaire").filter(|_| season == Season::Spring);
    if let Some(t) = pair
        && t >= 2
    {
        let symbol = match t {
            2 => "pebbles",
            3 => "yinyang",
            _ => "cranes",
        };
        let at = (CENTER_X - hw * 0.3, SOIL_Y + ry * 0.45);
        if let Some((x, y)) = settle(&mut soil, at, 8.0 * w * 1.5 * k, hw) {
            let _ = write!(
                e.front,
                r##"<g transform="translate({} {}) scale({})"><ellipse cy=".6" rx="8" ry="1.4" fill="{shadow}" opacity=".45"/><use href="#{symbol}"/></g>"##,
                Num(x),
                Num(y),
                Num(w * 1.5 * k)
            );
        }
    }
    if b.extras.weekend > 0 {
        // Folded from red washi; the gold crest comes with the second tier.
        let paper = if dark {
            Rgb::hex(0xb8433a).shift(0.05, 0.0)
        } else {
            Rgb::hex(0xb8433a)
        };
        let crest = if b.extras.weekend >= 2 {
            r##"<use href="#kuwagata"/>"##
        } else {
            ""
        };
        let at = (CENTER_X - hw * 0.66, SOIL_Y + ry * 0.22);
        if let Some((x, y)) = settle(&mut soil, at, 7.0 * w * 1.25 * k, hw) {
            let _ = write!(
                e.front,
                r##"<g transform="translate({} {}) scale({}) rotate(-6)"><ellipse cy=".5" rx="7" ry="1.6" fill="{shadow}" opacity=".45"/><use href="#kabuto" fill="{paper}"/>{crest}</g>"##,
                Num(x),
                Num(y),
                Num(w * 1.25 * k)
            );
        }
    }
    if tier("public-sponsor").is_some() {
        let at = (CENTER_X + hw * 0.16, SOIL_Y + ry * 0.62);
        if let Some((x, y)) = settle(&mut soil, at, 4.4 * w * 1.5 * k, hw) {
            // Frontmost of the soil eggs: on a small pot it stays on the soil,
            // its shadow (2.2 below its centre) short of the rim's lip.
            let across = ((x - CENTER_X) / (hw - 4.0).max(1.0)).clamp(-1.0, 1.0);
            let edge = SOIL_Y + 0.6 + (ry - 2.0).max(1.0) * (1.0 - across * across).sqrt();
            let y = y.min(edge - 2.2 * w * 1.5 * k);
            let _ = write!(
                e.front,
                r##"<g transform="translate({} {}) scale({})"><ellipse cy=".7" rx="4.4" ry="1.5" fill="{shadow}" opacity=".45"/><use href="#goen"/></g>"##,
                Num(x),
                Num(y),
                Num(w * 1.5 * k)
            );
        }
    }

    // ---- branches ----
    // A sprout's stem is drawn on its own: no branch to hang from yet.
    let hang = if crate::sprout::is_sprout(b) {
        None
    } else {
        tier("starstruck").and_then(|_| take_spot(&spots, &mut taken, 30.0))
    };
    if tier("starstruck") == Some(1) && hang.is_none() {
        // With nothing to hang from, the paper star lies on the soil.
        let star = if dark { "#f6efd2" } else { "#efe3b8" };
        let at = (CENTER_X + hw * 0.34, SOIL_Y + ry * 0.5);
        if let Some((x, y)) = settle(&mut soil, at, 3.8 * w * 1.25 * k, hw) {
            let _ = write!(
                e.front,
                r##"<g transform="translate({} {}) scale({})"><ellipse cy=".9" rx="3.6" ry="1" fill="{shadow}" opacity=".45"/><use href="#star5" transform="scale(1 .55) rotate(14)" fill="{star}" stroke="#b39a5a" stroke-width=".35"/></g>"##,
                Num(x),
                Num(y),
                Num(w * 1.25 * k)
            );
        }
    }
    if let Some(t) = tier("starstruck")
        && let Some((x, y)) = hang
    {
        // A young tree's small pot gets a small chime or star.
        let scale = if t == 1 { 1.25 } else { 1.5 } * k;
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
            // The clapper hangs inside the bell at its lip, where it strikes
            // the rim: drawn first, so the glass or metal covers its upper
            // half. The paper strip that catches the wind hangs below.
            // The painted chimes dance in the wind: the blue one slowly,
            // the golden one faster.
            let sway = match t {
                3 => r#" class="fw""#,
                t if t >= 4 => r#" class="fs""#,
                _ => "",
            };
            e.animated += usize::from(t >= 3);
            let _ = write!(
                g,
                r##"<g{sway}><path d="M0 4.2V12" stroke="#8a7a5a" stroke-width=".3"/><circle cy="9.5" r=".75" fill="{edge}"/><rect x="-1.2" y="12" width="2.4" height="7.4" fill="{strip}" stroke="#00000022" stroke-width=".2"/></g>"##
            );
            let _ = write!(
                g,
                r##"<path d="M0 0V3.2" stroke="#8a7a5a" stroke-width=".35"/><path d="M-4.2 9.4Q-4.4 3.4 0 3.2Q4.4 3.4 4.2 9.4Z" fill="{glass}" fill-opacity="{opacity}" stroke="{edge}" stroke-width=".4"/><path d="M-4.4 9.4h8.8" stroke="{edge}" stroke-width=".45"/><path d="M-2.4 4.8Q-3.2 6.4-3 8.2" fill="none" stroke="#fff" stroke-width=".45" opacity=".7"/>"##
            );
            if t == 3 {
                // Stars painted on the blue glass.
                g.push_str(r##"<g fill="#fff3c4"><use href="#star5" transform="translate(1.4 6) scale(.32)"/><use href="#star5" transform="translate(-.6 7.8) scale(.24)"/><use href="#star5" transform="translate(2.6 8.2) scale(.2)"/><circle cx="-1.6" cy="5.6" r=".22"/><circle cx="3" cy="5.2" r=".18"/></g>"##);
            }
        }
        g.push_str("</g>");
        e.front.push_str(&g);
    }
    if let Some(t) = tier("galaxy-brain") {
        // The firefly stays from x2 on, in summer.
        if t >= 2
            && season == Season::Summer
            && let Some((x, y)) = firefly_spot(scene, &mut taken)
        {
            let _ = write!(
                e.front,
                r##"<g class="ff"><g transform="translate({} {}) scale({})"><use href="#firefly"/></g></g>"##,
                Num(x),
                Num(y),
                Num(w)
            );
            e.animated += 2;
        }
        // From x3 a galaxy plant peeks from behind the trunk on its lit side,
        // the trunk covering a part: a begonia leaf, from x4 a petunia. The
        // roots never hide it: it rises clear of them.
        if t >= 3 {
            // Beside a slender young trunk the petunia is a small one: no
            // leaf, lower, smaller. The begonia keeps its size.
            let slender = trunk_girth(trunk, 24.0) < PLANT_GIRTH * 0.85;
            let big = || (crate::galaxy::petunia(true), 36.0, -12.0, 3.79);
            let small = || (crate::galaxy::petunia(false), 22.0, -12.0, 2.6);
            // The plants it may be, in turn: a full petunia that can't stand
            // clear beside the trunk tries the small one, lower, under the
            // branch in its way.
            let kinds = match (t, slender) {
                (3, _) => vec![(crate::galaxy::begonia(), 18.0, 28.0, 0.86)],
                (_, true) => vec![small()],
                _ => vec![big(), small()],
            };
            let shape = crate::galaxy::footprint(t == 3);
            // Where a plant of this kind stands, and whether it stands clear.
            let find = |&(_, h, rot, size): &(String, f64, f64, f64)| {
                let s = w * size;
                // The leaf's base sits just inside the trunk's edge; the
                // flower's centre a radius out.
                let out = if t == 3 {
                    -3.0
                } else {
                    crate::galaxy::PETUNIA_R * s
                };
                let place = |h: f64| trunk_edge(trunk, h).map(|edge| (edge - out, SOIL_Y - h));
                let shows = |(x, y): (f64, f64)| {
                    let (sn, cs) = (math::sin(math::rad(rot)), math::cos(math::rad(rot)));
                    let (mut seen, mut crossed) = (0, 0);
                    for &(a, b) in &shape {
                        let p = (x + s * (a * cs - b * sn), y + s * (a * sn + b * cs));
                        let hidden = covered(p, wood, roots);
                        if !hidden {
                            seen += 1;
                        }
                        // Wood other than the trunk it peeks from crosses it.
                        if (hidden || covered(p, twigs, &[])) && !covered(p, trunk, roots) {
                            crossed += 1;
                        }
                    }
                    let n = shape.len().max(1) as f64;
                    (f64::from(seen) / n, f64::from(crossed) / n)
                };
                // It peeks from behind the trunk: out along the soil a short
                // way, the leaf's length or the flower's radius, from where
                // the trunk meets
                // the soil, so its stem stands beside it however the trunk
                // leans, then from the trunk at the plant's own height.
                let reach = if t == 3 {
                    10.0 * s
                } else {
                    crate::galaxy::PETUNIA_R * s
                };
                let along = |at: f64| -> Vec<((f64, f64), (f64, f64))> {
                    (0..100)
                        .map(|k| 3.0 * f64::from(k))
                        .take_while(|&d| d <= reach)
                        .filter_map(|d| {
                            trunk_edge(trunk, at).map(|edge| (edge - out - d, SOIL_Y - h))
                        })
                        .map(|p| (p, shows(p)))
                        .collect()
                };
                let outward = along(STEM_FOOT);
                let beside = along(h);
                let clear = outward
                    .iter()
                    .chain(&beside)
                    .find(|c| c.1.0 >= 0.62 && c.1.1 <= 0.02)
                    .map(|c| c.0);
                // Failing that, the spot wood crosses least where two thirds
                // show; then the lowest that shows most, up the trunk.
                let upward = (1..14)
                    .filter_map(|k| place(h + 3.0 * f64::from(k)))
                    .map(|p| (p, shows(p)));
                let fallback = outward
                    .iter()
                    .chain(&beside)
                    .filter(|c| c.1.0 >= 0.62)
                    .min_by(|a, b| a.1.1.total_cmp(&b.1.1))
                    .copied()
                    .or_else(|| {
                        outward.iter().copied().chain(upward).fold(
                            None::<((f64, f64), (f64, f64))>,
                            |best, c| match best {
                                Some(b) if b.1.0 >= 0.62 || b.1.0 >= c.1.0 => Some(b),
                                _ => Some(c),
                            },
                        )
                    })
                    .map(|b| b.0);
                (clear, fallback)
            };
            // The first kind that stands clear; failing all, the first kind
            // where it shows best.
            let first = find(&kinds[0]);
            let (kind, spot) = match first.0 {
                Some(at) => (&kinds[0], Some(at)),
                None => kinds[1..]
                    .iter()
                    .find_map(|k| find(k).0.map(|at| (k, Some(at))))
                    .unwrap_or((&kinds[0], first.1)),
            };
            let (art, h, rot, size) = (kind.0.as_str(), kind.1, kind.2, kind.3);
            let s = w * size;
            if let Some((x, y)) = spot {
                // Lifted, it grows a longer stem down to the soil, curving a
                // little toward the trunk: a taller plant, not a floating one.
                if y < SOIL_Y - h - 0.5 {
                    let (foot, stem, width) = if t == 3 {
                        ((3.5, 7.0), "#5a2a36", 0.6)
                    } else {
                        ((2.4, 8.0), "#4f7d33", 0.5)
                    };
                    let (sn, cs) = (math::sin(math::rad(rot)), math::cos(math::rad(rot)));
                    let from = (
                        x + s * (foot.0 * cs - foot.1 * sn),
                        y + s * (foot.0 * sn + foot.1 * cs),
                    );
                    let to = (from.0 + 4.0, SOIL_Y + 3.0);
                    let _ = write!(
                        e.back,
                        r#"<path d="M{} {}Q{} {} {} {}" stroke="{stem}" stroke-width="{}" fill="none" stroke-linecap="round"/>"#,
                        Num(from.0),
                        Num(from.1),
                        Num(from.0 - 1.0),
                        Num(f64::midpoint(from.1, to.1)),
                        Num(to.0),
                        Num(to.1),
                        Num(width * s)
                    );
                }
                let _ = write!(
                    e.back,
                    r#"<g transform="translate({} {}) rotate({rot}) scale({})">{art}</g>"#,
                    Num(x),
                    Num(y),
                    Num(s)
                );
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Soil eggs never touch: each settles clear of those before it, even
    /// crowded on a small pot.
    #[test]
    fn soil_eggs_never_touch() {
        let mut soil = Vec::new();
        for (dx, r) in [
            (30.0, 12.0),
            (10.0, 7.0),
            (-18.0, 13.0),
            (-40.0, 9.0),
            (20.0, 5.0),
        ] {
            assert!(
                settle(&mut soil, (CENTER_X + dx, SOIL_Y), r, 70.0).is_some(),
                "no room for an egg of {r} at {dx}"
            );
        }
        assert_eq!(soil.len(), 5);
        // A pot with no room left lays no egg on another.
        let mut full = vec![(CENTER_X, SOIL_Y, 60.0)];
        assert_eq!(settle(&mut full, (CENTER_X, SOIL_Y), 10.0, 70.0), None);
        assert_eq!(full.len(), 1);
        for (i, a) in soil.iter().enumerate() {
            for b in &soil[i + 1..] {
                let d = (math::sq(a.0 - b.0) + math::sq(a.1 - b.1)).sqrt();
                assert!(d >= a.2 + b.2 - 1e-9, "{a:?} touches {b:?}");
            }
        }
    }

    /// Each seasonal egg shows in its season only, and the south's seasons
    /// run six months from the north's.
    #[test]
    fn seasonal_eggs_keep_their_season() {
        use crate::config::Config;
        use crate::render::{Theme, svg};
        let snapshot = crate::fixtures::by_name("veteran").unwrap().snapshot;
        // Each egg's mark and the middle month of its northern season.
        let marks = [
            ("#cranes", 4),
            ("#firefly", 7),
            (r#"rx="8.4" ry="1.1""#, 10),
            ("#eef5fa", 1),
        ];
        let in_season = |m: u32, middle: u32| matches!((m + 12 - middle) % 12, 0 | 1 | 11);
        for hemisphere in [Hemisphere::North, Hemisphere::South] {
            let mut config = Config::default();
            config.tree.hemisphere = hemisphere;
            for m in 1..=12 {
                let day = crate::date::Date::from_ymd(2025, m, 15);
                let drawn = svg(
                    &Bonsai::grow(&snapshot, &config, day),
                    &config,
                    Theme::Light,
                );
                let north = if hemisphere == Hemisphere::South {
                    (m + 5) % 12 + 1
                } else {
                    m
                };
                for (mark, middle) in marks {
                    let want = in_season(north, middle);
                    assert_eq!(
                        drawn.contains(mark),
                        want,
                        "{mark} {hemisphere:?} month {m}"
                    );
                }
            }
        }
    }

    #[test]
    fn spots_under_a_cloud_come_after_every_clear_spot() {
        let covered_high = (COVERED + 2, 200.0, 100.0);
        let clear_high = (5, 220.0, 110.0);
        let clear_low = (40, 240.0, 300.0);
        let ranked = rank(vec![covered_high, clear_high], vec![clear_low]);
        assert_eq!(ranked, vec![clear_high, clear_low, covered_high]);
    }
}
