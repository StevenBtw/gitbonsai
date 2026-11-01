//! Colors: sRGB hex, OKLab and OKLCH, and the leaf tint.
//!
//! All color math happens in OKLab (Björn Ottosson's perceptual space), so a
//! lightness step looks the same size on every hue. Conversions back to sRGB
//! reduce chroma until the color fits the gamut instead of clipping channels,
//! which keeps the hue.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::math;
use crate::rng::siphash13;

/// An sRGB color, written `#rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

/// A color in OKLCH: lightness in `[0, 1]`, chroma, hue in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    /// Perceptual lightness, 0 black to 1 white.
    pub l: f64,
    /// Chroma: 0 is gray, sRGB tops out around 0.32.
    pub c: f64,
    /// Hue in degrees, `[0, 360)`.
    pub h: f64,
}

/// The fallback leaf tones, for languages GitHub has no color for.
pub const FALLBACK: [(&str, Rgb); 5] = [
    ("Pine", Rgb::hex(0x3F6B45)),
    ("Maple", Rgb::hex(0xB5473A)),
    ("Ginkgo", Rgb::hex(0xD4A73C)),
    ("Wisteria", Rgb::hex(0x8C7BB8)),
    ("Cherry", Rgb::hex(0xE3A3B5)),
];

impl Rgb {
    /// The color `0xRRGGBB`.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "each channel is one byte of the value"
    )]
    pub const fn hex(value: u32) -> Self {
        Self {
            r: (value >> 16) as u8,
            g: (value >> 8) as u8,
            b: value as u8,
        }
    }

    /// This color in OKLCH.
    pub fn to_oklch(self) -> Oklch {
        let [l, a, b] = self.to_oklab();
        let h = math::deg(math::atan2(b, a));
        Oklch {
            l,
            c: (a * a + b * b).sqrt(),
            h: math::wrap_degrees(h),
        }
    }

    fn to_oklab(self) -> [f64; 3] {
        let r = to_linear(self.r);
        let g = to_linear(self.g);
        let b = to_linear(self.b);
        let l = math::cbrt(0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b);
        let m = math::cbrt(0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b);
        let s = math::cbrt(0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b);
        [
            0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
            1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
            0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
        ]
    }

    /// Lighter (positive) or darker (negative) by `dl` OKLCH lightness, with
    /// chroma changed by `dc`.
    #[must_use]
    pub fn shift(self, dl: f64, dc: f64) -> Self {
        let mut lch = self.to_oklch();
        lch.l = (lch.l + dl).clamp(0.0, 1.0);
        lch.c = (lch.c + dc).max(0.0);
        lch.to_rgb()
    }

    /// The hue turned by `degrees`.
    #[must_use]
    pub fn rotate_hue(self, degrees: f64) -> Self {
        let mut lch = self.to_oklch();
        lch.h = math::wrap_degrees(lch.h + degrees);
        lch.to_rgb()
    }

    /// `t` of the way from `self` to `other`, in OKLab.
    #[must_use]
    pub fn mix(self, other: Self, t: f64) -> Self {
        let a = self.to_oklab();
        let b = other.to_oklab();
        from_oklab([
            math::lerp(a[0], b[0], t),
            math::lerp(a[1], b[1], t),
            math::lerp(a[2], b[2], t),
        ])
    }

    /// The perceptual distance to `other`: Euclidean in OKLab.
    pub fn delta_e(self, other: Self) -> f64 {
        let a = self.to_oklab();
        let b = other.to_oklab();
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }
}

impl Oklch {
    /// This color in sRGB, with chroma reduced until it fits the gamut.
    pub fn to_rgb(self) -> Rgb {
        let lab = |c: f64| {
            let h = math::rad(self.h);
            [self.l, c * math::cos(h), c * math::sin(h)]
        };
        if in_gamut(lab(self.c)) {
            return from_oklab(lab(self.c));
        }
        let (mut lo, mut hi) = (0.0, self.c);
        for _ in 0..20 {
            let mid = f64::midpoint(lo, hi);
            if in_gamut(lab(mid)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        from_oklab(lab(lo))
    }
}

fn to_linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        math::pow((c + 0.055) / 1.055, 2.4)
    }
}

fn to_srgb(linear: f64) -> f64 {
    if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * math::pow(linear, 1.0 / 2.4) - 0.055
    }
}

fn oklab_to_linear([l, a, b]: [f64; 3]) -> [f64; 3] {
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3,
        -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3,
        -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701_0 * s3,
    ]
}

fn in_gamut(lab: [f64; 3]) -> bool {
    oklab_to_linear(lab)
        .iter()
        .all(|c| (-1e-6..=1.0 + 1e-6).contains(c))
}

fn from_oklab(lab: [f64; 3]) -> Rgb {
    let [r, g, b] = oklab_to_linear(lab);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to [0, 255] before the cast"
    )]
    let channel = |c: f64| {
        (to_srgb(c.clamp(0.0, 1.0)) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgb {
        r: channel(r),
        g: channel(g),
        b: channel(b),
    }
}

/// A linguist color tinted toward foliage: lightness clamped to
/// `[0.48, 0.74]`, chroma `clamp(C * 0.8, 0.05, 0.15)`, hue moved 25% of the
/// way toward leaf green (135°) along the shorter arc.
pub fn tint_leaf(color: Rgb) -> Rgb {
    let lch = color.to_oklch();
    let toward = math::wrap_degrees(135.0 - lch.h + 180.0) - 180.0;
    Oklch {
        l: lch.l.clamp(0.48, 0.74),
        c: (lch.c * 0.8).clamp(0.05, 0.15),
        h: math::wrap_degrees(lch.h + 0.25 * toward),
    }
    .to_rgb()
}

/// The fallback tone for a language GitHub has no color for, picked by a
/// stable hash of its name.
pub fn fallback(language: &str) -> Rgb {
    #[allow(clippy::cast_possible_truncation, reason = "the remainder is below 5")]
    let index = (siphash13(language.as_bytes()) % FALLBACK.len() as u64) as usize;
    FALLBACK[index].1
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// A color that is not `#rrggbb`.
#[derive(Debug, Clone, thiserror::Error)]
#[error("`{0}` is not a color; expected #rrggbb, for example #3e5c76")]
pub struct ParseColorError(String);

impl FromStr for Rgb {
    type Err = ParseColorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s
            .strip_prefix('#')
            .filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| ParseColorError(s.to_string()))?;
        let value = u32::from_str_radix(hex, 16).map_err(|_| ParseColorError(s.to_string()))?;
        Ok(Self::hex(value))
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        let c: Rgb = "#DEA584".parse().unwrap();
        assert_eq!(c, Rgb::hex(0xdea584));
        assert_eq!(c.to_string(), "#dea584");
        assert!("dea584".parse::<Rgb>().is_err());
        assert!("#dea58".parse::<Rgb>().is_err());
        assert!("#dea58g".parse::<Rgb>().is_err());
    }

    #[test]
    fn oklch_round_trips_in_gamut_colors() {
        for value in [0x000000, 0xffffff, 0xdea584, 0x3572a5, 0x3f6b45, 0xf1e05a] {
            let c = Rgb::hex(value);
            assert_eq!(c.to_oklch().to_rgb(), c, "{c}");
        }
        let white = Rgb::hex(0xffffff).to_oklch();
        assert!((white.l - 1.0).abs() < 1e-4 && white.c < 1e-4);
    }

    #[test]
    fn the_tint_stays_in_its_bounds_and_moves_toward_green() {
        // JavaScript's neon yellow and C's gray.
        for value in [0xf1e05a, 0x555555, 0xff00ff, 0x00add8] {
            let tinted = tint_leaf(Rgb::hex(value)).to_oklch();
            assert!((0.47..=0.75).contains(&tinted.l), "{value:06x}: {tinted:?}");
            assert!(tinted.c <= 0.151, "{value:06x}: {tinted:?}");
        }
        let magenta = Rgb::hex(0xff00ff).to_oklch().h;
        let tinted = tint_leaf(Rgb::hex(0xff00ff)).to_oklch().h;
        assert!(math::angle_between(tinted, 135.0) < math::angle_between(magenta, 135.0));
    }

    #[test]
    fn shifting_lightness_moves_lightness() {
        let bark = Rgb::hex(0x6b4a35);
        let lighter = bark.shift(0.12, 0.02).to_oklch();
        assert!((lighter.l - bark.to_oklch().l - 0.12).abs() < 0.01);
    }
}
