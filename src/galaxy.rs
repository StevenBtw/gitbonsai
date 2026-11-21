//! Galaxy Brain's two plants, peeking from behind the trunk: a rex begonia
//! "galaxy" leaf (x3) and a night sky petunia (x4). Each is drawn in its own
//! units around its stalk; [`crate::eggs`] places and scales it. Both are the
//! same drawing on every tree, from fixed random keys.
//!
//! Their specks are many: those of one size and opacity share one path, so a
//! leaf of 330 specks costs a handful of elements.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "small counts and radii in hundredths"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::math;
use crate::render::Num;
use crate::rng::Rng;

type P = (f64, f64);

/// Whether `p` lies inside the polygon `poly`.
fn inside(p: P, poly: &[P]) -> bool {
    let mut c = false;
    for (i, &(x1, y1)) in poly.iter().enumerate() {
        let (x2, y2) = poly[(i + 1) % poly.len()];
        if (y1 > p.1) != (y2 > p.1) && p.0 < (x2 - x1) * (p.1 - y1) / (y2 - y1) + x1 {
            c = !c;
        }
    }
    c
}

/// `n` points along a cubic Bézier from `a` (left out) to `d`.
fn bez(a: P, b: P, c: P, d: P, n: usize) -> Vec<P> {
    (1..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            let f = |a: f64, b: f64, c: f64, d: f64| {
                u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
            };
            (f(a.0, b.0, c.0, d.0), f(a.1, b.1, c.1, d.1))
        })
        .collect()
}

/// A value from `choices` by weight.
fn pick(rng: &mut Rng, choices: &[(f64, f64)]) -> f64 {
    let total: f64 = choices.iter().map(|c| c.1).sum();
    let mut at = rng.unit() * total;
    for &(v, w) in choices {
        if at < w {
            return v;
        }
        at -= w;
    }
    choices[choices.len() - 1].0
}

/// A normal draw with spread `sd`.
fn gauss(rng: &mut Rng, sd: f64) -> f64 {
    let u = rng.unit().max(1e-12);
    sd * (-2.0 * math::ln(u)).sqrt() * math::cos(math::TAU * rng.unit())
}

/// Specks gathered by color, opacity and radius: one path each.
#[derive(Default)]
struct Specks(BTreeMap<(&'static str, u8, u8), String>);

impl Specks {
    /// A speck at `p`, radius `r` in hundredths, opacity `op` in tenths.
    fn add(&mut self, p: P, r: f64, op: f64, fill: &'static str) {
        let (rk, ok) = ((r * 100.0).round() as u8, (op * 10.0).round() as u8);
        let d = self.0.entry((fill, rk, ok)).or_default();
        let _ = write!(
            d,
            "M{} {}a{r} {r} 0 1 0 {} 0a{r} {r} 0 1 0 {} 0",
            Num(p.0 - r),
            Num(p.1),
            Num(2.0 * r),
            Num(-2.0 * r),
            r = Num(r),
        );
    }

    fn markup(&self) -> String {
        let mut out = String::new();
        for ((fill, _, op), d) in &self.0 {
            let _ = write!(
                out,
                r#"<path d="{d}" fill="{fill}" opacity="{}"/>"#,
                Num(f64::from(*op) / 10.0)
            );
        }
        out
    }
}

/// The begonia leaf: a lopsided, finely toothed heart, its centre near-black
/// plum with dark veins, a silver speckle field thickening toward the edge
/// and a pink blush round the centre. Its stalk is at the origin, the leaf
/// reaching out to the left, about 20 units.
pub fn begonia() -> String {
    let mut rng = Rng::keyed(0, "galaxy/begonia");
    let mut base: Vec<P> = vec![(1.5, -1.0)];
    for (b, c, d) in [
        ((2.6, -6.4), (-4.0, -9.6), (-9.0, -8.8)),
        ((-14.0, -8.0), (-18.0, -4.2), (-18.4, 0.4)),
        ((-17.6, 4.4), (-12.0, 7.2), (-6.8, 6.6)),
        ((-2.4, 6.0), (2.4, 4.0), (1.5, 1.0)),
    ] {
        let a = base[base.len() - 1];
        base.extend(bez(a, b, c, d, 24));
    }
    // Fine teeth: every other outline point pushed out a little.
    let (cx, cy) = (-8.0, -0.6);
    let edge: Vec<P> = base
        .iter()
        .enumerate()
        .map(|(i, &(a, b))| {
            if i % 2 == 1 {
                let (dx, dy) = (a - cx, b - cy);
                let d = (dx * dx + dy * dy).sqrt().max(1e-9);
                (a + dx / d * 0.35, b + dy / d * 0.35)
            } else {
                (a, b)
            }
        })
        .collect();
    let path = |pts: &mut dyn Iterator<Item = P>| {
        let mut d = String::new();
        for (i, (a, b)) in pts.enumerate() {
            let _ = write!(d, "{}{} {}", if i == 0 { 'M' } else { 'L' }, Num(a), Num(b));
        }
        d.push('Z');
        d
    };
    let mut out = String::from(
        r##"<path d="M3.5 7Q2.2 3 .6 0" stroke="#5a2a36" stroke-width=".6" fill="none"/>"##,
    );
    let _ = write!(
        out,
        r##"<path d="{}" fill="#1c3a26" stroke="#3a1622" stroke-width=".55" stroke-opacity=".55" stroke-linejoin="round"/>"##,
        path(&mut edge.iter().copied())
    );
    // The plum heart round the stalk.
    let _ = write!(
        out,
        r##"<path d="{}" fill="#251520" opacity=".85"/>"##,
        path(
            &mut base
                .iter()
                .map(|&(a, b)| (cx + 0.42 * (a - cx) + 2.6, cy + 0.42 * (b - cy)))
        )
    );
    // Six dark veins from the stalk to the edge.
    let ends: Vec<P> = [14, 30, 46, 62, 78, 90].iter().map(|&k| base[k]).collect();
    let mut veins = String::new();
    for &(a, b) in &ends {
        let _ = write!(
            veins,
            "M.4 0Q{} {} {} {}",
            Num(a * 0.5),
            Num(b * 0.5 - 0.4),
            Num(a * 0.9),
            Num(b * 0.9)
        );
    }
    let _ = write!(
        out,
        r##"<path d="{veins}" stroke="#140b10" stroke-width=".5" fill="none" stroke-linecap="round" opacity=".5"/>"##
    );
    let near_vein = |(a, b): P| {
        ends.iter().any(|&(ex, ey)| {
            let t = ((a * ex + b * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
            (math::sq(a - t * ex) + math::sq(b - t * ey)).sqrt() < 0.45
        })
    };
    let mut specks = Specks::default();
    // The silver galaxy: denser toward the edge, clear of the veins.
    let mut k = 0;
    while k < 330 {
        let p = (rng.range(-18.6, 1.8), rng.range(-9.0, 7.0));
        if !inside(p, &edge) || near_vein(p) {
            continue;
        }
        let dist = (math::sq(p.0 - 0.4) + p.1 * p.1).sqrt() / 16.0;
        if rng.unit() > (dist * 1.5).min(1.0) {
            continue;
        }
        let r = pick(
            &mut rng,
            &[
                (0.08, 26.0),
                (0.12, 28.0),
                (0.16, 22.0),
                (0.22, 13.0),
                (0.32, 7.0),
                (0.45, 4.0),
            ],
        );
        let op = if rng.unit() < 0.5 { 0.8 } else { 1.0 };
        specks.add(p, r, op, "#eef0f2");
        k += 1;
    }
    // The pink blush round the centre.
    let mut k = 0;
    while k < 60 {
        let p = (rng.range(-9.0, 1.0), rng.range(-5.0, 4.0));
        let dist = (math::sq(p.0 - 0.4) + p.1 * p.1).sqrt();
        if inside(p, &edge) && (2.4..6.5).contains(&dist) && !near_vein(p) {
            let r = [0.2, 0.28, 0.36][rng.below(3) as usize];
            let op = if rng.unit() < 0.5 { 0.7 } else { 0.9 };
            specks.add(p, r, op, "#eaa6c8");
            k += 1;
        }
    }
    out.push_str(&specks.markup());
    out
}

/// Points spread over a plant's shape in its own units, for how much of it
/// shows: the begonia leaf (`leaf`) or the petunia.
pub fn footprint(leaf: bool) -> Vec<P> {
    let mut pts = Vec::new();
    for i in -10..=10 {
        for j in -10..=10 {
            let (u, v) = (f64::from(i) / 10.0, f64::from(j) / 10.0);
            if u * u + v * v > 1.0 {
                continue;
            }
            // The petunia's samples fill its five petals, not the circle
            // round them: no phantom flower between the petals counts as shown.
            pts.push(if leaf {
                (-8.5 + 9.5 * u, -1.0 + 7.0 * v)
            } else {
                let r = petunia_r(math::atan2(v, u));
                (r * u, 0.88 * r * v)
            });
        }
    }
    pts
}

/// The petunia's radius in its own units, before the squash.
pub const PETUNIA_R: f64 = 3.0;

/// The petunia's outline radius at angle `th`: five petals round
/// [`PETUNIA_R`], each edge a little ruffled.
fn petunia_r(th: f64) -> f64 {
    PETUNIA_R * (0.86 + 0.14 * math::cos(5.0 * th + 0.3) + 0.035 * math::sin(17.0 * th))
}

/// The petunia: creased magenta-purple petals in three depths, five soft
/// white patches and glowing stars clustered across it, a small dark throat.
/// Its throat is at the origin, about 6 units across; its stem hangs down
/// to the right, with one leaf unless the plant is a small one (`leaf`).
pub fn petunia(leaf: bool) -> String {
    let mut rng = Rng::keyed(0, "galaxy/petunia");
    let big_r = petunia_r;
    let pt = |f: f64, th: f64| {
        let r = f * big_r(th);
        (r * math::cos(th), 0.88 * r * math::sin(th))
    };
    let outline = |f: f64| {
        let mut d = String::new();
        for i in 0..120 {
            let (a, b) = pt(f, math::TAU * f64::from(i) / 120.0);
            let _ = write!(d, "{}{} {}", if i == 0 { 'M' } else { 'L' }, Num(a), Num(b));
        }
        d.push('Z');
        d
    };
    let mut out = String::from(
        r##"<path d="M2.4 8Q1.6 4.4 .3 1" stroke="#4f7d33" stroke-width=".5" fill="none"/>"##,
    );
    if leaf {
        out.push_str(
            r##"<path d="M1.2 5.2C-.6 4-3.6 4.2-4.8 5.9C-3 6.8-.4 6.6 1.2 5.2Z" fill="#5f8f3a"/>"##,
        );
    }
    let _ = write!(
        out,
        r##"<path d="{}" fill="#3b0f6a"/><path d="{}" fill="#55188e"/><path d="{}" fill="#6a22a8" opacity=".85"/>"##,
        outline(1.0),
        outline(0.8),
        outline(0.55)
    );
    // Creases from the throat to the notch between two petals.
    let mut creases = String::new();
    for k in 0..5 {
        let th = math::TAU * (f64::from(k) + 0.5) / 5.0 - 0.3 / 5.0;
        let (ex, ey) = pt(0.95, th);
        let _ = write!(
            creases,
            "M0 0Q{} {} {} {}",
            Num(ex * 0.5 + 0.2),
            Num(ey * 0.5),
            Num(ex),
            Num(ey)
        );
    }
    let _ = write!(
        out,
        r##"<path d="{creases}" stroke="#2c0a50" stroke-width=".16" fill="none" opacity=".8"/>"##
    );
    // A few soft white patches.
    for _ in 0..5 {
        let th = rng.range(0.0, math::TAU);
        let r = rng.range(1.0, 2.2);
        let (x, y) = (r * math::cos(th), 0.88 * r * math::sin(th));
        let _ = write!(
            out,
            r##"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="#e6c8f6" opacity=".45" transform="rotate({} {} {})"/>"##,
            Num(x),
            Num(y),
            Num(rng.range(0.35, 0.6)),
            Num(rng.range(0.25, 0.4)),
            Num(math::deg(th)),
            Num(x),
            Num(y)
        );
    }
    let inside_flower = |(a, b): P| {
        let th = math::atan2(b / 0.88, a);
        (a * a + math::sq(b / 0.88)).sqrt() < big_r(th) * 0.95
    };
    let centres: Vec<P> = (0..4)
        .map(|_| {
            let th = rng.range(0.0, math::TAU);
            let r = rng.range(1.0, 2.2);
            (r * math::cos(th), 0.88 * r * math::sin(th))
        })
        .collect();
    // Glowing stars: a faint halo and a bright core, half of them clustered.
    let mut specks = Specks::default();
    let mut k = 0;
    while k < 85 {
        let p = if rng.unit() < 0.5 {
            let c = centres[rng.below(4) as usize];
            (c.0 + gauss(&mut rng, 0.7), c.1 + gauss(&mut rng, 0.55))
        } else {
            let th = rng.range(0.0, math::TAU);
            let r = rng.range(0.85, 2.9);
            (r * math::cos(th), 0.88 * r * math::sin(th))
        };
        if !inside_flower(p) || (p.0 * p.0 + p.1 * p.1).sqrt() < 0.8 {
            continue;
        }
        let r = pick(
            &mut rng,
            &[
                (0.06, 30.0),
                (0.09, 30.0),
                (0.12, 20.0),
                (0.16, 13.0),
                (0.22, 7.0),
            ],
        );
        // Quiet stars: a faint, tight halo and a soft core, so the flower
        // stays an accent on a dark page too.
        specks.add(p, r * 1.8, 0.2, "#c98af2");
        specks.add(p, r, if rng.unit() < 0.5 { 0.6 } else { 0.8 }, "#fdebff");
        k += 1;
    }
    out.push_str(&specks.markup());
    out.push_str(concat!(
        r##"<ellipse rx=".75" ry=".64" fill="#2c0c48" opacity=".85"/>"##,
        r##"<ellipse rx=".42" ry=".36" fill="#1c0830"/>"##,
        r##"<circle cx=".08" cy=".04" r=".15" fill="#d7e0b0" opacity=".9"/>"##,
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The petunia's footprint lies within its petals, so placement never
    /// counts flower that isn't drawn.
    #[test]
    fn the_petunia_footprint_lies_in_its_petals() {
        for (x, y) in footprint(false) {
            let (u, v) = (x, y / 0.88);
            let r = (u * u + v * v).sqrt();
            assert!(
                r <= petunia_r(math::atan2(v, u)) + 1e-9,
                "({x:.2}, {y:.2}) outside"
            );
        }
    }

    /// Each plant is one fixed drawing of few elements: its specks share
    /// paths.
    #[test]
    fn the_plants_are_light() {
        for (name, svg) in [("begonia", begonia()), ("petunia", petunia(true))] {
            let elements = svg.matches('<').count();
            assert!(elements <= 40, "{name}: {elements} elements");
            assert_eq!(
                svg,
                if name == "begonia" {
                    begonia()
                } else {
                    petunia(true)
                }
            );
        }
    }
}
