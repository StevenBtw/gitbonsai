//! Wood: the week-by-week growth of the skeleton.
//!
//! The tree grows by space colonization (Runions, Lane and Prusinkiewicz
//! 2007), one step per complete week:
//!
//! - **Age makes it tall.** The leader rises to `H(age)` along the style's path.
//! - **Each language earns foliage pads.** A language with 50 or more capped
//!   contributions gets pads on its own side of the tree (a golden-angle home),
//!   more as its lifetime grows, at most one new pad per 16 weeks.
//! - **Contributions become attraction points** inside those pads. Your top
//!   language also feeds the crown, as apical dominance does.
//! - **Branches grow toward the points**, a few units per step, and every point
//!   a branch reaches deposits its contributions there.
//! - **Crown recession.** Pads the tree outgrows (below 30% of its height) are
//!   shed; their branches stay as bare wood and the language grows new pads
//!   higher up.
//!
//! Growth only ever appends: no node ever moves, and next week's skeleton is
//! this week's plus new nodes.

use std::collections::{BTreeMap, HashMap};

use crate::math;
use crate::noise::gauss;
use crate::rng::Rng;
use crate::style::{CENTER_X, Leader, SOIL_Y};
use crate::timeline::Timeline;

/// The height curve's time constant, in weeks.
pub const TAU_WEEKS: f64 = 26.0;
/// The age at which the trunk reaches `H_REF`: 25 years.
pub const W_REF: f64 = 1300.0;
/// The reference trunk height, in world units.
pub const H_REF: f64 = 280.0;
/// The golden angle in degrees.
pub const GOLDEN: f64 = 137.507_764;
/// Capped lifetime contributions before a language earns foliage pads.
pub const SPROUT_AT: f64 = 50.0;

/// Growth per step, in world units.
const STEP: f64 = 3.2;
/// How far an attraction point pulls.
const INFLUENCE: f64 = 42.0;
/// Within this distance a point is reached and removed.
const KILL: f64 = 6.0;
/// Spatial hash cell size.
const CELL: f64 = 21.0;
/// The most nodes a tree grows.
const MAX_NODES: usize = 3500;

/// Trunk height after `w` weeks: a log curve that slows but never stops.
pub fn height(w: f64) -> f64 {
    H_REF * math::ln(1.0 + w / TAU_WEEKS) / math::ln(1.0 + W_REF / TAU_WEEKS)
}

/// A point on the canvas.
pub type P = (f64, f64);

/// A node of the skeleton. Its parent always has a smaller index.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Position on the canvas.
    pub p: P,
    /// The node it grew from.
    pub parent: Option<usize>,
    /// Contributions deposited here.
    pub own: f64,
    /// The language whose points it grew toward; `None` for the leader.
    pub language: Option<String>,
}

/// A foliage pad: an ellipse a language's branches grow into.
#[derive(Debug, Clone, PartialEq)]
pub struct Pad {
    /// Center on the canvas.
    pub c: P,
    /// Half-width.
    pub rx: f64,
    /// Half-height.
    pub ry: f64,
    /// Its language.
    pub language: Option<String>,
    /// Shed when the tree outgrew it; its branch stays bare.
    pub retired: bool,
}

/// The grown wood.
#[derive(Debug, Clone, Default)]
pub struct Skeleton {
    /// Every node, parents before children.
    pub nodes: Vec<Node>,
    /// Every pad ever placed, shed ones included.
    pub pads: Vec<Pad>,
    /// Where the crown sits: the leader's tip.
    pub apex: P,
    /// Trunk height in world units.
    pub height: f64,
}

impl Skeleton {
    fn add(&mut self, p: P, parent: Option<usize>, language: Option<String>) -> usize {
        self.nodes.push(Node {
            p,
            parent,
            own: 0.0,
            language,
        });
        self.nodes.len() - 1
    }

    /// The total contributions carried by each node: its own plus everything
    /// above it (the pipe model's flow).
    pub fn flows(&self) -> Vec<f64> {
        let mut flow: Vec<f64> = self.nodes.iter().map(|n| n.own).collect();
        for i in (0..self.nodes.len()).rev() {
            if let Some(p) = self.nodes[i].parent {
                flow[p] += flow[i];
            }
        }
        flow
    }
}

struct Point {
    p: P,
    language: Option<String>,
    amount: f64,
    born: i32,
}

fn unit((x, y): P) -> P {
    let l = (x * x + y * y).sqrt().max(1e-9);
    (x / l, y / l)
}

/// Grows the skeleton over every complete week of `timeline`. `order` is the
/// order languages first appeared in, which fixes each language's home side.
pub fn grow(timeline: &Timeline, seed: u64, leader: &Leader, order: &[String]) -> Skeleton {
    let home = |language: &str| {
        let i = order.iter().position(|l| l == language).unwrap_or(0);
        math::rad(i as f64 * GOLDEN + 30.0)
    };
    #[allow(
        clippy::cast_possible_truncation,
        reason = "grid cells of a 512-unit canvas"
    )]
    let key = |p: P| ((p.0 / CELL).floor() as i32, (p.1 / CELL).floor() as i32);
    let apex_x = |h: f64| CENTER_X + leader.x_at(h);

    let mut sk = Skeleton::default();
    let root = sk.add((CENTER_X, SOIL_Y), None, None);
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    grid.entry(key((CENTER_X, SOIL_Y))).or_default().push(root);
    let mut points: Vec<Point> = Vec::new();
    let mut lifetime: BTreeMap<String, f64> = BTreeMap::new();
    let mut first_seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut active: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut next_pad: BTreeMap<String, usize> = BTreeMap::new();
    let mut h = 0.0;

    for (k, week) in timeline.complete_weeks().iter().enumerate() {
        h = height((k + 1) as f64);
        for (language, amount) in &week.by_language {
            *lifetime.entry(language.clone()).or_default() += amount;
            first_seen.entry(language.clone()).or_insert(k);
        }
        let top = lifetime
            .iter()
            .max_by(|a, c| a.1.total_cmp(c.1))
            .map(|(l, _)| l.clone());

        // Crown recession: pads the tree has outgrown are shed.
        for list in active.values_mut() {
            list.retain(|&i| {
                let keep = sk.pads[i].c.1 < SOIL_Y - 0.3 * h || h < 60.0;
                if !keep {
                    sk.pads[i].retired = true;
                }
                keep
            });
        }

        // New pads, as each language's lifetime grows.
        for (language, &life) in &lifetime {
            if life < SPROUT_AT {
                continue;
            }
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a small pad count"
            )]
            let by_amount = (1.0 + (1.0 + life / 150.0).log2()).floor() as usize;
            let want = by_amount.min(5).min(1 + (k - first_seen[language]) / 16);
            let have = active.get(language).map_or(0, Vec::len);
            for _ in have..want {
                let n = *next_pad
                    .entry(language.clone())
                    .and_modify(|n| *n += 1)
                    .or_insert(0);
                let mut r = Rng::keyed(seed, &format!("pad/{language}/{n}"));
                let v = r.range(0.25, 0.85);
                let y = SOIL_Y - h * v;
                let half_width = h * 0.75 * (1.0 - v).powf(0.8) + 10.0;
                let theta = home(language) + math::rad(n as f64 * GOLDEN);
                let natural = if math::cos(theta) >= 0.0 { 1.0 } else { -1.0 };
                let side = leader.pad_side(natural, r.unit());
                let reach = if Some(language) == top.as_ref() {
                    0.6
                } else {
                    1.0
                };
                let mag = (0.55 + 0.45 * math::cos(theta).abs()) * reach * r.range(0.75, 1.05);
                let mut x = apex_x(SOIL_Y - y) + side * mag * half_width;
                let size = (13.0 + 9.0 * (1.0 + life / 50.0).log10()) * r.range(0.85, 1.2);
                // Keep pads apart: slide outward along their side until clear.
                for _ in 0..10 {
                    let clear = sk.pads.iter().filter(|p| !p.retired).all(|p| {
                        (p.c.0 - x).abs() > 0.75 * (p.rx + size)
                            || (p.c.1 - y).abs() > 1.05 * (p.ry + size * 0.38)
                    });
                    if clear {
                        break;
                    }
                    x += side * 8.0;
                }
                sk.pads.push(Pad {
                    c: (x, y.min(SOIL_Y - 16.0)),
                    rx: size,
                    ry: size * 0.48,
                    language: Some(language.clone()),
                    retired: false,
                });
                active
                    .entry(language.clone())
                    .or_default()
                    .push(sk.pads.len() - 1);
            }
        }

        // The leader rises with age.
        if k % 2 == 0 {
            points.push(Point {
                p: (apex_x(h), SOIL_Y - h - 3.0),
                language: None,
                amount: 0.0,
                born: week.no,
            });
        }
        // This week's contributions, as attraction points.
        let mut rng = Rng::keyed(seed, &format!("week/{}", week.no));
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "at most 5 points"
        )]
        let n = ((week.total.sqrt() * 0.45).min(5.0) + rng.unit()).floor() as usize;
        if n == 0 {
            sk.nodes[root].own += week.total;
        }
        for _ in 0..n {
            let mut pick = rng.unit() * week.total.max(1e-9);
            let mut language = None;
            for (l, v) in &week.by_language {
                if pick < *v {
                    language = Some(l.clone());
                    break;
                }
                pick -= v;
            }
            let amount = week.total / n as f64;
            // Apical dominance: the top language also feeds the crown.
            let pads = language
                .as_ref()
                .and_then(|l| active.get(l))
                .filter(|list| !list.is_empty());
            let to_crown = match (&language, pads) {
                (Some(l), Some(_)) => Some(l) == top.as_ref() && rng.unit() < 0.3,
                _ => true,
            };
            if to_crown || pads.is_none() {
                points.push(Point {
                    p: (
                        apex_x(h) + gauss(&mut rng) * 9.0,
                        SOIL_Y - h - 2.0 + gauss(&mut rng) * 4.0,
                    ),
                    language: None,
                    amount,
                    born: week.no,
                });
            } else if let Some(list) = pads {
                #[allow(clippy::cast_possible_truncation, reason = "at most five pads")]
                let pad = &sk.pads[list[rng.below(list.len() as u32) as usize]];
                let a = rng.range(0.0, math::TAU);
                let rr = rng.unit().sqrt();
                points.push(Point {
                    p: (
                        pad.c.0 + math::cos(a) * rr * pad.rx,
                        pad.c.1 + math::sin(a) * rr * pad.ry,
                    ),
                    language,
                    amount,
                    born: week.no,
                });
            }
        }

        // One colonization step: each point pulls its nearest node.
        let mut pulls: BTreeMap<usize, (P, BTreeMap<Option<String>, usize>)> = BTreeMap::new();
        let mut keep = Vec::with_capacity(points.len());
        #[allow(clippy::cast_possible_truncation, reason = "a handful of cells")]
        let reach = (INFLUENCE / CELL).ceil() as i32;
        for pt in points.drain(..) {
            let (gx, gy) = key(pt.p);
            let mut best: Option<(usize, f64)> = None;
            for dx in -reach..=reach {
                for dy in -reach..=reach {
                    for &i in grid.get(&(gx + dx, gy + dy)).map_or(&[][..], Vec::as_slice) {
                        let q = sk.nodes[i].p;
                        let d = ((q.0 - pt.p.0).powi(2) + (q.1 - pt.p.1).powi(2)).sqrt();
                        if d < INFLUENCE && best.is_none_or(|(_, bd)| d < bd) {
                            best = Some((i, d));
                        }
                    }
                }
            }
            match best {
                Some((i, d)) if d < KILL => sk.nodes[i].own += pt.amount,
                Some((i, _)) => {
                    let q = sk.nodes[i].p;
                    let dir = unit((pt.p.0 - q.0, pt.p.1 - q.1));
                    let entry = pulls.entry(i).or_insert(((0.0, 0.0), BTreeMap::new()));
                    entry.0 = (entry.0.0 + dir.0, entry.0.1 + dir.1);
                    *entry.1.entry(pt.language.clone()).or_default() += 1;
                    keep.push(pt);
                }
                // A point nothing reached within a year feeds the trunk.
                None if week.no - pt.born > 52 => sk.nodes[root].own += pt.amount,
                None => keep.push(pt),
            }
        }
        points = keep;
        for (i, (pull, votes)) in pulls {
            if sk.nodes.len() >= MAX_NODES {
                break;
            }
            let d = unit(pull);
            // A slight upward tropism.
            let dir = unit((d.0, d.1 - 0.15));
            let q = sk.nodes[i].p;
            let np = (q.0 + dir.0 * STEP, q.1 + dir.1 * STEP);
            let language = if sk.nodes[i].language.is_some() {
                sk.nodes[i].language.clone()
            } else {
                votes
                    .into_iter()
                    .max_by_key(|(_, c)| *c)
                    .and_then(|(l, _)| l)
            };
            let j = sk.add(np, Some(i), language);
            grid.entry(key(np)).or_default().push(j);
        }
    }
    for pt in points {
        sk.nodes[root].own += pt.amount;
    }
    sk.apex = (apex_x(h), SOIL_Y - h - 3.0);
    sk.height = h;
    sk
}

#[cfg(test)]
mod tests {
    use super::*;

    fn share(h: f64) -> f64 {
        (h / H_REF * 1000.0).round() / 10.0
    }

    /// The height table.
    #[test]
    fn height_follows_the_documented_curve() {
        assert_eq!(share(height(4.0)), 3.6);
        assert_eq!(share(height(52.0)), 27.9);
        assert_eq!(share(height(260.0)), 61.0);
        assert_eq!(share(height(878.0)), 90.3);
        assert_eq!(share(height(1300.0)), 100.0);
    }
}
