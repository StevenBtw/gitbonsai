//! Local HTML pages: the preview, the design gallery and the timelapse
//! scrubber. These are for people at a desk, not for the README, so they may
//! use a little JavaScript.

use std::fmt::Write as _;

use crate::render::escape;

const STYLE: &str = r#"
:root{color-scheme:light dark;font:14px/1.45 system-ui,-apple-system,"Segoe UI",sans-serif}
body{margin:0;padding:24px 32px;background:#ffffff;color:#1f2328}
body.dark{background:#0d1117;color:#e6edf3}
h1{font-size:20px;margin:0 0 4px}h2{font-size:16px;margin:28px 0 2px}
p.about{margin:0 0 10px;opacity:.75}
.bar{display:flex;gap:12px;align-items:center;margin-bottom:12px;position:sticky;top:0;padding:8px 0;background:inherit;z-index:1}
button{font:inherit;padding:4px 12px;border-radius:6px;border:1px solid #8884;background:transparent;color:inherit;cursor:pointer}
.row{display:flex;gap:10px;flex-wrap:wrap;margin:6px 0 2px}
.row-label{font-size:12px;opacity:.6;margin-top:10px}
figure{margin:0;width:var(--w,240px)}
figure img{width:100%;display:block;border-radius:8px;border:1px solid #8882}
figcaption{font-size:12px;margin-top:3px}
figcaption small{display:block;opacity:.6}
body:not(.dark) img.dark,body.dark img.light{display:none}
"#;

const TOGGLE: &str = r#"<script>
const b=document.body;if(localStorage.bonsaiTheme==="dark")b.classList.add("dark");
function toggleTheme(){b.classList.toggle("dark");localStorage.bonsaiTheme=b.classList.contains("dark")?"dark":"light"}
function size(px){document.documentElement.style.setProperty("--w",px+"px")}
</script>"#;

fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>{}</title><style>{STYLE}</style></head><body>{body}{TOGGLE}</body></html>\n",
        escape(title)
    )
}

/// A tile: one tree, in both variants.
#[derive(Debug, Clone)]
pub struct Tile {
    /// The file path without `-light.svg` / `-dark.svg`.
    pub base: String,
    /// The headline under the tree.
    pub caption: String,
    /// The numbers under the headline.
    pub detail: String,
}

/// A gallery section: one account.
#[derive(Debug, Clone)]
pub struct Section {
    /// The account's name.
    pub title: String,
    /// What it exercises.
    pub about: String,
    /// Labeled rows of tiles.
    pub rows: Vec<(String, Vec<Tile>)>,
}

fn tile(t: &Tile) -> String {
    format!(
        r#"<figure><img class="light" src="{0}-light.svg" alt=""><img class="dark" src="{0}-dark.svg" alt=""><figcaption>{1}<small>{2}</small></figcaption></figure>"#,
        escape(&t.base),
        escape(&t.caption),
        escape(&t.detail)
    )
}

/// The design gallery: every account, style and growth stage on one page.
pub fn gallery(sections: &[Section]) -> String {
    let mut body = String::from(
        r#"<h1>gitbonsai gallery</h1><p class="about">Every fixture in every style, plus growth stages. Toggle the background to check both variants.</p><div class="bar"><button onclick="toggleTheme()">Light / dark</button><button onclick="size(180)">S</button><button onclick="size(240)">M</button><button onclick="size(360)">L</button></div>"#,
    );
    for section in sections {
        let _ = write!(
            body,
            r#"<h2>{}</h2><p class="about">{}</p>"#,
            escape(&section.title),
            escape(&section.about)
        );
        for (label, tiles) in &section.rows {
            let _ = write!(
                body,
                r#"<div class="row-label">{}</div><div class="row">"#,
                escape(label)
            );
            for t in tiles {
                body.push_str(&tile(t));
            }
            body.push_str("</div>");
        }
    }
    page("gitbonsai gallery", &body)
}

/// The preview written next to the outputs: both variants on GitHub's light
/// and dark backgrounds.
pub fn preview(login: &str, png: bool) -> String {
    let mut body = format!(
        r#"<h1>Bonsai for @{}</h1><p class="about">As GitHub will show it. Toggle the background to see the other variant.</p><div class="bar"><button onclick="toggleTheme()">Light / dark</button></div><div class="row" style="--w:400px">"#,
        escape(login)
    );
    body.push_str(&tile(&Tile {
        base: "bonsai".into(),
        caption: "Animated SVG".into(),
        detail: "bonsai-light.svg / bonsai-dark.svg".into(),
    }));
    if png {
        body.push_str(
            r#"<figure><img src="bonsai.png" alt=""><figcaption>Static PNG<small>bonsai.png, light variant</small></figcaption></figure>"#,
        );
    }
    body.push_str("</div>");
    let _ = write!(
        body,
        r#"<h2>README</h2><pre>&lt;picture&gt;
  &lt;source media="(prefers-color-scheme: dark)" srcset="bonsai/bonsai-dark.svg" /&gt;
  &lt;img src="bonsai/bonsai-light.svg" width="400" alt="My bonsai, grown from my GitHub activity" /&gt;
&lt;/picture&gt;</pre>"#
    );
    page(&format!("Bonsai for @{login}"), &body)
}

/// The timelapse scrubber over numbered frames.
pub fn timelapse(title: &str, frames: &[(String, String)]) -> String {
    let list: Vec<String> = frames
        .iter()
        .map(|(base, label)| format!("[{:?},{:?}]", base, label))
        .collect();
    let body = format!(
        r#"<h1>{}</h1><div class="bar"><button onclick="toggleTheme()">Light / dark</button><button id="play">Play</button><input id="t" type="range" min="0" max="{}" value="{}" style="width:420px"><span id="label"></span></div>
<figure style="--w:480px"><img id="l" class="light" alt=""><img id="d" class="dark" alt=""></figure>
<script>
const frames=[{}];const t=document.getElementById("t");
function show(i){{const f=frames[i];document.getElementById("l").src=f[0]+"-light.svg";document.getElementById("d").src=f[0]+"-dark.svg";document.getElementById("label").textContent=f[1]}}
t.oninput=()=>show(+t.value);show(+t.value);let timer=null;
document.getElementById("play").onclick=()=>{{if(timer){{clearInterval(timer);timer=null;return}}if(+t.value>=frames.length-1)t.value=0;timer=setInterval(()=>{{if(+t.value>=frames.length-1){{clearInterval(timer);timer=null;return}}t.value=+t.value+1;show(+t.value)}},120)}};
</script>"#,
        escape(title),
        frames.len().saturating_sub(1),
        frames.len().saturating_sub(1),
        list.join(",")
    );
    page(title, &body)
}
