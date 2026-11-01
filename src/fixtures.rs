//! Synthetic accounts for tests, the gallery and tuning.
//!
//! Each fixture is built from "spells" of steady activity. Weekly amounts
//! jitter by a seeded ±40% and spread over a few seeded weekdays, so the
//! fixtures look like people and are the same on every run.

use std::collections::BTreeMap;

use crate::color::Rgb;
use crate::date::{Date, DateRange};
use crate::rng::Rng;
use crate::snapshot::{Day, SCHEMA, Snapshot};

/// The day every fixture is observed on.
pub const OBSERVED_ON: &str = "2026-10-31";

/// A named synthetic account.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// Short name, for file names and `--fixture`.
    pub name: &'static str,
    /// What it exercises.
    pub about: &'static str,
    /// The account.
    pub snapshot: Snapshot,
}

/// Linguist colors for the languages the fixtures use.
const LINGUIST: [(&str, u32); 17] = [
    ("C", 0x555555),
    ("C++", 0xf34b7d),
    ("Elixir", 0x6e4a7e),
    ("Go", 0x00add8),
    ("Haskell", 0x5e5086),
    ("Java", 0xb07219),
    ("JavaScript", 0xf1e05a),
    ("Kotlin", 0xa97bff),
    ("Lua", 0x000080),
    ("OCaml", 0x3be133),
    ("Python", 0x3572a5),
    ("Ruby", 0x701516),
    ("Rust", 0xdea584),
    ("Scala", 0xc22d40),
    ("Shell", 0x89e051),
    ("Swift", 0xf05138),
    ("TypeScript", 0x3178c6),
];

struct Builder {
    name: &'static str,
    account_id: u64,
    observed_on: Date,
    days: BTreeMap<Date, Day>,
}

impl Builder {
    fn new(name: &'static str, account_id: u64) -> Self {
        Self {
            name,
            account_id,
            observed_on: date(OBSERVED_ON),
            days: BTreeMap::new(),
        }
    }

    /// `per_week` contributions every week from `from` to `to`, split over
    /// languages by weight; `private` is the unattributed share.
    fn spell(
        mut self,
        from: &str,
        to: &str,
        per_week: f64,
        mix: &[(&str, f64)],
        private: f64,
    ) -> Self {
        let (from, to) = (date(from), date(to).min(self.observed_on));
        let mut rng = Rng::keyed(self.account_id, &format!("fixture/{}/{from}", self.name));
        let weights: f64 = mix.iter().map(|(_, w)| w).sum::<f64>().max(1e-9);
        for week in from.week()..=to.week() {
            let jitter = if per_week >= 1.0 {
                rng.range(0.6, 1.4)
            } else {
                1.0
            };
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a few thousand at most"
            )]
            let total = (per_week * jitter).round().max(0.0) as u32;
            if total == 0 {
                continue;
            }
            let active = (total.div_ceil(4)).clamp(1, 7);
            let mut weekdays: Vec<u32> = (0..7).collect();
            for i in (1..weekdays.len()).rev() {
                weekdays.swap(i, rng.below(u32::try_from(i + 1).unwrap_or(1)) as usize);
            }
            for (k, &weekday) in weekdays.iter().take(active as usize).enumerate() {
                let day_date = Date::monday_of(week).add_days(i32::try_from(weekday).unwrap_or(0));
                if day_date < from || day_date > to {
                    continue;
                }
                let share = total / active
                    + u32::from(u32::try_from(k).unwrap_or(u32::MAX) < total % active);
                if share == 0 {
                    continue;
                }
                let day = self.days.entry(day_date).or_insert_with(|| Day {
                    date: day_date,
                    total: 0,
                    by_language: BTreeMap::new(),
                });
                day.total += share;
                let attributed = f64::from(share) * (1.0 - private);
                let mut given = 0u32;
                for (i, (language, weight)) in mix.iter().enumerate() {
                    let exact = attributed * weight / weights;
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "at most share"
                    )]
                    let n = if i + 1 == mix.len() {
                        (attributed.round() as u32).saturating_sub(given)
                    } else {
                        exact.round() as u32
                    };
                    let n = n.min(share - given);
                    if n > 0 {
                        *day.by_language.entry((*language).to_string()).or_default() += n;
                        given += n;
                    }
                }
            }
        }
        self
    }

    fn build(self) -> Snapshot {
        let days: Vec<Day> = self.days.into_values().filter(|d| d.total > 0).collect();
        let language_colors = days
            .iter()
            .flat_map(|d| d.by_language.keys())
            .map(|language| {
                let color = LINGUIST
                    .iter()
                    .find(|(l, _)| l == language)
                    .map(|(_, c)| Rgb::hex(*c));
                (language.clone(), color)
            })
            .collect();
        let from = days.first().map_or(self.observed_on, |d| {
            Date::from_ymd(d.date.year(), 1, 1).unwrap_or(d.date)
        });
        Snapshot {
            schema: SCHEMA,
            login: format!("fixture-{}", self.name),
            account_id: self.account_id,
            range: DateRange {
                from,
                to: self.observed_on,
            },
            observed_on: self.observed_on,
            collected_with: format!("gitbonsai {} fixtures", crate::VERSION),
            language_colors,
            days,
            truncated_windows: Vec::new(),
            achievements: BTreeMap::new(),
            archived_repos: Vec::new(),
        }
    }
}

fn date(text: &str) -> Date {
    text.parse().expect("fixture dates are valid")
}

/// Achievements and archived-repository counts per fixture, spread so every
/// easter egg and tier shows somewhere in the gallery.
fn eggs_for(name: &str) -> (&'static [(&'static str, u8)], usize) {
    match name {
        "veteran" => (
            &[
                ("pull-shark", 4),
                ("starstruck", 4),
                ("pair-extraordinaire", 4),
                ("galaxy-brain", 4),
                ("public-sponsor", 1),
                ("arctic-code-vault-contributor", 1),
                ("mars-2020-contributor", 1),
            ],
            24,
        ),
        "comeback" => (
            &[
                ("pull-shark", 2),
                ("pair-extraordinaire", 2),
                ("galaxy-brain", 1),
                ("starstruck", 1),
            ],
            3,
        ),
        "steady" => (
            &[
                ("pull-shark", 3),
                ("starstruck", 2),
                ("pair-extraordinaire", 3),
                ("galaxy-brain", 2),
            ],
            0,
        ),
        "sprout" => (&[("pull-shark", 1)], 0),
        "polyglot" => (
            &[
                ("galaxy-brain", 3),
                ("starstruck", 3),
                ("pair-extraordinaire", 2),
            ],
            6,
        ),
        "mono" => (
            &[
                ("pull-shark", 1),
                ("pair-extraordinaire", 3),
                ("galaxy-brain", 1),
            ],
            1,
        ),
        "mossgap" => (
            &[
                ("pair-extraordinaire", 4),
                ("galaxy-brain", 2),
                ("starstruck", 1),
            ],
            0,
        ),
        "dormant" => (
            &[
                ("starstruck", 3),
                ("public-sponsor", 1),
                ("arctic-code-vault-contributor", 1),
            ],
            2,
        ),
        "bot" => (
            &[
                ("pull-shark", 4),
                ("mars-2020-contributor", 1),
                ("galaxy-brain", 3),
            ],
            0,
        ),
        _ => (&[], 0),
    }
}

/// Every fixture, in gallery order.
pub fn all() -> Vec<Fixture> {
    let fixture = |name: &'static str, about, mut snapshot: Snapshot| {
        let (eggs, archived) = eggs_for(name);
        snapshot.achievements = eggs
            .iter()
            .map(|(slug, tier)| ((*slug).to_string(), *tier))
            .collect();
        snapshot.archived_repos = (0..archived)
            .map(|i| format!("{name}-archive-{i}"))
            .collect();
        Fixture {
            name,
            about,
            snapshot,
        }
    };
    vec![
        fixture(
            "veteran",
            "Since 2010 at ~3/week, then 120/week from 2023. Ruby went quiet in 2018: jin.",
            Builder::new("veteran", 100_001)
                .spell(
                    "2010-03-01",
                    "2013-12-31",
                    3.0,
                    &[("Python", 2.0), ("C", 1.0)],
                    0.1,
                )
                .spell(
                    "2014-01-01",
                    "2017-12-31",
                    3.0,
                    &[("JavaScript", 2.0), ("Ruby", 1.0), ("Python", 0.5)],
                    0.15,
                )
                .spell(
                    "2018-01-01",
                    "2022-12-31",
                    3.0,
                    &[("Go", 2.0), ("Python", 0.5), ("JavaScript", 0.5)],
                    0.2,
                )
                .spell(
                    "2023-01-02",
                    "2026-10-31",
                    120.0,
                    &[("Rust", 6.0), ("TypeScript", 2.5), ("Python", 1.0)],
                    0.1,
                )
                .build(),
        ),
        fixture(
            "steady",
            "25 per week for five years.",
            Builder::new("steady", 100_002)
                .spell(
                    "2021-11-01",
                    "2026-10-31",
                    25.0,
                    &[("Python", 5.0), ("TypeScript", 3.0), ("Go", 1.5)],
                    0.1,
                )
                .build(),
        ),
        fixture(
            "bot",
            "1,500 per week for six years: capped at 1,000, crosses level 11 this week.",
            Builder::new("bot", 100_003)
                .spell("2021-01-25", "2026-10-31", 1500.0, &[("Python", 1.0)], 0.05)
                .build(),
        ),
        fixture(
            "polyglot",
            "Fifteen languages over six years: nine limbs plus the misc limb.",
            polyglot(),
        ),
        fixture(
            "comeback",
            "Active 2016-2020, silent two years, back since 2022: jin and a live vein.",
            Builder::new("comeback", 100_005)
                .spell(
                    "2016-02-01",
                    "2020-05-31",
                    20.0,
                    &[("Python", 3.0), ("JavaScript", 2.0)],
                    0.1,
                )
                .spell(
                    "2022-06-06",
                    "2026-10-31",
                    30.0,
                    &[("Python", 2.0), ("Rust", 3.0)],
                    0.1,
                )
                .build(),
        ),
        fixture(
            "mono",
            "One language, 40 per week for eight years.",
            Builder::new("mono", 100_006)
                .spell("2018-10-01", "2026-10-31", 40.0, &[("Go", 1.0)], 0.0)
                .build(),
        ),
        fixture(
            "private",
            "Private contributions only: trunk and crown, no limbs.",
            Builder::new("private", 100_007)
                .spell("2023-05-01", "2026-10-31", 15.0, &[], 1.0)
                .build(),
        ),
        fixture(
            "mossgap",
            "A 40-week streak that ended a month ago, then two weeks back: receding moss.",
            Builder::new("mossgap", 100_008)
                .spell(
                    "2025-12-01",
                    "2026-09-13",
                    12.0,
                    &[("TypeScript", 2.0), ("Shell", 1.0)],
                    0.2,
                )
                .spell(
                    "2026-10-19",
                    "2026-10-31",
                    12.0,
                    &[("TypeScript", 2.0)],
                    0.2,
                )
                .build(),
        ),
        fixture(
            "sprout",
            "Three weeks old.",
            Builder::new("sprout", 100_009)
                .spell("2026-10-12", "2026-10-31", 10.0, &[("Rust", 1.0)], 0.1)
                .build(),
        ),
        fixture(
            "dormant",
            "Six good years, then nothing since mid-2024: bare canopy, a dormant bud.",
            Builder::new("dormant", 100_010)
                .spell(
                    "2018-03-05",
                    "2024-06-30",
                    18.0,
                    &[("Java", 2.0), ("Kotlin", 1.0)],
                    0.1,
                )
                .build(),
        ),
        fixture(
            "empty",
            "No contributions at all: a sprout in a training pot.",
            Builder::new("empty", 100_011).build(),
        ),
    ]
}

fn polyglot() -> Snapshot {
    const LANGUAGES: [&str; 15] = [
        "Python",
        "JavaScript",
        "Go",
        "Rust",
        "C",
        "C++",
        "Haskell",
        "Ruby",
        "Java",
        "Kotlin",
        "Swift",
        "Scala",
        "Elixir",
        "OCaml",
        "Lua",
    ];
    let mut builder = Builder::new("polyglot", 100_004);
    let start = date("2020-11-02");
    for (i, language) in LANGUAGES.iter().enumerate() {
        let from = start.add_days(i32::try_from(i).unwrap_or(0) * 98);
        let to = from.add_days(700);
        builder = builder.spell(
            &from.to_string(),
            &to.to_string(),
            14.0,
            &[(language, 1.0)],
            0.05,
        );
    }
    builder
        .spell(
            "2020-11-02",
            "2026-10-31",
            10.0,
            &[("Python", 1.0), ("Shell", 1.0)],
            0.2,
        )
        .build()
}

/// The fixture called `name`.
pub fn by_name(name: &str) -> Option<Fixture> {
    all().into_iter().find(|f| f.name == name)
}

/// Every fixture's name.
pub fn names() -> Vec<&'static str> {
    all().iter().map(|f| f.name).collect()
}
