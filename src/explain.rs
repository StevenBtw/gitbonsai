//! `gitbonsai explain`: every derived number, to answer "why does my tree
//! look like this?".

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::Bonsai;
use crate::eggs::{DRAWN, MAX_SEAMS};
use crate::foliage::{LanguageState, density};
use crate::growth::H_REF;
use crate::scene::{radius, samara_share};

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
        "  age                 {age} weeks ({:.1} years)",
        f64::from(age) / 52.18
    );
    let _ = writeln!(
        out,
        "  height              {:.1} units, {} of H_ref",
        sk.height,
        pct(sk.height / H_REF)
    );
    let _ = writeln!(
        out,
        "  capped lifetime     {:.0}",
        b.timeline.capped_lifetime()
    );
    let _ = writeln!(
        out,
        "  trunk base radius   {:.2} (Murray's law on {:.0} flowing through it)",
        radius(flows.first().copied().unwrap_or(0.0)),
        flows.first().copied().unwrap_or(0.0)
    );
    let shed = sk.pads.iter().filter(|p| p.retired).count();
    let _ = writeln!(
        out,
        "  skeleton            {} nodes, {} pads ({shed} shed by crown recession)",
        sk.nodes.len(),
        sk.pads.len()
    );
    if b.dormant() {
        let _ = writeln!(
            out,
            "  dormant             a year without contributions; the canopy is bare"
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
        let pads = sk
            .pads
            .iter()
            .filter(|p| !p.retired && p.language.as_deref() == Some(language.as_str()))
            .count();
        let _ = writeln!(
            out,
            "  {:<18} {state:<5}  lifetime {:>8.0}  window {:>6.1}/wk  density {:>6}  pads {pads}  leaves {}",
            truncate(language, 18),
            lifetime.get(language.as_str()).copied().unwrap_or(0.0),
            v,
            pct(density(v)),
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
        "  average             {:.1}/week, crown density {}",
        b.window.total,
        pct(density(b.window.total))
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
        "  streak              {} weeks now, {} longest",
        e.current_streak, e.longest_streak
    );
    let _ = writeln!(out, "  moss                {}", pct(e.moss));
    let _ = writeln!(out, "  pot tier            {}", e.pot_tier);
    let _ = writeln!(
        out,
        "  level               {} (samaras on {} of side leaves)",
        e.level,
        pct(samara_share(e.level) * 1.4)
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
