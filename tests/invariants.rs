//! The growth model's promises, checked on every fixture.

use gitbonsai::config::{Legend, Wind};
use gitbonsai::foliage::LanguageState;
use gitbonsai::growth::dome;
use gitbonsai::math::sq;
use gitbonsai::timeline::Timeline;
use gitbonsai::{Bonsai, Config, Style, Theme, fixtures, render};

fn config(style: Style) -> Config {
    let mut config = Config::default();
    config.tree.style = Some(style);
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

/// Mean distance from each cloud of `b` to the nearest cloud of `a`, both
/// ways: how far the crown moved between two frames.
fn crown_drift(a: &Bonsai, b: &Bonsai) -> f64 {
    let (ca, cb) = (&a.skeleton.clouds, &b.skeleton.clouds);
    // A resting tree's clouds are bare by then: the leaves test covers it.
    if ca.is_empty() || cb.is_empty() {
        return 0.0;
    }
    // Each cloud counts by its size: a tiny cloud just growing in moves
    // little of what shows.
    let near = |from: &[gitbonsai::growth::Cloud], to: &[gitbonsai::growth::Cloud]| {
        from.iter()
            .map(|p| {
                p.rx * to
                    .iter()
                    .map(|q| (sq(p.c.0 - q.c.0) + sq(p.c.1 - q.c.1)).sqrt())
                    .fold(f64::INFINITY, f64::min)
            })
            .sum::<f64>()
            / from.iter().map(|p| p.rx).sum::<f64>().max(1e-9)
    };
    f64::midpoint(near(ca, cb), near(cb, ca))
}

/// Wood as it shows, each piece by its thickness and length: (dead wood,
/// dead wood weighed by how bleached it is, all wood).
fn wood_mass(b: &Bonsai) -> (f64, f64, f64) {
    let sk = &b.skeleton;
    let flow = sk.flows();
    let (mut dead, mut white, mut all) = (0.0, 0.0, 0.0);
    for (i, n) in sk.nodes.iter().enumerate() {
        let Some(p) = n.parent else { continue };
        let q = sk.nodes[p].p;
        let m = gitbonsai::scene::radius(flow[i]) * (sq(n.p.0 - q.0) + sq(n.p.1 - q.1)).sqrt();
        all += m;
        if n.language.as_deref() == Some(gitbonsai::growth::JIN) {
            dead += m;
            white += m * n.bleach;
        }
    }
    (dead, white, all)
}

/// Monthly frames of a fixture's life, from its first active month.
fn monthly(name: &str) -> Vec<(gitbonsai::date::Date, Bonsai)> {
    let snapshot = fixtures::by_name(name).unwrap().snapshot;
    let (y, m, _) = snapshot.first_active().unwrap().ymd();
    (1..1200)
        .map(|k| {
            let i = y * 12 + m.cast_signed() - 1 + k;
            gitbonsai::date::Date::from_ymd(i / 12, (i % 12 + 1).cast_unsigned(), 1).unwrap()
        })
        .take_while(|d| *d <= snapshot.observed_on)
        .map(|d| (d, Bonsai::grow(&snapshot, &Config::default(), Some(d))))
        .collect()
}

/// A tree never shrinks as its style drifts: a trunk keeps the height of its
/// tallest month, however its style weighs its size since.
#[test]
fn a_tree_never_shrinks_as_its_style_drifts() {
    let mut shrunk = Vec::new();
    for name in [
        "steady", "showcase", "veteran", "comeback", "polyglot", "dormant", "mono", "private",
    ] {
        let mut tallest = 0.0_f64;
        for (day, b) in monthly(name) {
            let h = b.skeleton.height;
            if h < tallest - 0.5 {
                shrunk.push(format!(
                    "{name} {day}: {h:.1} below its tallest {tallest:.1}"
                ));
            }
            tallest = tallest.max(h);
        }
    }
    assert!(shrunk.is_empty(), "{shrunk:#?}");
}

/// The tree grows, it doesn't jump: month to month the height and the root's
/// girth never drop,
/// dead wood arrives a little at a time and is never cleared away, and the
/// crown moves only a little.
#[test]
fn growth_is_continuous_month_to_month() {
    let mut jumps = Vec::new();
    for name in [
        "steady", "showcase", "veteran", "comeback", "polyglot", "dormant",
    ] {
        let frames = monthly(name);
        for pair in frames.windows(2) {
            let ((_, a), (day, b)) = (&pair[0], &pair[1]);
            let (ha, hb) = (a.skeleton.height, b.skeleton.height);
            if hb < ha - 0.5 {
                jumps.push(format!("{name} {day}: height {ha:.1} -> {hb:.1}"));
            }
            // A trunk never shrinks: its root is never thinner than before.
            let (ga, gb) = (a.skeleton.flows()[0], b.skeleton.flows()[0]);
            if gb < ga * 0.999 {
                jumps.push(format!("{name} {day}: girth {ga:.0} -> {gb:.0}"));
            }
            // The style bends in a little each month as the last three years
            // change: never more than a few units a month, on top of
            // growth's own drift. The crown sits above the trunk top, so it
            // swings up to twice as far as its apex.
            let rise = gitbonsai::style::SOIL_Y - b.skeleton.apex.1;
            // A sprout still picking its side may swap it (a seedling's
            // small shift, `the_side_never_flips` after): the lean's size
            // still bends in slowly.
            let lean = if a.leader.side == b.leader.side {
                (b.leader.tilt - a.leader.tilt).abs()
            } else {
                (b.leader.tilt.abs() - a.leader.tilt.abs()).abs()
            } * rise;
            if lean > 6.0 {
                jumps.push(format!("{name} {day}: style leaned {lean:.1}"));
            }
            let drift = crown_drift(a, b);
            if drift > 8.0 + 2.0 * lean {
                jumps.push(format!("{name} {day}: crown moved {drift:.1}"));
            }
            // Dead wood dies in place and bleaches over a year: it whitens a
            // little each month, and never close to a third of it goes at
            // once. Wood counts by its size, and a change must show: 1% of all
            // the wood. 30% rather than a quarter: a large dead arm breaking
            // back on a tree with little other dead wood loses up to 30% in
            // its last month, which reads as weathering, not a jump.
            let ((da, wa, all), (db, wb, _)) = (wood_mass(a), wood_mass(b));
            let shows = 0.01 * all;
            if (da - db > 0.3 * da && da - db > shows) || wb - wa > 0.12 * db + shows {
                jumps.push(format!(
                    "{name} {day}: dead wood {da:.0} -> {db:.0}, bleached {wa:.0} -> {wb:.0}"
                ));
            }
        }
    }
    assert!(
        jumps.is_empty(),
        "{} jumps:\n{}",
        jumps.len(),
        jumps.join("\n")
    );
}

/// A resting tree loses its leaves over the year of silence, a little each
/// month, and stands bare when it goes dormant.
#[test]
fn leaves_fall_over_the_year_before_the_rest() {
    let leaves = |b: &Bonsai| gitbonsai::scene::Scene::build(b).tufts.len();
    let frames = monthly("dormant");
    let last = fixtures::by_name("dormant")
        .unwrap()
        .snapshot
        .days
        .iter()
        .rev()
        .find(|d| d.total > 0)
        .unwrap()
        .date;
    let after: Vec<(gitbonsai::date::Date, usize, bool)> = frames
        .iter()
        .filter(|(d, _)| *d > last)
        .map(|(d, b)| (*d, leaves(b), b.dormant()))
        .collect();
    let full = after[0].1;
    for pair in after.windows(2) {
        let ((_, a, _), (day, b, dormant)) = (pair[0], pair[1]);
        // The crown still widens a little with age while the leaves hold.
        assert!(b <= a + full / 20, "{day}: leaves came back without work");
        assert!(
            a.saturating_sub(b) <= full / 4,
            "{day}: {a} -> {b} leaves in one month of {full}"
        );
        if dormant {
            assert_eq!(b, 0, "{day}: a dormant tree is bare");
        }
    }
}

/// An account of a handful of contributions spread over eighteen years: a
/// day every four months, Ruby and Java in turn.
fn sparse() -> gitbonsai::Snapshot {
    use gitbonsai::date::Date;
    let mut s = dropped_trunk(1);
    let (start, end): (Date, Date) = ("2008-04-27".parse().unwrap(), s.observed_on);
    s.range.from = start;
    s.days.clear();
    let mut day = start;
    let mut k = 0;
    while day <= end {
        let lang = if k % 2 == 0 { "Ruby" } else { "Java" };
        s.days.push(gitbonsai::snapshot::Day {
            date: day,
            total: 2,
            by_language: [(lang.to_string(), 2)].into(),
            private: 0,
        });
        day = day.add_days(122);
        k += 1;
    }
    s
}

/// Size follows the work, not the calendar: a few contributions over many
/// years grow a small tree, not a tall bare one.
#[test]
fn a_sparse_old_account_is_a_small_tree() {
    let b = Bonsai::grow(&sparse(), &Config::default(), None);
    let busy = grow("steady");
    assert!(
        b.skeleton.height < busy.skeleton.height,
        "sparse {:.0} vs busy {:.0}",
        b.skeleton.height,
        busy.skeleton.height
    );
    // A trunk never shrinks, so it keeps the height of its tallest month.
    assert!(b.skeleton.height <= 105.0, "{:.0} tall", b.skeleton.height);
    let width = |b: &Bonsai| {
        let xs = b.skeleton.nodes.iter().map(|n| n.p.0);
        xs.clone().fold(f64::MIN, f64::max) - xs.fold(f64::MAX, f64::min)
    };
    assert!(
        width(&b) < width(&busy),
        "sparse {:.0} wide vs busy {:.0}",
        width(&b),
        width(&busy)
    );
}

/// Dead wood is a dead end: nothing living grows out of it. A dropped
/// branch takes everything on it down with it, and a language that comes
/// back sprouts from living wood.
#[test]
fn nothing_living_grows_from_dead_wood() {
    let dead = |n: &gitbonsai::growth::Node| n.language.as_deref() == Some(gitbonsai::growth::JIN);
    for name in ["veteran", "showcase", "comeback", "polyglot", "dormant"] {
        for (day, b) in monthly(name).iter().step_by(3) {
            let nodes = &b.skeleton.nodes;
            for (i, n) in nodes.iter().enumerate() {
                if let Some(p) = n.parent {
                    assert!(
                        !dead(&nodes[p]) || dead(n),
                        "{name} {day}: living node {i} grows from dead node {p}"
                    );
                }
            }
        }
    }
}

/// The seedling fills out into a tree over its second year: the crown at
/// most doubles from one month to the next, past its first few clouds.
#[test]
fn the_seedling_grows_into_a_tree_gradually() {
    for name in ["steady", "veteran", "comeback", "dormant"] {
        let frames = monthly(name);
        let clouds = |b: &Bonsai| -> f64 { b.skeleton.clouds.iter().map(|c| c.rx * c.ry).sum() };
        for pair in frames.windows(2).take(30) {
            let ((_, a), (day, b)) = (&pair[0], &pair[1]);
            let (ca, cb) = (clouds(a), clouds(b));
            assert!(
                cb <= ca * 2.0 + 150.0,
                "{name} {day}: crown {ca:.0} -> {cb:.0} in one month"
            );
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

/// The budget: at most 241 KB, 2,800 elements and 80 animated elements.
/// 0.3.0 raised it from 150 KB for crowns with 1.5 times the leaves; 0.4.0
/// from 190 KB for the windswept and formal upright crowns; 0.6.0 from
/// 215 KB and 2,500 elements by 12% for the new pots' detail.
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
                assert!(
                    svg.len() <= render::SVG_BUDGET,
                    "{label}: {} bytes",
                    svg.len()
                );
                assert!(elements <= 2800, "{label}: {elements} elements");
                assert!(animated <= 80, "{label}: {animated} animated");
            }
        }
    }
}

/// The heaviest tree synthetic data grows: sixteen years of daily work at
/// the weekly cap, a first language dropped after five years (dead wood),
/// then mostly private work beside two public languages, every sign and the
/// most kintsugi. Uncapped it weighs about 214 KB; it keeps within
/// [`render::SVG_BUDGET`].
#[test]
fn the_heaviest_tree_keeps_within_the_budget() {
    let mut s = fixtures::by_name("veteran").unwrap().snapshot;
    let from = s.observed_on.add_days(-16 * 365);
    s.range.from = from;
    s.days = (0..16 * 365)
        .map(|k| {
            let first = k < 5 * 365;
            gitbonsai::snapshot::Day {
                date: from.add_days(k + 1),
                total: 200,
                private: if first { 0 } else { 170 },
                by_language: if first {
                    [("Rust".to_string(), 200)].into()
                } else {
                    [("Go".to_string(), 20), ("Python".to_string(), 10)].into()
                },
            }
        })
        .collect();
    s.archived_repos = (0..40).map(|i| format!("archive-{i}")).collect();
    let config = Config::default();
    let bonsai = Bonsai::grow(&s, &config, None);
    for theme in Theme::ALL {
        let svg = render::svg(&bonsai, &config, theme);
        assert!(svg.len() <= render::SVG_BUDGET, "{} bytes", svg.len());
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
        "petunia",
        "begonia",
        "firefly",
        "glow",
        "pebble",
        "crane",
        "coin",
        "snow",
        "frost",
        "copter",
        "kintsugi",
        "achievement",
        "drip",
        "archived",
        "kabuto",
        "helmet",
        "weekend",
    ];
    for fixture in fixtures::all() {
        for side in [Legend::Left, Legend::Right] {
            let mut config = Config::default();
            config.render.legend = side;
            let bonsai = Bonsai::grow(&fixture.snapshot, &config, None);
            for theme in Theme::ALL {
                let svg = render::svg(&bonsai, &config, theme);
                let label = format!("{}/{side:?}/{}", fixture.name, theme.name());
                assert!(
                    svg.len() <= render::SVG_BUDGET,
                    "{label}: {} bytes",
                    svg.len()
                );
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
                        && bonsai.shows_leaves(language)
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
    // Observed in November: the autumn samara, not the spring cranes
    // (`seasonal_eggs_keep_their_season`).
    for egg in [
        "#fish4",
        "#goen",
        r#"rx="8.4" ry="1.1""#,
        "class=\"fs\"",
        "url(#pc)",
    ] {
        assert!(veteran.contains(egg), "veteran lacks {egg}");
    }
    let sprout = render("sprout");
    assert!(sprout.contains("#fish1\""));
    assert!(
        !sprout.contains("url(#pc)"),
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
    assert!(
        tiny.description().contains("just planted"),
        "not grown over 0 weeks"
    );
    let week = steady.truncated(steady.days[0].date.add_days(8));
    let week = Bonsai::grow(&week, &Config::default(), None);
    assert!(
        week.description().contains("grown over 1 week,"),
        "{}",
        week.description()
    );

    let dormant = grow("dormant");
    assert!(dormant.dormant());
    assert!(dormant.description().contains("Dormant"));
}

/// The description names the dead wood's languages, says "a bit of" for a
/// small share of the leaves, and says when the work is mostly private.
#[test]
fn the_description_names_what_the_tree_shows() {
    let showcase = grow("showcase").description();
    assert!(showcase.contains("C# turned to deadwood."), "{showcase}");
    let path = format!(
        "{}/site/snapshots/stevenbtw.snapshot.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let snapshot = gitbonsai::Snapshot::load(std::path::Path::new(&path)).unwrap();
    let mine = Bonsai::grow(&snapshot, &Config::default(), None).description();
    assert!(
        mine.contains("Leaves of Rust, Python, a bit of TypeScript."),
        "{mine}"
    );
    let private = grow("private").description();
    assert!(private.contains("Mostly private work."), "{private}");
}

#[test]
fn the_configured_wind_is_respected() {
    let snapshot = fixtures::by_name("steady").unwrap().snapshot;
    let mut config = config(Style::Fukinagashi);
    config.tree.wind = Some(Wind::Left);
    let bonsai = Bonsai::grow(&snapshot, &config, None);
    assert!(bonsai.leader.tilt < -0.5 && bonsai.leader.sweep < 0.0);
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
    assert!(sprout.skeleton.height >= 76.0, "a sprout is tall and thin");
}

#[test]
fn the_crown_holds_three_to_six_years() {
    let timeline = |name: &str| Timeline::new(&fixtures::by_name(name).unwrap().snapshot);
    let veteran = dome::crown_years(&timeline("veteran"), &[]);
    assert!((3..=6).contains(&veteran.len()), "{veteran:?}");
    // 2022 was led by Python with some Rust, but Rust became a trunk only in
    // December 2023: judged by the trunks of its day, 2022 is limb wood.
    assert_eq!(
        dome::crown_years(&timeline("comeback"), &[]),
        [2023, 2024, 2025, 2026]
    );
}

#[test]
fn a_pause_leaves_a_window() {
    let snapshot = fixtures::by_name("comeback").unwrap().snapshot;
    let (lo, hi) = dome::window_band(&Timeline::new(&snapshot)).expect("comeback paused");
    assert!(hi - lo > 10.0, "{lo}..{hi}");
    let bonsai = Bonsai::grow(&snapshot, &Config::default(), None);
    // The window grows and shrinks with the tree's style.
    let f = gitbonsai::style::size_factor(&bonsai.leader);
    let (lo, hi) = (lo * f, hi * f);
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
            private: 0,
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
        // Fireflies are a summer thing.
        let snapshot = fixtures::by_name(name).unwrap().snapshot;
        let summer = gitbonsai::date::Date::from_ymd(2026, 7, 15);
        let bonsai = Bonsai::grow(&snapshot, &Config::default(), summer);
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
        let (s, c) = (gitbonsai::math::sin(TILT), gitbonsai::math::cos(TILT));
        for (c3, rx, ry, rz, _, _) in &scene.masses {
            let (cx, cy, _) = project(*c3);
            let hw = rx * WORLD;
            let up = (gitbonsai::math::sq(ry * c) + gitbonsai::math::sq(rz * s)).sqrt() * WORLD;
            let down = rz * s * WORLD;
            let v = if y < cy { up } else { down };
            let inside = gitbonsai::math::sq((x - cx) / hw)
                + gitbonsai::math::sq((y - cy) / v.max(1e-6))
                < 1.0;
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

/// Work in the week under way brings a resting account's leaves back now,
/// not when the week completes.
#[test]
fn work_this_week_brings_the_leaves_back() {
    let mut s = fixtures::by_name("dormant").unwrap().snapshot;
    let mut day = s.observed_on.add_days(1);
    while day.days_since(gitbonsai::date::Date::monday_of(day.week())) != 2 {
        day = day.add_days(1);
    }
    s.days.push(gitbonsai::snapshot::Day {
        date: day,
        total: 3,
        by_language: std::collections::BTreeMap::new(),
        private: 3,
    });
    s.range.to = day;
    s.observed_on = day;
    let b = Bonsai::grow(&s, &Config::default(), None);
    assert!(
        (b.fullness(None) - 1.0).abs() < 1e-9,
        "{}",
        b.fullness(None)
    );
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

/// `explain` gives the tree's size on the canvas: the trunk at its foot, and
/// its wood and foliage taken together, so styles can be compared.
#[test]
fn explain_shows_the_trees_size() {
    let report = gitbonsai::explain::report(&grow("showcase"));
    let number = |key: &str| -> f64 {
        let line = report
            .lines()
            .find(|l| l.trim_start().starts_with(key))
            .unwrap_or_else(|| panic!("no {key} line in\n{report}"));
        line.trim_start()[key.len()..]
            .split_whitespace()
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("no number in {line}"))
    };
    assert!(number("trunk at its foot") > 1.0);
    assert!(number("size") > 100.0);
}

/// Kintsugi mends the pot it is on: its gold clips to the same wall the pot
/// is drawn with, oval, bowl or rectangle.
#[test]
fn kintsugi_follows_every_pots_wall() {
    let mut b = grow("veteran");
    assert_ne!(b.archived.len(), 0, "the veteran has archived repos");
    let clip = |svg: &str, id: &str| -> String {
        let at = svg
            .find(&format!(r#"<clipPath id="{id}"><path d=""#))
            .unwrap_or_else(|| panic!("no clip {id}"));
        let rest = &svg[at..];
        let start = rest.find(r#"<path d=""#).unwrap() + 9;
        rest[start..start + rest[start..].find('"').unwrap()].to_string()
    };
    for level in [1u8, 3, 6, 7, 8, 9, 10] {
        b.extras.pot_tier = level;
        let svg = render::svg(&b, &Config::default(), Theme::Light);
        assert_eq!(clip(&svg, "pc"), clip(&svg, "pw"), "level {level}");
    }
}

/// Each pot is mended its own way: nothing on the plastic training pot,
/// bare cracks in the terracotta, copper staples across the cracks in the
/// brown clay oval, silver laid in black lacquer on the vintage clay, gold
/// on the glazed pots.
#[test]
fn each_pot_is_mended_its_own_way() {
    let mut b = grow("veteran");
    for level in 0..11u8 {
        b.extras.pot_tier = level;
        let svg = render::svg(&b, &Config::default(), Theme::Light);
        let gold = svg.contains(r##"fill="#c9972f""##);
        let staples = svg.contains(r#"<g id="stp""#);
        let silver = svg.contains(r##"stroke="#a9aaa6""##);
        match level {
            0 => assert!(!svg.contains("url(#pc)"), "level {level} is mended"),
            8 => assert!(
                silver && !gold && !staples,
                "level {level}: silver {silver}"
            ),
            1 => assert!(
                svg.contains("url(#pc)") && !staples && !gold && !silver,
                "level {level}: staples {staples}, gold {gold}, silver {silver}"
            ),
            2 => assert!(
                staples && !gold && !silver,
                "level {level}: staples {staples}"
            ),
            _ => assert!(
                gold && !staples && !silver,
                "level {level}: gold {gold}, staples {staples}"
            ),
        }
    }
}

/// Two trees in one row, both in the hobnail bowl, keep every id their own.
#[test]
fn a_row_of_two_bowls_keeps_its_ids_unique() {
    let mut b = grow("veteran");
    b.extras.pot_tier = 7;
    let svg = render::svg(&b, &Config::default(), Theme::Light);
    let row = gitbonsai::row::row(
        &[("a".into(), svg.clone()), ("b".into(), svg)],
        Theme::Light,
    );
    let mut ids: Vec<&str> = row
        .split(r#"id=""#)
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), n, "an id appears twice in the row");
}

/// A tree lit from its other side mirrors its whole pot, the new pots' gloss
/// and chop included.
#[test]
fn a_left_lit_tree_mirrors_its_whole_pot() {
    let mut seen = 0;
    for f in fixtures::all() {
        let mut b = Bonsai::grow(&f.snapshot, &Config::default(), None);
        if b.is_empty() {
            continue;
        }
        for level in [8u8, 9] {
            b.extras.pot_tier = level;
            let svg = render::svg(&b, &Config::default(), Theme::Light);
            let Some(mirror) = svg.find("matrix(-1 0 0 1") else {
                continue;
            };
            seen += 1;
            let mark = if level == 8 {
                "class=\"chop\""
            } else {
                "class=\"gloss\""
            };
            let at = svg
                .find(mark)
                .unwrap_or_else(|| panic!("{} level {level}: no {mark}", f.name));
            assert!(
                at > mirror,
                "{} level {level}: {mark} drawn outside the mirror",
                f.name
            );
        }
    }
    assert!(seen > 0, "no fixture is lit from the left");
}

/// A tree with no contributions yet is a seedling in its training pot, with
/// a rim, in the middle of its frame.
#[test]
fn a_seedling_keeps_its_training_pot() {
    let mut snapshot = fixtures::by_name("steady").unwrap().snapshot;
    snapshot.days.clear();
    let b = Bonsai::grow(&snapshot, &Config::default(), None);
    assert!(b.is_empty());
    assert_eq!(b.extras.pot_tier, 0);
    let svg = render::svg(&b, &Config::default(), Theme::Light);
    assert!(
        svg.contains("M256 404Q254 396 257 389"),
        "the seedling's stem"
    );
    assert!(
        svg.contains(r##"fill="#2e2e2e""##),
        "the plastic training pot"
    );
}

/// A broad crown stands in a pot at least six tenths as wide as it spreads.
#[test]
fn a_broad_crown_gets_a_pot_at_least_six_tenths_its_spread() {
    for f in fixtures::all() {
        let b = Bonsai::grow(&f.snapshot, &Config::default(), None);
        if b.leader.cascade.abs() > 0.0 || b.is_empty() {
            continue;
        }
        let spread = b.crown_spread();
        assert!(
            b.pot >= 0.6 * spread - 1e-9,
            "{}: pot {} for a crown {spread:.0} wide",
            f.name,
            b.pot
        );
    }
}

/// `explain` names the tree's pot with its level.
#[test]
fn explain_names_the_pot() {
    let report = gitbonsai::explain::report(&grow("showcase"));
    let line = report
        .lines()
        .find(|l| l.trim_start().starts_with("pot "))
        .unwrap_or_else(|| panic!("no pot line in\n{report}"));
    assert!(line.contains("level 1, terracotta"), "{line}");
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
            // Private work: no language, and counted as private.
            d.by_language.clear();
            d.private = d.total;
        }
    }
    let config =
        Config::from_toml("[leaves.private_periods]\n\"2026-06..2026-12\" = [\"C#\"]\n").unwrap();
    let b = Bonsai::grow(&snapshot, &config, None);
    assert_ne!(b.state(Some("C#")), LanguageState::Jin);
    let report = gitbonsai::explain::report(&b);
    assert!(!report.contains("C# (dead)"), "{report}");
}

/// The legend names a dropped language's deadwood only when the tree draws
/// it: a few contributions long ago can be dropped without any wood to show.
#[test]
fn the_legend_names_only_deadwood_the_tree_shows() {
    let snapshot = fixtures::by_name("showcase").unwrap().snapshot;
    let mut config = Config::default();
    config.render.legend = Legend::Left;
    let mut b = Bonsai::grow(&snapshot, &config, None);
    assert_eq!(b.state(Some("C#")), LanguageState::Jin);
    assert!(
        b.skeleton.deadwood.contains("C#"),
        "the dead C# trunk shows"
    );
    let svg = render::svg(&b, &config, Theme::Light);
    assert!(svg.contains("deadwood: C#"));
    // The same dropped language, with no dead wood drawn.
    b.skeleton.deadwood.clear();
    let svg = render::svg(&b, &config, Theme::Light);
    assert!(!svg.contains("deadwood"), "nothing dead to point at");
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
    // Around the one-year mark the legend says winter exactly when the
    // drawing has no leaves.
    for k in 355..=372 {
        let b = Bonsai::grow(&later, &config, Some(last.add_days(k)));
        let svg = render::svg(&b, &config, Theme::Light).to_lowercase();
        let leaves = gitbonsai::scene::Scene::build(&b).tufts.len();
        assert_eq!(
            svg.contains("winter"),
            leaves == 0,
            "day {k}: {leaves} leaves"
        );
        assert_eq!(
            b.dormant(),
            k >= 364,
            "day {k}: a year without contributions"
        );
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

/// A language first used this week, before the week is complete, already
/// has leaves, so it is alive, not resting.
#[test]
fn a_language_picked_up_this_week_is_alive() {
    let mut snapshot = fixtures::by_name("mono").unwrap().snapshot;
    let day = snapshot.observed_on;
    let entry = snapshot.days.iter_mut().find(|d| d.date == day);
    let mut by_language = std::collections::BTreeMap::new();
    by_language.insert("Zig".to_string(), 5);
    match entry {
        Some(d) => {
            d.total += 5;
            d.by_language.insert("Zig".to_string(), 5);
        }
        None => snapshot.days.push(gitbonsai::snapshot::Day {
            date: day,
            total: 5,
            by_language,
            private: 0,
        }),
    }
    let b = Bonsai::grow(&snapshot, &Config::default(), None);
    assert!(b.window.of_language(Some("Zig")) > 0.0, "the window has it");
    assert_eq!(b.state(Some("Zig")), LanguageState::Alive);
}

/// Kintsugi mends the pot under its carved bands and maker's seal: the gold
/// never runs across them.
#[test]
fn kintsugi_runs_under_the_bands_and_seal() {
    let mut b = grow("veteran");
    assert_ne!(b.archived.len(), 0, "the veteran has archived repos");
    // The antique pot: its carved bands and maker's seal.
    b.extras.pot_tier = 6;
    let svg = render::svg(&b, &Config::default(), Theme::Light);
    let gold = svg.find(r#"clip-path="url(#pc)""#).expect("kintsugi");
    let seal = svg.find(r##"fill="#7a2e22""##).expect("a maker's seal");
    assert!(seal > gold, "the seal is drawn over the gold");
    let band = svg.rfind(r#"stroke-width=".9"/>"#).expect("carved bands");
    assert!(band > gold, "the bands are drawn over the gold");
}

/// A small account that dropped its second trunk language, as in a review:
/// Rust every week since 2023, Go beside it until mid-2025.
fn dropped_trunk(per_week: u32) -> gitbonsai::Snapshot {
    use gitbonsai::date::{Date, DateRange};
    use gitbonsai::snapshot::{Day, SCHEMA};
    let (start, end): (Date, Date) = ("2023-01-02".parse().unwrap(), "2026-11-16".parse().unwrap());
    let stop_go: Date = "2025-06-30".parse().unwrap();
    let mut days = Vec::new();
    let mut week = start;
    while week <= end {
        for k in 0..per_week {
            let day = week.add_days(i32::try_from(k).unwrap() * 3);
            if day > end {
                continue;
            }
            let mut by_language = std::collections::BTreeMap::new();
            by_language.insert("Rust".to_string(), 1);
            if day <= stop_go {
                by_language.insert("Go".to_string(), 1);
            }
            let total = by_language.values().sum();
            days.push(Day {
                date: day,
                total,
                by_language,
                private: 0,
            });
        }
        week = week.add_days(7);
    }
    days.sort_by_key(|d| d.date);
    days.dedup_by_key(|d| d.date);
    gitbonsai::Snapshot {
        schema: SCHEMA,
        login: "small".into(),
        account_id: 12345,
        range: DateRange {
            from: "2023-01-01".parse().unwrap(),
            to: end,
        },
        observed_on: end,
        collected_with: "test".into(),
        language_colors: std::collections::BTreeMap::new(),
        days,
        truncated_windows: Vec::new(),
        achievements: std::collections::BTreeMap::new(),
        archived_repos: Vec::new(),
        achievement_dates: std::collections::BTreeMap::new(),
        archived_dates: std::collections::BTreeMap::new(),
        start: None,
    }
}

/// A cascade's leaves grow on its wood: spread along its pads, no cloud
/// reaches past the wood in it by more than a pad's open edge.
#[test]
fn a_cascade_keeps_its_leaves_on_wood() {
    let mut trees: Vec<(String, gitbonsai::Snapshot)> = fixtures::all()
        .into_iter()
        .map(|f| (f.name.to_string(), f.snapshot))
        .collect();
    for name in ["jacobwilliams", "steipete", "stevenbtw", "tj", "torvalds"] {
        let path = format!(
            "{}/site/snapshots/{name}.snapshot.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let s = gitbonsai::Snapshot::load(std::path::Path::new(&path)).unwrap();
        trees.push((name.to_string(), s));
    }
    for (name, snapshot) in &trees {
        let b = Bonsai::grow(snapshot, &config(Style::Kengai), None);
        let sk = &b.skeleton;
        for cl in &sk.clouds {
            let near = |n: &&gitbonsai::growth::Node| (n.p.1 - cl.c.1).abs() <= cl.ry + 8.0;
            let wood = |side: f64| {
                sk.nodes
                    .iter()
                    .filter(near)
                    .map(|n| side * (n.p.0 - cl.c.0))
                    .fold(f64::NEG_INFINITY, f64::max)
            };
            // A young cascade's pads are still easing in: a little slack.
            let open = 8.0_f64.max(0.4 * cl.rx) + 5.0;
            for side in [-1.0, 1.0] {
                assert!(
                    wood(side) + open >= cl.rx,
                    "{name}: a cloud at ({:.0}, {:.0}) reaches {:.0} past its wood",
                    cl.c.0,
                    cl.c.1,
                    cl.rx - wood(side)
                );
            }
        }
    }
}

/// A dropped language grows no living wood: its arms died, and its cloud
/// keeps its place bare, with no twigs hung from the living wood near it.
#[test]
fn a_dropped_language_grows_no_living_wood() {
    let mut trees: Vec<(String, Bonsai)> = fixtures::all()
        .into_iter()
        .map(|f| {
            (
                f.name.to_string(),
                Bonsai::grow(&f.snapshot, &Config::default(), None),
            )
        })
        .collect();
    for per in [1, 2, 4] {
        trees.push((
            format!("dropped trunk, {per} a week"),
            Bonsai::grow(&dropped_trunk(per), &Config::default(), None),
        ));
    }
    for (name, b) in &trees {
        let living = b
            .skeleton
            .nodes
            .iter()
            .filter(|n| {
                n.language
                    .as_deref()
                    .is_some_and(|l| b.state(Some(l)) == gitbonsai::foliage::LanguageState::Jin)
            })
            .count();
        assert_eq!(
            living, 0,
            "{name}: {living} living nodes of a dropped language"
        );
    }
}

/// Dead wood never outweighs the living wood it grows beside: wherever a
/// branch splits into living and dead, the heaviest living part is heavier,
/// so the trunk is always the living one, however small the account.
#[test]
fn dead_wood_never_outweighs_the_living() {
    let mut trees: Vec<(String, Bonsai)> = fixtures::all()
        .into_iter()
        .map(|f| {
            (
                f.name.to_string(),
                Bonsai::grow(&f.snapshot, &Config::default(), None),
            )
        })
        .collect();
    for per in [1, 2] {
        trees.push((
            format!("dropped trunk, {per} a week"),
            Bonsai::grow(&dropped_trunk(per), &Config::default(), None),
        ));
    }
    for (name, b) in &trees {
        let sk = &b.skeleton;
        let flow = sk.flows();
        let dead = |i: usize| sk.nodes[i].language.as_deref() == Some(gitbonsai::growth::JIN);
        let mut kids: Vec<Vec<usize>> = vec![Vec::new(); sk.nodes.len()];
        for (i, n) in sk.nodes.iter().enumerate() {
            if let Some(p) = n.parent {
                kids[p].push(i);
            }
        }
        for (p, ks) in kids.iter().enumerate() {
            if dead(p) {
                continue;
            }
            let living = ks
                .iter()
                .filter(|&&k| !dead(k))
                .map(|&k| flow[k])
                .fold(0.0, f64::max);
            let jin = ks
                .iter()
                .filter(|&&k| dead(k))
                .map(|&k| flow[k])
                .fold(0.0, f64::max);
            if living > 0.0 && jin > 0.0 {
                assert!(
                    living > jin,
                    "{name}: node {p}: dead {jin:.0} outweighs living {living:.0}"
                );
            }
        }
    }
}

/// A dropped language is deadwood only: no cloud carries its leaves, in the
/// crown or anywhere else.
#[test]
fn a_dropped_language_grows_no_leaves() {
    let mut trees: Vec<(String, Bonsai)> = fixtures::all()
        .into_iter()
        .map(|f| {
            (
                f.name.to_string(),
                Bonsai::grow(&f.snapshot, &Config::default(), None),
            )
        })
        .collect();
    for per in [1, 2] {
        trees.push((
            format!("dropped trunk, {per} a week"),
            Bonsai::grow(&dropped_trunk(per), &Config::default(), None),
        ));
    }
    // A dropped language's clouds keep their places in the crown, bare.
    for (name, b) in &trees {
        for c in &b.skeleton.clouds {
            for p in &c.patches {
                if let Some(l) = p.2.as_deref()
                    && b.state(Some(l)) == LanguageState::Jin
                {
                    assert_eq!(b.fullness(Some(l)), 0.0, "{name}: {l} leaves on a cloud");
                }
            }
        }
    }
}

/// One trunk language, dropped while it still held most of the crown years,
/// and several small living languages after it, none with 18% of the crown.
fn dropped_lead() -> gitbonsai::Snapshot {
    use gitbonsai::date::Date;
    let mut s = dropped_trunk(2);
    let switch: Date = "2025-06-30".parse().unwrap();
    let small = ["Rust", "Zig", "Lua", "Nim", "Elm", "Ruby"];
    for (i, d) in s.days.iter_mut().enumerate() {
        d.by_language.clear();
        if d.date <= switch {
            d.by_language.insert("Go".to_string(), 3);
        } else {
            d.by_language.insert(small[i % small.len()].to_string(), 1);
        }
        d.total = d.by_language.values().sum();
    }
    s
}

#[test]
fn a_dropped_lead_hands_the_crown_to_the_living() {
    let b = Bonsai::grow(&dropped_lead(), &Config::default(), None);
    assert_eq!(b.state(Some("Go")), LanguageState::Jin, "Go is dropped");
    // Go's clouds keep their places in the crown, bare: none of its leaves
    // are drawn, and the living languages crown the tree.
    let langs: std::collections::BTreeSet<String> = b
        .skeleton
        .clouds
        .iter()
        .flat_map(|c| c.patches.iter().filter_map(|p| p.2.clone()))
        .filter(|l| b.state(Some(l)) != LanguageState::Jin)
        .collect();
    assert!(!langs.is_empty(), "a living language crowns the tree");
    assert_eq!(b.fullness(Some("Go")), 0.0, "no Go leaves");
}

/// Every side branch starts on its parent's surface and reaches back inside
/// at most half the parent's girth, no wider than the parent: it can leave
/// no gap at the join, and never shows through the parent's far side.
#[test]
fn branches_start_inside_their_parent() {
    for fixture in fixtures::all() {
        let snapshot = &fixture.snapshot;
        let Some(first) = snapshot.first_active() else {
            continue;
        };
        let span = snapshot.observed_on.days_since(first);
        for k in 1..=4 {
            let day = first.add_days(span * k / 4);
            let b = Bonsai::grow(snapshot, &Config::default(), Some(day));
            let scene = gitbonsai::scene::Scene::build(&b);
            for (ci, (r, fork, _)) in &scene.joins {
                if scene.strands.contains(ci) {
                    continue;
                }
                let (p, w) = scene.chains[*ci].0[0];
                let off = (sq(p[0] - fork[0]) + sq(p[1] - fork[1])).sqrt();
                let label = format!("{} on {day}, chain {ci}", fixture.name);
                assert!(
                    off <= r + 1e-6,
                    "{label}: starts outside its parent ({off} > {r})"
                );
                assert!(off >= 0.5 * r - 1e-6, "{label}: reaches {off} of {r} deep");
                assert!(w <= *r + 1e-6, "{label}: {w} wide on a parent of {r}");
            }
        }
    }
}

/// No living wood ends blunt: every chain end lies in a cloud's leaf mass,
/// where it can't be seen, or the wood tapers gently to a twig before it
/// ends. Else a thick limb or trunk top reads as a sawn-off stump against
/// the sky. Nor does a twig run through the leaves and out the other side,
/// a bare stick poking out of the crown. Dead wood may end snapped.
#[test]
fn no_wood_ends_blunt() {
    // Thicker than a twig (world units, before the tip's taper) is wood a
    // stump shows on; a gentle taper thins by at most `SLOPE` a unit of length.
    const TWIG: f64 = gitbonsai::scene::TWIG;
    const SLOPE: f64 = 0.15;
    // Judged over the last stretch: lower down wood may taper as it grew.
    const BACK: f64 = 30.0;
    let mut trees: Vec<(String, gitbonsai::Snapshot)> = fixtures::all()
        .into_iter()
        .map(|f| (f.name.to_string(), f.snapshot))
        .collect();
    for name in ["steipete", "stevenbtw", "tj", "torvalds"] {
        let path = format!(
            "{}/site/snapshots/{name}.snapshot.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let s = gitbonsai::Snapshot::load(std::path::Path::new(&path)).unwrap();
        trees.push((name.to_string(), s));
    }
    let mut blunt = Vec::new();
    for (name, snapshot) in &trees {
        for style in std::iter::once(None).chain(Style::ALL.into_iter().map(Some)) {
            let config = style.map_or_else(Config::default, config);
            let b = Bonsai::grow(snapshot, &config, None);
            let scene = gitbonsai::scene::Scene::build(&b);
            // How deep in a leaf mass a point lies: 0 at its center, 1 on its
            // rim; a dome has no leaves below its flat bottom.
            let depth = |e: [f64; 3]| {
                scene
                    .pads
                    .iter()
                    .filter(|(c, ..)| e[1] >= c[1])
                    .map(|(c, rx, ry)| (sq((e[0] - c[0]) / rx) + sq((e[1] - c[1]) / ry)).sqrt())
                    .fold(f64::INFINITY, f64::min)
            };
            for (ci, (wood, dead)) in scene.chains.iter().enumerate() {
                let Some(&(e, re)) = wood.last() else {
                    continue;
                };
                // An end inside other wood doesn't show.
                if dead.is_some() || gitbonsai::scene::in_wood(&scene.chains, ci, e, re) {
                    continue;
                }
                // How thick the wood is near its end, less what a gentle taper
                // would thin it by on the way: the girth the end shows.
                let mut arc = 0.0;
                let mut r = 0.0f64;
                for j in (0..wood.len()).rev() {
                    if j + 1 < wood.len() {
                        let (a, b) = (wood[j].0, wood[j + 1].0);
                        arc += (sq(b[0] - a[0]) + sq(b[1] - a[1])).sqrt();
                    }
                    if arc > BACK {
                        break;
                    }
                    r = r.max(wood[j].1 - SLOPE * arc);
                }
                let d = depth(e);
                let style = style.map_or("own".to_string(), |s| s.to_string());
                let at = format!("{name}/{style} chain {ci} at ({:.0}, {:.0})", e[0], e[1]);
                if r > TWIG && d > 0.85 {
                    blunt.push(format!("{at}: ends {r:.1} thick, depth {d:.2}"));
                } else if d > 1.3 && wood[1..].iter().any(|&(p, _)| depth(p) <= 0.85) {
                    blunt.push(format!("{at}: a twig through the leaves, depth {d:.2}"));
                }
            }
        }
    }
    assert!(
        blunt.is_empty(),
        "{} blunt ends:\n{}",
        blunt.len(),
        blunt.join("\n")
    );
}

/// A cascade's legacy trunk turns to deadwood over a year and a half once a
/// second language's trunk lies along it: it whitens a little each month,
/// never all at once.
#[test]
fn a_legacy_trunk_dies_slowly() {
    let path = format!(
        "{}/site/snapshots/jacobwilliams.snapshot.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let snapshot = gitbonsai::Snapshot::load(std::path::Path::new(&path)).unwrap();
    let (y, m, _) = snapshot.first_active().unwrap().ymd();
    let mut last = 0.0_f64;
    for k in 1..1200 {
        let i = y * 12 + m.cast_signed() - 1 + k;
        let d = gitbonsai::date::Date::from_ymd(i / 12, (i % 12 + 1).cast_unsigned(), 1).unwrap();
        if d > snapshot.observed_on {
            break;
        }
        let jin = Bonsai::grow(&snapshot, &Config::default(), Some(d))
            .skeleton
            .legacy_jin;
        assert!(
            jin - last <= 0.1,
            "{d}: legacy deadwood {last:.2} -> {jin:.2}"
        );
        last = jin;
    }
    // Python came along in early 2026: by late 2026 it is over half dead,
    // and white by mid-2027.
    assert!(
        (0.4..0.8).contains(&last),
        "the legacy trunk is dying: {last:.2}"
    );
}

/// The side a tree streams to is its sprout's: once it has 60
/// contributions, no month moves it to the other side.
#[test]
fn the_side_never_flips() {
    let path = format!(
        "{}/site/snapshots/jacobwilliams.snapshot.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut snapshots: Vec<(String, gitbonsai::Snapshot)> = fixtures::all()
        .into_iter()
        .map(|f| (f.name.to_string(), f.snapshot))
        .collect();
    snapshots.push((
        "jacobwilliams".into(),
        gitbonsai::Snapshot::load(std::path::Path::new(&path)).unwrap(),
    ));
    for (name, snapshot) in snapshots {
        let mut seen = 0;
        let Some(sixtieth) = snapshot.days.iter().find_map(|d| {
            seen += d.total;
            (seen >= 60).then_some(d.date)
        }) else {
            continue;
        };
        let (y, m, _) = sixtieth.ymd();
        let mut sides = (0..1200)
            .map(|k| {
                let i = y * 12 + m.cast_signed() + k;
                gitbonsai::date::Date::from_ymd(i / 12, (i % 12 + 1).cast_unsigned(), 1).unwrap()
            })
            .take_while(|d| *d <= snapshot.observed_on)
            .map(|d| {
                (
                    d,
                    Bonsai::grow(&snapshot, &Config::default(), Some(d))
                        .leader
                        .side,
                )
            });
        let Some((_, first)) = sides.next() else {
            continue;
        };
        for (d, side) in sides {
            assert_eq!(side, first, "{name} {d}");
        }
    }
}

/// The same tree on the left is the right one's mirror image about the
/// pot's center line: every piece of wood, every pad and every leaf.
#[test]
fn a_left_tree_mirrors_the_right() {
    use gitbonsai::growth::dome::Reach;
    use gitbonsai::scene::Scene;
    use gitbonsai::style::Leader;
    let path = format!(
        "{}/site/snapshots/jacobwilliams.snapshot.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let jacob = gitbonsai::Snapshot::load(std::path::Path::new(&path)).unwrap();
    for (name, snapshot, style) in [
        ("jacobwilliams", jacob.clone(), Style::Kengai),
        (
            "legacy",
            fixtures::by_name("legacy").unwrap().snapshot,
            Style::Kengai,
        ),
        ("jacobwilliams", jacob, Style::Fukinagashi),
        (
            "polyglot",
            fixtures::by_name("polyglot").unwrap().snapshot,
            Style::Shakan,
        ),
    ] {
        let base = Bonsai::grow(&snapshot, &config(style), None);
        let on = |side: f64| {
            let mut b = base.clone();
            b.leader = Leader::new(style, None, side);
            b.skeleton = gitbonsai::growth::grow(
                &b.timeline,
                b.account_id,
                &b.leader,
                &[],
                Reach::default(),
                None,
            );
            Scene::build(&b)
        };
        let (right, left) = (on(1.0), on(-1.0));
        let mirrors = |a: [f64; 3], b: [f64; 3], what: &str| {
            assert!(
                (a[0] + b[0]).abs() < 1e-6
                    && (a[1] - b[1]).abs() < 1e-6
                    && (a[2] - b[2]).abs() < 1e-6,
                "{name} {style:?} {what}: {a:?} vs {b:?}"
            );
        };
        assert_eq!(right.chains.len(), left.chains.len());
        for (r, l) in right.chains.iter().zip(&left.chains) {
            assert_eq!(r.0.len(), l.0.len());
            for (p, q) in r.0.iter().zip(&l.0) {
                mirrors(p.0, q.0, "wood");
            }
        }
        // The cascade spills to its side, the foot stands off center on the other.
        if style == Style::Kengai {
            let (foot, tip) = (right.chains[0].0[0].0, right.chains[0].0.last().unwrap().0);
            assert!(foot[0] < 0.0 && tip[0] > 0.0, "{name}: {foot:?} {tip:?}");
        }
        assert_eq!(right.pads.len(), left.pads.len());
        for (r, l) in right.pads.iter().zip(&left.pads) {
            mirrors(r.0, l.0, "pad");
        }
        assert_eq!(right.tufts.len(), left.tufts.len());
        for (r, l) in right.tufts.iter().zip(&left.tufts) {
            mirrors(r.p, l.p, "leaf");
            mirrors(r.n, l.n, "leaf normal");
        }
    }
}

/// A semi-cascade's white trunk reads as an honor, not as a dropped
/// language, and the legend names only leaves the crown draws: Fortran's
/// trunk gets its own row and line, and JavaScript, with no cloud of its
/// own, gets no leaf.
#[test]
fn the_white_trunk_reads_as_a_classic_still_growing() {
    let path = format!(
        "{}/site/snapshots/jacobwilliams.snapshot.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let snapshot = gitbonsai::Snapshot::load(std::path::Path::new(&path)).unwrap();
    let mut config = Config::default();
    config.render.legend = gitbonsai::config::Legend::Left;
    let bonsai = Bonsai::grow(&snapshot, &config, None);
    let svg = render::svg(&bonsai, &config, Theme::Light);
    assert!(
        svg.contains("white trunk: Fortran, still growing"),
        "no white trunk row"
    );
    assert!(
        !svg.contains(">JavaScript<"),
        "JavaScript has no leaves to show"
    );
    let text = bonsai.description();
    assert!(
        text.contains("A white Fortran trunk: a classic language, still growing."),
        "{text}"
    );
}

/// A windswept crown may hang low downwind, but only a cascade goes past
/// the rim: every leaf of a windswept tree stays above it.
#[test]
fn a_windswept_crown_stays_above_the_rim() {
    let mut trees: Vec<(String, gitbonsai::Snapshot)> = fixtures::all()
        .into_iter()
        .map(|f| (f.name.to_string(), f.snapshot))
        .collect();
    for name in ["jacobwilliams", "steipete", "stevenbtw", "torvalds"] {
        let path = format!(
            "{}/site/snapshots/{name}.snapshot.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let s = gitbonsai::Snapshot::load(std::path::Path::new(&path)).unwrap();
        trees.push((name.to_string(), s));
    }
    for (name, snapshot) in &trees {
        let b = Bonsai::grow(snapshot, &config(Style::Fukinagashi), None);
        let scene = gitbonsai::scene::Scene::build(&b);
        // Scene heights are above the soil, where the rim lies.
        let low = scene
            .tufts
            .iter()
            .map(|t| t.p[1])
            .fold(f64::INFINITY, f64::min);
        assert!(low > 0.0, "{name}: a leaf {low:.1} below the rim");
    }
}

/// Private work stays in the skirt and the inner crown: the top of a crown
/// belongs to a public language whenever the tree has one.
#[test]
fn private_work_never_tops_the_crown() {
    use gitbonsai::growth::clouds::lead_of;
    let mut trees: Vec<(String, gitbonsai::Snapshot)> = fixtures::all()
        .into_iter()
        .map(|f| (f.name.to_string(), f.snapshot))
        .collect();
    for name in ["jacobwilliams", "steipete", "stevenbtw", "tj", "torvalds"] {
        let path = format!(
            "{}/site/snapshots/{name}.snapshot.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let s = gitbonsai::Snapshot::load(std::path::Path::new(&path)).unwrap();
        trees.push((name.to_string(), s));
    }
    let mut bad = Vec::new();
    for (name, snapshot) in &trees {
        let styles = std::iter::once(None).chain(Style::ALL.into_iter().map(Some));
        for style in styles {
            let b = Bonsai::grow(snapshot, &style.map_or_else(Config::default, config), None);
            let clouds = &b.skeleton.clouds;
            if clouds.iter().all(|c| lead_of(c).is_none()) {
                continue;
            }
            // Heights grow upward on the canvas as y shrinks.
            let top = clouds
                .iter()
                .min_by(|a, b| (a.c.1 - a.ry).total_cmp(&(b.c.1 - b.ry)));
            if top.is_some_and(|c| lead_of(c).is_none()) {
                bad.push(format!("{name} {style:?}"));
            }
        }
    }
    assert!(bad.is_empty(), "private work tops: {bad:?}");
}

/// How much of each language the picture shows: a coarse raster of the
/// leaves, each cell keeping the frontmost leaf, counted by the language
/// whose color that leaf is nearest. Private work counts as "".
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "canvas cells"
)]
fn visible_leaves(b: &Bonsai) -> std::collections::BTreeMap<String, usize> {
    // Canvas units a cell is wide and high.
    const CELL: f64 = 2.0;
    use gitbonsai::scene::{Scene, WORLD, project};
    let scene = Scene::build(b);
    let langs: Vec<String> = b
        .order
        .iter()
        .filter(|l| b.shows_leaves(l))
        .cloned()
        .collect();
    let mut front: std::collections::BTreeMap<(i64, i64), (f64, String)> = Default::default();
    for t in &scene.tufts {
        let lang = if t.private {
            String::new()
        } else {
            let d = |l: &String| {
                let c = b.maple.leaf(Some(l), 0.5);
                sq(f64::from(c.r) - f64::from(t.color.r))
                    + sq(f64::from(c.g) - f64::from(t.color.g))
                    + sq(f64::from(c.b) - f64::from(t.color.b))
            };
            langs
                .iter()
                .min_by(|a, b| d(a).total_cmp(&d(b)))
                .cloned()
                .unwrap_or_default()
        };
        let (x, y, depth) = project(t.p);
        let depth = depth + t.lift;
        let r = 4.0 * WORLD * t.size;
        let cell = |v: f64| (v / CELL).floor() as i64;
        for cx in cell(x - r)..=cell(x + r) {
            for cy in cell(y - r)..=cell(y + r) {
                let at = |c: i64| (c as f64 + 0.5) * CELL;
                if sq(at(cx) - x) + sq(at(cy) - y) > r * r {
                    continue;
                }
                let e = front
                    .entry((cx, cy))
                    .or_insert((f64::NEG_INFINITY, String::new()));
                if depth > e.0 {
                    *e = (depth, lang.clone());
                }
            }
        }
    }
    let mut seen = std::collections::BTreeMap::new();
    for (_, l) in front.into_values() {
        *seen.entry(l).or_insert(0) += 1;
    }
    seen
}

/// Every language the crown draws shows: a small language's cloud is never
/// buried behind a big one's.
#[test]
fn every_language_in_the_crown_shows() {
    let mut trees: Vec<(String, gitbonsai::Snapshot)> = fixtures::all()
        .into_iter()
        .map(|f| (f.name.to_string(), f.snapshot))
        .collect();
    for name in ["jacobwilliams", "steipete", "stevenbtw", "tj", "torvalds"] {
        let path = format!(
            "{}/site/snapshots/{name}.snapshot.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let s = gitbonsai::Snapshot::load(std::path::Path::new(&path)).unwrap();
        trees.push((name.to_string(), s));
    }
    // Cells of 2 by 2: at least a small patch of leaves, about 15 by 15.
    const SHOWN: usize = 60;
    let mut hidden = Vec::new();
    for (name, snapshot) in &trees {
        for style in std::iter::once(None).chain(Style::ALL.into_iter().map(Some)) {
            let b = Bonsai::grow(snapshot, &style.map_or_else(Config::default, config), None);
            let seen = visible_leaves(&b);
            for l in b.order.iter().filter(|l| b.shows_leaves(l)) {
                let n = seen.get(l).copied().unwrap_or(0);
                if n < SHOWN {
                    hidden.push(format!("{name} {style:?} {l}: {n}"));
                }
            }
        }
    }
    assert!(hidden.is_empty(), "hidden: {hidden:?}");
}

/// A cascade carries the foliage its work earned: gathered into pads along
/// the arc, its clouds lie side by side, not stacked on one another.
#[test]
fn a_cascade_shows_its_foliage() {
    let mut trees: Vec<(String, gitbonsai::Snapshot)> = ["legacy", "showcase", "veteran"]
        .into_iter()
        .map(|n| (n.to_string(), fixtures::by_name(n).unwrap().snapshot))
        .collect();
    for name in ["jacobwilliams", "stevenbtw", "torvalds"] {
        let path = format!(
            "{}/site/snapshots/{name}.snapshot.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let s = gitbonsai::Snapshot::load(std::path::Path::new(&path)).unwrap();
        trees.push((name.to_string(), s));
    }
    // Leaves shown, as if each style grew to the same scale: the styles'
    // sizes are balanced apart from how a cascade carries its foliage.
    let shown = |s: &gitbonsai::Snapshot, style: Style| -> f64 {
        let b = Bonsai::grow(s, &config(style), None);
        let n: usize = visible_leaves(&b).values().sum();
        let f = gitbonsai::style::size_factor(&b.leader);
        n as f64 / (f * f)
    };
    let mut thin = Vec::new();
    for (name, s) in &trees {
        let (cascade, upright) = (shown(s, Style::Kengai), shown(s, Style::Moyogi));
        let share = cascade / upright;
        // Stacked in its pads it showed 22 to 36% (0.4.0); laid side by side
        // it shows 40 to 54%. An arc never carries as much as a crown.
        if share < 0.38 {
            thin.push(format!("{name}: {share:.2}"));
        }
    }
    assert!(thin.is_empty(), "thin cascades: {thin:?}");
}

/// A left tree is lit from its open side, as the right one's mirror image:
/// its eggs lie mirrored about the pot's center line, and its galaxy plant
/// peeks out on the light's side: left of the trunk on a right tree, right of
/// it on a left one.
#[test]
fn a_left_tree_is_furnished_as_the_mirror_image() {
    use gitbonsai::growth::dome::Reach;
    use gitbonsai::style::Leader;
    let snapshot = fixtures::by_name("veteran").unwrap().snapshot;
    let config = Config::default();
    let base = Bonsai::grow(&snapshot, &config, None);
    let svg = |side: f64| {
        let mut b = base.clone();
        b.leader = Leader::new(b.leader.style, None, side);
        b.skeleton = gitbonsai::growth::grow(
            &b.timeline,
            b.account_id,
            &b.leader,
            &[],
            Reach::default(),
            None,
        );
        render::svg(&b, &config, Theme::Light)
    };
    let (right, left) = (svg(1.0), svg(-1.0));
    // Where an egg is drawn, before any mirroring, and whether a mirror
    // group holds it.
    let at = |svg: &str, egg: &str| -> (f64, f64, bool) {
        let i = svg.find(egg).unwrap_or_else(|| panic!("no {egg}"));
        let head = &svg[..i];
        let t = head.rfind("translate(").unwrap() + "translate(".len();
        let xy: Vec<f64> = head[t..]
            .split(')')
            .next()
            .unwrap()
            .split(' ')
            .map(|v| v.parse().unwrap())
            .collect();
        let mirrored = head
            .rfind(r#"<g transform="matrix(-1 0 0 1 512 0)">"#)
            .is_some_and(|m| head[m..].matches("<g").count() > head[m..].matches("</g>").count());
        (xy[0], xy[1], mirrored)
    };
    for egg in [
        r##"fill="#3b0f6a""##,
        r##"<use href="#goen"/>"##,
        r##"<use href="#fish4""##,
    ] {
        let (r, l) = (at(&right, egg), at(&left, egg));
        assert!(!r.2 && l.2, "{egg}: only the left tree's is mirrored");
        assert!(
            (r.0 - l.0).abs() < 2.0 && (r.1 - l.1).abs() < 2.0,
            "{egg}: {r:?} vs {l:?}"
        );
    }
    let petunia = at(&right, r##"fill="#3b0f6a""##);
    assert!(
        petunia.0 < gitbonsai::style::CENTER_X,
        "the petunia peeks out on the light's side"
    );
}

/// A configured wind sets the tree's side, its light with it: no tree
/// streams toward its lit side.
#[test]
fn a_configured_wind_sets_the_side() {
    use gitbonsai::config::Wind;
    let snapshot = fixtures::by_name("showcase").unwrap().snapshot;
    for (wind, side) in [(Wind::Left, -1.0), (Wind::Right, 1.0)] {
        let mut config = Config::default();
        config.tree.wind = Some(wind);
        let b = Bonsai::grow(&snapshot, &config, None);
        assert!(
            (b.leader.side - side).abs() < 1e-9,
            "{wind:?}: {}",
            b.leader.side
        );
    }
}

/// The pot is repotted in steps and never gets smaller, month to month,
/// whatever the tree does: a pause, a dormant year, a comeback.
#[test]
fn the_pot_never_shrinks() {
    for name in [
        "steady", "showcase", "veteran", "comeback", "dormant", "polyglot",
    ] {
        let frames = monthly(name);
        for pair in frames.windows(2) {
            let ((_, a), (day, b)) = (&pair[0], &pair[1]);
            assert!(
                gitbonsai::POTS.contains(&b.pot),
                "{name} {day}: pot {} off the ladder",
                b.pot
            );
            assert!(b.pot >= a.pot, "{name} {day}: pot {} -> {}", a.pot, b.pot);
        }
    }
}

/// The pot's shadow on the table in a light picture: the middle of its last,
/// widest step, the one furthest from the light; and the line a round pot's
/// base or the bowl's saucer stands on, the middle of its contact step (a
/// rectangle's shadow is a flat slab, without one).
fn table_shadow(svg: &str) -> Option<(f64, Option<f64>)> {
    let step = |op: &str| {
        let end = format!(r#"opacity="{op}"/>"#);
        svg.split('<')
            .find(|e| e.contains(r##"fill="#3b3226""##) && e.trim_end().ends_with(&end))
    };
    let attr = |element: &str, name: &str| -> f64 {
        let key = format!(" {name}=\"");
        element
            .split(&key)
            .nth(1)
            .and_then(|v| v.split('"').next())
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("no {name} in {element}"))
    };
    let (contact, last) = (step("0.34")?, step("0.04")?);
    let middle = if last.starts_with("ellipse") {
        attr(last, "cx")
    } else {
        let d = last.split(" d=\"").nth(1)?.split('"').next()?;
        let n: Vec<f64> = d
            .split(|c: char| c.is_ascii_alphabetic() || c == ' ')
            .filter_map(|v| v.parse().ok())
            .collect();
        f64::midpoint(n[3], n[5])
    };
    let line = contact.starts_with("ellipse").then(|| attr(contact, "cy"));
    Some((middle, line))
}

/// The pot changes only at a repotting, its depth too: between two springs,
/// with no repotting in a growth spurt, the pot keeps its width and depth
/// however much the trunk thickens.
#[test]
fn the_pot_keeps_its_depth_between_repottings() {
    // The pot's foot, from its shadow on the table: a round pot's base or the
    // bowl's saucer; a rectangle's depth rule is tested in `pot`.
    let foot = |b: &Bonsai| {
        table_shadow(&render::svg(b, &Config::default(), Theme::Light)).and_then(|s| s.1)
    };
    let (mut held, mut spurts) = (0, 0);
    for name in ["steady", "showcase", "veteran", "comeback", "polyglot"] {
        let frames: Vec<_> = monthly(name)
            .into_iter()
            .filter(|(_, b)| !b.is_empty() && !gitbonsai::sprout::is_sprout(b))
            .map(|(day, b)| (day, b.pot, foot(&b), b.extras.pot_tier))
            .collect();
        for pair in frames.windows(2) {
            let ((_, pot_a, foot_a, level_a), (day, pot_b, foot_b, level_b)) = (pair[0], pair[1]);
            let (Some(foot_a), Some(foot_b)) = (foot_a, foot_b) else {
                continue;
            };
            // Repotted on the first of March, the start of spring; or a new
            // pot earned by a longer streak, which comes the day it is earned.
            if day.ymd().1 == 3 || level_a != level_b {
                continue;
            }
            // Out of season, in a growth spurt: deeper with its width, unless
            // it is already as deep as a pot gets.
            if pot_b > pot_a {
                spurts += 1;
                assert!(
                    foot_b > foot_a || foot_a - 404.0 > 55.9,
                    "{name} {day}: repotted from {pot_a} to {pot_b}, the foot stays at {foot_a}"
                );
                continue;
            }
            held += 1;
            assert!(
                (foot_a - foot_b).abs() < 1e-9,
                "{name} {day}: the pot's foot moved from {foot_a} to {foot_b}"
            );
        }
    }
    assert!(held > 100, "only {held} months checked");
    eprintln!("{spurts} out-of-season repottings checked");
}

/// The trunk is cut level at its foot, so nothing of it lies on the soil
/// under the roots; and each twig is drawn as its own outlined path.
#[test]
fn the_trunk_ends_at_its_foot_and_twigs_are_drawn_one_by_one() {
    let (mut layered, mut dead_layered, mut dead_behind) = (false, false, false);
    for f in fixtures::all() {
        let b = Bonsai::grow(&f.snapshot, &Config::default(), None);
        if b.is_empty() || gitbonsai::sprout::is_sprout(&b) {
            continue;
        }
        let svg = render::svg(&b, &Config::default(), Theme::Light);
        let clip = svg
            .split(r#"<clipPath id="soil"><path d=""#)
            .nth(1)
            .and_then(|r| r.split('"').next())
            .unwrap_or_else(|| panic!("{}: no soil clip", f.name));
        // Cut level at the foot: straight lines only, no arc or curve.
        assert!(
            clip.chars()
                .filter(char::is_ascii_alphabetic)
                .all(|c| "MmLlHhVvZz".contains(c)),
            "{}: the soil cut curves",
            f.name
        );
        // Each twig is its own outlined path, so one can hide the outline of
        // another instead of all merging into one shape. Their order, back
        // to front, is not checked here.
        // Dead twigs too, in their own ink, layered with the living ones:
        // a living twig can lie in front of a dead one.
        let twigs = svg
            .split(r#"stroke-width="0.35" stroke-linejoin="round">"#)
            .nth(1)
            .unwrap_or("");
        layered |= twigs
            .split("<g")
            .next()
            .is_some_and(|g| g.matches("<path d=").count() > 1);
        dead_layered |= twigs.split(r#"stroke-width=".7">"#).skip(1).any(|run| {
            run.split("</g>")
                .next()
                .is_some_and(|g| g.matches("<path d=").count() > 1)
        });
        dead_behind |= twigs.split(r#"stroke-width=".7">"#).skip(1).any(|run| {
            run.split_once("</g>")
                .is_some_and(|(_, after)| after.starts_with("<path d="))
        });
    }
    assert!(layered, "no tree draws its twigs one by one");
    assert!(dead_layered, "no tree draws its dead twigs one by one");
    assert!(dead_behind, "no dead twig lies behind a living one");
}

/// A row of trees is lit by one light: a left tree under it casts its
/// shadow the way a right tree does, and it stands exactly as before.
#[test]
fn one_light_lights_a_row_from_one_side() {
    // The pot's middle on the canvas.
    const CENTER_X: f64 = 256.0;
    // Read from an oval's shadow; the bowl and the rectangles cast none.
    let shadow = |svg: &str| table_shadow(svg).map(|s| s.0);
    let soil = |svg: &str| {
        svg.split(r#"<clipPath id="soil">"#)
            .nth(1)
            .map(|r| r.split("</clipPath>").next().unwrap_or("").to_string())
    };
    let mut row = Config::default();
    row.render.light = Some(1.0);
    let mut left = 0;
    for f in fixtures::all() {
        let b = Bonsai::grow(&f.snapshot, &Config::default(), None);
        if b.is_empty() || gitbonsai::sprout::is_sprout(&b) {
            continue;
        }
        let own = render::svg(&b, &Config::default(), Theme::Light);
        let lit = render::svg(&b, &row, Theme::Light);
        assert!(
            shadow(&lit).is_none_or(|x| x > CENTER_X),
            "{}: the row is not lit from the upper left",
            f.name
        );
        assert_eq!(
            soil(&own),
            soil(&lit),
            "{}: the row's light moved the tree",
            f.name
        );
        if b.leader.side < 0.0 && shadow(&own).is_some() {
            left += 1;
            assert!(
                shadow(&own).is_some_and(|x| x < CENTER_X),
                "{}: a left tree on its own is not lit from its open side, the upper right",
                f.name
            );
        }
    }
    assert!(left > 0, "no fixture grows left");
}

/// Each style's median size, relative to the mean of the five styles a
/// fixture grows into: (chokkan, moyogi, shakan, fukinagashi, kengai).
/// Fixtures whose five trees are alike (too young for a style) are left out.
fn style_sizes() -> [f64; 5] {
    use gitbonsai::config::Style;
    let styles = [
        Style::Chokkan,
        Style::Moyogi,
        Style::Shakan,
        Style::Fukinagashi,
        Style::Kengai,
    ];
    let mut rel: Vec<Vec<f64>> = vec![Vec::new(); 5];
    for f in fixtures::all() {
        let sizes: Vec<f64> = styles
            .iter()
            .map(|&s| {
                let mut c = Config::default();
                c.tree.style = Some(s);
                let b = Bonsai::grow(&f.snapshot, &c, None);
                if b.is_empty() || gitbonsai::sprout::is_sprout(&b) {
                    0.0
                } else {
                    gitbonsai::timelapse::shape(&b).iter().sum::<f64>()
                }
            })
            .collect();
        let (lo, hi) = sizes.iter().fold((f64::INFINITY, 0.0_f64), |(lo, hi), &s| {
            (lo.min(s), hi.max(s))
        });
        if lo <= 0.0 || hi - lo < 1e-6 * hi {
            continue;
        }
        let mean = sizes.iter().sum::<f64>() / 5.0;
        for (i, s) in sizes.iter().enumerate() {
            rel[i].push(s / mean);
        }
    }
    let mut out = [0.0; 5];
    for (o, r) in out.iter_mut().zip(rel.iter_mut()) {
        r.sort_by(f64::total_cmp);
        let n = r.len();
        *o = f64::midpoint(r[(n - 1) / 2], r[n / 2]);
    }
    out
}

/// The styles move towards one size, each by at most 5%: grown from the
/// same work, the uprights grow 5% and the windswept and cascade trees
/// shrink 5%, measured as `explain` measures a tree's size. The baseline is
/// the fixtures' before 0.6.0.
#[test]
fn styles_move_towards_one_size_by_at_most_five_percent() {
    const BASELINE: [f64; 5] = [0.867, 0.889, 0.891, 1.236, 1.078];
    const MOVE: [f64; 5] = [1.05, 1.05, 1.05, 0.95, 0.95];
    let now = style_sizes();
    eprintln!("style sizes now: {now:?}");
    for i in 0..5 {
        let want = BASELINE[i] * MOVE[i];
        assert!(
            (now[i] - want).abs() < 0.015,
            "style {i}: {} (baseline {}, want {want})",
            now[i],
            BASELINE[i]
        );
        assert!(
            (now[i] / BASELINE[i] - 1.0).abs() <= 0.05,
            "style {i} moved {}",
            now[i] / BASELINE[i]
        );
    }
}
