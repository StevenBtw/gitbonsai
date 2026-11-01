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
    Oklch {
        l: lch.l.clamp(0.50, 0.68),
        c: (lch.c * 0.95).clamp(0.10, 0.16),
        h: maple_hue(lch.h),
    }
    .to_rgb()
}

/// `c` moved along the maple arc (and a little in lightness) until it is at
/// least ΔE 0.07 from every color already placed.
fn distinct(c: Rgb, placed: &[Rgb]) -> Rgb {
    let clear = |x: Rgb| placed.iter().all(|p| p.delta_e(x) >= 0.07);
    if clear(c) {
        return c;
    }
    for step in 1..8u32 {
        for sign in [1.0, -1.0] {
            let mut lch = c.to_oklch();
            // Stay on the arc: move along it, not around the color wheel.
            let arc = (lch.h - 322.0).rem_euclid(360.0) + sign * 16.0 * f64::from(step);
            lch.h = (322.0 + arc.clamp(0.0, 186.0)).rem_euclid(360.0);
            lch.l = (lch.l + sign * 0.03 * f64::from(step)).clamp(0.46, 0.72);
            let x = lch.to_rgb();
            if clear(x) {
                return x;
            }
        }
    }
    c
}

/// Every language's leaf color, and the crown's mix.
#[derive(Debug, Clone, Default)]
pub struct MaplePalette {
    /// Leaf color by language.
    pub colors: BTreeMap<String, Rgb>,
    /// The crown's languages by share of the foliage window.
    pub crown: Vec<(String, f64)>,
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
        let mut placed = vec![MAPLE_GREEN];
        for language in all {
            let c = if let Some(&verbatim) = overrides.get(&language) {
                verbatim
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
        Self { colors, crown }
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
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn rust_becomes_orange_and_javascript_gold() {
        let rust = to_maple(Rgb::hex(0xdea584)).to_oklch().h;
        let js = to_maple(Rgb::hex(0xf1e05a)).to_oklch().h;
        assert!((35.0..70.0).contains(&rust), "{rust}");
        assert!((70.0..100.0).contains(&js), "{js}");
    }
}
