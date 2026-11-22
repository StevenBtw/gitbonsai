//! Wood: how an account's history becomes a tree.
//!
//! The account is read as calendar years, months and languages ([`life`]).
//! Languages with a large lifetime share become trunks ([`trunks`]). The
//! newest years are a domed crown of clouds, older ones low era limbs, and a
//! pause a bare window of trunk ([`dome`]); a first-year account is a single
//! seedling stem ([`seedling`]). Age shows as girth, spread and fullness,
//! never as height. A tree is composed from its whole history each time, so
//! it is restyled as it matures; its history is never erased.

pub mod clouds;
pub mod dome;
pub mod life;
pub mod seedling;
#[cfg(test)]
pub(crate) mod testkit;
pub mod trunks;
pub mod wood;

use crate::date::Date;
use crate::style::Leader;
use crate::timeline::Timeline;

/// The golden angle in degrees.
pub const GOLDEN: f64 = 137.507_764;

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
    /// How bleached dead wood ([`JIN`]) is, from 0 (still bark-brown, just
    /// died) to 1 (white).
    pub bleach: f64,
    /// Part of a strand along the trunk ([`crate::growth::trunks::Strand`]).
    pub strand: bool,
}

/// The grown wood.
#[derive(Debug, Clone, Default)]
pub struct Skeleton {
    /// Every node, parents before children.
    pub nodes: Vec<Node>,
    /// Where the crown sits: the leader's tip.
    pub apex: P,
    /// Trunk height in world units.
    pub height: f64,
    /// Depth of each node (toward the viewer is positive); empty means flat.
    pub node_z: Vec<f64>,
    /// Domed foliage clouds.
    pub clouds: Vec<Cloud>,
    /// The account has been silent for a year: the tree stands bare.
    pub dormant: bool,
    /// Strands wound round a trunk while the tree grows: each wound node
    /// with the trunk node it lies on and its angle around it, then the
    /// strand's free nodes. [`wood::hug`] lays them on the trunk and empties
    /// it, so a grown tree holds none.
    pub wound: Vec<(Vec<(usize, usize, f64)>, Vec<usize>)>,
    /// The language that founded the trunk was dropped: a shari, a strip of
    /// dead wood up the main trunk, bleached this far (0 to 1).
    pub shari: Option<f64>,
    /// How far a cascade's legacy trunk has turned to white deadwood, 0 to
    /// 1 (kengai with a second language's living strand along it).
    pub legacy_jin: f64,
    /// The dropped languages whose dead wood the tree shows: a dead limb,
    /// twig, stub or strand, or the shari. The legend names only these.
    pub deadwood: std::collections::BTreeSet<String>,
}

/// The language that marks deadwood (jin) in the skeleton.
pub const JIN: &str = "__jin__";

/// A flat-bottomed, domed foliage cloud.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cloud {
    /// Center of the flat bottom, on the canvas.
    pub c: P,
    /// Depth of the center.
    pub z: f64,
    /// Half-width.
    pub rx: f64,
    /// Dome height.
    pub ry: f64,
    /// Half-depth.
    pub rz: f64,
    /// Color patches: anchor (canvas), anchor depth, language, weight.
    pub patches: Vec<(P, f64, Option<String>, f64)>,
}

impl Skeleton {
    /// Mirrors the skeleton about the pot's center line: wood, apex and clouds.
    pub fn mirror(&mut self) {
        let m = |x: &mut f64| *x = crate::style::CENTER_X - (*x - crate::style::CENTER_X);
        for n in &mut self.nodes {
            m(&mut n.p.0);
        }
        m(&mut self.apex.0);
        for c in &mut self.clouds {
            m(&mut c.c.0);
            for p in &mut c.patches {
                m(&mut p.0.0);
            }
        }
    }

    /// Scales the grown tree by `f` about its foot on the soil: its wood's
    /// lengths, its clouds and its height; the wood keeps its thickness.
    pub fn scale(&mut self, f: f64) {
        let (cx, cy) = (crate::style::CENTER_X, crate::style::SOIL_Y);
        let s = |p: &mut P| *p = (cx + (p.0 - cx) * f, cy + (p.1 - cy) * f);
        for n in &mut self.nodes {
            s(&mut n.p);
        }
        for z in &mut self.node_z {
            *z *= f;
        }
        s(&mut self.apex);
        self.height *= f;
        for c in &mut self.clouds {
            s(&mut c.c);
            c.z *= f;
            c.rx *= f;
            c.ry *= f;
            c.rz *= f;
            for p in &mut c.patches {
                s(&mut p.0);
                p.1 *= f;
            }
        }
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

/// Grows the skeleton of `timeline`, seeded by the account id; `periods`
/// declare languages for private work (first day, last day, languages);
/// `past` is the furthest the tree reached before, which it never falls
/// short of.
pub fn grow(
    timeline: &Timeline,
    seed: u64,
    leader: &Leader,
    periods: &[(Date, Date, Vec<String>)],
    past: dome::Reach,
    recent: Option<dome::Recent>,
) -> Skeleton {
    dome::grow(timeline, seed, leader, periods, past, recent)
}
