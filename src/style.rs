//! Styles: the leader's path and which side foliage pads favor.
//!
//! A style only bends the leader and biases pad placement. It never changes
//! growth amounts, language states or colors. The leader's position is a
//! function of height only, so growing taller never moves the wood below.

use crate::config::{Style, Wind};
use crate::noise::fbm;
use crate::rng::Rng;

/// The soil line on the 512-unit canvas.
pub const SOIL_Y: f64 = 404.0;

/// The canvas center.
pub const CENTER_X: f64 = 256.0;

/// A style made concrete for one account: its seeded lean and wind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leader {
    /// The style.
    pub style: Style,
    seed: u64,
    /// Sideways lean per unit of height (shakan, fukinagashi); signed.
    lean: f64,
    /// How far the leader meanders, as a share of height.
    meander: f64,
    /// `+1` if foliage favors the right, `-1` the left, `0` for no preference.
    favor: f64,
}

impl Leader {
    /// The leader of `style` for the account `seed`.
    pub fn new(style: Style, seed: u64, wind: Option<Wind>) -> Self {
        let mut rng = Rng::keyed(seed, "lean");
        let sign = rng.sign();
        let amount = rng.range(0.0, 1.0);
        let downwind = match wind {
            Some(Wind::Left) => -1.0,
            Some(Wind::Right) => 1.0,
            None => Rng::keyed(seed, "wind").sign(),
        };
        let (lean, meander, favor) = match style {
            Style::Chokkan => (0.0, 0.02, 0.0),
            Style::Moyogi => (0.0, 0.14, 0.0),
            // Slanting 20° to 35°; foliage favors the side opposite the lean.
            Style::Shakan => (sign * (0.22 + 0.16 * amount), 0.05, -sign),
            // Windswept: leaning downwind, and everything streams that way.
            Style::Fukinagashi => (downwind * (0.3 + 0.18 * amount), 0.05, downwind),
        };
        Self {
            style,
            seed,
            lean,
            meander,
            favor,
        }
    }

    /// The leader's horizontal offset from the trunk base at height `h`.
    pub fn x_at(&self, h: f64) -> f64 {
        self.lean * h + self.meander * h * fbm(self.seed, h / 80.0)
    }

    /// The side a new pad goes to: its natural side, or flipped toward the
    /// style's favored side when `u` (uniform in `[0, 1)`) says so.
    pub fn pad_side(&self, natural: f64, u: f64) -> f64 {
        let strength = match self.style {
            Style::Fukinagashi => 0.9,
            Style::Shakan => 0.7,
            _ => 0.0,
        };
        if self.favor != 0.0 && u < strength {
            self.favor
        } else {
            natural
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chokkan_is_nearly_straight_and_shakan_leans() {
        let straight = Leader::new(Style::Chokkan, 5, None);
        assert!(straight.x_at(250.0).abs() < 10.0);
        let slant = Leader::new(Style::Shakan, 5, None);
        assert!(slant.x_at(250.0).abs() > 40.0);
    }

    #[test]
    fn fukinagashi_follows_the_configured_wind() {
        for (wind, sign) in [(Wind::Left, -1.0), (Wind::Right, 1.0)] {
            let leader = Leader::new(Style::Fukinagashi, 9, Some(wind));
            assert!(leader.x_at(200.0) * sign > 40.0);
            assert_eq!(leader.pad_side(-sign, 0.1), sign);
        }
    }

    #[test]
    fn the_leader_depends_on_height_only() {
        let leader = Leader::new(Style::Moyogi, 42, None);
        assert_eq!(leader.x_at(120.0), leader.x_at(120.0));
    }
}
