// gitbonsai.dev: the theme toggle, the timelapse player, "see your own tree"
// (api.gitbonsai.dev) and the how-to. Compiled to app.js by build.sh.

/** The element with `id`; the page always has it. */
function $<T extends HTMLElement = HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el as T;
}
const root = document.documentElement;
const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;

/* Theme: opens dark, the toggle's choice is remembered */
type Theme = "light" | "dark";
try {
  const t = localStorage.getItem("gitbonsai-theme");
  if (t === "light" || t === "dark") root.dataset.theme = t;
} catch {}
const theme = (): Theme => (root.dataset.theme === "light" ? "light" : "dark");
function paint(): void {
  document.querySelectorAll<HTMLImageElement>("img[data-tree]").forEach((i) => {
    i.src = `${i.dataset.tree}-${theme()}.svg`;
  });
  document.querySelectorAll<HTMLImageElement>("img[data-thumb]").forEach((i) => {
    i.src = `${i.dataset.thumb}-${theme()}.png`;
  });
  $("theme").setAttribute("aria-label", theme() === "dark" ? "Switch to light mode" : "Switch to dark mode");
  if (showing) showTree();
  if (cur) {
    stop();
    load(true);
  }
}
$("theme").addEventListener("click", () => {
  root.dataset.theme = theme() === "dark" ? "light" : "dark";
  try {
    localStorage.setItem("gitbonsai-theme", root.dataset.theme);
  } catch {}
  paint();
});

/* Hero gallery: the plate shows one tree, the row of thumbnails under it picks
   which. The Showcase is in the page itself; plates.json adds the rest. */
interface Plate {
  id: string;
  name: string;
  /** The shape the SVG records: chokkan, moyogi, shakan, fukinagashi or kengai. */
  style: string;
  /** The SVG's own description, for the alt text. */
  alt: string;
  /** The SVG's viewBox: x, y, width, height. */
  box: [number, number, number, number];
  /** The viewBox of the same tree rendered with its legend on the right. */
  lbox: [number, number, number, number];
}
const gallery = $("gallery");
const heroImg = $<HTMLImageElement>("heroimg");
const heroCanvas = $("herocanvas");
const heroLegend = $<HTMLInputElement>("herolegend");
// The legend's text is unreadable at phone width, so its cell is hidden there (style.css).
const narrow = matchMedia("(max-width: 600px)");
const withLegend = (): boolean => heroLegend.checked && !narrow.matches;
const variant = (p: Plate): string => `hero/${p.id}${withLegend() ? "-legend" : ""}`;
let plates: Plate[] = [];
let shown = -1;
const plateNo = (i: number): string => String(i + 1).padStart(2, "0");
// Grow-in, once: the plates before the Showcase pass by, then it rests there.
// Not with reduced motion, not when the page opens scrolled past the hero, and
// never again once a visitor picks a plate.
const growIn = !reduce && scrollY < 120;
let touched = false;
if (growIn) heroCanvas.classList.add("waiting");

// Every tree sits on one canvas at one scale, the pot where the SVG puts it, so a
// sprout is small next to a tree of eighteen years. With the legend on, each tree
// and its legend are fitted to that same canvas instead, so the plate never resizes.
function place(p: Plate, all: Plate[]): void {
  const x0 = Math.min(...all.map((q) => q.box[0]));
  const y0 = Math.min(...all.map((q) => q.box[1]));
  const w = Math.max(...all.map((q) => q.box[0] + q.box[2])) - x0;
  const h = Math.max(...all.map((q) => q.box[1] + q.box[3])) - y0;
  heroCanvas.style.aspectRatio = `${w} / ${h}`;
  if (withLegend()) {
    const a = p.lbox[2] / p.lbox[3];
    const canvas = w / h;
    const width = a >= canvas ? 100 : (a / canvas) * 100;
    const height = a >= canvas ? (canvas / a) * 100 : 100;
    heroImg.style.left = `${(100 - width) / 2}%`;
    heroImg.style.top = `${(100 - height) / 2}%`;
    heroImg.style.width = `${width}%`;
    return;
  }
  heroImg.style.left = `${((p.box[0] - x0) / w) * 100}%`;
  heroImg.style.top = `${((p.box[1] - y0) / h) * 100}%`;
  heroImg.style.width = `${(p.box[2] / w) * 100}%`;
}
function showPlate(i: number): void {
  const p = plates[i];
  if (!p) return;
  shown = i;
  heroImg.dataset.tree = variant(p);
  heroImg.src = `${variant(p)}-${theme()}.svg`;
  heroImg.alt = p.alt;
  place(p, plates);
  $("herono").textContent = plateNo(i);
  $("herofor").textContent = p.name;
  $("herostyle").textContent = p.style;
  check(i);
}
/** Marks thumbnail `i` as the chosen radio, with the roving tab stop on it. */
function check(i: number): void {
  gallery.querySelectorAll<HTMLElement>(".thumb").forEach((b, j) => {
    b.setAttribute("aria-checked", String(j === i));
    b.tabIndex = j === i ? 0 : -1;
  });
}
for (const ev of ["pointerdown", "keydown"]) {
  gallery.addEventListener(ev, () => {
    touched = true;
    heroCanvas.classList.remove("waiting");
  });
}
// The legend box counts as a choice too: it stops the grow-in and redraws the plate.
heroLegend.addEventListener("change", () => {
  touched = true;
  heroCanvas.classList.remove("waiting");
  if (shown >= 0) showPlate(shown);
});
narrow.addEventListener("change", () => {
  if (heroLegend.checked && shown >= 0) showPlate(shown);
});
/** Shows the plates before `home` in turn, then `home`; stops as soon as a visitor picks one. */
async function playGrowIn(home: number): Promise<void> {
  const seq = [...plates.keys()].slice(0, home + 1);
  const ready = seq.map((i) => {
    const img = new Image();
    img.src = `hero/${plates[i]?.id}-${theme()}.svg`;
    return img.decode().catch(() => {});
  });
  // Each tree is fetched before it shows; a slow line just starts on the Showcase.
  const loaded = await Promise.race([
    Promise.all(ready).then(() => true),
    new Promise<boolean>((r) => setTimeout(() => r(false), 2500)),
  ]);
  heroCanvas.classList.remove("waiting");
  if (touched) return;
  if (!loaded) return showPlate(home);
  for (const i of seq) {
    if (touched) return;
    showPlate(i);
    if (i < home) await new Promise((r) => setTimeout(r, 900));
  }
}
gallery.addEventListener("keydown", (e) => {
  const at = [...gallery.children].findIndex((b) => b.getAttribute("aria-checked") === "true");
  const n = plates.length;
  const to =
    e.key === "ArrowRight" || e.key === "ArrowDown" ? (at + 1) % n
    : e.key === "ArrowLeft" || e.key === "ArrowUp" ? (at + n - 1) % n
    : e.key === "Home" ? 0
    : e.key === "End" ? n - 1
    : -1;
  if (to < 0) return;
  e.preventDefault();
  showPlate(to);
  (gallery.children[to] as HTMLElement | undefined)?.focus();
});
// "See it on @…" in Reading a tree: that plate, with its legend where there is room, in view.
// Without plates.json the link just scrolls up to the hero.
document.querySelectorAll<HTMLAnchorElement>(".seeit[data-plate]").forEach((a) => {
  a.addEventListener("click", (e) => {
    const i = plates.findIndex((p) => p.id === a.dataset.plate);
    if (i < 0) return;
    e.preventDefault();
    touched = true;
    heroCanvas.classList.remove("waiting");
    if (!narrow.matches) heroLegend.checked = true;
    showPlate(i);
    heroCanvas.scrollIntoView({ behavior: reduce ? "auto" : "smooth", block: "center" });
    // Focus follows to the chosen thumbnail, so a keyboard visitor is where they look.
    (gallery.children[i] as HTMLElement | undefined)?.focus({ preventScroll: true });
  });
});
// Reading a tree: each card shows as many of its further rows as fit beside
// the tallest card in its line, so the cards end level with no empty space;
// the rest fold behind "Show more", and "Show less" below them folds them
// back. Without script every row shows.
type Fold = { rows: HTMLElement[]; card: HTMLElement; b: HTMLButtonElement; open: boolean };
const folds: Fold[] = [];
document.querySelectorAll<HTMLElement>(".theme .legend.extra").forEach((dl, k) => {
  dl.id ||= `more-${k}`;
  const b = document.createElement("button");
  b.type = "button";
  b.className = "more";
  b.setAttribute("aria-controls", dl.id);
  const f: Fold = {
    rows: Array.from(dl.children) as HTMLElement[],
    card: dl.closest(".theme") as HTMLElement,
    b,
    open: false,
  };
  b.addEventListener("click", () => {
    f.open = !f.open;
    if (f.open) unfold(f, f.rows.map(() => true));
    else fill();
  });
  dl.after(b);
  folds.push(f);
});
// Shows the further rows `shown` marks, all of them when open; the toggle
// stays while any is folded.
function unfold(f: Fold, shown: boolean[]): void {
  f.rows.forEach((r, i) => (r.hidden = !f.open && !shown[i]));
  f.b.hidden = !f.open && shown.every(Boolean);
  f.b.setAttribute("aria-expanded", String(f.open));
  f.b.textContent = f.open ? "Show less" : "Show more";
}
// Folds every closed card, then lets each take, in order, every further row
// that still fits the height its line already has: a row too tall is
// skipped, and a shorter one after it may still fit.
function fill(): void {
  const closed = folds.filter((f) => !f.open);
  closed.forEach((f) => unfold(f, f.rows.map(() => false)));
  for (const f of closed) {
    const line = f.card.offsetHeight;
    const shown = f.rows.map(() => false);
    f.rows.forEach((_, i) => {
      shown[i] = true;
      unfold(f, shown);
      if (f.card.offsetHeight > line) shown[i] = false;
    });
    unfold(f, shown);
  }
}
fill();
let refit = 0;
addEventListener("resize", () => {
  cancelAnimationFrame(refit);
  refit = requestAnimationFrame(fill);
});
void document.fonts.ready.then(fill);
fetch("plates.json")
  .then((r) => r.json() as Promise<Plate[]>)
  .then(
    (all) => {
      plates = all;
      plates.forEach((p, i) => {
        const b = document.createElement("button");
        b.className = "thumb";
        b.type = "button";
        b.setAttribute("role", "radio");
        b.title = b.ariaLabel = `Plate ${plateNo(i)}: ${p.name}, ${p.style}`;
        const img = document.createElement("img");
        img.dataset.thumb = `hero/${p.id}-thumb`;
        img.src = `${img.dataset.thumb}-${theme()}.png`;
        img.alt = "";
        const no = document.createElement("span");
        no.className = "no";
        no.textContent = plateNo(i);
        b.append(img, no);
        b.addEventListener("click", () => showPlate(i));
        // Fetch the full tree as soon as a thumbnail is pointed at, so the plate swaps at once.
        const warm = (): void => void (new Image().src = `${variant(p)}-${theme()}.svg`);
        b.addEventListener("pointerenter", warm, { once: true });
        b.addEventListener("focus", warm, { once: true });
        gallery.appendChild(b);
      });
      heroCanvas.classList.add("scaled");
      $("yourno").textContent = plateNo(plates.length);
      const home = Math.max(0, plates.findIndex((p) => p.id === "showcase"));
      // A checked plate and one tab stop before the row is shown, also during the grow-in.
      check(home);
      gallery.hidden = false;
      heroLegend.disabled = false;
      if (growIn) void playGrowIn(home);
      else showPlate(home);
    },
    // Without plates.json the hero keeps the Showcase from the page.
    () => heroCanvas.classList.remove("waiting"),
  );

/* Timelapse player: one MP4 per tree and theme, frames on a shared canvas */
interface Frame {
  /** ISO week, as the SVG records it. */
  wk: string;
  /** The SVG's description of the tree that week. */
  d: string;
}
interface Timelapse {
  w: number;
  h: number;
  fps: number;
  frames: Frame[];
}
type Example = [id: string, name: string, kind: "simulated" | "real", blurb: string];
// The Showcase on top, opened on load (see the frames fetch); the real
// profiles under it by total contributions.
const EX: Example[] = [
  ["showcase", "Showcase", "simulated", "Fourteen years; C# swapped for Java, early private work, then Python, Rust and TypeScript."],
  ["stevenbtw", "@StevenBtw", "real", "Around 9 years; Python and Rust on two trunks. Quiet summers without commits swept it to one side: windswept."],
  ["tj", "@tj", "real", "Around 19 years; JavaScript, later Go. Long pauses slant it; resting since 2021, so bare for the winter."],
  ["torvalds", "@torvalds", "real", "Around 16 years of C, every single week: a thick, symmetric formal upright."],
  ["steipete", "@steipete", "real", "Around 17 years; TypeScript is the trunk, with Swift on its own strand. Quiet spells between bursts of work start to bend its crown downwind, an upright turning windswept."],
];
const PLAY = "M8 5.5v13l10.5-6.5z";
const PAUSE = "M7 5h3.5v14H7zM13.5 5H17v14h-3.5z";
const stage = $("stage");
const scrub = $<HTMLInputElement>("scrub");
const play = $<HTMLButtonElement>("play");
const picks = $("picks");
let FRAMES: Record<string, Timelapse> | null = null;
let cur: string | null = null;
let idx = 0;
let playing = false;
const vid = document.createElement("video");
vid.muted = true;
vid.playsInline = true;
vid.preload = "auto";
vid.setAttribute("aria-hidden", "true");
stage.appendChild(vid);

/** The chosen timelapse; only called once one is chosen. */
function T(): Timelapse {
  const t = FRAMES && cur ? FRAMES[cur] : undefined;
  if (!t) throw new Error("no timelapse chosen");
  return t;
}
const last = (): number => T().frames.length - 1;

function load(keep: boolean): void {
  const at = keep ? idx : last();
  vid.src = `tl/${cur}-${theme()}.mp4`;
  vid.addEventListener("loadedmetadata", () => show(at), { once: true });
}
function select(id: string): void {
  stop();
  cur = id;
  stage.style.aspectRatio = `${T().w} / ${T().h}`;
  scrub.max = String(last());
  const pick = picks.querySelector<HTMLElement>(`[data-id="${id}"]`);
  picks.querySelectorAll<HTMLElement>(".pick").forEach((p) => {
    p.setAttribute("aria-checked", String(p === pick));
    p.tabIndex = p === pick ? 0 : -1;
  });
  // On phones the picks are a sideways strip: bring the chosen one into it.
  if (pick) picks.scrollLeft += pick.getBoundingClientRect().left - picks.getBoundingClientRect().left - 16;
  load(false);
  label(last());
}
function label(i: number): void {
  const f = T().frames[i];
  if (!f) return;
  idx = i;
  scrub.value = String(i);
  const b = document.createElement("b");
  b.textContent = f.wk;
  $("readout").replaceChildren(b, ` · ${i + 1} / ${T().frames.length}`);
  $("caption").textContent = f.d;
}
function show(i: number): void {
  label(i);
  vid.currentTime = (i + 0.5) / T().fps;
}
function tick(): void {
  if (!playing) return;
  label(Math.min(last(), Math.floor(vid.currentTime * T().fps)));
  requestAnimationFrame(tick);
}
function stop(): void {
  playing = false;
  vid.pause();
  $("playicon").setAttribute("d", PLAY);
  play.setAttribute("aria-label", "Play timelapse");
}
play.addEventListener("click", () => {
  if (!cur) return;
  if (playing) return stop();
  if (idx >= last()) show(0);
  // A whole life plays in about ten seconds, whatever its length.
  vid.playbackRate = Math.min(4, Math.max(0.1, T().frames.length / T().fps / (reduce ? 20 : 10)));
  playing = true;
  void vid.play();
  requestAnimationFrame(tick);
  $("playicon").setAttribute("d", PAUSE);
  play.setAttribute("aria-label", "Pause timelapse");
});
vid.addEventListener("ended", () => {
  stop();
  label(last());
});
scrub.addEventListener("input", () => {
  if (cur) {
    stop();
    show(Number(scrub.value));
  }
});
for (const [id, name, kind, blurb] of EX) {
  const b = document.createElement("button");
  b.className = "pick";
  b.type = "button";
  b.setAttribute("role", "radio");
  b.dataset.id = id;
  b.innerHTML = `<span class="n">${name} <span class="t">${kind}</span></span><span class="s">${blurb}</span>`;
  b.addEventListener("click", () => {
    if (FRAMES) select(id);
  });
  picks.appendChild(b);
}
picks.addEventListener("keydown", (e) => {
  if (!FRAMES || !["ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight"].includes(e.key)) return;
  e.preventDefault();
  const ids = EX.map((x) => x[0]);
  const step = e.key === "ArrowDown" || e.key === "ArrowRight" ? 1 : ids.length - 1;
  const next = ids[(ids.indexOf(cur ?? "") + step) % ids.length];
  if (!next) return;
  select(next);
  picks.querySelector<HTMLElement>(`[data-id="${next}"]`)?.focus();
});
fetch("frames.json")
  .then((r) => r.json() as Promise<Record<string, Timelapse>>)
  .then(
    (f) => {
      FRAMES = f;
      select("showcase");
    },
    () => {
      $("caption").textContent = "The timelapses didn't load. Reload the page to try again.";
    },
  );

/* Your tree: grown by api.gitbonsai.dev, shown only as <img> from a blob URL */
const API = "https://api.gitbonsai.dev/v1/tree/";
/** A 200 from the API. */
interface Tree {
  login: string;
  observed_on: string;
  style: string;
  gitbonsai: string;
  legend: boolean;
  light: string;
  dark: string;
}
const ERRORS = {
  invalid_login: "That isn't a GitHub username: letters, digits and single hyphens, up to 39 characters.",
  no_such_user: "There's no GitHub user with that username.",
  rate_limited: "That's the limit of new trees from here for this hour. This one can grow here in {wait}.",
  busy: "Too many trees at once. GitHub lets the grower read only so much per hour; yours can grow here in {wait}.",
  failed: "This tree didn't grow. Try again in a while.",
} as const;
// Own keys only: an error code such as "toString" must not find a method.
const errorText = (code: unknown): string =>
  typeof code === "string" && Object.hasOwn(ERRORS, code) ? ERRORS[code as keyof typeof ERRORS] : ERRORS.failed;
const art = $("yourart");
const grow = $<HTMLButtonElement>("grow");
const legend = $<HTMLInputElement>("legend");
let grown: { login: string; light: string; dark: string } | null = null;
let showing = false;
let asked: string | null = null;
let timer: number | undefined;
// Grow and the legend box are locked while a tree grows or a wait runs down,
// so one request is ever in flight and the box always matches the tree shown.
function lock(on: boolean): void {
  grow.disabled = on;
  legend.disabled = on;
}
function say(text: string): void {
  const p = document.createElement("p");
  p.className = "msg";
  p.textContent = text;
  art.replaceChildren(p);
  showing = false;
}
function showTree(): void {
  if (!grown) return;
  const img = document.createElement("img");
  img.src = grown[theme()];
  img.alt = `The bonsai grown from the public activity of @${grown.login}`;
  art.replaceChildren(img);
  showing = true;
}
const inWords = (s: number): string => (s < 90 ? `${s} seconds` : `${Math.ceil(s / 60)} minutes`);
/** A wait the visitor can't skip: seedlings still waiting (three when everyone's
 *  trees queue, one when this visitor has had theirs), the wait counting down, and
 *  the way round it, their own profile, where our limits don't apply. */
function hold(seconds: number, template: string, pots: number): void {
  let left = seconds;
  lock(true);
  const bench = document.createElement("div");
  bench.className = "bench";
  for (let i = 0; i < pots; i++) {
    const img = document.createElement("img");
    img.dataset.tree = "hero/waiting"; // paint() swaps its theme
    img.src = `hero/waiting-${theme()}.svg`;
    img.alt = "";
    bench.append(img);
  }
  const [before = "", after = ""] = template.split("{wait}");
  const count = document.createElement("span");
  count.setAttribute("aria-live", "off"); // the plate reads the message once, not every second
  count.textContent = inWords(left);
  const line = document.createElement("p");
  line.className = "why";
  line.append(before, count, after);
  const way = document.createElement("p");
  way.textContent = "Or grow it where it lives: add it to your profile, and it grows from your own activity, private work included.";
  const go = document.createElement("a");
  go.className = "btn btn-outline";
  go.href = "#add";
  go.textContent = "Add it to your profile";
  const rest = document.createElement("div");
  rest.className = "rest";
  rest.append(bench, line, way, go);
  art.replaceChildren(rest);
  showing = false;
  timer = window.setInterval(() => {
    if (--left > 0) {
      count.textContent = inWords(left);
      return;
    }
    window.clearInterval(timer);
    timer = undefined;
    lock(false);
    say("Ready to try again.");
  }, 1000);
}
async function fetchTree(login: string): Promise<void> {
  if (grow.disabled) return;
  lock(true);
  asked = login;
  say(`Reading the public activity of @${login}. A first tree can take up to a minute.`);
  let r: Response | null = null;
  let body: Partial<Tree> & { error?: unknown } = {};
  try {
    r = await fetch(API + encodeURIComponent(login) + (legend.checked ? "?legend=1" : ""));
    // A body that isn't a JSON object (null, a number) takes the failure path.
    const parsed: unknown = await r.json();
    if (parsed && typeof parsed === "object") body = parsed as typeof body;
  } catch {}
  lock(false);
  if (!r) return say("The grower can't be reached right now. Try again in a while.");
  if (!r.ok || !body.login || !body.light || !body.dark) {
    const text = errorText(body.error);
    const wait = parseInt(r.headers.get("Retry-After") ?? "", 10);
    // Only the waits get the seedlings; a mistyped username keeps its one plain line.
    if (body.error === "busy" || body.error === "rate_limited") {
      return hold(wait > 0 ? wait : 60, text, body.error === "busy" ? 3 : 1);
    }
    return say(text);
  }
  if (grown) {
    URL.revokeObjectURL(grown.light);
    URL.revokeObjectURL(grown.dark);
  }
  const url = (svg: string): string => URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
  grown = { login: body.login, light: url(body.light), dark: url(body.dark) };
  $("yourlogin").textContent = `@${body.login}`;
  $("yourstyle").textContent = body.style ?? "";
  $("yourweek").textContent = body.observed_on ?? "";
  showTree();
}
$("form").addEventListener("submit", (e) => {
  e.preventDefault();
  const login = $<HTMLInputElement>("login").value.trim().replace(/^@/, "");
  if (login) void fetchTree(login);
});
legend.addEventListener("change", () => {
  if (asked) void fetchTree(asked);
});

/* How-to: the drawn screen plays the three steps in a loop, in step with the list */
const scenes = [...document.querySelectorAll<HTMLElement>(".scene")];
const steps = [...document.querySelectorAll<HTMLElement>(".step")];
const paths = ["you/you · new file", "you/you · Actions", "you/you · profile"];
document.querySelectorAll<HTMLElement>(".type .ln, .files div").forEach((el) => {
  el.style.setProperty("--i", String([...(el.parentElement?.children ?? [])].indexOf(el)));
});
let scene = 0;
let loop: number | undefined;
function go(n: number): void {
  scene = n;
  scenes.forEach((s, i) => {
    s.classList.remove("on");
    void s.offsetWidth; // restart the scene's animations
    s.classList.toggle("on", i === n);
  });
  steps.forEach((s, i) => s.setAttribute("aria-current", String(i === n)));
  $("scenepath").textContent = paths[n] ?? "";
}
steps.forEach((s, i) =>
  s.querySelector(".h")?.addEventListener("click", () => {
    window.clearInterval(loop);
    go(i);
  }),
);
document.querySelectorAll<HTMLButtonElement>(".copy").forEach((b) =>
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    const pre = b.nextElementSibling;
    if (!pre) return;
    // Through a promise, so a clipboard that's missing or throws (an insecure
    // page, an old browser) still falls back to selecting the text.
    Promise.resolve()
      .then(() => navigator.clipboard.writeText(pre.textContent ?? ""))
      .then(
      () => {
        b.textContent = "Copied";
        setTimeout(() => {
          b.textContent = "Copy";
        }, 1600);
      },
      () => {
        const r = document.createRange();
        r.selectNodeContents(pre);
        const s = getSelection();
        s?.removeAllRanges();
        s?.addRange(r);
        b.textContent = "Selected";
      },
    );
  }),
);

paint();
go(0);
if (!reduce) loop = window.setInterval(() => go((scene + 1) % 3), 5200);

export {};
