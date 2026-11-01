//! The growth model's promises, checked on every fixture.

use gitbonsai::config::Wind;
use gitbonsai::foliage::LanguageState;
use gitbonsai::growth::H_REF;
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

/// Next season's tree is this season's tree plus growth: no node ever moves,
/// and no branch ever carries less than before.
#[test]
fn wood_only_grows() {
    for name in [
        "veteran", "comeback", "polyglot", "bot", "mossgap", "steady",
    ] {
        let snapshot = fixtures::by_name(name).unwrap().snapshot;
        for style in Style::ALL {
            let config = config(style);
            let first = snapshot.first_active().unwrap();
            let mut day = first.add_days(13 * 7);
            let mut previous = Bonsai::grow(&snapshot, &config, Some(first)).skeleton;
            while day <= snapshot.observed_on {
                let now = Bonsai::grow(&snapshot, &config, Some(day)).skeleton;
                assert!(
                    now.nodes.len() >= previous.nodes.len(),
                    "{name}/{style} {day}: lost nodes"
                );
                for (old, new) in previous.nodes.iter().zip(&now.nodes) {
                    assert_eq!(
                        (old.p, old.parent),
                        (new.p, new.parent),
                        "{name}/{style} {day}: wood moved"
                    );
                }
                let (old_flow, new_flow) = (previous.flows(), now.flows());
                for (i, (a, b)) in old_flow.iter().zip(&new_flow).enumerate() {
                    assert!(b + 1e-6 >= *a, "{name}/{style} {day}: node {i} thinned");
                }
                assert!(now.height >= previous.height);
                previous = now;
                day = day.add_days(26 * 7);
            }
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

/// The veteran: tall from age, level 8, Ruby turned to jin.
#[test]
fn the_veteran_matches_worked_example_a() {
    let bonsai = grow("veteran");
    let height = bonsai.skeleton.height / H_REF;
    assert!((0.88..0.92).contains(&height), "{height}");
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
        private.skeleton.pads.is_empty(),
        "no languages, no language pads"
    );

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
