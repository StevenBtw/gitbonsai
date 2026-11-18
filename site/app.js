"use strict";
const $ = id => document.getElementById(id);
const root = document.documentElement;
const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;

/* Theme: opens dark, the toggle's choice is remembered */
try { const t = localStorage.getItem("gitbonsai-theme"); if (t === "light" || t === "dark") root.dataset.theme = t; } catch (e) {}
const theme = () => root.dataset.theme === "light" ? "light" : "dark";
function paint() {
  document.querySelectorAll("img[data-tree]").forEach(i => { i.src = i.dataset.tree + "-" + theme() + ".svg"; });
  $("theme").setAttribute("aria-label", theme() === "dark" ? "Switch to light mode" : "Switch to dark mode");
  if (showing) showTree();
  if (cur) { stop(); load(true); }
}
$("theme").addEventListener("click", () => {
  root.dataset.theme = theme() === "dark" ? "light" : "dark";
  try { localStorage.setItem("gitbonsai-theme", root.dataset.theme); } catch (e) {}
  paint();
});

/* Timelapse player: one MP4 per tree and theme, frames on a shared canvas */
const EX = [
  ["showcase", "Showcase", "simulated", "Fourteen years; C# swapped for Java, early private work, then Python, Rust and TypeScript."],
  ["newcomer", "Newcomer", "simulated", "A few months in, three languages: a thin sprout in its first pot."],
  ["stevenbtw", "@StevenBtw", "real", "Around 9 years; mainly Python and Rust, Rust coming away from the trunk on its own strand."],
  ["tj", "@tj", "real", "Around 19 years; JavaScript, later Go; resting since 2021, so bare for the winter."],
  ["torvalds", "@torvalds", "real", "Around 16 years of C, every single week; one steady trunk."],
  ["steipete", "@steipete", "real", "Around 17 years; a recent burst of work, its newer languages on their own strands."]
];
const PLAY = "M8 5.5v13l10.5-6.5z", PAUSE = "M7 5h3.5v14H7zM13.5 5H17v14h-3.5z";
const stage = $("stage"), scrub = $("scrub"), play = $("play");
let FRAMES = null, cur = null, idx = 0, playing = false;
const vid = document.createElement("video");
vid.muted = true; vid.playsInline = true; vid.preload = "auto"; vid.setAttribute("aria-hidden", "true");
stage.appendChild(vid);
const T = () => FRAMES[cur], last = () => T().frames.length - 1;

function load(keep) {
  const at = keep ? idx : last();
  vid.src = "tl/" + cur + "-" + theme() + ".mp4";
  vid.addEventListener("loadedmetadata", () => show(at), { once: true });
}
function select(id) {
  stop(); cur = id;
  stage.style.aspectRatio = T().w + " / " + T().h;
  scrub.max = last();
  const picks = $("picks"), pick = picks.querySelector('[data-id="' + id + '"]');
  picks.querySelectorAll(".pick").forEach(p => { p.setAttribute("aria-checked", p === pick); p.tabIndex = p === pick ? 0 : -1; });
  // on phones the picks are a sideways strip: bring the chosen one into it
  picks.scrollLeft += pick.getBoundingClientRect().left - picks.getBoundingClientRect().left - 16;
  load(false); label(last());
}
function label(i) {
  const f = T().frames[i]; idx = i; scrub.value = i;
  const b = document.createElement("b"); b.textContent = f.wk;
  $("readout").replaceChildren(b, " · " + (i + 1) + " / " + T().frames.length);
  $("caption").textContent = f.d;
}
function show(i) { label(i); vid.currentTime = (i + 0.5) / T().fps; }
function tick() {
  if (!playing) return;
  label(Math.min(last(), Math.floor(vid.currentTime * T().fps)));
  requestAnimationFrame(tick);
}
function stop() { playing = false; vid.pause(); $("playicon").setAttribute("d", PLAY); play.setAttribute("aria-label", "Play timelapse"); }
play.addEventListener("click", () => {
  if (!cur) return;
  if (playing) return stop();
  if (idx >= last()) show(0);
  // a whole life plays in about ten seconds, whatever its length
  vid.playbackRate = Math.min(4, Math.max(0.1, (T().frames.length / T().fps) / (reduce ? 20 : 10)));
  playing = true; vid.play(); requestAnimationFrame(tick);
  $("playicon").setAttribute("d", PAUSE); play.setAttribute("aria-label", "Pause timelapse");
});
vid.addEventListener("ended", () => { stop(); label(last()); });
scrub.addEventListener("input", () => { if (cur) { stop(); show(+scrub.value); } });
EX.forEach(([id, name, kind, blurb]) => {
  const b = document.createElement("button");
  b.className = "pick"; b.type = "button"; b.setAttribute("role", "radio"); b.dataset.id = id;
  b.innerHTML = '<span class="n">' + name + ' <span class="t">' + kind + '</span></span><span class="s">' + blurb + "</span>";
  b.addEventListener("click", () => FRAMES && select(id));
  $("picks").appendChild(b);
});
$("picks").addEventListener("keydown", e => {
  if (!FRAMES || !["ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight"].includes(e.key)) return;
  e.preventDefault();
  const ids = EX.map(x => x[0]), n = (ids.indexOf(cur) + (e.key === "ArrowDown" || e.key === "ArrowRight" ? 1 : ids.length - 1)) % ids.length;
  select(ids[n]); $("picks").querySelector('[data-id="' + ids[n] + '"]').focus();
});
fetch("frames.json").then(r => r.json()).then(f => {
  FRAMES = f;
  const s = f.showcase.frames; $("heroweek").textContent = s[s.length - 1].wk;
  select("stevenbtw");
}, () => { $("caption").textContent = "The timelapses didn't load. Reload the page to try again."; });

/* Your tree: grown by api.gitbonsai.dev, shown only as <img> from a blob URL */
const API = "https://api.gitbonsai.dev/v1/tree/";
const ERRORS = {
  invalid_login: "That isn't a GitHub login: letters, digits and single hyphens, up to 39 characters.",
  no_such_user: "There's no GitHub user with that login.",
  rate_limited: "That's the limit of new trees from here for this hour. Try again in {wait}.",
  busy: "The grower is busy with other trees. Try again in {wait}.",
  failed: "This tree didn't grow. Try again in a while."
};
const art = $("yourart"), grow = $("grow"), legend = $("legend");
let grown = null, showing = false, asked = null, timer = null;
// Grow and the legend box are locked while a tree grows or a wait runs down,
// so one request is ever in flight and the box always matches the tree shown.
function lock(on) { grow.disabled = on; legend.disabled = on; }
function say(text) { const p = document.createElement("p"); p.className = "msg"; p.textContent = text; art.replaceChildren(p); showing = false; }
function showTree() {
  const img = document.createElement("img");
  img.src = grown[theme()]; img.alt = "The bonsai grown from the public activity of @" + grown.login;
  art.replaceChildren(img); showing = true;
}
function inWords(s) { return s < 90 ? s + " seconds" : Math.ceil(s / 60) + " minutes"; }
function hold(seconds, template) {
  let left = seconds;
  lock(true);
  say(template.replace("{wait}", inWords(left)));
  timer = setInterval(() => {
    if (--left > 0) return say(template.replace("{wait}", inWords(left)));
    clearInterval(timer); timer = null; lock(false); say("Ready to try again.");
  }, 1000);
}
async function fetchTree(login) {
  if (grow.disabled) return;
  lock(true); asked = login;
  say("Reading the public activity of @" + login + ". A first tree can take up to a minute.");
  let r = null, body = {};
  try {
    r = await fetch(API + encodeURIComponent(login) + (legend.checked ? "?legend=1" : ""));
    body = await r.json();
  } catch (e) {}
  lock(false);
  if (!r) return say("The grower can't be reached right now. Try again in a while.");
  if (!r.ok || !body.light) {
    const text = ERRORS[body.error] || ERRORS.failed;
    const wait = parseInt(r.headers.get("Retry-After"), 10);
    return text.includes("{wait}") ? hold(wait > 0 ? wait : 60, text) : say(text);
  }
  if (grown) { URL.revokeObjectURL(grown.light); URL.revokeObjectURL(grown.dark); }
  const url = svg => URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
  grown = { login: body.login, light: url(body.light), dark: url(body.dark) };
  $("yourlogin").textContent = "@" + body.login;
  $("yourstyle").textContent = body.style;
  $("yourweek").textContent = body.observed_on;
  showTree();
}
$("form").addEventListener("submit", e => {
  e.preventDefault();
  const login = $("login").value.trim().replace(/^@/, "");
  if (login) fetchTree(login);
});
legend.addEventListener("change", () => { if (asked) fetchTree(asked); });

/* How-to: the drawn screen plays the three steps in a loop, in step with the list */
const scenes = [...document.querySelectorAll(".scene")], steps = [...document.querySelectorAll(".step")];
const paths = ["you/you · new file", "you/you · Actions", "you/you · profile"];
document.querySelectorAll(".type .ln, .files div").forEach(el => el.style.setProperty("--i", [...el.parentNode.children].indexOf(el)));
let scene = 0, loop = null;
function go(n) {
  scene = n;
  scenes.forEach((s, i) => { s.classList.remove("on"); void s.offsetWidth; s.classList.toggle("on", i === n); });
  steps.forEach((s, i) => s.setAttribute("aria-current", i === n));
  $("scenepath").textContent = paths[n];
}
steps.forEach((s, i) => s.querySelector(".h").addEventListener("click", () => { clearInterval(loop); go(i); }));
document.querySelectorAll(".copy").forEach(b => b.addEventListener("click", e => {
  e.stopPropagation();
  const pre = b.nextElementSibling;
  navigator.clipboard.writeText(pre.textContent).then(() => { b.textContent = "Copied"; setTimeout(() => { b.textContent = "Copy"; }, 1600); }, () => {
    const r = document.createRange(); r.selectNodeContents(pre); const s = getSelection(); s.removeAllRanges(); s.addRange(r); b.textContent = "Selected";
  });
}));

paint();
go(0);
if (!reduce) loop = setInterval(() => go((scene + 1) % 3), 5200);
