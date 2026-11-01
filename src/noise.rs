//! Deterministic noise: hashed lattice values, value noise and fractal
//! Brownian motion, for the organic wobble of trunks, cracks and glow.
//!
//! Everything is a pure function of a seed and a coordinate, built on the
//! crate's stable SipHash, so it is identical on every platform.

use crate::rng::{Rng, siphash13};

/// A value in `[0, 1)` for the integer `i` under `seed`.
pub fn hash01(seed: u64, i: i64) -> f64 {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8..].copy_from_slice(&i.to_le_bytes());
    (siphash13(&bytes) >> 11) as f64 / (1u64 << 53) as f64
}

/// Smooth 1D value noise in `[-1, 1]`.
pub fn noise1(seed: u64, x: f64) -> f64 {
    let i = x.floor();
    let f = x - i;
    let s = f * f * (3.0 - 2.0 * f);
    #[allow(
        clippy::cast_possible_truncation,
        reason = "lattice coordinates are small"
    )]
    let i = i as i64;
    let (a, b) = (hash01(seed, i), hash01(seed, i + 1));
    (a + (b - a) * s) * 2.0 - 1.0
}

/// Fractal Brownian motion: four octaves of [`noise1`], in about `[-1, 1]`.
pub fn fbm(seed: u64, x: f64) -> f64 {
    let mut sum = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    for octave in 0..4u64 {
        sum += amplitude * noise1(seed.wrapping_add(octave * 7919), x * frequency);
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    sum / 1.875
}

/// A rough normal deviate (sum of three uniforms), mean 0, deviation about 0.6.
pub fn gauss(rng: &mut Rng) -> f64 {
    (rng.unit() + rng.unit() + rng.unit() - 1.5) * 1.15
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_bounded_and_continuous() {
        for k in 0..2000 {
            let x = f64::from(k) * 0.013;
            let v = noise1(7, x);
            assert!((-1.0..=1.0).contains(&v));
            assert!((noise1(7, x + 1e-4) - v).abs() < 1e-2);
        }
        assert!((-1.2..=1.2).contains(&fbm(3, 4.2)));
    }

    #[test]
    fn noise_depends_only_on_its_inputs() {
        assert_eq!(fbm(11, 2.5), fbm(11, 2.5));
        assert_ne!(fbm(11, 2.5), fbm(12, 2.5));
    }
}
