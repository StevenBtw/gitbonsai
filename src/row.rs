//! A row of trees at one scale: several rendered bonsai merged into one SVG.
//!
//! Every tree keeps its own world units, so a sprout stands small beside an
//! old tree, and the row scales evenly to any width. Pots stand on one line
//! and each tree's name sits under it. Ids are prefixed per tree, so shared
//! symbols and clip paths never collide.

use std::fmt::Write as _;

use crate::Error;
use crate::date::Date;
use crate::render::{Coarse, Theme};

/// Space between two trees, in world units.
pub const GAP: f64 = 28.0;
/// Room under the trees for the names.
const NAME_ROOM: f64 = 40.0;

/// The trees `(name, svg)` side by side in one SVG, for `theme`.
pub fn row(trees: &[(String, String)], theme: Theme) -> String {
    let ink = match theme {
        Theme::Dark => "#e6edf3",
        Theme::Light => "#1f2328",
    };
    let parts: Vec<([f64; 4], String)> = trees
        .iter()
        .enumerate()
        .map(|(i, (_, svg))| inner(svg, &format!("r{i}")))
        .collect();
    let tall = parts.iter().map(|(vb, _)| vb[3]).fold(0.0, f64::max);
    let width =
        parts.iter().map(|(vb, _)| vb[2]).sum::<f64>() + GAP * parts.len().saturating_sub(1) as f64;
    let names: Vec<&str> = trees.iter().map(|(n, _)| n.as_str()).collect();
    let mut out = String::new();
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" role="img" aria-labelledby="row-t"><title id="row-t">Bonsai at one scale: {}</title>"#,
        Coarse(width),
        Coarse(tall + NAME_ROOM),
        escape(&names.join(", "))
    );
    let mut x = 0.0;
    for ((vb, body), name) in parts.iter().zip(&names) {
        let _ = write!(
            out,
            r#"<svg x="{}" y="{}" width="{}" height="{}" viewBox="{} {} {} {}" overflow="visible">{body}</svg>"#,
            Coarse(x),
            Coarse(tall - vb[3]),
            Coarse(vb[2]),
            Coarse(vb[3]),
            Coarse(vb[0]),
            Coarse(vb[1]),
            Coarse(vb[2]),
            Coarse(vb[3]),
        );
        let _ = write!(
            out,
            r#"<text x="{}" y="{}" text-anchor="middle" font-family="-apple-system,Segoe UI,Helvetica,Arial,sans-serif" font-size="24" font-weight="600" fill="{ink}">{}</text>"#,
            Coarse(x + vb[2] / 2.0),
            Coarse(tall + 30.0),
            escape(name)
        );
        x += vb[2] + GAP;
    }
    out.push_str("</svg>");
    out
}

/// A tree SVG's view box and its content, titles dropped and every id
/// prefixed with `prefix`.
fn inner(svg: &str, prefix: &str) -> ([f64; 4], String) {
    let open = svg.find("<svg").unwrap_or(0);
    let start = svg[open..].find('>').map_or(0, |i| open + i + 1);
    let end = svg.rfind("</svg>").unwrap_or(svg.len()).max(start);
    let root = &svg[open..start];
    let mut vb = [0.0; 4];
    if let Some(v) = root
        .split("viewBox=\"")
        .nth(1)
        .and_then(|r| r.split('"').next())
    {
        for (slot, n) in vb.iter_mut().zip(v.split_whitespace()) {
            *slot = n.parse().unwrap_or(0.0);
        }
    }
    let body = drop_element(&drop_element(&svg[start..end], "title"), "desc");
    (vb, prefix_ids(&body, prefix))
}

/// `body` without its `<tag ...>...</tag>` elements.
fn drop_element(body: &str, tag: &str) -> String {
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(a) = rest.find(&open) {
        out.push_str(&rest[..a]);
        match rest[a..].find(&close) {
            Some(b) => rest = &rest[a + b + close.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Every `id="x"`, `href="#x"` and `url(#x)` in `body` renamed to
/// `prefix` + `x`.
fn prefix_ids(body: &str, prefix: &str) -> String {
    const MARKS: [&str; 3] = ["id=\"", "href=\"#", "url(#"];
    let mut out = String::with_capacity(body.len() + body.len() / 20);
    let mut rest = body;
    loop {
        let next = MARKS
            .iter()
            .filter_map(|m| rest.find(m).map(|i| (i, m.len())))
            .min();
        let Some((i, len)) = next else { break };
        out.push_str(&rest[..i + len]);
        out.push_str(prefix);
        rest = &rest[i + len..];
    }
    out.push_str(rest);
    out
}

/// One tree of a row from the command line: `Name=snapshot`, or
/// `Name=snapshot@YYYY-MM-DD` to show it as it was that day. The snapshot
/// is a file or `fixture:<name>`.
///
/// # Errors
///
/// When the name, the snapshot or the day is missing or malformed.
pub fn spec(text: &str) -> Result<(String, String, Option<Date>), Error> {
    let bad = |why: &str| {
        Error::Config(format!(
            "row tree `{text}`: {why}; write Name=snapshot[@YYYY-MM-DD]"
        ))
    };
    let (name, rest) = text.split_once('=').ok_or_else(|| bad("no name"))?;
    if name.trim().is_empty() || rest.trim().is_empty() {
        return Err(bad("empty name or snapshot"));
    }
    // A trailing `@...` without path characters is a day.
    let (source, day) = match rest.rsplit_once('@') {
        Some((source, day)) if !day.contains(['/', '\\', '.']) => (
            source,
            Some(day.parse::<Date>().map_err(|_| bad("not a day"))?),
        ),
        _ => (rest, None),
    };
    if source.trim().is_empty() {
        return Err(bad("empty snapshot"));
    }
    Ok((name.to_string(), source.to_string(), day))
}

/// Text safe inside SVG.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bonsai, Config, fixtures, render};

    fn tree(name: &str) -> String {
        let snapshot = fixtures::by_name(name).unwrap().snapshot;
        let config = Config::default();
        render::svg(
            &Bonsai::grow(&snapshot, &config, None),
            &config,
            Theme::Light,
        )
    }

    fn ids(svg: &str) -> Vec<&str> {
        svg.split("id=\"")
            .skip(1)
            .map(|s| s.split('"').next().unwrap())
            .collect()
    }

    #[test]
    fn a_tree_is_named_with_an_optional_day() {
        assert_eq!(
            spec("Newbie=fixture:newbie").unwrap(),
            ("Newbie".to_string(), "fixture:newbie".to_string(), None)
        );
        let (_, source, day) = spec("@tj=gallery/tj.snapshot.json@2024-05-01").unwrap();
        assert_eq!(source, "gallery/tj.snapshot.json");
        assert_eq!(day, Some("2024-05-01".parse().unwrap()));
        assert!(spec("no-name").is_err());
        assert!(spec("x=a.json@soon").is_err());
        assert!(spec("x=@2024-05-01").is_err(), "a day without a snapshot");
    }

    #[test]
    fn ids_stay_unique_and_every_reference_resolves() {
        let a = tree("mono");
        let out = row(&[("A".into(), a.clone()), ("B".into(), a)], Theme::Light);
        let all = ids(&out);
        let unique: std::collections::BTreeSet<&str> = all.iter().copied().collect();
        assert_eq!(all.len(), unique.len(), "duplicate ids");
        for part in out.split("href=\"#").skip(1) {
            let target = part.split('"').next().unwrap();
            assert!(unique.contains(target), "#{target} points nowhere");
        }
        for part in out.split("url(#").skip(1) {
            let target = part.split(')').next().unwrap();
            assert!(unique.contains(target), "url(#{target}) points nowhere");
        }
    }

    #[test]
    fn trees_keep_one_scale_and_their_names() {
        let (a, b) = (tree("mono"), tree("steady"));
        let width = |svg: &str| -> f64 {
            let vb = svg
                .split("viewBox=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap();
            vb.split(' ').nth(2).unwrap().parse().unwrap()
        };
        let out = row(
            &[("Mono".into(), a.clone()), ("Steady".into(), b.clone())],
            Theme::Dark,
        );
        assert!((width(&out) - (width(&a) + width(&b) + GAP)).abs() < 1.0);
        assert!(out.contains(">Mono</text>") && out.contains(">Steady</text>"));
        assert_eq!(out.matches("<svg").count(), 3, "one row holding two trees");
    }
}
