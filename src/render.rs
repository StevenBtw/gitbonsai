//! SVG output: number formats, path data, themes and the document shell.
//!
//! Output is deterministic to the byte: fixed attribute order, a fixed number
//! format (two decimals, or one for leaf placement and wood outlines, trailing
//! zeros dropped, `-0` written as `0`), and no clock.

use std::fmt::{self, Write as _};

use crate::config::Config;
use crate::paint::{self, maple_leaf_path};
use crate::{Bonsai, VERSION};

/// A light or dark variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Theme {
    /// For light backgrounds (`#ffffff`).
    Light,
    /// For dark backgrounds (`#0d1117`).
    Dark,
}

impl Theme {
    /// Both variants.
    pub const ALL: [Self; 2] = [Self::Light, Self::Dark];

    /// `light` or `dark`, as in the file names.
    pub fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

fn write_number(f: &mut fmt::Formatter<'_>, value: f64, decimals: usize) -> fmt::Result {
    let scale = if decimals == 1 { 10.0 } else { 100.0 };
    let rounded = (value * scale).round() / scale;
    if rounded == 0.0 || !rounded.is_finite() {
        return f.write_str("0");
    }
    let text = format!("{rounded:.decimals$}");
    f.write_str(text.trim_end_matches('0').trim_end_matches('.'))
}

/// A number in the output format: two decimals, no trailing zeros.
#[derive(Debug, Clone, Copy)]
pub struct Num(pub f64);

/// A number at one decimal, for positions where a tenth of a unit is invisible.
#[derive(Debug, Clone, Copy)]
pub struct Coarse(pub f64);

impl Num {
    /// `value` at one decimal.
    pub fn coarse(value: f64) -> Coarse {
        Coarse(value)
    }
}

impl fmt::Display for Num {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_number(f, self.0, 2)
    }
}

impl fmt::Display for Coarse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_number(f, self.0, 1)
    }
}

/// SVG path data.
#[derive(Debug, Clone, Default)]
pub struct PathData {
    d: String,
    coarse: bool,
    /// The current point, for relative line commands.
    at: (f64, f64),
}

impl PathData {
    /// Empty path data, at two decimals.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty path data at one decimal, for long outlines.
    pub fn coarse() -> Self {
        Self {
            d: String::new(),
            coarse: true,
            at: (0.0, 0.0),
        }
    }

    fn point(&mut self, (x, y): (f64, f64)) {
        self.at = (x, y);
        if self.coarse {
            let _ = write!(self.d, "{} {}", Coarse(x), Coarse(y));
        } else {
            let _ = write!(self.d, "{} {}", Num(x), Num(y));
        }
    }

    /// Move to.
    pub fn m(&mut self, p: (f64, f64)) -> &mut Self {
        self.d.push('M');
        self.point(p);
        self
    }

    /// Line to. Coarse paths write it relative (`l dx dy`), which is shorter;
    /// the current point advances by the rounded step, so rounding never drifts.
    pub fn l(&mut self, p: (f64, f64)) -> &mut Self {
        if self.coarse {
            let dx = ((p.0 - self.at.0) * 10.0).round() / 10.0;
            let dy = ((p.1 - self.at.1) * 10.0).round() / 10.0;
            let _ = write!(self.d, "l{} {}", Coarse(dx), Coarse(dy));
            self.at = (self.at.0 + dx, self.at.1 + dy);
        } else {
            self.d.push('L');
            self.point(p);
        }
        self
    }

    /// Quadratic curve to `to` with control point `c`.
    pub fn q(&mut self, c: (f64, f64), to: (f64, f64)) -> &mut Self {
        self.d.push('Q');
        self.point(c);
        self.d.push(' ');
        self.point(to);
        self
    }

    /// Cubic curve to `to` with control points `c1` and `c2`.
    pub fn c(&mut self, c1: (f64, f64), c2: (f64, f64), to: (f64, f64)) -> &mut Self {
        self.d.push('C');
        self.point(c1);
        self.d.push(' ');
        self.point(c2);
        self.d.push(' ');
        self.point(to);
        self
    }

    /// Elliptical arc to `to`.
    pub fn a(&mut self, rx: f64, ry: f64, sweep: bool, to: (f64, f64)) -> &mut Self {
        let _ = write!(self.d, "A{} {} 0 0 {} ", Num(rx), Num(ry), u8::from(sweep));
        self.point(to);
        self
    }

    /// Close the subpath.
    pub fn z(&mut self) -> &mut Self {
        self.d.push('Z');
        self
    }

    /// Whether nothing was drawn.
    pub fn is_empty(&self) -> bool {
        self.d.is_empty()
    }

    /// The `d` attribute's value.
    pub fn as_str(&self) -> &str {
        &self.d
    }
}

/// The animation rules. Reduced motion turns them all off.
const ANIMATION_CSS: &str = concat!(
    "<style>",
    ".mf{transform-box:fill-box;transform-origin:center;animation:mf 9s linear infinite}",
    "@keyframes mf{0%{transform:translate(0,0) rotate(0);opacity:0}8%{opacity:1}45%{transform:translate(16px,55px) rotate(160deg)}",
    "100%{transform:translate(-8px,120px) rotate(380deg);opacity:0}}",
    ".tw{animation:tw 3.2s ease-in-out infinite}@keyframes tw{0%,100%{opacity:.35}50%{opacity:1}}",
    ".dp{animation:dr 3s linear infinite}@keyframes dr{0%{opacity:0;transform:translate(0,0)}12%{opacity:1}75%{opacity:1}100%{opacity:0;transform:translate(0,14px)}}",
    ".gl{animation:gl 4.4s ease-in-out infinite}@keyframes gl{0%,100%{opacity:.35}55%{opacity:1}}",
    ".fs{transform-box:fill-box;transform-origin:50% 0;animation:fs 3.6s ease-in-out infinite alternate}@keyframes fs{from{transform:rotate(-9deg)}to{transform:rotate(9deg)}}",
    ".ff{animation:ff 9s ease-in-out infinite}@keyframes ff{0%,100%{transform:translate(0,0)}25%{transform:translate(7px,-4px)}50%{transform:translate(2px,-10px)}75%{transform:translate(-6px,-3px)}}",
    "@media (prefers-reduced-motion:reduce){.mf{animation:none;opacity:0}.tw,.dp,.gl,.fs,.ff{animation:none}}",
    "</style>",
);

/// Shared shapes, drawn with `<use>`: leaf tufts, samaras, and the eggs.
const SYMBOLS: &str = concat!(
    r##"<g id="t"><use href="#ml" transform="rotate(-38) scale(.9)"/><use href="#ml" transform="rotate(34) scale(.95)"/><use href="#ml" transform="translate(0 -1) scale(1.08)"/></g>"##,
    r##"<g id="u"><use href="#ml" transform="rotate(-22)"/><use href="#ml" transform="rotate(26) translate(1 0) scale(.92)"/></g>"##,
    r##"<g id="sm"><path d="M0 .2C-1.9.6-4.6 3.2-4.6 7.6C-3.2 7.4-1.4 4.6-.1 1.4Z" opacity=".88"/><path d="M0 .2C1.9.6 4.6 3.2 4.6 7.6C3.2 7.4 1.4 4.6.1 1.4Z" opacity=".88"/><circle r="1.25"/><path d="M-.15 0L.15 0L0-3Z"/></g>"##,
    r##"<path id="star5" d="M0-3.6L1.06-1.46L3.42-1.11L1.71.55L2.12 2.91L0 1.8L-2.12 2.91L-1.71.55L-3.42-1.11L-1.06-1.46Z"/>"##,
    r##"<g id="fish1"><path d="M-4 0L-5.9-1.7L-5.4.1L-6 1.6Z"/><path d="M-4.2 0C-2.6-1.9 1.6-2.1 3.8-.4C2.2.3 1.4 1.4-1 1.3C-2.7 1.2-3.6.6-4.2 0Z"/><circle cx="2.2" cy="-.7" r=".38" fill="#2a2a2a"/></g>"##,
    r##"<g id="fish2"><path d="M-2.4-1C-4.8-3.8-7.2-3-6.9-1.4C-6.5-.7-5.2-.9-4.8-.6C-5.9.5-5.6 2-4.1 1.7C-3.2 1.5-2.8.4-2.4-.2Z"/><path d="M-.4-3.3L.9-4.9L1.9-3.4Z"/><path d="M-2.6-1C-2.4-3.5 3-3.9 4.1-1.2C4.7.6 2.4 1.5-.2 1.4C-1.9 1.3-2.9.4-2.6-1Z"/><path d="M.6.9L1.6 2.4L2.2 1Z"/><circle cx="2.8" cy="-1.4" r=".5" fill="#2a2a2a"/></g>"##,
    r##"<g id="fish3"><path d="M-6.4 0L-9.4-3.6L-7.9-.2L-9.4 3.2L-6.4.2Z"/><path d="M-.6-2.9L.7-5.1L1.5-3Z"/><path d="M-3.6-2.3L-2.8-3.6L-2.1-2.4Z"/><path d="M-6.7 0C-4.2-2.7 2.6-3.3 6.8-.6C7.5-.2 7.5.2 6.8.4C2.7 1.7-4.2 1.7-6.7 0Z"/><path d="M1 .9L3 2.8L3.4 1.1Z"/><circle cx="5" cy="-.7" r=".42" fill="#2a2a2a"/><path d="M3.6-1.2q.4.8 0 1.6" fill="none" stroke="#000" stroke-width=".25" opacity=".4"/></g>"##,
    r##"<g id="fish4"><path d="M-12-3.4L-8.4-.3L-12.2 2.6L-11-.2Z"/><path d="M-1.2-3.6L1.4-7.6L2.8-3.2Z"/><path d="M1 .9L3.2 3.4L3.8 1Z"/><path d="M-9.2 0C-7-3.2-1-4.3 4-3.4C6.6-3 8.6-1.6 9.6-.4L9.2 0C7.6.8 4.6 1.5 0 1.5C-4 1.5-7.2 1.1-9.2 0Z"/><path d="M4.3-1.6q.4.9 0 1.8M3.4-1.7q.4.9 0 1.8" fill="none" stroke="#000" stroke-width=".3" opacity=".4"/><circle cx="6.4" cy="-1.3" r=".45" fill="#2a2a2a"/></g>"##,
    r##"<g id="glowworm"><path d="M3.4-.5Q2-1.2.4-.7Q-1-.3-2.2-.5" fill="none" stroke="#6f6248" stroke-width="1.1" stroke-linecap="round" opacity=".75"/><g class="gl"><circle cx="-3" cy="-.5" r="3.6" fill="#b8ffd0" opacity=".16"/><circle cx="-3" cy="-.5" r="1.8" fill="#c9ffb8" opacity=".35"/><circle cx="-3" cy="-.5" r=".7" fill="#f3ffe0"/></g></g>"##,
    r##"<g id="firefly"><g class="gl"><circle r="5" fill="#fff3a6" opacity=".14"/><circle r="2.4" fill="#fff0a0" opacity=".3"/><circle r=".9" fill="#fffbe0"/></g><path d="M-.3-1.2L-1.6-2.1M.3-1.2L1.6-2.1" stroke="#5a5040" stroke-width=".3" opacity=".45"/><path d="M0-1.4V-.6" stroke="#3a3226" stroke-width=".5" opacity=".5"/></g>"##,
    r##"<g id="pebbles"><ellipse cx="-2.3" cy="-1.5" rx="3" ry="1.9" fill="#8c8e86" transform="rotate(-14 -2.3 -1.5)"/><ellipse cx="2.2" cy="-1.3" rx="2.5" ry="1.65" fill="#a7a99f" transform="rotate(16 2.2 -1.3)"/><path d="M-4-2.6Q-2.6-3.4-1-2.9M1-2.4Q2.2-2.9 3.4-2.3" fill="none" stroke="#fff" stroke-width=".45" opacity=".45"/></g>"##,
    r##"<g id="yinyang"><g transform="translate(0 -1.6) scale(1 .55)"><path d="M0-4.4A4.4 4.4 0 0 1 0 4.4A2.2 2.2 0 0 1 0 0A2.2 2.2 0 0 0 0-4.4Z" fill="#f1eee6" stroke="#5a4632" stroke-width=".45"/><path d="M0-4.4A4.4 4.4 0 0 0 0 4.4A2.2 2.2 0 0 0 0 0A2.2 2.2 0 0 1 0-4.4Z" fill="#26272a" stroke="#5a4632" stroke-width=".45"/></g><path d="M-3.6-2.6Q-2.4-3.6-.8-3.4" fill="none" stroke="#fff" stroke-width=".4" opacity=".5"/><path d="M1.2-3.2Q2.6-3.2 3.4-2.2" fill="none" stroke="#fff" stroke-width=".4" opacity=".7"/></g>"##,
    r##"<g id="cranes"><g><path d="M-4.6 0v3.6M-3.4 0v3.6" stroke="#2a2a2a" stroke-width=".35"/><ellipse cx="-4" cy="-1.8" rx="3.2" ry="2" fill="#f7f5ee"/><path d="M-7.2-1.6L-8.6-.4L-6.6-.8Z" fill="#1d1d1d"/><path d="M-2.2-2.8Q-1.2-6-1.4-9" stroke="#1d1d1d" stroke-width=".9" fill="none"/><circle cx="-1.4" cy="-9.2" r=".95" fill="#f7f5ee"/><circle cx="-1.3" cy="-9.9" r=".45" fill="#d6332a"/><path d="M-.6-9.3L1.6-8.9L-.6-8.8Z" fill="#9a8a5a"/></g><g><path d="M4.6 0v3.6M3.4 0v3.6" stroke="#2a2a2a" stroke-width=".35"/><ellipse cx="4" cy="-1.8" rx="3.2" ry="2" fill="#f7f5ee"/><path d="M7.2-1.6L8.6-.4L6.6-.8Z" fill="#1d1d1d"/><path d="M2.2-2.8Q1.2-5.6 1.6-8.4" stroke="#1d1d1d" stroke-width=".9" fill="none"/><circle cx="1.6" cy="-8.6" r=".95" fill="#f7f5ee"/><circle cx="1.5" cy="-9.3" r=".45" fill="#d6332a"/><path d="M.8-8.7L-1.4-8.3L.8-8.2Z" fill="#9a8a5a"/></g></g>"##,
    r##"<g id="taketombo"><path d="M0 0V8" stroke="#b08a52" stroke-width=".9" stroke-linecap="round"/><path d="M-7.5-.9Q0-2.2 7.5-.9Q7.8.1 7.5.5Q0-.6-7.5.5Q-7.8.1-7.5-.9Z" fill="#c9a66b"/><circle r=".9" fill="#8a6a3a"/></g>"##,
    r##"<g id="goen"><g transform="scale(1 .5)"><circle r="4.4" fill="#c9a243"/><circle r="3.9" fill="none" stroke="#a9832e" stroke-width=".35"/><circle r="1.25" fill="#4a3826"/><path d="M-3.2 1.2Q-2 2.6-.4 3.4M3.2 1.2Q2 2.6.4 3.4" fill="none" stroke="#9c7a2a" stroke-width=".35"/><path d="M-2.6-2.2l.8.5M-2.1-2.9l.6.7M2.6-2.2l-.8.5M2.1-2.9l-.6.7" stroke="#9c7a2a" stroke-width=".3"/></g><path d="M-2.8-1.2Q-1.4-2.2.4-2" fill="none" stroke="#fff3c4" stroke-width=".45" opacity=".8"/></g>"##,
);

/// The SVG document for one variant.
pub fn svg(bonsai: &Bonsai, config: &Config, theme: Theme) -> String {
    let painted = paint::paint(bonsai, config, theme);
    let size = config.render.size;
    let (x, y, w, h) = painted.frame;
    let height = (f64::from(size) * h / w).round();
    let (year, week) = bonsai.observed_on.iso_week();
    let mut out = String::with_capacity(painted.body.len() + 16 * 1024);
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}" width="{size}" height="{height}" role="img" aria-labelledby="t d">"#,
        Num(x),
        Num(y),
        Num(w),
        Num(h)
    );
    let _ = write!(
        out,
        r#"<title id="t">Bonsai for @{}</title><desc id="d">{}</desc>"#,
        escape(&bonsai.login),
        escape(&bonsai.description())
    );
    let _ = write!(
        out,
        "<metadata>gitbonsai {VERSION}; observed {year}-W{week:02}; {}; {}</metadata>",
        bonsai.leader.style,
        theme.name()
    );
    out.push_str("<defs>");
    if config.render.animate {
        out.push_str(ANIMATION_CSS);
    }
    let _ = write!(out, r#"<path id="ml" d="{}"/>"#, maple_leaf_path());
    out.push_str(SYMBOLS);
    out.push_str("</defs>");
    out.push_str(&painted.body);
    out.push_str("</svg>\n");
    out
}

/// Escapes text for XML content and attributes.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_have_two_decimals_at_most_and_no_negative_zero() {
        assert_eq!(Num(1.0).to_string(), "1");
        assert_eq!(Num(1.256).to_string(), "1.26");
        assert_eq!(Num(-0.001).to_string(), "0");
        assert_eq!(Num(-12.3).to_string(), "-12.3");
        assert_eq!(Num::coarse(1.26).to_string(), "1.3");
        assert_eq!(Num::coarse(-0.04).to_string(), "0");
    }

    #[test]
    fn text_is_escaped() {
        assert_eq!(escape("a<b & \"c\""), "a&lt;b &amp; &quot;c&quot;");
    }
}
