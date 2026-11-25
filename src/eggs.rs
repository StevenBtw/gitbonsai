//! Achievement easter eggs and kintsugi.
//!
//! Each GitHub achievement adds one small, fitting object; the object grows
//! with the tier (x1 to x4). The common one-click achievements (Quickdraw,
//! YOLO, Heart On Your Sleeve, Open Sourcerer) are left out, so the tree stays
//! calm. Every egg has a fixed spot, so they never collide:
//!
//! | Achievement | x1 | x2 | x3 | x4 |
//! |---|---|---|---|---|
//! | Pull Shark | small stone fish | polished ivory white shark | stone tuna | stone shark |
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
use crate::pot::SHADE;
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

/// Where a string hangs from the wood drawn at canvas `x`: just inside the
/// underside of the piece (points with their radius) whose line passes
/// within its radius, and a little, of `(x, y)`. `None` where none does.
fn underside(pieces: &[&[Vec<(f64, f64, f64)>]], x: f64, y: f64) -> Option<f64> {
    pieces
        .iter()
        .flat_map(|p| p.iter())
        .flat_map(|w| w.windows(2))
        .filter(|s| (s[0].0 - x) * (s[1].0 - x) <= 0.0 && s[0].0 != s[1].0)
        .map(|s| {
            let t = (x - s[0].0) / (s[1].0 - s[0].0);
            (
                s[0].1 + (s[1].1 - s[0].1) * t,
                s[0].2 + (s[1].2 - s[0].2) * t,
            )
        })
        .filter(|&(c, r)| (c - y).abs() <= r + 6.0)
        .min_by(|a, b| (a.0 - y).abs().total_cmp(&(b.0 - y).abs()))
        .map(|(c, r)| c + 0.6 * r)
}

/// Branch points in the upper tree to hang from, on the wood as drawn
/// (`wood`), least crowded by leaves first. A point the drawn wood has moved
/// away from is left out: nothing would hold the string.
fn branch_spots(scene: &Scene, wood: &[&[Vec<(f64, f64, f64)>]]) -> Vec<(usize, f64, f64)> {
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
            let Some(hang) = underside(wood, x, y) else {
                continue;
            };
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
            // Nor from wood with no leaves over it, where it would stick
            // out of the crown or hang in a fan of bare twigs.
            if !leaves.is_empty() && !leaves.iter().any(|l| (l.0 - x).abs() < reach && l.1 < hang) {
                crowd += COVERED;
            }
            if y < SOIL_Y - 90.0 {
                upper.push((crowd, x, hang));
            } else {
                lower.push((crowd, x, hang));
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

/// Places a soil egg at `at`, or at the nearest spot sideways along the
/// soil within the pot, and takes that spot. `(r, down, up)` is how far it
/// reaches from its centre: to each side, down to its front, and up.
/// The spot keeps its sides on the soil, its front short of the rim's lip
/// (one asked for too far forward lies back from it) and clear of the eggs
/// in `soil`; where the rim stands above the soil, on the bowl and a
/// rectangle, up to 40% of the egg may sit behind it instead; of those, the nearest that keeps off the wood and roots in
/// `ground`, else the one that covers least of them. With no spot left it
/// gets none: the egg is left out rather than laid on another.
fn settle(
    soil: &mut Vec<(f64, f64, f64)>,
    at: (f64, f64),
    (r, down, up): (f64, f64, f64),
    pot: &Pot,
    (wood, roots): (&[Vec<(f64, f64, f64)>], &[Vec<(f64, f64)>]),
) -> Option<(f64, f64)> {
    // The soil as drawn, kept 2 inside its edge and the lip.
    let (reach_x, reach_y) = pot.soil_reach();
    let (sx, sy) = (reach_x - 6.0, (reach_y - 4.0).max(1.0));
    let on_soil =
        |x: f64, y: f64| math::sq((x - CENTER_X) / sx) + math::sq((y - SOIL_Y - 0.6) / sy) <= 1.0;
    let room = (0.85 * reach_x - r).max(0.0);
    let step = (0.5 * r).max(1.0);
    let behind = pot.shape != crate::pot::Shape::Oval;
    let rect = pot.shape == crate::pot::Shape::Rect;
    let (_, x, y) = (0..60)
        .map(|i| {
            let k = step * f64::from((i + 1) / 2);
            if i % 2 == 1 { at.0 + k } else { at.0 - k }
        })
        .filter(|x| (x - CENTER_X).abs() <= room)
        .map(|x| {
            let lowest = if behind {
                // At least 60% of it above the rim's front edge.
                pot.rim_front_y(x) + up - 0.6 * (up + down)
            } else {
                let across = ((x - CENTER_X) / sx).clamp(-1.0, 1.0);
                SOIL_Y + 0.6 + sy * (1.0 - across * across).sqrt() - down
            };
            (x, at.1.min(lowest))
        })
        .filter(|&(x, y)| {
            let sides = if rect {
                // Within the opening, clear of the frame at the sides.
                (x - CENTER_X).abs() + r <= reach_x - 2.0
            } else if behind {
                // Its top on the soil; its front may go behind the rim.
                on_soil(x - r, y - up) && on_soil(x + r, y - up)
            } else {
                on_soil(x - r, y) && on_soil(x + r, y)
            };
            sides
                && soil
                    .iter()
                    .all(|&(a, b, q)| math::sq(a - x) + math::sq(b - y) >= math::sq(r + q))
        })
        .map(|(x, y)| {
            let on = [
                (-r, 0.0),
                (0.0, 0.0),
                (r, 0.0),
                (-0.5 * r, -up),
                (0.5 * r, -up),
                (0.0, -up),
            ]
            .iter()
            .filter(|&&(dx, dy)| covered((x + dx, y + dy), wood, roots))
            .count();
            (on, x, y)
        })
        .min_by_key(|s| s.0)?;
    soil.push((x, y, r));
    Some((x, y))
}

/// A spot a galaxy plant may take, for [`peek`]: where it stands, the
/// share of it that shows, the share other wood crosses, whether its stem
/// (and the petunia's leaf) is behind the trunk, and whether the part that
/// must show, the petunia's centre, does.
#[derive(Debug, Clone, Copy)]
struct Spot {
    at: (f64, f64),
    seen: f64,
    crossed: f64,
    tucked: bool,
    centred: bool,
}

/// How much of a galaxy plant shows: most of it, the trunk hiding an edge
/// of it and its stem. It is measured against the trunk's line, which is
/// narrower than its flared foot, so the plant shows a little less.
const PEEK: f64 = 0.8;

/// Chooses where a galaxy plant peeks from among `spots`: you get a spot
/// that stands clear, if any, and the best one that doesn't.
///
/// A plant peeks when its stem is behind the trunk, its centre shows, and
/// between seven tenths and nearly all of it shows: most, never all. Clear, no other wood crosses it;
/// of those, the one closest to [`PEEK`]. Failing that, the peeking spot
/// other wood crosses least, then any with its stem tucked away and its
/// centre in view, then any with its stem tucked away. You get no spot at
/// all only when the stem shows everywhere.
fn peek(spots: &[Spot]) -> (Option<(f64, f64)>, Option<(f64, f64)>) {
    let peeking = |s: &&Spot| s.tucked && s.centred && (0.7..=0.94).contains(&s.seen);
    let near = |a: &Spot, b: &Spot| (a.seen - PEEK).abs().total_cmp(&(b.seen - PEEK).abs());
    let clear = spots
        .iter()
        .filter(peeking)
        .filter(|s| s.crossed <= 0.02)
        .min_by(|a, b| near(a, b))
        .map(|s| s.at);
    let fallback = spots
        .iter()
        .filter(peeking)
        .min_by(|a, b| a.crossed.total_cmp(&b.crossed).then(near(a, b)))
        .or_else(|| {
            spots
                .iter()
                .filter(|s| s.tucked && s.centred)
                .min_by(|a, b| near(a, b))
        })
        .or_else(|| spots.iter().filter(|s| s.tucked).min_by(|a, b| near(a, b)))
        .map(|s| s.at);
    (clear, fallback)
}

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
    let spots = branch_spots(scene, &[wood, twigs]);
    let mut taken: Vec<(f64, f64)> = Vec::new();

    // The soil eggs keep to the pot's size, and each to a spot of its own:
    // on a small pot they shrink, and one that would touch another moves
    // along the soil.
    let k = (hw / POT_FULL).clamp(0.55, 1.0);
    let mut soil: Vec<(f64, f64, f64)> = Vec::new();
    let ground = (wood, roots);

    // ---- back soil ----
    if tier("galaxy-brain") == Some(1) && season == Season::Summer {
        let at = (CENTER_X - hw * 0.45, SOIL_Y + ry * 0.15);
        if let Some((x, y)) = settle(
            &mut soil,
            at,
            (4.0 * w * 1.1 * k, 1.0 * w * 1.1 * k, 2.0 * w * 1.1 * k),
            pot,
            ground,
        ) {
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
    // The kabuto, furthest out, settles first: it has the least room.
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
        if let Some((x, y)) = settle(
            &mut soil,
            at,
            (7.0 * w * 1.25 * k, 2.1 * w * 1.25 * k, 6.0 * w * 1.25 * k),
            pot,
            ground,
        ) {
            let _ = write!(
                e.front,
                r##"<g transform="translate({} {}) scale({}) rotate(-6)"><ellipse cy=".5" rx="7" ry="1.6" fill="{shadow}" opacity=".45"/><use href="#kabuto" fill="{paper}"/>{crest}</g>"##,
                Num(x),
                Num(y),
                Num(w * 1.25 * k)
            );
        }
    }
    if let Some(t) = tier("pull-shark") {
        let (symbol, half) = match t {
            1 => ("fish1", 5.5),
            2 => ("ivoryshark", 7.2),
            3 => ("fish3", 6.75),
            _ => ("fish4", 12.0),
        };
        // The ivory shark, tall with its fins, lies further back.
        let depth = if t == 2 { 0.05 } else { 0.35 };
        let at = (CENTER_X + hw * 0.5, SOIL_Y + ry * depth);
        if let Some((x, y)) = settle(
            &mut soil,
            at,
            (half * w * 1.3 * k, 3.0 * w * 1.3 * k, 4.0 * w * 1.3 * k),
            pot,
            ground,
        ) {
            let _ = write!(
                e.front,
                r#"<g transform="translate({} {}) scale({})">"#,
                Num(x),
                Num(y),
                Num(w * 1.3 * k)
            );
            if t == 2 {
                // A polished ivory white shark, seen three quarters on, at a
                // little over half the size it is drawn and a little flatter:
                // its back lit above a crisp crease from snout to tail, the
                // flank and belly a step darker each. It lies sunk in the
                // soil, its belly and fins under it, with a faint shadow.
                let (ivory, edge) = if dark {
                    (Rgb::hex(0xede5d3), Rgb::hex(0xb09c76))
                } else {
                    (Rgb::hex(0xf6f0e2), Rgb::hex(0xc3b08c))
                };
                let _ = write!(
                    e.front,
                    r##"<ellipse cx=".6" cy="1" rx="{}" ry="1" fill="{shadow}" opacity=".28"/><clipPath id="sunk"><rect x="-12" y="-12" width="24" height="14.1"/></clipPath><use href="#{symbol}" clip-path="url(#sunk)" transform="scale(.55 .46)" fill="{ivory}" stroke="{edge}" stroke-width=".36" stroke-linejoin="round"/><ellipse cx="1.2" cy="1.05" rx="{}" ry=".42" fill="{}" opacity=".9"/></g>"##,
                    Num(half * 0.85),
                    Num(half * 0.62),
                    paints.soil.shift(-0.04, 0.0)
                );
            } else {
                let paint = paints.stone;
                // The tuna carries its own gloss along its back, and is
                // drawn at three quarters of its size.
                let (gloss, size) = if t == 3 {
                    ("", r#" transform="scale(.75)""#)
                } else {
                    (
                        r##"<path d="M-2-2.2Q.4-3.2 2.6-2.2" fill="none" stroke="#fff" stroke-width=".55" opacity=".45"/>"##,
                        "",
                    )
                };
                let _ = write!(
                    e.front,
                    r##"<ellipse cy="1.4" rx="{half}" ry="1.6" fill="{shadow}" opacity=".45"/><use href="#{symbol}"{size} fill="{paint}" stroke="{}" stroke-width=".3" stroke-linejoin="round"/>{gloss}<g fill="{}" opacity=".3"><circle cx="-2" cy="-.6" r=".35"/><circle cx="1" cy="-1.4" r=".28"/><circle cx="-.4" cy=".4" r=".3"/></g></g>"##,
                    paint.shift(-0.18, 0.0),
                    paint.shift(-0.2, 0.0)
                );
            }
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
        if let Some((x, y)) = settle(
            &mut soil,
            at,
            (8.0 * w * 1.5 * k, 2.0 * w * 1.5 * k, 4.0 * w * 1.5 * k),
            pot,
            ground,
        ) {
            let _ = write!(
                e.front,
                r##"<g transform="translate({} {}) scale({})"><ellipse cy=".6" rx="8" ry="1.4" fill="{shadow}" opacity=".45"/><use href="#{symbol}"/></g>"##,
                Num(x),
                Num(y),
                Num(w * 1.5 * k)
            );
        }
    }
    if tier("public-sponsor").is_some() {
        let at = (CENTER_X + hw * 0.16, SOIL_Y + ry * 0.62);
        if let Some((x, y)) = settle(
            &mut soil,
            at,
            (4.4 * w * 1.5 * k, 2.2 * w * 1.5 * k, 2.2 * w * 1.5 * k),
            pot,
            ground,
        ) {
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
        if let Some((x, y)) = settle(
            &mut soil,
            at,
            (3.8 * w * 1.25 * k, 1.9 * w * 1.25 * k, 1.9 * w * 1.25 * k),
            pot,
            ground,
        ) {
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
            let sway = format!(r#" class="{}""#, furin_sway(t));
            e.animated += 1;
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
            // Each kind: its drawing, height, tilt, size, and the points of
            // it that stay behind the trunk.
            let big = || {
                let hidden = &crate::galaxy::PETUNIA_HIDDEN[..];
                (crate::galaxy::petunia(true), 36.0, -12.0, 3.79, hidden)
            };
            let small = || {
                let hidden = &crate::galaxy::PETUNIA_HIDDEN[..1];
                (crate::galaxy::petunia(false), 22.0, -12.0, 2.6, hidden)
            };
            // The plants it may be, in turn: a full petunia that can't stand
            // clear beside the trunk tries the small one, lower, under the
            // branch in its way.
            let kinds = match (t, slender) {
                (3, _) => vec![(
                    crate::galaxy::begonia(),
                    18.0,
                    28.0,
                    0.86,
                    &crate::galaxy::BEGONIA_HIDDEN[..],
                )],
                (_, true) => vec![small()],
                _ => vec![big(), small()],
            };
            let shape = crate::galaxy::footprint(t == 3);
            // A hairline crossing a plant is no matter; a branch is.
            let branches: Vec<Vec<(f64, f64, f64)>> = twigs
                .iter()
                .filter(|s| s.iter().any(|p| p.2 >= 1.2))
                .cloned()
                .collect();
            // Where a plant of this kind stands, and whether it stands clear.
            let find = |&(_, h, rot, size, hidden): &(String, f64, f64, f64, &[(f64, f64)])| {
                // The petunia's centre always shows; the begonia has none to keep.
                let shown: &[(f64, f64)] = if t == 3 { &[] } else { &[(0.0, 0.0)] };
                let s = w * size;
                // The leaf's base sits just inside the trunk's edge; the
                // flower's centre a radius out.
                let out = if t == 3 {
                    -3.0
                } else {
                    crate::galaxy::PETUNIA_R * s
                };
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
                        if (hidden || covered(p, &branches, &[])) && !covered(p, trunk, roots) {
                            crossed += 1;
                        }
                    }
                    let n = shape.len().max(1) as f64;
                    (f64::from(seen) / n, f64::from(crossed) / n)
                };
                // It peeks from behind the trunk, its stem behind it: tried
                // from the plant's own height up, from a little way out of
                // the trunk's edge to well behind it, the lowest height
                // where one stands clear winning.
                let span = if t == 3 {
                    10.0 * s
                } else {
                    crate::galaxy::PETUNIA_R * s
                };
                let (sn, cs) = (math::sin(math::rad(rot)), math::cos(math::rad(rot)));
                let foot = crate::galaxy::stem_foot(t == 3);
                let on = |at: (f64, f64), (a, b): (f64, f64)| {
                    (at.0 + s * (a * cs - b * sn), at.1 + s * (a * sn + b * cs))
                };
                let mut best = (None, None);
                // It stays near the soil, lifted 12 units at most: higher, it
                // would float up the trunk among the limbs and leaves.
                for k in 0..5 {
                    let up = h + 3.0 * f64::from(k);
                    let Some(edge) = trunk_edge(trunk, up) else {
                        continue;
                    };
                    let spots: Vec<Spot> = (-4..=12)
                        .map(|f| {
                            let at = (edge - out + span * f64::from(f) / 8.0, SOIL_Y - up);
                            let (seen, crossed) = shows(at);
                            // Its stem and leaf stay behind the trunk, and so
                            // does the longer stem a lifted plant grows down
                            // to the soil.
                            let from = on(at, foot);
                            let down = (from.0 + 9.0, SOIL_Y + 3.0);
                            let lifted = up > h + 0.5;
                            let tucked = hidden.iter().all(|&p| covered(on(at, p), trunk, roots))
                                && (!lifted
                                    || [0.2, 0.4, 0.6, 0.8].iter().all(|&u| {
                                        let p = (
                                            from.0 + (down.0 - from.0) * u,
                                            from.1 + (down.1 - from.1) * u,
                                        );
                                        covered(p, trunk, roots)
                                    }));
                            Spot {
                                at,
                                seen,
                                crossed,
                                tucked,
                                // Twigs are drawn in front of it too.
                                centred: shown.iter().all(|&p| {
                                    let p = on(at, p);
                                    !covered(p, wood, roots) && !covered(p, &branches, &[])
                                }),
                            }
                        })
                        .collect();
                    let (clear, fallback) = peek(&spots);
                    if clear.is_some() {
                        best = (clear, clear);
                        break;
                    }
                    if best.1.is_none() {
                        best.1 = fallback;
                    }
                }
                let (clear, fallback) = best;
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
                    let foot = crate::galaxy::stem_foot(t == 3);
                    let (stem, width) = if t == 3 {
                        ("#5a2a36", 0.6)
                    } else {
                        ("#4f7d33", 0.5)
                    };
                    let (sn, cs) = (math::sin(math::rad(rot)), math::cos(math::rad(rot)));
                    let from = (
                        x + s * (foot.0 * cs - foot.1 * sn),
                        y + s * (foot.0 * sn + foot.1 * cs),
                    );
                    // On the slant, down behind the trunk.
                    let to = (from.0 + 9.0, SOIL_Y + 3.0);
                    let _ = write!(
                        e.back,
                        r#"<path d="M{} {}Q{} {} {} {}" stroke="{stem}" stroke-width="{}" fill="none" stroke-linecap="round"/>"#,
                        Num(from.0),
                        Num(from.1),
                        Num(from.0 + 2.0),
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

/// How much more a mend darkens in the wall's shade than the glaze does:
/// metal shows the turn from the light more than a glaze.
const MEND_SHADE: f64 = 2.5;
/// The staple's id suffix per shade step.
const STEP_ID: [&str; 3] = ["", "1", "2"];

/// How a pot's breaks are mended.
#[derive(Clone, Copy, PartialEq)]
enum Mend {
    /// Gold in lacquer, on a glazed pot.
    Gold,
    /// A fine silver seam laid in black lacquer, on the vintage clay.
    Silver,
    /// A hairline in the clay with copper staples across it, on the brown
    /// clay oval.
    Staples,
    /// The bare hairline, on the terracotta.
    Crack,
}

/// Kintsugi: every archived repository is a break in the pot, mended the
/// pot's own way: gold on a glazed pot, silver in black lacquer on the
/// vintage clay, copper staples on the brown clay oval, a bare crack in the
/// terracotta, and nothing on the plastic training pot. A break starts at the rim and runs down past the foot
/// (clipped by the pot, so it continues underneath), meandering on two scales
/// with a width that swells and thins like poured lacquer. Breaks sit at
/// golden-angle steps around the pot in archive order, so archiving another
/// repository never moves an old one; only those on the front half are
/// visible. A mend takes the wall's shade steps on the side away from `lit`,
/// darker and duller than the glaze. `dark` lifts the silver on a dark page.
pub fn kintsugi(archived: &[String], seed: u64, pot: &Pot, dark: bool, lit: f64) -> String {
    let mend = match pot.level {
        0 => return String::new(),
        1 => Mend::Crack,
        8 => Mend::Silver,
        level if crate::pot::model(level).glazed => Mend::Gold,
        _ => Mend::Staples,
    };
    let Pot {
        hw,
        ry,
        depth,
        bw,
        glaze,
        ..
    } = *pot;
    // A seam runs down the wall: round it on an oval or a bowl, over a
    // bowl's belly, straight down a rectangle's face.
    let at = |a: f64, t: f64| {
        let rad = pot.half_width(t.clamp(0.0, 1.0)) + (bw - hw) * (t - t.clamp(0.0, 1.0));
        let y = match pot.shape {
            crate::pot::Shape::Rect => SOIL_Y + ry + depth * t,
            crate::pot::Shape::Bowl => SOIL_Y + depth * t + ry * rad / hw * math::sin(a),
            crate::pot::Shape::Oval => SOIL_Y + depth * t + ry * math::sin(a),
        };
        (CENTER_X + rad * math::cos(a), y)
    };
    let base = hash01(seed, 0x6b69) * 360.0;
    // The wall's shade steps, lit from the left, mirrored for a left tree.
    let lit_sign = if lit < 0.0 { -1.0 } else { 1.0 };
    let zone = |x: f64| {
        let s = (x - CENTER_X) * lit_sign / hw;
        SHADE.iter().filter(|(f, _)| s >= *f).count()
    };
    let tone_of = |z: usize| if z == 0 { 0.0 } else { SHADE[z - 1].1 };
    let (mut stapled, mut clipped) = ([false; 3], false);
    let mut veins = String::new();
    // The staples, drawn after every crack: one crack never runs over
    // another's staple.
    let mut pins = String::new();
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
        let mut line = PathData::coarse();
        line.m((pts[0].0, pts[0].1));
        for p in &pts[1..] {
            line.l((p.0, p.1));
        }
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
        // Copper staples across the crack about every 20 units, at least
        // two: more on a longer crack. The wall holds 1/1.29 of the crack,
        // the rest runs past the foot; the first staple sits near the rim,
        // where a mender starts, the last a little above the foot.
        let mut staples = Vec::new();
        if mend == Mend::Staples {
            let length: f64 = pts
                .windows(2)
                .map(|w| (math::sq(w[1].0 - w[0].0) + math::sq(w[1].1 - w[0].1)).sqrt())
                .sum();
            let (first, last) = (0.11 / 1.29, 0.89 / 1.29);
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a crack's length is positive and a few hundred at most"
            )]
            let count = (length / 1.29 / 20.0).round().max(2.0) as usize;
            for j in 0..count {
                let f = first + (last - first) * j as f64 / (count - 1) as f64;
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a share of the crack's points"
                )]
                let k = (f * (n - 1) as f64).round() as usize;
                let (pb, nb) = (pts[k.saturating_sub(1)], pts[(k + 1).min(n - 1)]);
                let turn = math::deg(math::atan2(nb.1 - pb.1, nb.0 - pb.0)) + 90.0;
                staples.push((pts[k].0, pts[k].1, turn));
            }
        }
        // The seam takes the wall's shade: on each step into shade it is
        // darker and duller, cut where the glaze steps.
        let mut zones = [false; 3];
        for p in &pts {
            zones[zone(p.0)] = true;
        }
        let split = zones.iter().filter(|z| **z).count() > 1;
        for z in (0..3).filter(|z| zones[*z]) {
            let tone = tone_of(z);
            let metal = |c: u32| Rgb::hex(c).shift(tone * MEND_SHADE, tone * MEND_SHADE * 0.2);
            let mut s = String::new();
            match mend {
                Mend::Gold => {
                    let _ = write!(
                        s,
                        r#"<path d="{}" fill="{}" opacity=".5"/><path d="{}" fill="{}"/><path d="{}" fill="none" stroke="{}" stroke-width=".35" opacity=".85"/>"#,
                        outline(0.55).as_str(),
                        glaze.shift(-0.16 + tone, 0.0),
                        outline(0.0).as_str(),
                        metal(0xc9_972f),
                        highlight.as_str(),
                        metal(0xf6_dc8a)
                    );
                }
                Mend::Silver => {
                    let pewter = metal(if dark { 0xb9_bab5 } else { 0xa9_aaa6 });
                    let _ = write!(
                        s,
                        r##"<path d="{l}" fill="none" stroke="#1b1311" stroke-width="2" stroke-linejoin="round" opacity=".8"/><path d="{l}" fill="none" stroke="{pewter}" stroke-width=".9" stroke-linejoin="round"/>"##,
                        l = line.as_str()
                    );
                }
                Mend::Crack | Mend::Staples => {
                    // A hairline in a deeper shade of the clay.
                    let _ = write!(
                        s,
                        r#"<path d="{}" fill="none" stroke="{}" stroke-width=".8" stroke-linejoin="round"/>"#,
                        line.as_str(),
                        glaze.mix(Rgb::hex(0x1a_0f0a), 0.6).shift(tone, 0.0)
                    );
                }
            }
            if split {
                clipped = true;
                let _ = write!(veins, r#"<g clip-path="url(#kz{z})">{s}</g>"#);
            } else {
                veins.push_str(&s);
            }
        }
        // A staple whole, over the crack, in the shade where it sits.
        for (x, y, turn) in staples {
            let z = zone(x);
            stapled[z] = true;
            let _ = write!(
                pins,
                r##"<use href="#stp{}" transform="translate({} {})rotate({})"/>"##,
                STEP_ID[z],
                Num::coarse(x),
                Num::coarse(y),
                Num::coarse(turn)
            );
        }
    }
    veins.push_str(&pins);
    if veins.is_empty() {
        return String::new();
    }
    let mut defs = String::new();
    // One staple per shade step, drawn once: two holes in the clay and a
    // muted copper bridge across the crack.
    let hole = glaze.mix(Rgb::hex(0x1a_0f0a), 0.7);
    for z in (0..3).filter(|z| stapled[*z]) {
        let tone = tone_of(z);
        let metal = |c: u32| Rgb::hex(c).shift(tone * MEND_SHADE, tone * MEND_SHADE * 0.2);
        let h = hole.shift(tone, 0.0);
        let _ = write!(
            defs,
            r#"<g id="stp{}"><circle cx="-3" r=".75" fill="{h}"/><circle cx="3" r=".75" fill="{h}"/><path d="M-3 0V-.4H3V0" fill="none" stroke="{}" stroke-width="1.15" stroke-linejoin="round"/><path d="M-2.4-.75H2.4" stroke="{}" stroke-width=".35" opacity=".8"/></g>"#,
            STEP_ID[z],
            metal(0x8a_5a3c),
            metal(0xc4_906a)
        );
    }
    if clipped {
        let bounds = [-3.0, SHADE[0].0, SHADE[1].0, 3.0];
        for z in 0..3 {
            let (a, b) = (
                CENTER_X + lit_sign * bounds[z] * hw,
                CENTER_X + lit_sign * bounds[z + 1] * hw,
            );
            let _ = write!(
                defs,
                r#"<clipPath id="kz{z}"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath>"#,
                Num(a.min(b)),
                Num(SOIL_Y - ry - 40.0),
                Num((b - a).abs()),
                Num(depth + 2.0 * ry + 120.0)
            );
        }
    }
    if !defs.is_empty() {
        veins.insert_str(0, &format!("<defs>{defs}</defs>"));
    }
    let wall = pot.wall();
    // On the pot's wall, which stands as high as the pot does.
    crate::pot::lift(
        pot,
        format!(
            r#"<clipPath id="pc"><path d="{}"/></clipPath><g clip-path="url(#pc)">{veins}</g>"#,
            wall.as_str()
        ),
    )
}

/// The class a furin of tier `t` sways by in the wind, each finer one
/// faster: the clear glass slowly, the blue nearly as fast as the golden.
fn furin_sway(t: u8) -> &'static str {
    match t {
        2 => "fw",
        3 => "fm",
        _ => "fs",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every furin sways, each finer one faster: the clear glass slowly,
    /// the blue nearly as fast as the golden one.
    #[test]
    fn every_furin_sways_each_finer_one_faster() {
        let period = |t: u8| {
            let class = furin_sway(t);
            let rule = crate::render::ANIMATION_CSS
                .split(&format!(".{class}{{"))
                .nth(1)
                .expect(class);
            let at = rule.find("s ease").unwrap();
            rule[..at]
                .rsplit(' ')
                .next()
                .unwrap()
                .parse::<f64>()
                .unwrap()
        };
        let (clear, blue, golden) = (period(2), period(3), period(4));
        assert!(clear > blue && blue > golden, "{clear} {blue} {golden}");
        assert!(blue - golden < 0.6, "{blue} {golden}");
        assert_eq!(golden, 2.4);
    }

    /// A galaxy plant peeks: its stem always behind the trunk and some of
    /// it too, most of it showing, the petunia's centre always in view.
    #[test]
    fn a_galaxy_plant_peeks_with_its_stem_behind_the_trunk() {
        let spot = |x: f64, seen: f64, tucked: bool| Spot {
            at: (x, 319.0),
            seen,
            crossed: 0.0,
            tucked,
            centred: true,
        };
        // Out in full with its stem showing, or peeking with it tucked.
        let spots = [
            spot(3.0, 0.97, false),
            spot(19.0, 0.8, true),
            spot(88.0, 0.19, true),
        ];
        assert_eq!(peek(&spots).0, Some((19.0, 319.0)));
        // Of the spots that peek, the one closest to [`PEEK`].
        let spots = [
            spot(3.0, 0.93, true),
            spot(19.0, 0.78, true),
            spot(88.0, 0.63, true),
        ];
        assert_eq!(peek(&spots).0, Some((19.0, 319.0)));
        // A spot that hides the flower's centre does not peek.
        let mut hid = [spot(3.0, 0.8, true), spot(19.0, 0.74, true)];
        hid[0].centred = false;
        assert_eq!(peek(&hid).0, Some((19.0, 319.0)));
        // With other wood across every peeking spot, none is clear; the one
        // it crosses least still keeps its stem behind the trunk.
        let mut crossed = [
            spot(3.0, 0.97, false),
            spot(19.0, 0.8, true),
            spot(88.0, 0.8, true),
        ];
        crossed[1].crossed = 0.3;
        crossed[2].crossed = 0.1;
        assert_eq!(peek(&crossed), (None, Some((88.0, 319.0))));
        // Nothing peeks: of the spots with the stem tucked, one that keeps
        // the centre in view wins over one closer to [`PEEK`].
        let mut last = [spot(3.0, 0.55, true), spot(19.0, 0.3, true)];
        last[0].centred = false;
        assert_eq!(peek(&last), (None, Some((19.0, 319.0))));
    }

    /// Pull Shark x2 lays an ivory white shark trinket on the soil, in place
    /// of the painted goldfish.
    #[test]
    fn pull_shark_x2_is_an_ivory_white_shark() {
        let config = crate::Config::default();
        let b = Bonsai::grow(
            &crate::fixtures::by_name("comeback").unwrap().snapshot,
            &config,
            None,
        );
        assert_eq!(b.achievements.get("pull-shark"), Some(&2));
        let svg = crate::render::svg(&b, &config, crate::render::Theme::Light);
        assert!(svg.contains(r##"href="#ivoryshark""##));
        assert!(!svg.contains("fish2"));
    }

    /// Every staple lies over every crack: none is drawn under a crack that
    /// runs across it.
    #[test]
    fn staples_lie_over_every_crack() {
        let pot = Pot {
            hw: 120.0,
            ry: 22.0,
            depth: 80.0,
            bw: 96.0,
            glaze: Rgb::hex(0x6b_4e3d),
            shape: crate::pot::Shape::Oval,
            level: 2,
        };
        let names: Vec<String> = (0..12).map(|i| format!("repo{i}")).collect();
        let svg = kintsugi(&names, 7, &pot, false, 1.0);
        let first = svg.find(r##"href="#stp"##).expect("staples");
        assert!(
            !svg[first..].contains(r#"stroke-width=".8""#),
            "a crack over a staple"
        );
    }

    /// A crack in the brown clay oval gets a copper staple about every 20
    /// units, at least two: a mend, not a seam of stitches.
    #[test]
    fn staples_sit_about_twenty_apart() {
        let pot = Pot {
            hw: 120.0,
            ry: 22.0,
            depth: 80.0,
            bw: 96.0,
            glaze: Rgb::hex(0x6b_4e3d),
            shape: crate::pot::Shape::Oval,
            level: 2,
        };
        let names: Vec<String> = (0..12).map(|i| format!("repo{i}")).collect();
        let svg = kintsugi(&names, 7, &pot, false, 1.0);
        // A crack split by the wall's shade is one crack: count its path once.
        let mut paths: Vec<&str> = svg
            .split(r#"<path d=""#)
            .skip(1)
            .filter(|p| p.contains(r#"stroke-width=".8""#))
            .map(|p| p.split('"').next().unwrap())
            .collect();
        paths.sort_unstable();
        paths.dedup();
        let cracks = paths.len();
        let staples = svg.matches(r##"href="#stp"##).count();
        assert!(cracks > 0, "{svg}");
        // About 80 deep, so about four to a crack.
        assert!(
            staples >= 2 * cracks && staples <= 5 * cracks,
            "{staples} staples on {cracks} cracks"
        );
    }

    /// A mend takes the wall's shade: gold on the shaded side is darker and
    /// duller than on the lit side, and the shade follows the light.
    #[test]
    fn gold_dims_on_the_shaded_side() {
        let pot = Pot {
            hw: 120.0,
            ry: 22.0,
            depth: 80.0,
            bw: 96.0,
            glaze: Rgb::hex(0x2e_5a8c),
            shape: crate::pot::Shape::Oval,
            level: 3,
        };
        let names: Vec<String> = (0..12).map(|i| format!("repo{i}")).collect();
        let gold = Rgb::hex(0xc9_972f).to_oklch();
        let golds = |lit: f64| {
            let svg = kintsugi(&names, 7, &pot, false, lit);
            let mut fills: Vec<String> = Vec::new();
            for part in svg.split(r##"fill="#"##).skip(1) {
                let hex = &part[..6];
                if !fills.iter().any(|f| f == hex) {
                    fills.push(hex.to_string());
                }
            }
            (svg, fills)
        };
        let (right, fills) = golds(1.0);
        let shaded: Vec<_> = fills
            .iter()
            .map(|f| Rgb::hex(u32::from_str_radix(f, 16).unwrap()).to_oklch())
            .filter(|c| (c.h - gold.h).abs() < 15.0 && c.l < gold.l - 0.05)
            .collect();
        assert!(!shaded.is_empty(), "no dimmed gold in {fills:?}");
        assert!(shaded.iter().all(|c| c.c < gold.c), "dimmed gold is duller");
        let (left, _) = golds(-1.0);
        assert_ne!(right, left, "the shade follows the light");
    }

    /// A staple is drawn once, whole, on whichever step of shade its crack
    /// crosses there: the light's side never adds or drops one.
    #[test]
    fn each_staple_is_drawn_once() {
        let pot = Pot {
            hw: 120.0,
            ry: 22.0,
            depth: 80.0,
            bw: 96.0,
            glaze: Rgb::hex(0x6b_4e3d),
            shape: crate::pot::Shape::Oval,
            level: 2,
        };
        let names: Vec<String> = (0..12).map(|i| format!("repo{i}")).collect();
        for lit in [1.0, -1.0] {
            let svg = kintsugi(&names, 7, &pot, false, lit);
            let mut at: Vec<&str> = svg
                .split("<use")
                .skip(1)
                .map(|u| u.split("transform=").nth(1).unwrap())
                .collect();
            let all = at.len();
            at.sort_unstable();
            at.dedup();
            assert!(all > 0);
            assert_eq!(at.len(), all, "a staple drawn twice, lit {lit}");
        }
    }

    /// Every soil egg, asked for anywhere on the soil, the front edge too,
    /// on the bowl and on each rectangle: its sides on the soil, and at
    /// least 60% of it above the rim's front edge, the rest behind the rim.
    /// One asked for at the front stays there, partly behind the rim,
    /// instead of moving back.
    #[test]
    fn soil_eggs_show_at_least_sixty_percent_above_the_rim() {
        let w = WORLD;
        // (half-width, down, up) of each soil egg as drawn at full size.
        let mut eggs = vec![
            (4.0 * 1.1, 1.0 * 1.1, 2.0 * 1.1),
            (7.0 * 1.25, 2.1 * 1.25, 6.0 * 1.25),
            (8.0 * 1.5, 2.0 * 1.5, 4.0 * 1.5),
            (4.4 * 1.5, 2.2 * 1.5, 2.2 * 1.5),
            (3.8 * 1.25, 1.9 * 1.25, 1.9 * 1.25),
        ];
        for half in [5.5, 6.5, 9.0, 12.0] {
            eggs.push((half * 1.3, 3.0 * 1.3, 4.0 * 1.3));
        }
        let ground = (&[][..], &[][..]);
        for (shape, level) in [
            (crate::pot::Shape::Bowl, 7),
            (crate::pot::Shape::Rect, 8),
            (crate::pot::Shape::Rect, 9),
            (crate::pot::Shape::Rect, 10),
        ] {
            let pot = Pot {
                hw: 110.0,
                ry: 20.0,
                depth: 50.0,
                bw: 90.0,
                glaze: Rgb::hex(0),
                shape,
                level,
            };
            let mut partly = false;
            for &(r, down, up) in &eggs {
                let (r, down, up) = (r * w, down * w, up * w);
                for x in [-0.6, -0.3, 0.0, 0.3, 0.6] {
                    for y in [0.0, 0.5, 1.0, 1.5] {
                        let at = (CENTER_X + x * pot.hw, SOIL_Y + y * pot.ry);
                        let Some((ex, ey)) =
                            settle(&mut Vec::new(), at, (r, down, up), &pot, ground)
                        else {
                            continue;
                        };
                        let shown = (pot.rim_front_y(ex) - (ey - up)) / (up + down);
                        assert!(
                            shown >= 0.6 - 1e-9,
                            "{level}: an egg {r} wide at {ex} {ey} shows {shown:.2}"
                        );
                        assert!(
                            (ex - CENTER_X).abs() + r <= pot.soil_reach().0,
                            "{level}: an egg {r} wide at {ex} reaches past the side"
                        );
                        partly |= shown < 0.99;
                    }
                }
            }
            assert!(partly, "{level}: no egg sits partly behind the rim");
        }
    }

    /// Soil eggs never touch: each settles clear of those before it, even
    /// crowded on a small pot.
    #[test]
    fn soil_eggs_never_touch() {
        let pot = Pot {
            hw: 70.0,
            ry: 12.0,
            depth: 20.0,
            bw: 50.0,
            glaze: Rgb::hex(0),
            shape: crate::pot::Shape::Oval,
            level: 3,
        };
        let ground = (&[][..], &[][..]);
        let mut soil = Vec::new();
        for (dx, r) in [
            (30.0, 12.0),
            (10.0, 7.0),
            (-18.0, 13.0),
            (-40.0, 9.0),
            (20.0, 5.0),
        ] {
            assert!(
                settle(
                    &mut soil,
                    (CENTER_X + dx, SOIL_Y),
                    (r, 0.5 * r, r),
                    &pot,
                    ground
                )
                .is_some(),
                "no room for an egg of {r} at {dx}"
            );
        }
        assert_eq!(soil.len(), 5);
        // A pot with no room left lays no egg on another.
        let mut full = vec![(CENTER_X, SOIL_Y, 60.0)];
        assert_eq!(
            settle(
                &mut full,
                (CENTER_X, SOIL_Y),
                (10.0, 5.0, 10.0),
                &pot,
                ground
            ),
            None
        );
        assert_eq!(full.len(), 1);
        for (i, a) in soil.iter().enumerate() {
            for b in &soil[i + 1..] {
                let d = (math::sq(a.0 - b.0) + math::sq(a.1 - b.1)).sqrt();
                assert!(d >= a.2 + b.2 - 1e-9, "{a:?} touches {b:?}");
            }
        }
    }

    /// A soil egg lies on open soil: off the trunk's foot, and short of the
    /// rim's lip at the front.
    #[test]
    fn soil_eggs_lie_clear_of_the_trunk_and_the_lip() {
        let pot = Pot {
            hw: 120.0,
            ry: 20.0,
            depth: 30.0,
            bw: 90.0,
            glaze: Rgb::hex(0),
            shape: crate::pot::Shape::Oval,
            level: 3,
        };
        let trunk = [vec![
            (CENTER_X, SOIL_Y + 12.0, 10.0),
            (CENTER_X, SOIL_Y - 60.0, 8.0),
        ]];
        let ground = (&trunk[..], &[][..]);
        let (x, _) = settle(
            &mut Vec::new(),
            (CENTER_X, SOIL_Y + 6.0),
            (4.0, 2.0, 4.0),
            &pot,
            ground,
        )
        .unwrap();
        assert!((x - CENTER_X).abs() >= 14.0, "on the trunk at {x}");
        // Asked for at the very front, it lies back from the lip.
        let (r, d) = (6.0, 3.0);
        let (x, y) = settle(
            &mut Vec::new(),
            (CENTER_X + 30.0, SOIL_Y + 19.0),
            (r, d, 2.0 * d),
            &pot,
            ground,
        )
        .unwrap();
        let lip = SOIL_Y + (pot.ry - 1.2) * (1.0 - math::sq((x - CENTER_X) / pot.hw)).sqrt();
        assert!(y + d <= lip - 2.0, "on the lip: {y} against {lip}");
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

    /// A hanging egg's string starts in the wood as drawn, never in the air
    /// beside it.
    #[test]
    fn a_string_hangs_from_the_underside_of_drawn_wood() {
        let limb = [vec![(0.0, 100.0, 4.0), (50.0, 100.0, 4.0)]];
        let wood = [&limb[..]];
        // Just inside the underside, under the point it was asked for.
        assert_eq!(underside(&wood, 20.0, 98.0), Some(102.4));
        // Off the wood: in the air above it, or past its end.
        assert_eq!(underside(&wood, 20.0, 88.0), None);
        assert_eq!(underside(&wood, 60.0, 100.0), None);
    }
}
