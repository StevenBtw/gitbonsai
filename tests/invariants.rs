//! The growth model's promises, checked on every fixture.

use gitbonsai::config::{Legend, Wind};
use gitbonsai::foliage::LanguageState;
use gitbonsai::growth::dome;
use gitbonsai::timeline::Timeline;
use gitbonsai::{Bonsai, Config, Style, Theme, fixtures, render};

fn config(style: Style) -> Config {
    let mut config = Config::default();
    config.tree.style = style;
    config
}

fn grow(name: &str) -> Bonsai {
    Bonsai::grow(
        &fixtures::by_name(name).unwrap().snapshot,
        &Config::default(),
        None,
    )
}

/// History is never erased: as the tree is restyled over the years, every
/// active year keeps its place in it, and the trunk never thins.
#[test]
fn history_is_never_erased() {
    use gitbonsai::growth::life::Life;
    for name in [
        "veteran", "comeback", "polyglot", "bot", "mossgap", "steady", "showcase",
    ] {
        let snapshot = fixtures::by_name(name).unwrap().snapshot;
        let first = snapshot.first_active().unwrap();
        let mut day = first.add_days(13 * 7);
        let mut before = 0.0;
        while day <= snapshot.observed_on {
            let bonsai = Bonsai::grow(&snapshot, &Config::default(), Some(day));
            let root = bonsai.skeleton.flows()[0];
            assert!(
                root + 1e-6 >= before,
                "{name} {day}: trunk thinned {before} -> {root}"
            );
            let timeline = Timeline::new(&snapshot.truncated(day));
            let life = Life::of(&timeline, &[]).unwrap();
            let active: std::collections::BTreeSet<i32> = (0..life.years.len())
                .filter(|&i| life.is_active_year(i))
                .map(|i| life.years[i].y)
                .collect();
            assert_eq!(
                dome::years_grown(&timeline),
                active,
                "{name} {day}: a year went missing"
            );
            before = root;
            day = day.add_days(26 * 7);
        }
    }
}

/// Pot tier and nebari only go up.
#[test]
fn the_pot_and_roots_never_downgrade() {
    for fixture in fixtures::all() {
        let snapshot = &fixture.snapshot;
        let Some(first) = snapshot.first_active() else {
            continue;
        };
        let mut day = first;
        let (mut tier, mut nebari) = (0, 0.0);
        while day <= snapshot.observed_on {
            let bonsai = Bonsai::grow(snapshot, &Config::default(), Some(day));
            assert!(
                bonsai.extras.pot_tier >= tier,
                "{}: pot downgraded on {day}",
                fixture.name
            );
            assert!(
                bonsai.extras.nebari >= nebari,
                "{}: roots receded on {day}",
                fixture.name
            );
            tier = bonsai.extras.pot_tier;
            nebari = bonsai.extras.nebari;
            day = day.add_days(91);
        }
    }
}

/// A language that appears later never changes the tree from before it.
#[test]
fn history_is_never_rewritten() {
    let base = fixtures::by_name("steady").unwrap().snapshot;
    let mut extended = base.clone();
    for day in &mut extended.days {
        if day.date.year() == 2025 {
            day.total += 3;
            day.by_language.insert("Zig".into(), 3);
        }
    }
    let before: gitbonsai::date::Date = "2024-12-29".parse().unwrap();
    let a = Bonsai::grow(&base, &Config::default(), Some(before));
    let b = Bonsai::grow(&extended, &Config::default(), Some(before));
    assert_eq!(a.skeleton.nodes, b.skeleton.nodes);
}

#[test]
fn rendering_is_deterministic() {
    for fixture in fixtures::all() {
        for theme in Theme::ALL {
            let render = || {
                render::svg(
                    &Bonsai::grow(&fixture.snapshot, &Config::default(), None),
                    &Config::default(),
                    theme,
                )
            };
            assert_eq!(render(), render(), "{}", fixture.name);
        }
    }
}

/// The budget: at most 150 KB, 2,500 elements and 80 animated elements.
#[test]
fn every_output_stays_within_budget() {
    for fixture in fixtures::all() {
        for style in Style::ALL {
            let config = config(style);
            let bonsai = Bonsai::grow(&fixture.snapshot, &config, None);
            for theme in Theme::ALL {
                let svg = render::svg(&bonsai, &config, theme);
                let label = format!("{}/{style}/{}", fixture.name, theme.name());
                let elements = svg.matches('<').count() - svg.matches("</").count();
                let animated = [
                    "class=\"mf\"",
                    "class=\"tw\"",
                    "class=\"dp\"",
                    "class=\"gl\"",
                    "class=\"fs\"",
                    "class=\"fw\"",
                    "class=\"ff\"",
                ]
                .iter()
                .map(|c| svg.matches(c).count())
                .sum::<usize>();
                assert!(svg.len() <= 150 * 1024, "{label}: {} bytes", svg.len());
                assert!(elements <= 2500, "{label}: {elements} elements");
                assert!(animated <= 80, "{label}: {animated} animated");
            }
        }
    }
}

#[test]
fn animation_can_be_left_out_and_respects_reduced_motion() {
    let snapshot = fixtures::by_name("bot").unwrap().snapshot;
    let mut config = Config::default();
    let animated = render::svg(
        &Bonsai::grow(&snapshot, &config, None),
        &config,
        Theme::Light,
    );
    assert!(animated.contains("@keyframes"));
    assert!(animated.contains("prefers-reduced-motion:reduce"));
    assert!(
        animated.contains("class=\"mf\""),
        "the bot's milestone drops momiji"
    );
    assert!(animated.contains("class=\"dp\""), "level 11 drips");
    config.render.animate = false;
    let still = render::svg(
        &Bonsai::grow(&snapshot, &config, None),
        &config,
        Theme::Light,
    );
    assert!(!still.contains("@keyframes"));
    assert!(!still.contains("class=\"mf\""));
    assert!(!still.contains("class=\"dp\""));
}

#[test]
fn the_svg_is_self_contained_and_accessible() {
    for fixture in fixtures::all() {
        let svg = render::svg(
            &Bonsai::grow(&fixture.snapshot, &Config::default(), None),
            &Config::default(),
            Theme::Dark,
        );
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("<title id=\"t\">Bonsai for @fixture-"));
        assert!(svg.contains("<desc id=\"d\">"));
        assert_eq!(svg.matches("http://").count(), 1, "only the namespace");
        assert!(!svg.contains("<text"), "no rendered text");
        assert!(!svg.contains("<image"), "no embedded images");
        assert!(!svg.contains("filter="), "no filters");
    }
}

/// The legend sits on either side, stays within budget, lists the living
/// languages, and never gives away a surprise.
#[test]
fn the_legend_explains_without_spoiling() {
    let spoilers = [
        "fish",
        "shark",
        "tuna",
        "star",
        "furin",
        "fūrin",
        "moon",
        "firefly",
        "glow",
        "pebble",
        "crane",
        "coin",
        "snow",
        "copter",
        "kintsugi",
        "achievement",
        "drip",
        "archived",
    ];
    for fixture in fixtures::all() {
        for side in [Legend::Left, Legend::Right] {
            let mut config = Config::default();
            config.render.legend = side;
            let bonsai = Bonsai::grow(&fixture.snapshot, &config, None);
            for theme in Theme::ALL {
                let svg = render::svg(&bonsai, &config, theme);
                let label = format!("{}/{side:?}/{}", fixture.name, theme.name());
                assert!(svg.len() <= 150 * 1024, "{label}: {} bytes", svg.len());
                if bonsai.is_empty() {
                    assert!(!svg.contains("<text"), "{label}: nothing to explain yet");
                    continue;
                }
                let words: String = svg
                    .split("<text")
                    .skip(1)
                    .map(|t| t.split("</text>").next().unwrap_or("").to_lowercase())
                    .collect();
                assert!(
                    words.contains("leaves") && words.contains("signs"),
                    "{label}"
                );
                for language in &bonsai.order {
                    if bonsai.state(Some(language)) == LanguageState::Alive
                        && bonsai.order.len() <= 8
                    {
                        assert!(
                            words.contains(&language.to_lowercase()),
                            "{label}: {language}"
                        );
                    }
                }
                for word in spoilers {
                    assert!(!words.contains(word), "{label}: says {word}");
                }
            }
        }
    }
}

/// The veteran: level 8, Ruby turned to jin.
#[test]
fn the_veteran_matches_worked_example_a() {
    let bonsai = grow("veteran");
    assert_eq!(bonsai.extras.level, 8);
    assert_eq!(bonsai.state(Some("Ruby")), LanguageState::Jin);
    assert_eq!(bonsai.state(Some("Rust")), LanguageState::Alive);
}

/// The cap holds the bot at 1,000 a week; level 11 this week.
#[test]
fn the_bot_is_capped_and_irrigated() {
    let bonsai = grow("bot");
    assert!(
        bonsai
            .timeline
            .weeks
            .iter()
            .all(|w| w.total <= 1000.0 + 1e-9)
    );
    assert_eq!(bonsai.extras.level, 11);
    assert_eq!(bonsai.extras.bloom, Some(11));
    assert!(bonsai.extras.drip);
}

#[test]
fn a_returning_language_lives_and_a_lost_one_turns_to_jin() {
    let bonsai = grow("comeback");
    assert_eq!(bonsai.state(Some("Python")), LanguageState::Alive);
    assert_eq!(bonsai.state(Some("JavaScript")), LanguageState::Jin);
}

#[test]
fn easter_eggs_follow_the_achievements() {
    let render = |name: &str| render::svg(&grow(name), &Config::default(), Theme::Light);
    let veteran = render("veteran");
    for egg in [
        "#fish4",
        "#cranes",
        "#goen",
        "#taketombo",
        "class=\"fs\"",
        "clip-path",
    ] {
        assert!(veteran.contains(egg), "veteran lacks {egg}");
    }
    let sprout = render("sprout");
    assert!(sprout.contains("#fish1\""));
    assert!(
        !sprout.contains("clip-path"),
        "no archived repos, no kintsugi"
    );
}

#[test]
fn edge_accounts_still_grow_something() {
    let empty = grow("empty");
    assert!(empty.is_empty());
    assert!(render::svg(&empty, &Config::default(), Theme::Light).contains("seedling"));

    let private = grow("private");
    assert!(
        !private.skeleton.clouds.is_empty(),
        "private work grows leaves"
    );
    assert!(
        private
            .skeleton
            .clouds
            .iter()
            .all(|c| c.patches.iter().all(|p| p.2.is_none())),
        "and they carry no language"
    );

    let steady = fixtures::by_name("steady").unwrap().snapshot;
    let one_day = steady.truncated(steady.days[0].date);
    let tiny = Bonsai::grow(&one_day, &Config::default(), None);
    assert!(tiny.skeleton.nodes.len() > 1, "a single day still sprouts");

    let dormant = grow("dormant");
    assert!(dormant.dormant());
    assert!(dormant.description().contains("Dormant"));
}

#[test]
fn the_configured_wind_is_respected() {
    let snapshot = fixtures::by_name("steady").unwrap().snapshot;
    let mut config = config(Style::Fukinagashi);
    config.tree.wind = Some(Wind::Left);
    let bonsai = Bonsai::grow(&snapshot, &config, None);
    assert!(bonsai.leader.x_at(150.0) < -20.0);
}

/// Truncating with `--as-of` gives the tree collection would have given then.
#[test]
fn as_of_equals_a_snapshot_collected_that_day() {
    let snapshot = fixtures::by_name("veteran").unwrap().snapshot;
    let day = "2019-06-30".parse().unwrap();
    let a = Bonsai::grow(&snapshot, &Config::default(), Some(day));
    let b = Bonsai::grow(&snapshot.truncated(day), &Config::default(), None);
    assert_eq!(
        render::svg(&a, &Config::default(), Theme::Light),
        render::svg(&b, &Config::default(), Theme::Light)
    );
    assert_eq!(a.observed_on, day);
}

/// Age shows as girth, spread and fullness, never as height.
#[test]
fn the_tree_is_never_tall() {
    for fixture in fixtures::all() {
        let snapshot = &fixture.snapshot;
        let Some(first) = snapshot.first_active() else {
            continue;
        };
        let span = snapshot.observed_on.days_since(first);
        for k in 1..=4 {
            let day = first.add_days(span * k / 4);
            let bonsai = Bonsai::grow(snapshot, &Config::default(), Some(day));
            assert!(
                bonsai.skeleton.height <= 230.0,
                "{} on {day}: {}",
                fixture.name,
                bonsai.skeleton.height
            );
        }
    }
    let newbie = fixtures::by_name("steady").unwrap().snapshot;
    let sprout = Bonsai::grow(
        &newbie,
        &Config::default(),
        Some("2022-03-06".parse().unwrap()),
    );
    assert!(sprout.skeleton.height >= 80.0, "a sprout is tall and thin");
}

#[test]
fn the_crown_holds_three_to_six_years() {
    let timeline = |name: &str| Timeline::new(&fixtures::by_name(name).unwrap().snapshot);
    let veteran = dome::crown_years(&timeline("veteran"), &[]);
    assert!((3..=6).contains(&veteran.len()), "{veteran:?}");
    assert_eq!(
        dome::crown_years(&timeline("comeback"), &[]),
        [2022, 2023, 2024, 2025, 2026]
    );
}

#[test]
fn a_pause_leaves_a_window() {
    let snapshot = fixtures::by_name("comeback").unwrap().snapshot;
    let (lo, hi) = dome::window_band(&Timeline::new(&snapshot)).expect("comeback paused");
    assert!(hi - lo > 10.0, "{lo}..{hi}");
    let bonsai = Bonsai::grow(&snapshot, &Config::default(), None);
    for c in &bonsai.skeleton.clouds {
        let h = gitbonsai::style::SOIL_Y - c.c.1;
        assert!(
            !(lo..hi).contains(&h),
            "a cloud at {h} fills the window {lo}..{hi}"
        );
    }
}

#[test]
fn a_dormant_tree_stands_bare() {
    let dormant = grow("dormant");
    let scene = gitbonsai::scene::Scene::build(&dormant);
    assert!(scene.tufts.is_empty(), "no leaves in winter");
    assert!(
        scene.chains.len() > 50,
        "but fine twigs: {}",
        scene.chains.len()
    );
}

#[test]
fn private_accounts_grow_silver() {
    let private = grow("private");
    let scene = gitbonsai::scene::Scene::build(&private);
    assert!(scene.tufts.len() > 100, "{}", scene.tufts.len());
    let silver = gitbonsai::color::Rgb::hex(0xb8bcc2);
    for t in &scene.tufts {
        assert!(t.color.delta_e(silver) < 0.05, "{}", t.color);
    }
}

#[test]
fn a_dormant_tree_grows_leaves_again() {
    let mut snapshot = fixtures::by_name("dormant").unwrap().snapshot;
    let before = Bonsai::grow(&snapshot, &Config::default(), None);
    let start = snapshot.observed_on.add_days(1);
    for i in 0..56 {
        let mut by_language = std::collections::BTreeMap::new();
        by_language.insert("Rust".to_string(), 4);
        snapshot.days.push(gitbonsai::snapshot::Day {
            date: start.add_days(i),
            total: 4,
            by_language,
        });
    }
    snapshot.observed_on = start.add_days(56);
    snapshot.range.to = snapshot.observed_on;
    let after = Bonsai::grow(&snapshot, &Config::default(), None);
    assert!(!gitbonsai::scene::Scene::build(&after).tufts.is_empty());
    assert!(after.skeleton.flows()[0] >= before.skeleton.flows()[0]);
}

/// Both painted wind chimes dance: the blue one slowly, the golden one faster.
#[test]
fn the_wind_chimes_sway_at_their_own_pace() {
    let render = |name: &str| render::svg(&grow(name), &Config::default(), Theme::Light);
    let blue = render("polyglot");
    let golden = render("veteran");
    assert!(blue.contains("class=\"fw\""), "the blue chime sways");
    assert!(golden.contains("class=\"fs\""), "the golden chime sways");
    let period = |svg: &str, class: &str| -> f64 {
        let rule = svg.split(&format!(".{class}{{")).nth(1).unwrap();
        let anim = rule.split("animation:").nth(1).unwrap();
        anim.split_whitespace()
            .nth(1)
            .unwrap()
            .trim_end_matches('s')
            .parse()
            .unwrap()
    };
    assert!(
        period(&blue, "fw") > period(&golden, "fs"),
        "blue is the slower"
    );
}

/// The firefly glows in open air near the leaves, never lost inside a cloud.
#[test]
fn the_firefly_hovers_in_the_open() {
    use gitbonsai::scene::{Scene, TILT, WORLD, project};
    for name in ["polyglot", "steady"] {
        let bonsai = grow(name);
        let svg = render::svg(&bonsai, &Config::default(), Theme::Light);
        let at = svg
            .split("<g class=\"ff\"><g transform=\"translate(")
            .nth(1)
            .unwrap_or_else(|| panic!("{name} has no firefly"));
        let xy: Vec<f64> = at
            .split(')')
            .next()
            .unwrap()
            .split(' ')
            .map(|v| v.parse().unwrap())
            .collect();
        let (x, y) = (xy[0], xy[1]);
        let scene = Scene::build(&bonsai);
        let (s, c) = (TILT.sin(), TILT.cos());
        for (c3, rx, ry, rz, _, _) in &scene.masses {
            let (cx, cy, _) = project(*c3);
            let hw = rx * WORLD;
            let up = ((ry * c).powi(2) + (rz * s).powi(2)).sqrt() * WORLD;
            let down = rz * s * WORLD;
            let v = if y < cy { up } else { down };
            let inside = ((x - cx) / hw).powi(2) + ((y - cy) / v.max(1e-6)).powi(2) < 1.0;
            assert!(
                !inside,
                "{name}: firefly at {x} {y} sits in a cloud at {cx} {cy}"
            );
        }
    }
}

/// A resting account drops nothing: its last languages rest, they don't die.
#[test]
fn a_resting_account_drops_no_language() {
    let dormant = grow("dormant");
    for language in ["Java", "Kotlin"] {
        assert_ne!(
            dormant.state(Some(language)),
            LanguageState::Jin,
            "{language}"
        );
    }
}

#[test]
fn the_legend_names_private_work_and_winter() {
    let mut config = Config::default();
    config.render.legend = Legend::Left;
    let text = |name: &str| -> String {
        let svg = render::svg(&grow(name), &config, Theme::Light);
        svg.split("<text")
            .skip(1)
            .map(|t| t.split("</text>").next().unwrap_or("").to_lowercase())
            .collect()
    };
    assert!(text("private").contains("private work"));
    assert!(text("showcase").contains("private work"));
    assert!(
        !text("mono").contains("private work"),
        "only when there is some"
    );
    assert!(text("dormant").contains("winter"));
}

#[test]
fn explain_shows_private_work() {
    let report = gitbonsai::explain::report(&grow("showcase"));
    let line = report
        .lines()
        .find(|l| l.contains("private work"))
        .expect("a private work line");
    assert!(line.contains('%'), "{line}");
}

/// Work declared in `[leaves.private_periods]` is that language everywhere:
/// its own color, a place in the legend, and in `explain`.
#[test]
fn declared_private_work_is_named_everywhere() {
    let snapshot = fixtures::by_name("private").unwrap().snapshot;
    let config =
        Config::from_toml("[leaves.private_periods]\n\"2023..2026\" = [\"Go\"]\n").unwrap();
    let b = Bonsai::grow(&snapshot, &config, None);
    assert!(b.maple.colors.contains_key("Go"), "Go has its own color");
    assert!(
        b.order.iter().any(|l| l == "Go"),
        "and a place in the legend"
    );
    let report = gitbonsai::explain::report(&b);
    assert!(
        report
            .lines()
            .any(|l| l.trim_start().starts_with("trunks") && l.contains("Go")),
        "{report}"
    );
    assert!(!report.contains("silver leaves"), "{report}");
}

/// A language declared for recent private work is not deadwood, in the
/// legend as in the tree.
#[test]
fn declared_recent_work_keeps_a_language_alive() {
    let mut snapshot = fixtures::by_name("showcase").unwrap().snapshot;
    let from: gitbonsai::date::Date = "2026-06-01".parse().unwrap();
    for d in &mut snapshot.days {
        if d.date >= from {
            d.by_language.clear();
        }
    }
    let config =
        Config::from_toml("[leaves.private_periods]\n\"2026-06..2026-12\" = [\"C#\"]\n").unwrap();
    let b = Bonsai::grow(&snapshot, &config, None);
    assert_ne!(b.state(Some("C#")), LanguageState::Jin);
    let report = gitbonsai::explain::report(&b);
    assert!(!report.contains("C# (dead)"), "{report}");
}

/// The legend says winter exactly when the canopy is bare, and names no
/// silver leaves on a bare tree.
#[test]
fn the_legend_and_the_drawing_agree_on_winter() {
    let snapshot = fixtures::by_name("private").unwrap().snapshot;
    let last = snapshot
        .days
        .iter()
        .rev()
        .find(|d| d.total > 0)
        .unwrap()
        .date;
    let mut later = snapshot.clone();
    later.observed_on = last.add_days(400);
    later.range.to = later.observed_on;
    let mut config = Config::default();
    config.render.legend = Legend::Left;
    for k in 355..=372 {
        let b = Bonsai::grow(&later, &config, Some(last.add_days(k)));
        assert_eq!(b.dormant(), b.skeleton.dormant, "day {k}");
    }
    let bare = Bonsai::grow(&later, &config, None);
    assert!(bare.skeleton.dormant);
    let svg = render::svg(&bare, &config, Theme::Light).to_lowercase();
    assert!(svg.contains("winter"));
    assert!(!svg.contains("private work"), "no silver leaves to name");
}

/// A mature tree carries samaras for its lifetime level, enough to see.
#[test]
fn the_lifetime_level_shows_as_samaras() {
    let b = grow("showcase");
    assert!(b.extras.level >= 3, "level {}", b.extras.level);
    let scene = gitbonsai::scene::Scene::build(&b);
    let samaras = scene.tufts.iter().filter(|t| t.samara).count();
    assert!(samaras >= 5, "{samaras} samaras");
}
