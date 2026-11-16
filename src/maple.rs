//! Leaf colors on the Japanese maple palette.
//!
//! Japanese maples come in purple, crimson, orange, gold and green, never in
//! blue. Each language's linguist hue is remapped monotonically onto that arc
//! (cut at blue-violet), so languages keep their order and rough identity,
//! and every leaf stays plausible. Lightness and chroma share one band, so all
//! languages look like the same tree.

use std::collections::BTreeMap;

use crate::color::{self, Oklch, Rgb};
use crate::foliage::Window;

/// The leaf color of contributions without language data: private or
/// corporate work, silver grey.
pub const PRIVATE_GREY: Rgb = Rgb::hex(0xb8bcc2);

/// The leaf color of anything without a language of its own.
pub const MAPLE_GREEN: Rgb = Rgb::hex(0x6f9440);

/// The hue (OKLCH degrees) a linguist hue `h` becomes on the maple arc.
pub fn maple_hue(h: f64) -> f64 {
    // Input hue (unwrapped from 265°) to output hue, piecewise linear.
    const ANCHORS: [(f64, f64); 8] = [
        (265.0, 322.0), // violet: deep purple
        (330.0, 348.0), // magenta: burgundy
        (385.0, 383.0), // red: crimson
        (415.0, 408.0), // orange: orange ('Katsura')
        (455.0, 440.0), // yellow: gold
        (500.0, 480.0), // green: fresh green
        (560.0, 495.0), // teal: green
        (625.0, 508.0), // blue: deep green
    ];
    let x = if h < 265.0 { h + 360.0 } else { h };
    for w in ANCHORS.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        if (x0..=x1).contains(&x) {
            return (y0 + (y1 - y0) * (x - x0) / (x1 - x0)).rem_euclid(360.0);
        }
    }
    ANCHORS[7].1.rem_euclid(360.0)
}

/// A linguist color moved onto the maple palette.
pub fn to_maple(c: Rgb) -> Rgb {
    let lch = c.to_oklch();
    if lch.c < 0.035 {
        // A colorless linguist color (C is #555555) has no hue to keep: it
        // becomes the deep burgundy of a dark-leaved maple.
        return Oklch {
            l: 0.45,
            c: 0.115,
            h: 4.0,
        }
        .to_rgb();
    }
    Oklch {
        l: lch.l.clamp(0.50, 0.68),
        c: (lch.c * 0.95).clamp(0.10, 0.16),
        h: maple_hue(lch.h),
    }
    .to_rgb()
}

/// Languages with a maple color of their own, where the linguist color
/// would land badly on the arc: TypeScript's blue sits right beside
/// Python's, so it keeps a blue-violet of its own, and
/// JavaScript keeps its bright yellow.
fn house(language: &str) -> Option<Rgb> {
    let lch = match language {
        "TypeScript" => Oklch {
            l: 0.55,
            c: 0.14,
            h: 285.0,
        },
        "JavaScript" => Oklch {
            l: 0.83,
            c: 0.16,
            h: 98.0,
        },
        "Java" => Oklch {
            l: 0.56,
            c: 0.17,
            h: 28.0,
        },
        _ => return None,
    };
    Some(lch.to_rgb())
}

/// How far apart (Oklab ΔE) every leaf color stays from every other, and
/// from private work's silver: clearly different at a glance.
const APART: f64 = 0.1;

/// `c`, or the nearest color on the maple arc that is at least [`APART`]
/// from every color already placed: hues along the arc, lightness from deep
/// to bright, at full maple chroma. When the arc has no clear color left
/// (dozens of languages), `c` is kept: a near clash beats an off-palette
/// leaf.
fn distinct(c: Rgb, placed: &[Rgb]) -> Rgb {
    let clear = |x: Rgb| placed.iter().all(|p| p.delta_e(x) >= APART);
    if clear(c) {
        return c;
    }
    let mut best: Option<(f64, Rgb)> = None;
    for step in 0..=46u32 {
        // The arc: purple (322°) through red and orange to green (148°).
        let h = (322.0 + 4.0 * f64::from(step)).rem_euclid(360.0);
        for k in 0..=9u32 {
            let l = 0.44 + 0.04 * f64::from(k);
            let x = Oklch { l, c: 0.14, h }.to_rgb();
            if clear(x) {
                let d = c.delta_e(x);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, x));
                }
            }
        }
    }
    best.map_or(c, |(_, x)| x)
}

/// Every language's leaf color, and the crown's mix.
#[derive(Debug, Clone)]
pub struct MaplePalette {
    /// Leaf color by language.
    pub colors: BTreeMap<String, Rgb>,
    /// The crown's languages by share of the foliage window.
    pub crown: Vec<(String, f64)>,
    /// The color of work without language data.
    pub private: Rgb,
}

/// Private work's leaf tone on a theme: muted at night, where silver would
/// glow.
pub fn private_tone(c: Rgb, dark: bool) -> Rgb {
    if dark { c.shift(-0.18, -0.01) } else { c }
}

impl MaplePalette {
    /// Colors for every language, most active first, so the busiest languages
    /// keep their natural hue and quieter ones yield on clashes. Config colors
    /// are used verbatim.
    pub fn new(
        window: &Window,
        order: &[String],
        linguist: &BTreeMap<String, Option<Rgb>>,
        overrides: &BTreeMap<String, Rgb>,
        private: Rgb,
    ) -> Self {
        let mut ranked: Vec<(&String, f64)> =
            window.by_language.iter().map(|(l, v)| (l, *v)).collect();
        ranked.sort_by(|a, c| c.1.total_cmp(&a.1).then(a.0.cmp(c.0)));
        let mut all: Vec<String> = ranked.iter().map(|(l, _)| (*l).clone()).collect();
        for l in order {
            if !all.contains(l) {
                all.push(l.clone());
            }
        }
        let mut colors = BTreeMap::new();
        let mut placed = vec![MAPLE_GREEN, private];
        for language in all {
            let c = if let Some(&verbatim) = overrides.get(&language) {
                verbatim
            } else if let Some(own) = house(&language) {
                distinct(own, &placed)
            } else {
                let raw = linguist
                    .get(&language)
                    .copied()
                    .flatten()
                    .unwrap_or_else(|| color::fallback(&language));
                distinct(to_maple(raw), &placed)
            };
            placed.push(c);
            colors.insert(language, c);
        }
        let total: f64 = window.by_language.values().sum();
        let crown = window
            .by_language
            .iter()
            .filter(|(_, v)| **v > 0.0)
            .map(|(l, v)| (l.clone(), v / total.max(1e-9)))
            .collect();
        Self {
            colors,
            crown,
            private,
        }
    }

    /// A leaf's color: its language's maple color; crown leaves (no language)
    /// pick a language by share, using `pick` in `[0, 1)`.
    pub fn leaf(&self, language: Option<&str>, pick: f64) -> Rgb {
        match language {
            Some(l) => self.colors.get(l).copied().unwrap_or(MAPLE_GREEN),
            None => {
                let mut acc = 0.0;
                for (l, share) in &self.crown {
                    acc += share;
                    if pick < acc {
                        return self.colors.get(l).copied().unwrap_or(MAPLE_GREEN);
                    }
                }
                MAPLE_GREEN
            }
        }
    }

    /// The color of work GitHub reports without a language.
    pub fn private(&self) -> Rgb {
        self.private
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_becomes_burgundy() {
        let burgundy = Oklch {
            l: 0.45,
            c: 0.115,
            h: 4.0,
        }
        .to_rgb();
        assert_eq!(to_maple(Rgb::hex(0x0055_5555)), burgundy);
        assert_ne!(to_maple(Rgb::hex(0x00de_a584)), burgundy);
    }

    #[test]
    fn private_work_is_silver_and_muted_at_night() {
        let silver = Rgb::hex(0x00b8_bcc2);
        assert_eq!(private_tone(silver, false), silver);
        assert!(private_tone(silver, true).to_oklch().l < silver.to_oklch().l - 0.1);
    }
    use crate::math::angle_between;

    #[test]
    fn no_leaf_is_blue() {
        for h in (0..360).step_by(5) {
            let out = maple_hue(f64::from(h));
            // The maple arc runs from purple (322°) through red to green (148°).
            assert!(angle_between(out, 235.0) > 85.0, "{h} -> {out}");
        }
    }

    #[test]
    fn typescript_is_blue_purple_and_javascript_bright_yellow() {
        let mut window = Window::default();
        for l in ["TypeScript", "JavaScript", "Python"] {
            window.by_language.insert(l.to_string(), 1.0);
        }
        let linguist: BTreeMap<String, Option<Rgb>> = [
            ("TypeScript", 0x0031_78c6),
            ("JavaScript", 0x00f1_e05a),
            ("Python", 0x0035_72a5),
        ]
        .into_iter()
        .map(|(l, c)| (l.to_string(), Some(Rgb::hex(c))))
        .collect();
        let p = MaplePalette::new(&window, &[], &linguist, &BTreeMap::new(), PRIVATE_GREY);
        let ts = p.colors["TypeScript"].to_oklch();
        let js = p.colors["JavaScript"].to_oklch();
        let py = p.colors["Python"].to_oklch();
        assert!(angle_between(ts.h, 285.0) < 10.0 && ts.c > 0.12, "{ts:?}");
        assert!((85.0..110.0).contains(&js.h) && js.l > 0.75, "{js:?}");
        assert!((120.0..170.0).contains(&py.h), "Python stays green: {py:?}");
        let mut pinned = BTreeMap::new();
        pinned.insert("TypeScript".to_string(), Rgb::hex(0x0012_3456));
        let p = MaplePalette::new(&window, &[], &linguist, &pinned, PRIVATE_GREY);
        assert_eq!(p.colors["TypeScript"], Rgb::hex(0x0012_3456), "config wins");
    }

    /// GitHub's colors for a busy polyglot, busiest first.
    const POLYGLOT: [(&str, u32); 10] = [
        ("TypeScript", 0x0031_78c6),
        ("Python", 0x0035_72a5),
        ("Rust", 0x00de_a584),
        ("JavaScript", 0x00f1_e05a),
        ("Go", 0x0000_add8),
        ("Java", 0x00b0_7219),
        ("C", 0x0055_5555),
        ("Swift", 0x00f0_5138),
        ("Ruby", 0x0070_1516),
        ("Shell", 0x0089_e051),
    ];

    fn polyglot() -> MaplePalette {
        let mut window = Window::default();
        let linguist: BTreeMap<String, Option<Rgb>> = POLYGLOT
            .iter()
            .enumerate()
            .map(|(i, (l, c))| {
                window
                    .by_language
                    .insert((*l).to_string(), 100.0 - i as f64);
                ((*l).to_string(), Some(Rgb::hex(*c)))
            })
            .collect();
        MaplePalette::new(&window, &[], &linguist, &BTreeMap::new(), PRIVATE_GREY)
    }

    #[test]
    fn languages_stay_apart_from_the_configured_private_color() {
        let mut window = Window::default();
        window.by_language.insert("Go".into(), 10.0);
        let linguist: BTreeMap<String, Option<Rgb>> =
            [("Go".to_string(), Some(Rgb::hex(0x0000_add8)))].into();
        let natural = MaplePalette::new(&window, &[], &linguist, &BTreeMap::new(), PRIVATE_GREY);
        let go = natural.colors["Go"];
        let p = MaplePalette::new(&window, &[], &linguist, &BTreeMap::new(), go);
        assert_eq!(p.private, go);
        assert!(
            p.colors["Go"].delta_e(go) >= APART,
            "Go moves off the silver"
        );
    }

    #[test]
    fn java_is_fall_red() {
        let java = polyglot().colors["Java"].to_oklch();
        assert!(
            angle_between(java.h, 28.0) < 14.0 && java.c > 0.13,
            "{java:?}"
        );
    }

    #[test]
    fn every_language_is_clearly_apart() {
        let p = polyglot();
        let colors: Vec<(&str, Rgb)> = POLYGLOT.iter().map(|(l, _)| (*l, p.colors[*l])).collect();
        for (i, (a, ca)) in colors.iter().enumerate() {
            assert!(
                ca.delta_e(PRIVATE_GREY) >= 0.1,
                "{a} looks like private work"
            );
            for (b, cb) in &colors[i + 1..] {
                assert!(ca.delta_e(*cb) >= 0.1, "{a} and {b}: {}", ca.delta_e(*cb));
            }
        }
    }

    #[test]
    fn rust_becomes_orange_and_javascript_gold() {
        let rust = to_maple(Rgb::hex(0xdea584)).to_oklch().h;
        let js = to_maple(Rgb::hex(0xf1e05a)).to_oklch().h;
        assert!((35.0..70.0).contains(&rust), "{rust}");
        assert!((70.0..100.0).contains(&js), "{js}");
    }
}
