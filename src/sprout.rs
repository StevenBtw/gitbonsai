//! The first contributions: a sprout drawn leaf by leaf.
//!
//! Until [`SPROUT_UNTIL`] lifetime contributions the tree is a sprout: a
//! hooked shoot in its seed coat, then two seed leaves, each in the language
//! of the contribution that opened it, then soft three-lobed juvenile leaves
//! in turned pairs, colored by the two largest shares of the work (private
//! work silver). The stem turns woody, the seed leaves yellow and drop, the
//! low pairs fall and the top gathers a tuft. The sprout grows along the
//! stem of the seedling it becomes and ends at its top, in the same frame
//! and pot, so the hand-over shows no jump.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a handful of leaves on a 512-unit canvas"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::Bonsai;
use crate::color::Rgb;
use crate::growth::life::Lang;
use crate::maple::{MAPLE_GREEN, private_tone};
use crate::paint::{Paints, relax, tube};
use crate::render::Num;
use crate::scene::{Scene, WORLD, project};

/// Lifetime contributions at which the sprout hands over to the seedling.
pub const SPROUT_UNTIL: f64 = 60.0;

/// A juvenile maple leaf, pointing up from its base: soft, three shallow lobes.
const JUV: &str = "M0 0C-1.5-.5-2.3-1.7-1.9-2.8L-2.5-3.1C-1.8-3.9-1.1-4.2-.5-4.3L0-5.6L.5-4.3C1.1-4.2 1.8-3.9 2.5-3.1L1.9-2.8C2.3-1.7 1.5-.5 0 0Z";
/// Young stem green.
const STEM: Rgb = Rgb::hex(0x6f8f3a);
/// A yellowing seed leaf.
const OLD: Rgb = Rgb::hex(0xd2c06a);
/// When each pair of true leaves opens, in lifetime contributions.
const BORN: [f64; 8] = [4.0, 10.0, 18.0, 26.0, 33.0, 38.0, 43.0, 48.0];
/// When each pair drops: the lowest first, the last at the hand-over.
const DROP: [f64; 8] = [35.0, 40.0, 44.0, 47.0, 50.0, 53.0, 56.0, SPROUT_UNTIL];
/// The least a pair stands below the next, as a share of the stem for each
/// unit of leaf size: about a leaf's height.
const PAIR_GAP: f64 = 0.2;
/// Pairs from this one gather into the top tuft; the two below stay low.
const TUFT: usize = 5;

/// Lifetime contributions, as the legend counts them.
pub fn count(b: &Bonsai) -> f64 {
    b.timeline.capped_lifetime()
}

/// Whether `b` is drawn as a sprout.
pub fn is_sprout(b: &Bonsai) -> bool {
    !b.is_empty() && count(b).round() < SPROUT_UNTIL
}

/// Every contribution's language in order, as runs: private work in a
/// declared period split over its languages, as the growth reads it.
fn runs(b: &Bonsai) -> Vec<(Lang, f64)> {
    let mut out = Vec::new();
    for d in &b.timeline.days {
        let declared = b
            .periods
            .iter()
            .find(|(a, z, langs)| *a <= d.date && d.date <= *z && !langs.is_empty())
            .map(|(_, _, langs)| langs);
        out.extend(d.by_language.iter().map(|(l, v)| (Some(l.clone()), *v)));
        match declared {
            Some(langs) => {
                let each = d.unattributed / langs.len() as f64;
                out.extend(langs.iter().map(|l| (Some(l.clone()), each)));
            }
            None => out.push((None, d.unattributed)),
        }
    }
    out.retain(|r| r.1 > 0.0);
    out
}

/// The language of the `k`-th contribution (from 1).
fn nth(runs: &[(Lang, f64)], k: f64) -> Lang {
    let mut acc = 0.0;
    for (l, v) in runs {
        acc += v;
        if acc >= k - 0.5 {
            return l.clone();
        }
    }
    runs.last().and_then(|r| r.0.clone())
}

/// The two largest shares of the work, private work as one, and the first's
/// part of the two.
fn top_two(runs: &[(Lang, f64)]) -> (Option<Lang>, Option<Lang>, f64) {
    let mut by: BTreeMap<Lang, f64> = BTreeMap::new();
    for (l, v) in runs {
        *by.entry(l.clone()).or_default() += v;
    }
    let mut ranked: Vec<(Lang, f64)> = by.into_iter().collect();
    ranked.sort_by(|a, c| c.1.total_cmp(&a.1).then(a.0.cmp(&c.0)));
    let mut it = ranked.into_iter();
    match (it.next(), it.next()) {
        (Some(a), Some(c)) => (Some(a.0), Some(c.0), a.1 / (a.1 + c.1)),
        (Some(a), None) => (Some(a.0), None, 1.0),
        _ => (None, None, 1.0),
    }
}

/// Eases from 0 at `a` to 1 at `z`.
fn ease(a: f64, z: f64, v: f64) -> f64 {
    let t = ((v - a) / (z - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The stem's height in sketch units at `n` contributions.
fn height(n: f64) -> f64 {
    9.0 + 3.3 * ((n - 1.0).max(0.0) / 0.6).sqrt()
}

/// How far the seedling's own drawing has taken over from the juvenile
/// leaves, 0 to 1, at `n` contributions.
pub fn handover(n: f64) -> f64 {
    ease(40.0, 54.0, n)
}

/// The stem at `n` contributions along the seedling stem of `scene`, as
/// points and radii on the canvas, and the canvas units per sketch unit.
fn stem(scene: &Scene, n: f64) -> Option<(Vec<(f64, f64, f64)>, f64)> {
    let mut pts: Vec<(f64, f64, f64, f64)> = scene
        .chains
        .first()?
        .0
        .iter()
        .map(|(p, r)| {
            let (x, y, z) = project(*p);
            (x, y, r * WORLD, z)
        })
        .collect();
    for _ in 0..8 {
        pts = relax(&pts);
    }
    // The stem ends at its top; past it the chain turns into the leaves.
    let peak = (0..pts.len())
        .min_by(|&a, &c| pts[a].1.total_cmp(&pts[c].1))
        .unwrap_or(0);
    pts.truncate(peak + 1);
    let (fx, fy) = (pts.first()?.0, pts.first()?.1);
    let top = pts.last().map_or(1.0, |p| fy - p.1).max(1.0);
    let k = height(n) / height(SPROUT_UNTIL);
    // Straighter while small: the seedling's bends come in as it grows, and
    // from the hand-over on it is the seedling's stem, scaled.
    let xk = k * (k + (1.0 - k) * handover(n));
    let stem = pts
        .iter()
        .map(|p| (fx + (p.0 - fx) * xk, fy + (p.1 - fy) * k, p.2))
        .collect();
    // Sketch units to canvas: the full sprout reaches the seedling's top.
    Some((stem, top / height(SPROUT_UNTIL)))
}

/// Draws the sprout of `b` along the seedling stem of `scene`: the
/// seedling's painted pieces (`seedling`) fade in as the juvenile leaves
/// drop. You also get how many leaves flutter (none unless `animate`), and
/// `None` unless `b` is a sprout.
pub fn draw(
    b: &Bonsai,
    scene: &Scene,
    paints: &Paints,
    seedling: &[(f64, String)],
    animate: bool,
) -> Option<(String, usize)> {
    if !is_sprout(b) {
        return None;
    }
    let n = count(b).round().max(1.0);
    let (stem, u) = stem(scene, n)?;
    let (fx, fy) = (stem[0].0, stem[0].1);
    let k = height(n) / height(SPROUT_UNTIL);
    let w = handover(n);
    let chain = &scene.chains.first()?.0;
    let h = height(n);
    // The point on the stem `y` sketch units up, in sketch units from the foot.
    let at = |y: f64| {
        let want = y * u;
        let mut prev = (stem[0].0, stem[0].1);
        for p in &stem {
            let up = fy - p.1;
            if up >= want {
                let below = fy - prev.1;
                let t = if up > below {
                    (want - below) / (up - below)
                } else {
                    0.0
                };
                let x = prev.0 + (p.0 - prev.0) * t;
                return ((x - fx) / u, -y);
            }
            prev = (p.0, p.1);
        }
        ((prev.0 - fx) / u, -y)
    };

    let shift = paints.leaf_shift;
    let green = STEM.shift(shift, 0.0);
    let runs = runs(b);
    let color = |l: &Lang| match l {
        Some(l) => b.maple.leaf(Some(l), 0.0).shift(shift, 0.0),
        None => private_tone(b.maple.private(), paints.dark),
    };
    let mut out = String::new();
    // Each leaf flutters about its stalk, alternating tempos.
    let mut moving = 0usize;
    let mut sway = || {
        if !animate {
            return "";
        }
        moving += 1;
        if moving % 2 == 1 { "lf" } else { "lg" }
    };
    let _ = write!(
        out,
        r#"<g transform="translate({} {})scale({})">"#,
        Num(fx),
        Num(fy),
        Num(u)
    );
    if n < 2.0 {
        // Hooked, just through the soil, the seed coat still on its tip.
        let _ = write!(
            out,
            r#"<path d="M0 0C0-3.5 2.5-5 2.8-2.8" fill="none" stroke="{green}" stroke-width=".9" stroke-linecap="round"/><ellipse cx="3" cy="-2.2" rx="1.5" ry="1.05" fill="{}"/></g>"#,
            Rgb::hex(0x7a5a3a).shift(shift, 0.0)
        );
        return Some((out, 0));
    }
    out.push_str("</g>");
    // The stem, green until it turns woody.
    let wood = paints.bark.mix(green, 1.0 - ease(20.0, 28.0, n));
    let _ = write!(
        out,
        r#"<path d="{}" fill="{wood}"/>"#,
        tube(&stem, true, true).as_str()
    );
    let _ = write!(
        out,
        r#"<g transform="translate({} {})scale({})">"#,
        Num(fx),
        Num(fy),
        Num(u)
    );
    // Seed leaves, each in the language of the contribution that opened it.
    let yellow = ease(26.0, 31.0, n);
    let old = OLD.shift(shift, 0.0);
    let (cx, cy) = at((h * 0.5).min(8.0));
    let cs = (0.7 + 0.18 * n).min(1.2);
    for (opens, gone, ang) in [(2.0, 42.0, -62.0), (3.0, 33.0, 62.0)] {
        if n >= opens && n < gone {
            let c = color(&nth(&runs, opens)).mix(old, yellow);
            cot(&mut out, (cx, cy), ang, cs, c, sway());
        }
    }
    if (33.0..45.0).contains(&n) {
        cot(&mut out, (9.0, 1.0), 96.0, 0.9, old, "");
    }
    out.push_str("</g>");
    // The seedling's own drawing, scaled to the sprout's height: whole by
    // 46, so it never shows faint behind the juvenile leaves.
    if w > 0.0 {
        let _ = write!(
            out,
            r#"<g opacity="{}" transform="translate({} {})scale({})">"#,
            Num(ease(40.0, 46.0, n)),
            Num(fx * (1.0 - k)),
            Num(fy * (1.0 - k)),
            Num(k)
        );
        for (_, piece) in seedling {
            out.push_str(piece);
        }
        out.push_str("</g>");
    }
    let _ = write!(
        out,
        r#"<g transform="translate({} {})scale({})">"#,
        Num(fx),
        Num(fy),
        Num(u)
    );
    if n >= 4.0 {
        let (x, y) = at(h + 0.3);
        let _ = write!(
            out,
            r#"<circle cx="{}" cy="{}" r=".8" fill="{}"/>"#,
            Num(x),
            Num(y),
            Rgb::hex(0xa8c46a).shift(shift, 0.0)
        );
    }
    // True leaves in turned pairs; later the top ones gather into a tuft,
    // two pairs stay low on the stem, and pair by pair they drop as the
    // seedling's leaves come in.
    let (l1, l2, share) = top_two(&runs);
    let gather = ease(26.0, 40.0, n);
    let fan = [88.0, 64.0, 40.0];
    // The side the seedling grows its other leaves on, where the tuft's
    // second color gathers, so the hand-over keeps each color in place.
    let is_l1 = |t: &crate::scene::Tuft| match &l1 {
        Some(Some(l)) => !t.private && t.color == b.maple.leaf(Some(l), 0.0),
        Some(None) => t.private,
        None => true,
    };
    let others: Vec<f64> = scene
        .tufts
        .iter()
        .filter(|t| !is_l1(t))
        .map(|t| t.p[0])
        .collect();
    // The tuft ends as wide as the seedling's leaves: a two-language
    // seedling's spread about 50 units.
    let (lo, hi) = scene
        .tufts
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), t| {
            let x = project(t.p).0;
            (lo.min(x), hi.max(x))
        });
    let fit = if hi > lo { (hi - lo) / 50.0 } else { 1.0 }.clamp(0.6, 1.1);
    let apex = chain.last().map_or(0.0, |p| p.0[0]);
    let l2_side = if others.iter().sum::<f64>() > apex * others.len() as f64 {
        1.0
    } else {
        -1.0
    };
    // A pair stays where the stem's tip was when it opened: the stem grows
    // on above it, so older pairs stand lower and spaced out, the newest at
    // the top. The stem's growth slows, so a pair keeps the height of the
    // larger leaves below the next, easing in as that one grows to its size:
    // a grown pair's leaves never reach into a new one.
    let open = BORN.iter().take_while(|&&born| n >= born).count();
    let full = |i: usize| 1.25 - 0.05 * i as f64;
    let size = |i: usize| (0.3 + (n - BORN[i]) / 0.6 / 16.0).min(1.0) * full(i);
    let mut at_h: Vec<f64> = (0..open)
        .map(|i| {
            let young = 0.5 + 0.45 * (i + 1) as f64 / BORN.len() as f64;
            (height(BORN[i]) / h).min(0.95).max(young - 0.15)
        })
        .collect();
    for i in (0..open.saturating_sub(1)).rev() {
        let gap = size(i).max(size(i + 1)) * size(i + 1) / full(i + 1);
        at_h[i] = at_h[i].min(at_h[i + 1] - PAIR_GAP * gap);
    }
    let mut leaf = 0usize;
    for (i, born) in BORN.iter().enumerate().take(open) {
        let age = (n - born) / 0.6;
        let tuft = i >= TUFT;
        let grown = at_h[i];
        let f = if tuft {
            grown + (0.88 + 0.025 * (i - TUFT) as f64 - grown) * gather
        } else {
            grown - 0.15 * gather
        };
        // It opens small and grows to its size over about a dozen more.
        let mut s = (0.3 + age / 16.0).min(1.0) * (1.25 - 0.05 * i as f64);
        let mut spread = if i % 2 == 0 {
            62.0 - 3.0 * i as f64
        } else {
            38.0 - 2.0 * i as f64
        };
        if tuft {
            spread += (fan[i - TUFT] - spread) * gather;
            s *= (1.0 + 0.3 * gather) * (1.0 + (fit - 1.0) * gather);
        }
        let (x, y) = at(h * f.min(0.97));
        let mut pair = [0, 3].map(|d| {
            if ((leaf + d) % 10) as f64 >= (10.0 * share).round() {
                l2.as_ref()
            } else {
                l1.as_ref()
            }
        });
        leaf += 6;
        if n >= DROP[i] {
            continue;
        }
        if tuft && gather > 0.5 && pair[0] != pair[1] && (pair[0] == l2.as_ref()) != (l2_side < 0.0)
        {
            pair.swap(0, 1);
        }
        for (side, lang) in [-1.0, 1.0].into_iter().zip(pair) {
            // The light comes from the tree's open side: the leaf away from
            // it a shade darker.
            let c = lang.map_or(MAPLE_GREEN.shift(shift, 0.0), color);
            let c = if side * b.leader.side > 0.0 {
                c.shift(-0.03, 0.0)
            } else {
                c
            };
            blade(&mut out, (x, y), side * spread, s, c, green, sway());
        }
    }
    out.push_str("</g>");
    Some((out, moving))
}

/// Draws a seed leaf at `p`, turned `ang` degrees, `s` long: narrow and
/// unlobed. A `class` sets it fluttering about its base.
fn cot(out: &mut String, p: (f64, f64), ang: f64, s: f64, fill: Rgb, class: &str) {
    let _ = write!(
        out,
        r#"<g transform="translate({} {})"><g{}><path d="M0 0C-1.3-{} -1.2-{} 0-{}C1.2-{} 1.3-{} 0 0Z" transform="rotate({})" fill="{fill}"/></g></g>"#,
        Num(p.0),
        Num(p.1),
        class_attr(class),
        Num(2.0 * s),
        Num(4.6 * s),
        Num(6.0 * s),
        Num(4.6 * s),
        Num(2.0 * s),
        Num(ang)
    );
}

/// Writes ` class="…"`, or nothing for an empty class.
fn class_attr(class: &str) -> String {
    if class.is_empty() {
        String::new()
    } else {
        format!(r#" class="{class}""#)
    }
}

/// Draws a juvenile leaf on a short stalk from `p`, turned `ang` degrees,
/// at size `s`. A `class` sets it fluttering where its stalk leaves the stem.
#[allow(clippy::too_many_arguments, reason = "a leaf's drawing settings")]
fn blade(out: &mut String, p: (f64, f64), ang: f64, s: f64, fill: Rgb, stalk: Rgb, class: &str) {
    let a = crate::math::rad(ang);
    let len = 1.6 + 1.2 * s;
    // Drawn about the stalk's base, so a flutter turns it there.
    let e = (crate::math::sin(a) * len, -crate::math::cos(a) * len);
    let _ = write!(
        out,
        r##"<g transform="translate({} {})"><g{}><path d="M0 0Q{} {} {} {}" fill="none" stroke="{stalk}" stroke-width=".3"/><g transform="translate({} {})rotate({})scale({})"><path d="{JUV}" fill="{fill}"/><path d="M0-.2V-4.6" stroke="#fff" stroke-opacity=".25" stroke-width=".25"/></g></g></g>"##,
        Num(p.0),
        Num(p.1),
        class_attr(class),
        Num(e.0 / 2.0),
        Num(e.1.min(0.0) - 0.5),
        Num(e.0),
        Num(e.1),
        Num(e.0),
        Num(e.1),
        Num(ang * 0.9),
        Num(s)
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;
    use crate::growth::testkit::snap;
    use crate::render::{self, Theme};
    use crate::{Config, Style};

    /// `n` days of one contribution: Python, then private work, three to two.
    fn grown(n: i32) -> Bonsai {
        let start: Date = "2026-07-30".parse().unwrap();
        let days: Vec<(Date, u32, &[(&str, u32)])> = (0..n)
            .map(|k| {
                let lang: &[(&str, u32)] = if k % 5 == 1 || k % 5 == 3 {
                    &[]
                } else {
                    &[("Python", 1)]
                };
                (start.add_days(k), 1, lang)
            })
            .collect();
        Bonsai::grow(
            &snap(&days, start.add_days(n - 1)),
            &Config::default(),
            None,
        )
    }

    fn svg(b: &Bonsai) -> String {
        render::svg(b, &Config::default(), Theme::Light)
    }

    /// Seed leaves as drawn, by their fill.
    fn seed_leaves(svg: &str) -> Vec<&str> {
        svg.split(r#"<path d="M0 0C-1.3-"#)
            .skip(1)
            .filter_map(|p| p.split(r#"fill=""#).nth(1)?.get(..7))
            .collect()
    }

    #[test]
    fn the_stages_arrive_at_their_counts() {
        let one = svg(&grown(1));
        assert!(one.contains("M0 0C0-3.5"), "a hooked shoot");
        assert_eq!(seed_leaves(&one).len(), 0);
        assert_eq!(seed_leaves(&svg(&grown(2))).len(), 1);
        assert_eq!(seed_leaves(&svg(&grown(3))).len(), 2);
        assert!(!svg(&grown(3)).contains(JUV), "no true leaves yet");
        assert!(svg(&grown(5)).contains(JUV), "true leaves");
        assert!(seed_leaves(&svg(&grown(50))).is_empty(), "seed leaves gone");
        assert!(!svg(&grown(60)).contains(JUV), "the seedling took over");
        assert!(grown(1).description().contains("seed coat"));
        assert!(grown(20).description().contains("sprout"));
    }

    #[test]
    fn a_private_contribution_opens_a_silver_seed_leaf() {
        // The second contribution is private, the third Python.
        let svg = svg(&grown(3));
        let silver = private_tone(crate::maple::PRIVATE_GREY, false).to_string();
        let fills = seed_leaves(&svg);
        assert_eq!(fills[0], silver);
        assert_ne!(fills[1], silver);
    }

    #[test]
    fn every_sprout_is_deterministic_and_within_budget() {
        for n in 1..=61 {
            let b = grown(n);
            for theme in Theme::ALL {
                let svg = render::svg(&b, &Config::default(), theme);
                assert_eq!(svg, render::svg(&b, &Config::default(), theme), "{n}");
                let elements = svg.matches('<').count() - svg.matches("</").count();
                assert!(svg.len() <= 215 * 1024, "{n}: {} bytes", svg.len());
                assert!(elements <= 2500, "{n}: {elements} elements");
            }
        }
        // Every style draws the same sprout.
        let mut config = Config::default();
        config.tree.style = Some(Style::Kengai);
        assert!(render::svg(&grown(20), &config, Theme::Dark).contains(JUV));
    }

    /// The opaque pixels above the soil: how many, and their centroid as a
    /// share of the image.
    #[cfg(feature = "png")]
    fn mass(b: &Bonsai) -> (f64, f64, f64) {
        let tree = resvg::usvg::Tree::from_str(&svg(b), &resvg::usvg::Options::default()).unwrap();
        let size = tree.size();
        let scale = 400.0 / size.width();
        let (w, h) = (400, (size.height() * scale).ceil() as u32);
        let mut pix = resvg::tiny_skia::Pixmap::new(w, h).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pix.as_mut(),
        );
        let (_, y0, _, fh) = render::frame(b, &Config::default());
        let soil = ((crate::style::SOIL_Y - 12.0 - y0) / fh * f64::from(h)) as u32;
        let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
        for y in 0..soil.min(h) {
            for x in 0..w {
                if pix.pixel(x, y).is_some_and(|p| p.alpha() > 128) {
                    a += 1.0;
                    cx += f64::from(x);
                    cy += f64::from(y);
                }
            }
        }
        (a, cx / a / f64::from(w), cy / a / f64::from(h))
    }

    /// No jump at the hand-over: across 59, 60 and 61 the frame stays, the
    /// stem top stays put, and what is drawn above the soil keeps its size
    /// and place.
    #[test]
    fn the_hand_over_is_continuous() {
        let frames: Vec<(Bonsai, (f64, f64, f64))> = [59, 60, 61]
            .into_iter()
            .map(|n| {
                let b = grown(n);
                let (s, _) = stem(&Scene::build(&b), f64::from(n)).unwrap();
                (b, s[s.len() - 1])
            })
            .collect();
        assert!(is_sprout(&frames[0].0) && !is_sprout(&frames[1].0));
        for pair in frames.windows(2) {
            let ((a, ta), (b, tb)) = (&pair[0], &pair[1]);
            let moved = (crate::math::sq(ta.0 - tb.0) + crate::math::sq(ta.1 - tb.1)).sqrt();
            assert!(moved < 3.0, "stem top moved {moved:.1}");
            assert_eq!(
                render::frame(a, &Config::default()),
                render::frame(b, &Config::default())
            );
            #[cfg(feature = "png")]
            {
                let ((ma, xa, ya), (mb, xb, yb)) = (mass(a), mass(b));
                assert!(ma > 500.0, "something is drawn");
                assert!((ma - mb).abs() < 0.15 * mb, "drawn area {ma} -> {mb}");
                let shift = (crate::math::sq(xa - xb) + crate::math::sq(ya - yb)).sqrt();
                assert!(shift < 0.02, "centroid moved {shift:.3} of the image");
            }
        }
    }
}
