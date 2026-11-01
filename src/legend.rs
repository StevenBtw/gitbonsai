//! The optional legend: each language's leaf color and what the tree's signs
//! mean, set beside the tree. Only signs this tree actually shows are listed,
//! and the surprises around the pot are never explained.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "positions on a 512-unit canvas; contribution totals are small and positive"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::Bonsai;
use crate::color::Rgb;
use crate::config::Legend;
use crate::foliage::LanguageState;
use crate::maple::MAPLE_GREEN;
use crate::paint::{Frame, Paints};
use crate::render::{Num, escape};

/// The most languages listed; the rest are counted.
const MAX_LANGUAGES: usize = 8;

/// What the tree shows besides its languages.
#[derive(Debug, Clone, Copy, Default)]
pub struct Signs {
    /// Bronze tips: leaves that grew last week.
    pub fresh: bool,
    /// Leaves falling: a level crossed this week.
    pub falling: bool,
    /// The pot's glaze.
    pub glaze: Option<Rgb>,
}

/// A row's marker, drawn left of its words.
enum Mark {
    Leaf(Rgb),
    Twig(Rgb),
    Trunk,
    Seeds,
    Falling,
    Moss,
    Pot(Rgb),
}

/// One row: a heading, a marked line, or a plain line under the marks.
struct Row {
    heading: bool,
    mark: Option<Mark>,
    text: String,
    note: Option<&'static str>,
}

impl Row {
    fn heading(text: &str) -> Self {
        Self {
            heading: true,
            mark: None,
            text: text.to_string(),
            note: None,
        }
    }

    fn sign(mark: Mark, text: String) -> Self {
        Self {
            heading: false,
            mark: Some(mark),
            text,
            note: None,
        }
    }
}

/// `n` with thousands separators.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// An age in words: weeks, then months, then years.
fn age(weeks: u32) -> String {
    let plural = |n: u32, unit: &str| format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
    if weeks < 9 {
        plural(weeks, "week")
    } else if weeks < 104 {
        plural(weeks * 700 / 3044, "month")
    } else {
        plural(weeks * 100 / 5218, "year")
    }
}

/// A language name, cut to `max` characters.
fn short(name: &str, max: usize) -> String {
    let mut out: String = name.chars().take(max).collect();
    if name.chars().count() > max {
        out.push('…');
    }
    out
}

fn rows(b: &Bonsai, paints: &Paints, signs: Signs) -> Vec<Row> {
    // Living languages first, busiest first; then resting ones by lifetime.
    // Deadwood shares one line: it is history, not what the tree is now.
    let mut lifetime: BTreeMap<&str, f64> = BTreeMap::new();
    for day in &b.timeline.days {
        for (language, amount) in &day.by_language {
            *lifetime.entry(language.as_str()).or_default() += amount;
        }
    }
    let total = |l: &str| lifetime.get(l).copied().unwrap_or(0.0);
    let of = |state: LanguageState| -> Vec<&String> {
        let mut v: Vec<&String> = b
            .order
            .iter()
            .filter(|l| b.state(Some(l)) == state)
            .collect();
        v.sort_by(|x, y| {
            let key = |l: &str| {
                if state == LanguageState::Alive {
                    b.window.of_language(Some(l))
                } else {
                    total(l)
                }
            };
            key(y).total_cmp(&key(x)).then(x.cmp(y))
        });
        v
    };
    let (alive, resting, dead) = (
        of(LanguageState::Alive),
        of(LanguageState::Bare),
        of(LanguageState::Jin),
    );

    let mut rows = vec![Row::heading("Leaves")];
    let shown: Vec<&String> = alive.iter().chain(&resting).copied().collect();
    for language in shown.iter().take(MAX_LANGUAGES) {
        let (mark, note) = if b.state(Some(language)) == LanguageState::Alive {
            let leaf = b
                .maple
                .leaf(Some(language), 0.5)
                .shift(paints.leaf_shift, 0.0);
            (Mark::Leaf(leaf), None)
        } else {
            (Mark::Twig(paints.bark), Some("resting"))
        };
        rows.push(Row {
            heading: false,
            mark: Some(mark),
            text: short(language, 16),
            note,
        });
    }
    if shown.len() > MAX_LANGUAGES {
        rows.push(Row {
            heading: false,
            mark: None,
            text: format!("+{} more", shown.len() - MAX_LANGUAGES),
            note: None,
        });
    }
    if !dead.is_empty() {
        let mut names: Vec<String> = dead.iter().take(2).map(|l| short(l, 12)).collect();
        if dead.len() > 2 {
            names.push(format!("+{}", dead.len() - 2));
        }
        rows.push(Row::sign(
            Mark::Twig(paints.jin),
            format!("deadwood: {}", names.join(", ")),
        ));
    }

    rows.push(Row::heading("Signs"));
    rows.push(Row::sign(
        Mark::Trunk,
        format!(
            "{}, {} contributions",
            age(b.timeline.age_weeks()),
            thousands(b.timeline.capped_lifetime().round() as u64)
        ),
    ));
    if signs.fresh {
        let bronze = MAPLE_GREEN
            .mix(Rgb::hex(0xc8553d), 0.45)
            .shift(0.06 + paints.leaf_shift, 0.0);
        rows.push(Row::sign(Mark::Leaf(bronze), "new growth this week".into()));
    }
    if b.extras.level >= 3 {
        rows.push(Row::sign(
            Mark::Seeds,
            format!("lifetime level {}", b.extras.level),
        ));
    }
    if signs.falling {
        rows.push(Row::sign(Mark::Falling, "a new level this week".into()));
    }
    if b.extras.current_streak > 0 && b.extras.moss > 0.0 {
        rows.push(Row::sign(
            Mark::Moss,
            format!("{}-week streak", b.extras.current_streak),
        ));
    }
    if let Some(glaze) = signs.glaze {
        rows.push(Row::sign(
            Mark::Pot(glaze),
            format!("longest streak {} weeks", b.extras.longest_streak),
        ));
    }
    rows
}

/// The legend's markup, beside `frame` on `side`; `frame` grows to hold it.
/// Empty when the legend is off or the tree has nothing to explain yet.
pub fn legend(
    b: &Bonsai,
    paints: &Paints,
    signs: Signs,
    side: Legend,
    frame: &mut Frame,
) -> String {
    if side == Legend::Off || b.is_empty() {
        return String::new();
    }
    let rows = rows(b, paints, signs);
    // Sized to the tree, so the words read the same at any tree size.
    let fs = (frame.2.max(frame.3 * 1.2) * 0.036).clamp(10.0, 24.0);
    let lh = fs * 1.7;
    let mark = fs * 1.2;
    let indent = mark + fs * 0.7;
    let width = rows
        .iter()
        .map(|r| {
            let words = r.text.chars().count() + r.note.map_or(0, |n| n.len() + 3);
            let left = if r.heading { 0.0 } else { indent };
            left + words as f64 * fs * 0.52
        })
        .fold(0.0, f64::max);
    let gap = fs * 1.6;
    let height = rows.len() as f64 * lh;

    // Make room: beside the tree, and taller when the list is.
    let grid = |v: f64| (v / 8.0).ceil() * 8.0;
    let room = grid(width + gap);
    let x0 = match side {
        Legend::Left => {
            frame.0 -= room;
            frame.0 + fs * 0.6
        }
        _ => frame.0 + frame.2 + gap,
    };
    frame.2 += room;
    let need = height + 2.0 * fs;
    if need > frame.3 {
        let grow = grid(need - frame.3);
        frame.1 -= grow;
        frame.3 += grow;
    }
    let top = frame.1 + (frame.3 - height) / 2.0;

    let (ink, muted) = if paints.dark {
        ("#d4cbb9", "#988e7c")
    } else {
        ("#4f473b", "#8c816e")
    };
    let mut s = format!(
        r#"<g font-family="system-ui,-apple-system,'Segoe UI',sans-serif" font-size="{}" fill="{ink}">"#,
        Num(fs)
    );
    for (i, row) in rows.iter().enumerate() {
        let cy = top + (i as f64 + 0.5) * lh;
        let baseline = cy + fs * 0.35;
        let Some(m) = &row.mark else {
            // Headings sit flush left and bold; a plain line lines up with the words.
            let _ = write!(
                s,
                r#"<text x="{}" y="{}" fill="{muted}"{}>{}</text>"#,
                Num(if row.heading { x0 } else { x0 + indent }),
                Num(baseline),
                if row.heading {
                    r#" font-weight="600" letter-spacing=".04em""#
                } else {
                    ""
                },
                escape(&row.text)
            );
            continue;
        };
        let mx = x0 + mark / 2.0;
        s.push_str(&marker(m, mx, cy, mark, paints));
        let _ = write!(
            s,
            r#"<text x="{}" y="{}">{}"#,
            Num(x0 + indent),
            Num(baseline),
            escape(&row.text)
        );
        if let Some(note) = row.note {
            let _ = write!(s, r#"<tspan fill="{muted}"> · {note}</tspan>"#);
        }
        s.push_str("</text>");
    }
    s.push_str("</g>");
    s
}

/// A row's marker, `size` across, centered on `(x, y)`.
fn marker(mark: &Mark, x: f64, y: f64, size: f64, paints: &Paints) -> String {
    let h = size / 2.0;
    match mark {
        Mark::Leaf(color) => format!(
            r##"<use href="#ml" transform="translate({} {})scale({})" fill="{color}"/>"##,
            Num(x),
            Num(y + h),
            Num(size / 5.2)
        ),
        Mark::Twig(color) => format!(
            r#"<path d="M{} {}L{} {}M{} {}L{} {}" stroke="{color}" stroke-width="{}" stroke-linecap="round" fill="none"/>"#,
            Num(x - h),
            Num(y + h * 0.8),
            Num(x + h),
            Num(y - h * 0.8),
            Num(x),
            Num(y),
            Num(x + h * 0.15),
            Num(y - h * 0.9),
            Num(size * 0.14)
        ),
        Mark::Trunk => format!(
            r#"<path d="M{} {}Q{} {} {} {}L{} {}Q{} {} {} {}Z" fill="{}"/>"#,
            Num(x - h * 0.55),
            Num(y + h),
            Num(x - h * 0.1),
            Num(y),
            Num(x - h * 0.15),
            Num(y - h),
            Num(x + h * 0.2),
            Num(y - h),
            Num(x + h * 0.15),
            Num(y),
            Num(x + h * 0.6),
            Num(y + h),
            paints.bark
        ),
        Mark::Seeds => format!(
            r##"<use href="#sm" transform="translate({} {})scale({})" fill="{}"/>"##,
            Num(x),
            Num(y - h * 0.7),
            Num(size / 9.0),
            paints.crimson
        ),
        Mark::Falling => format!(
            r##"<use href="#ml" transform="translate({} {})rotate(140)scale({})" fill="{}"/>"##,
            Num(x),
            Num(y - h * 0.4),
            Num(size / 5.6),
            paints.crimson
        ),
        Mark::Moss => format!(
            r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="{}"/><ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="{}"/>"#,
            Num(x - h * 0.3),
            Num(y + h * 0.3),
            Num(h * 0.6),
            Num(h * 0.3),
            paints.moss[0],
            Num(x + h * 0.35),
            Num(y + h * 0.15),
            Num(h * 0.45),
            Num(h * 0.25),
            paints.moss[1]
        ),
        Mark::Pot(glaze) => format!(
            r#"<path d="M{} {}H{}L{} {}H{}Z" fill="{glaze}"/>"#,
            Num(x - h),
            Num(y - h * 0.35),
            Num(x + h),
            Num(x + h * 0.7),
            Num(y + h * 0.55),
            Num(x - h * 0.7)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_ages_read_naturally() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(26_301), "26,301");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(age(1), "1 week");
        assert_eq!(age(5), "5 weeks");
        assert_eq!(age(96), "22 months");
        assert_eq!(age(869), "16 years");
    }
}
