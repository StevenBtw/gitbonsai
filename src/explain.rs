//! `gitbonsai explain`: every derived number, to answer "why does my tree
//! look like this?".

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::Bonsai;
use crate::date::Date;
use crate::eggs::{DRAWN, MAX_SEAMS};
use crate::foliage::LanguageState;
use crate::growth::clouds::lead_of;
use crate::growth::dome::crown_years;
use crate::growth::life::Life;
use crate::growth::trunks::{Trunk, trunks};
use crate::scene::{SAMARA_RATE, radius, samara_share};

/// The report, as plain text.
pub fn report(b: &Bonsai) -> String {
    let mut out = String::new();
    let e = &b.extras;
    let pct = |x: f64| format!("{:.1}%", x * 100.0);
    let sk = &b.skeleton;
    let flows = sk.flows();
    let age = b.timeline.age_weeks();

    let _ = writeln!(
        out,
        "@{} (account {}) as of {}, style {}",
        b.login, b.account_id, b.observed_on, b.leader.style
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "Wood");
    match b.timeline.first_active {
        Some(first) => {
            let _ = writeln!(
                out,
                "  growing since       {first} (first sustained activity)"
            );
        }
        None => {
            let _ = writeln!(out, "  no contributions yet: a seedling in a training pot");
        }
    }
    let _ = writeln!(
        out,
        "  age                 {age} {} ({:.1} years)",
        if age == 1 { "week" } else { "weeks" },
        f64::from(age) / 52.18
    );
    match b.leader.mix {
        Some(m) => {
            let _ = writeln!(
                out,
                "  style               {} from the last three years: pauses {}, past one language {}, legacy {}",
                b.leader.style,
                pct(m.irregular + 0.0),
                pct(m.spread + 0.0),
                pct(m.legacy + 0.0)
            );
        }
        None => {
            let _ = writeln!(out, "  style               {} (configured)", b.leader.style);
        }
    }
    let _ = writeln!(
        out,
        "  height              {:.1} units (age shows as girth and spread)",
        sk.height
    );
    let _ = writeln!(
        out,
        "  capped lifetime     {:.0}",
        b.timeline.capped_lifetime()
    );
    if crate::sprout::is_sprout(b) {
        let _ = writeln!(
            out,
            "  sprout              drawn leaf by leaf until {:.0} contributions",
            crate::sprout::SPROUT_UNTIL
        );
    }
    let _ = writeln!(
        out,
        "  trunk base radius   {:.2} (Murray's law on {:.0} flowing through it)",
        radius(flows.first().copied().unwrap_or(0.0)),
        flows.first().copied().unwrap_or(0.0)
    );
    let _ = writeln!(
        out,
        "  trunk at its foot   {:.1} wide, as drawn",
        2.0 * b.foot_girth()
    );
    let _ = writeln!(
        out,
        "  size                {:.0} (wood by its girth and leaf pads by their foliage, as a timelapse weighs change)",
        crate::timelapse::shape(b).iter().sum::<f64>()
    );
    let _ = writeln!(
        out,
        "  skeleton            {} nodes, {} clouds",
        sk.nodes.len(),
        sk.clouds.len()
    );
    if b.dormant() {
        let _ = writeln!(
            out,
            "  dormant             a year without contributions; the canopy is bare"
        );
    }
    if let Some(life) = Life::of(&b.timeline, &b.periods) {
        let names = |t: &[Trunk]| {
            t.iter()
                .map(|t| {
                    let name = t.lang.as_deref().unwrap_or("(private work)");
                    if t.dropped {
                        format!("{name} (dead)")
                    } else {
                        name.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let _ = writeln!(out, "  trunks              {}", names(&trunks(&life)));
        let crown = crown_years(&b.timeline, &b.periods);
        if let (Some(a), Some(z)) = (crown.first(), crown.last()) {
            let _ = writeln!(out, "  crown               {a} to {z}");
        }
    }
    let total: f64 = b.timeline.days.iter().map(|d| d.total).sum();
    // Work declared in `[leaves.private_periods]` has a language.
    let declared = |day: Date| {
        b.periods
            .iter()
            .any(|(a, z, langs)| *a <= day && day <= *z && !langs.is_empty())
    };
    let private: f64 = b
        .timeline
        .days
        .iter()
        .filter(|d| !declared(d.date))
        .map(|d| d.unattributed)
        .sum();
    if total > 0.0 && private > 0.0 {
        let _ = writeln!(
            out,
            "  private work        {} of contributions are private: silver leaves",
            pct(private / total)
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "Languages");
    let mut lifetime: BTreeMap<&str, f64> = BTreeMap::new();
    for day in &b.timeline.days {
        for (language, amount) in &day.by_language {
            *lifetime.entry(language.as_str()).or_default() += amount;
        }
    }
    for language in &b.order {
        let state = match b.state(Some(language)) {
            LanguageState::Alive => "alive",
            LanguageState::Bare => "bare",
            LanguageState::Jin => "jin",
        };
        let v = b.window.of_language(Some(language));
        let clouds = sk
            .clouds
            .iter()
            .filter(|c| lead_of(c).as_deref() == Some(language.as_str()))
            .count();
        let _ = writeln!(
            out,
            "  {:<18} {state:<5}  lifetime {:>8.0}  window {:>6.1}/wk  leaves {:>6}  clouds {clouds}  color {}",
            truncate(language, 18),
            lifetime.get(language.as_str()).copied().unwrap_or(0.0),
            v,
            pct(b.fullness(Some(language))),
            b.maple
                .colors
                .get(language)
                .map_or_else(|| "-".to_string(), ToString::to_string)
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "Leaves (13-week window)");
    let _ = writeln!(
        out,
        "  average             {:.1}/week; leaves left {} (they follow how recent the work is, not how much)",
        b.window.total,
        pct(b.fullness(None))
    );
    let _ = writeln!(
        out,
        "  last complete week  {:.0} (bronze new growth)",
        b.last_week_total
    );

    let _ = writeln!(out);
    let _ = writeln!(out, "Extras");
    let _ = writeln!(
        out,
        "  streak              {} {} now, {} longest",
        e.current_streak,
        if e.current_streak == 1 {
            "week"
        } else {
            "weeks"
        },
        e.longest_streak
    );
    let _ = writeln!(out, "  moss                {}", pct(e.moss));
    let _ = writeln!(
        out,
        "  pot                 level {}, {}",
        e.pot_tier,
        crate::pot::model(e.pot_tier).name
    );
    let _ = writeln!(
        out,
        "  level               {} (samaras on {} of side leaves)",
        e.level,
        pct(samara_share(e.level) * SAMARA_RATE)
    );
    if let Some(level) = e.bloom {
        let _ = writeln!(
            out,
            "  milestone           level {level} this week: falling momiji"
        );
    }
    if e.drip {
        let _ = writeln!(out, "  drip irrigation     yes (level 11)");
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "Easter eggs");
    if b.achievements.is_empty() {
        let _ = writeln!(out, "  achievements        none collected");
    }
    for (slug, tier) in &b.achievements {
        let drawn = if DRAWN.contains(&slug.as_str()) {
            ""
        } else {
            "  (common: not drawn)"
        };
        let _ = writeln!(out, "  {slug:<30} x{tier}{drawn}");
    }
    let _ = writeln!(
        out,
        "  kintsugi            {} archived repos ({} counted; those facing the front show)",
        b.archived.len(),
        b.archived.len().min(MAX_SEAMS)
    );
    if !b.truncated_windows.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "Truncated windows (unattributed, so they feed the trunk)"
        );
        for w in &b.truncated_windows {
            let _ = writeln!(out, "  {} .. {}", w.from, w.to);
        }
    }
    out
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_string()
    } else {
        let mut cut: String = text.chars().take(width - 1).collect();
        cut.push('…');
        cut
    }
}
