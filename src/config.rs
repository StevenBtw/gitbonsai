//! `bonsai.toml`: the few things a person chooses about their tree.
//!
//! Everything has a default, so a missing file is a valid config. Unknown
//! keys are errors, so a typo never silently does nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::color::Rgb;
use crate::date::Date;
use crate::snapshot::Snapshot;

/// A private period key: `"2019..2023"` (whole years) or
/// `"2019-03..2021-06"` (months), both ends included.
///
/// # Errors
///
/// When the key is malformed or ends before it starts.
pub fn parse_period(key: &str) -> Result<(Date, Date), Error> {
    let bad = || {
        Error::Config(format!(
            "[leaves.private_periods] `{key}`: write \"2019..2023\" or \"2019-03..2021-06\""
        ))
    };
    let (a, b) = key.split_once("..").ok_or_else(bad)?;
    let start = |s: &str| -> Option<(i32, u32)> {
        match s.split_once('-') {
            None => Some((s.trim().parse().ok()?, 1)),
            Some((y, m)) => Some((y.trim().parse().ok()?, m.trim().parse().ok()?)),
        }
    };
    let (ya, ma) = start(a).ok_or_else(bad)?;
    let (yb, mb) = match b.split_once('-') {
        None => (b.trim().parse().map_err(|_| bad())?, 12),
        Some((y, m)) => (
            y.trim().parse().map_err(|_| bad())?,
            m.trim().parse().map_err(|_| bad())?,
        ),
    };
    let year = |y: i32| (1..=9999).contains(&y);
    if !(1..=12).contains(&ma) || !(1..=12).contains(&mb) || !year(ya) || !year(yb) {
        return Err(bad());
    }
    let from = Date::from_ymd(ya, ma, 1).ok_or_else(bad)?;
    let next = if mb == 12 {
        yb.checked_add(1).and_then(|y| Date::from_ymd(y, 1, 1))
    } else {
        Date::from_ymd(yb, mb + 1, 1)
    };
    let to = next.ok_or_else(bad)?.add_days(-1);
    if to < from {
        return Err(bad());
    }
    Ok((from, to))
}

/// The widest PNG, in pixels: about ten times the default.
pub const MAX_SIZE: u32 = 4096;

/// The most languages that can be pinned to a leaf color slot.
pub const MAX_PINS: usize = 5;

/// The whole config.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// The tree's shape.
    pub tree: TreeConfig,
    /// Leaf colors.
    pub leaves: LeavesConfig,
    /// Blossoms.
    pub blossoms: BlossomsConfig,
    /// The pot.
    pub pot: PotConfig,
    /// The output.
    pub render: RenderConfig,
}

/// `[tree]`
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TreeConfig {
    /// The bonsai style; unset, the last three years pick it ([`crate::style::Mix`]).
    pub style: Option<Style>,
    /// The side the tree leans, streams and cascades to; its sprout picks it
    /// when unset ([`crate::style::side`]).
    pub wind: Option<Wind>,
}

/// `[leaves]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LeavesConfig {
    /// Languages that take the named color slots first, in this order.
    pub pins: Vec<String>,
    /// The color of every language outside the named slots.
    pub other: Rgb,
    /// Colors used verbatim, without the foliage tint.
    pub colors: BTreeMap<String, Rgb>,
    /// The color of work GitHub reports without a language (private or
    /// corporate repositories).
    pub private: Rgb,
    /// Languages to show for private work in a period, keyed `"2019..2023"`
    /// (whole years) or `"2019-03..2021-06"` (months).
    pub private_periods: BTreeMap<String, Vec<String>>,
}

impl Default for LeavesConfig {
    fn default() -> Self {
        Self {
            pins: Vec::new(),
            other: Rgb::hex(0x9AA58A),
            colors: BTreeMap::new(),
            private: crate::maple::PRIVATE_GREY,
            private_periods: BTreeMap::new(),
        }
    }
}

/// `[blossoms]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BlossomsConfig {
    /// Petal color.
    pub color: Rgb,
}

impl Default for BlossomsConfig {
    fn default() -> Self {
        Self {
            color: Rgb::hex(0xF4C7D3),
        }
    }
}

/// `[pot]`
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PotConfig {
    /// The glaze, used from pot tier 3 up.
    pub glaze: Option<Rgb>,
}

/// `[render]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RenderConfig {
    /// Whether the SVG sways. `false` leaves every animation rule out.
    pub animate: bool,
    /// Display width in pixels; the height follows the tree's frame.
    pub size: u32,
    /// A legend beside the tree: what the colors and signs mean.
    pub legend: Legend,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            animate: true,
            size: 400,
            legend: Legend::Off,
        }
    }
}

/// Where the legend goes, if anywhere.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Legend {
    /// No legend: just the tree.
    #[default]
    Off,
    /// Left of the tree.
    Left,
    /// Right of the tree.
    Right,
}

impl std::str::FromStr for Legend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "off" => Ok(Self::Off),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            _ => Err(format!("legend must be off, left or right, not {s:?}")),
        }
    }
}

/// A bonsai style.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// Formal upright: a straight, vertical trunk.
    Chokkan,
    /// Informal upright: a trunk that curves in decaying S-bends.
    #[default]
    Moyogi,
    /// Slanting: the whole tree leans 20° to 35°.
    Shakan,
    /// Windswept: everything streams downwind.
    Fukinagashi,
    /// Cascade: the trunk spills over the rim and hangs below the pot.
    Kengai,
}

impl Style {
    /// Every style, in the order the gallery shows them.
    pub const ALL: [Self; 5] = [
        Self::Moyogi,
        Self::Chokkan,
        Self::Shakan,
        Self::Fukinagashi,
        Self::Kengai,
    ];

    /// The name used in config and on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Self::Chokkan => "chokkan",
            Self::Moyogi => "moyogi",
            Self::Shakan => "shakan",
            Self::Fukinagashi => "fukinagashi",
            Self::Kengai => "kengai",
        }
    }
}

impl fmt::Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Style {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|style| style.name() == s)
            .ok_or_else(|| {
                Error::Config(format!(
                    "unknown style `{s}`; use one of chokkan, moyogi, shakan, fukinagashi, kengai"
                ))
            })
    }
}

/// The side a tree leans, streams and cascades to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Wind {
    /// Blowing toward the left.
    Left,
    /// Blowing toward the right.
    Right,
}

impl Config {
    /// Reads `path`. A missing file means all defaults.
    ///
    /// # Errors
    ///
    /// When the file can't be read or is invalid.
    pub fn load(path: &Path) -> Result<Self, Error> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_toml(&text).map_err(|e| match e {
                Error::Config(msg) => Error::Config(format!("{}: {msg}", path.display())),
                other => other,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::io(format!("can't read {}", path.display()), e)),
        }
    }

    /// Parses and validates a config.
    ///
    /// # Errors
    ///
    /// When the TOML is invalid, has unknown keys, or fails [`Config::validate`].
    pub fn from_toml(text: &str) -> Result<Self, Error> {
        let config: Self =
            toml::from_str(text).map_err(|e| Error::Config(e.message().to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// Checks what serde can't: pin count and duplicates.
    ///
    /// # Errors
    ///
    /// With a message naming the setting to fix.
    pub fn validate(&self) -> Result<(), Error> {
        let pins = &self.leaves.pins;
        if pins.len() > MAX_PINS {
            return Err(Error::Config(format!(
                "{} pins in [leaves] pins; at most {MAX_PINS} languages can be pinned",
                pins.len()
            )));
        }
        let mut seen = BTreeSet::new();
        for pin in pins {
            if !seen.insert(pin) {
                return Err(Error::Config(format!(
                    "`{pin}` is pinned twice in [leaves] pins"
                )));
            }
        }
        if !(1..=MAX_SIZE).contains(&self.render.size) {
            return Err(Error::Config(format!(
                "[render] size must be 1 to {MAX_SIZE} px, not {}",
                self.render.size
            )));
        }
        let mut periods: Vec<(Date, Date, &String)> = Vec::new();
        for key in self.leaves.private_periods.keys() {
            let (from, to) = parse_period(key)?;
            periods.push((from, to, key));
        }
        periods.sort();
        for pair in periods.windows(2) {
            if pair[1].0 <= pair[0].1 {
                return Err(Error::Config(format!(
                    "[leaves.private_periods] `{}` and `{}` overlap; each month belongs to one period",
                    pair[0].2, pair[1].2
                )));
            }
        }
        Ok(())
    }

    /// The declared private periods: first day, last day, languages.
    pub fn private_periods(&self) -> Vec<(Date, Date, Vec<String>)> {
        self.leaves
            .private_periods
            .iter()
            .filter_map(|(k, langs)| parse_period(k).ok().map(|(a, b)| (a, b, langs.clone())))
            .collect()
    }

    /// Warnings for settings that name a language the snapshot never uses.
    /// Language names are GitHub's, exact and case-sensitive.
    pub fn warnings(&self, snapshot: &Snapshot) -> Vec<String> {
        let known: BTreeSet<&str> = snapshot
            .days
            .iter()
            .flat_map(|d| d.by_language.keys())
            .map(String::as_str)
            .collect();
        let mut warnings = Vec::new();
        for pin in &self.leaves.pins {
            if !known.contains(pin.as_str()) {
                warnings.push(format!(
                    "pinned language `{pin}` never appears in the snapshot; its slot stays reserved (names are case-sensitive, e.g. `C++`)"
                ));
            }
        }
        for language in self.leaves.colors.keys() {
            if !known.contains(language.as_str()) {
                warnings.push(format!(
                    "[leaves.colors] names `{language}`, which never appears in the snapshot"
                ));
            }
        }
        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> crate::date::Date {
        s.parse().unwrap()
    }

    #[test]
    fn private_grey_is_configurable() {
        assert_eq!(Config::default().leaves.private, Rgb::hex(0x00b8_bcc2));
        let c = Config::from_toml("[leaves]\nprivate = \"#aabbcc\"\n").unwrap();
        assert_eq!(c.leaves.private, Rgb::hex(0x00aa_bbcc));
    }

    #[test]
    fn the_png_size_has_bounds() {
        let sized = |n: u32| {
            let mut c = Config::default();
            c.render.size = n;
            c.validate()
        };
        assert!(sized(0).is_err());
        assert!(sized(MAX_SIZE).is_ok());
        assert!(sized(MAX_SIZE + 1).is_err(), "a 72 MB PNG is a typo");
        assert!(sized(99_999).is_err());
    }

    #[test]
    fn private_periods_parse_years_and_months() {
        assert_eq!(
            parse_period("2019..2023").unwrap(),
            (d("2019-01-01"), d("2023-12-31"))
        );
        assert_eq!(
            parse_period("2019-03..2021-06").unwrap(),
            (d("2019-03-01"), d("2021-06-30"))
        );
        assert!(parse_period("2023..2019").is_err());
        assert!(parse_period("2019..2020-00").is_err(), "no month 0");
        assert!(parse_period("2019-13..2020").is_err(), "no month 13");
        assert!(
            parse_period("2019..2147483647").is_err(),
            "no year overflow"
        );
        assert!(parse_period("2147483647..2147483647").is_err());
        assert!(parse_period("soon").is_err());
        let bad = Config::from_toml("[leaves.private_periods]\n\"2023..2019\" = [\"Go\"]\n");
        assert!(bad.is_err());
        let overlap = Config::from_toml(
            "[leaves.private_periods]\n\"2019..2023\" = [\"Go\"]\n\"2021-03..2024\" = [\"Rust\"]\n",
        );
        assert!(overlap.is_err(), "overlapping periods are reported");
        let apart = Config::from_toml(
            "[leaves.private_periods]\n\"2019..2020\" = [\"Go\"]\n\"2021..2022\" = [\"Rust\"]\n",
        );
        assert!(apart.is_ok());
    }

    #[test]
    fn an_empty_file_is_all_defaults() {
        let config = Config::from_toml("").unwrap();
        assert_eq!(config, Config::default());
        assert_eq!(config.tree.style, None);
        assert!(config.render.animate);
    }

    #[test]
    fn the_documented_example_parses() {
        let config = Config::from_toml(
            r##"
            [tree]
            style = "fukinagashi"
            wind = "left"

            [leaves]
            pins = ["Rust", "Python"]
            other = "#9AA58A"

            [leaves.colors]
            Rust = "#C0703A"
            "C++" = "#4F7FA8"

            [blossoms]
            color = "#F4C7D3"

            [pot]
            glaze = "#3E5C76"

            [render]
            animate = false
            size = 300
            "##,
        )
        .unwrap();
        assert_eq!(config.tree.style, Some(Style::Fukinagashi));
        assert_eq!(config.tree.wind, Some(Wind::Left));
        assert_eq!(config.leaves.colors["C++"], Rgb::hex(0x4F7FA8));
        assert!(!config.render.animate);
    }

    #[test]
    fn mistakes_are_config_errors() {
        let cases = [
            "[leaves]\npins = [\"a\", \"b\", \"c\", \"d\", \"e\", \"f\"]",
            "[leaves]\npins = [\"Rust\", \"Rust\"]",
            "[blossoms]\ncolor = \"pink\"",
            "[tree]\nstyle = \"bunjingi\"",
            "[tree]\nwind = \"up\"",
            "[tree]\nstlye = \"moyogi\"",
        ];
        for case in cases {
            let err = Config::from_toml(case).unwrap_err();
            assert_eq!(err.exit_code(), 2, "{case}: {err}");
        }
    }
}
