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
use crate::snapshot::Snapshot;

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
    /// The bonsai style.
    pub style: Style,
    /// Wind direction for fukinagashi; from the seed when unset.
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
}

impl Default for LeavesConfig {
    fn default() -> Self {
        Self {
            pins: Vec::new(),
            other: Rgb::hex(0x9AA58A),
            colors: BTreeMap::new(),
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
    /// Display width in pixels. The viewBox stays 512.
    pub size: u32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            animate: true,
            size: 400,
        }
    }
}

/// A bonsai style. Kengai (cascade) comes later.
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
}

impl Style {
    /// Every style, in the order the gallery shows them.
    pub const ALL: [Self; 4] = [Self::Moyogi, Self::Chokkan, Self::Shakan, Self::Fukinagashi];

    /// The name used in config and on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Self::Chokkan => "chokkan",
            Self::Moyogi => "moyogi",
            Self::Shakan => "shakan",
            Self::Fukinagashi => "fukinagashi",
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
                    "unknown style `{s}`; use one of chokkan, moyogi, shakan, fukinagashi"
                ))
            })
    }
}

/// The wind direction of a fukinagashi tree.
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
        if self.render.size == 0 {
            return Err(Error::Config("[render] size must be at least 1".into()));
        }
        Ok(())
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

    #[test]
    fn an_empty_file_is_all_defaults() {
        let config = Config::from_toml("").unwrap();
        assert_eq!(config, Config::default());
        assert_eq!(config.tree.style, Style::Moyogi);
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
        assert_eq!(config.tree.style, Style::Fukinagashi);
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
            "[tree]\nstyle = \"kengai\"",
            "[tree]\nwind = \"up\"",
            "[tree]\nstlye = \"moyogi\"",
        ];
        for case in cases {
            let err = Config::from_toml(case).unwrap_err();
            assert_eq!(err.exit_code(), 2, "{case}: {err}");
        }
    }
}
