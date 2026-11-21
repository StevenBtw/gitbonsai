//! A timelapse paced by change: each frame shows about the same amount of
//! growth, so quiet years pass quickly and busy ones get room.
//!
//! A tree's shape is its wood, weighted by girth, and its leaf pads, weighted
//! by the foliage each language has left, laid on a coarse grid. Seasons only
//! change how the leaves are painted, so they never count as change.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "cells and steps on a 640-unit canvas, never negative"
)]

use crate::Bonsai;
use crate::config::Hemisphere;
use crate::growth::JIN;
use crate::scene::radius;
use crate::style::{CENTER_X, SOIL_Y};

/// Cells on each side of the grid.
const GRID: usize = 48;
/// The canvas the grid covers: wide enough for a windswept crown, tall
/// enough for an old tree, and below the soil for a cascade.
const LEFT: f64 = CENTER_X - 320.0;
const TOP: f64 = SOIL_Y - 480.0;
const SPAN: f64 = 640.0;

/// The tree's shape on the grid: how much wood and foliage lies in each
/// cell.
pub fn shape(b: &Bonsai) -> Vec<f64> {
    let mut grid = vec![0.0; GRID * GRID];
    let mut put = |x: f64, y: f64, w: f64| {
        let cell = |v: f64, from: f64| {
            // Off the grid counts at its edge.
            (((v - from) / SPAN * GRID as f64).floor().max(0.0) as usize).min(GRID - 1)
        };
        grid[cell(y, TOP) * GRID + cell(x, LEFT)] += w;
    };
    let sk = &b.skeleton;
    let flow = sk.flows();
    for (i, n) in sk.nodes.iter().enumerate() {
        let Some(p) = n.parent else { continue };
        let (a, c) = (sk.nodes[p].p, n.p);
        let len = (crate::math::sq(c.0 - a.0) + crate::math::sq(c.1 - a.1)).sqrt();
        let steps = (len / 4.0).ceil().max(1.0);
        // Wood covers its width along its length.
        let w = 2.0 * radius(flow[i]) * len / steps;
        for k in 0..steps as usize {
            let t = (k as f64 + 0.5) / steps;
            put(a.0 + (c.0 - a.0) * t, a.1 + (c.1 - a.1) * t, w);
        }
    }
    for cl in &sk.clouds {
        // A pad's foliage: its patches, each as full as its language still is.
        let total: f64 = cl.patches.iter().map(|p| p.3).sum();
        let full = if total > 0.0 {
            cl.patches
                .iter()
                .map(|p| {
                    let f = match p.2.as_deref() {
                        Some(JIN) => 0.0,
                        Some(l) => b.fullness.get(l).copied().unwrap_or(1.0),
                        None => 1.0,
                    };
                    p.3 * f
                })
                .sum::<f64>()
                / total
        } else {
            1.0
        };
        // The dome's face, sampled on a small lattice.
        let area = std::f64::consts::FRAC_PI_2 * cl.rx * cl.ry;
        let mut pts = Vec::new();
        for u in [-0.75, -0.25, 0.25, 0.75] {
            for v in [0.15, 0.45, 0.75] {
                if u * u + v * v <= 1.0 {
                    pts.push((cl.c.0 + u * cl.rx, cl.c.1 - v * cl.ry));
                }
            }
        }
        let w = area * full / pts.len().max(1) as f64;
        for (x, y) in pts {
            put(x, y, w);
        }
    }
    grid
}

/// How far two shapes differ.
pub fn change(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum()
}

/// Whether month `m` (1 to 12) has the tree in full leaf.
pub fn leafy(m: u32, hemisphere: Hemisphere) -> bool {
    m == match hemisphere {
        Hemisphere::North => 7,
        Hemisphere::South => 1,
    }
}

/// The frames to keep, by index, from candidates a month apart: `change[i]`
/// is how far candidate `i` differs from the one before it (the first's is
/// unused), `leafy[i]` whether it shows the tree in full leaf. A frame is
/// kept once enough change has gathered, the threshold set so about
/// `frames` are kept; in a slow month only in full leaf. In a quiet stretch
/// a frame in full leaf is kept when none was for `floor` candidates, so a
/// pause still shows. The first and
/// the last are always kept.
pub fn pick(change: &[f64], leafy: &[bool], frames: usize, floor: usize) -> Vec<usize> {
    let n = change.len();
    if n <= frames.max(2) {
        return (0..n).collect();
    }
    let keep = |t: f64| {
        let mut kept = vec![0];
        let (mut gathered, mut since) = (0.0, 0);
        for i in 1..n - 1 {
            gathered += change[i];
            since += 1;
            // In a slow month the frame waits for full leaf, so quiet years
            // show the same season and don't flicker.
            let due = gathered >= t && (leafy[i] || change[i] >= 0.25 * t);
            if due || (since >= floor && leafy[i]) {
                kept.push(i);
                gathered = 0.0;
                since = 0;
            }
        }
        kept.push(n - 1);
        kept
    };
    // The lowest threshold that keeps no more than asked: more change, fewer
    // frames.
    let (mut lo, mut hi) = (0.0, change.iter().sum::<f64>() + 1.0);
    for _ in 0..60 {
        let mid = f64::midpoint(lo, hi);
        if keep(mid).len() > frames {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    keep(hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Quiet years pass in a frame or so a year; a burst of change gets
    /// most of the frames.
    #[test]
    fn frames_follow_change_not_time() {
        // Ten years a month apart: a busy first two, then quiet.
        let n = 120;
        let change: Vec<f64> = (0..n).map(|i| if i < 24 { 10.0 } else { 0.1 }).collect();
        let leafy: Vec<bool> = (0..n).map(|i| i % 12 == 6).collect();
        let kept = pick(&change, &leafy, 40, 9);
        assert!(kept.len() <= 40, "{} frames", kept.len());
        assert_eq!(kept.first(), Some(&0));
        assert_eq!(kept.last(), Some(&(n - 1)));
        let busy = kept.iter().filter(|&&i| i < 24).count();
        assert!(
            busy >= 20,
            "only {busy} of {} frames in the busy years",
            kept.len()
        );
        // The quiet years still show, about once a year, in full leaf.
        let quiet: Vec<usize> = kept
            .iter()
            .copied()
            .filter(|&i| (24..n - 1).contains(&i))
            .collect();
        assert!(quiet.len() >= 6, "quiet years in {} frames", quiet.len());
        assert!(quiet.iter().all(|&i| leafy[i]), "{quiet:?}");
    }

    /// A short life keeps every month.
    #[test]
    fn a_short_life_keeps_every_frame() {
        let kept = pick(&[0.0, 1.0, 2.0], &[false; 3], 120, 9);
        assert_eq!(kept, vec![0, 1, 2]);
    }

    /// A tree grown on is a changed shape; the same tree is not.
    #[test]
    fn a_tree_changes_as_it_grows_and_not_with_the_season() {
        let f = crate::fixtures::all()
            .into_iter()
            .find(|f| f.name == "veteran")
            .expect("veteran");
        let config = crate::Config::default();
        let day = |s: &str| s.parse::<crate::date::Date>().expect("a day");
        let at = |d: &str| shape(&Bonsai::grow(&f.snapshot, &config, Some(day(d))));
        let (spring, summer) = (at("2024-04-15"), at("2024-07-15"));
        let later = at("2025-07-15");
        let season = change(&spring, &summer);
        let growth = change(&summer, &later);
        assert!(growth > 0.0, "a year's growth shows no change");
        assert!(
            season < growth,
            "a season ({season:.0}) changes more than a year ({growth:.0})"
        );
    }
}
