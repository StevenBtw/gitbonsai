// gitbonsai.dev/exhibition: trees shown by their owners, with search, filters, sort,
// stars and a large view. Compiled to gallery.js by build.sh.
import {
  AGES,
  API,
  type Entry,
  type Query,
  authUrl,
  findIndex,
  isGrown,
  languagesByCount,
  parseList,
  parseResult,
  parseStarred,
  resultText,
  svgUrl,
  view as filtered,
} from "./gallery-core.js";

/** The element with `id`; the page always has it. */
function $<T extends HTMLElement = HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el as T;
}
const root = document.documentElement;

/* Theme: opens dark, the toggle's choice is remembered (the same key as the front page) */
// shortcut: duplicated from app.ts; move to a shared module if a third page appears
type Theme = "light" | "dark";
try {
  const t = localStorage.getItem("gitbonsai-theme");
  if (t === "light" || t === "dark") root.dataset.theme = t;
} catch {}
const theme = (): Theme => (root.dataset.theme === "light" ? "light" : "dark");
$("theme").addEventListener("click", () => {
  root.dataset.theme = theme() === "dark" ? "light" : "dark";
  try {
    localStorage.setItem("gitbonsai-theme", root.dataset.theme);
  } catch {}
  paint();
});

const grid = $("grid");
const status = $("status");
const search = $<HTMLInputElement>("search");
const style = $<HTMLSelectElement>("style");
const lang = $<HTMLSelectElement>("lang");
const age = $<HTMLSelectElement>("age");
for (const [k, label] of AGES) age.append(new Option(label, k));
const sort = $<HTMLSelectElement>("sort");
const dlg = $<HTMLDialogElement>("view");
let all: Entry[] = [];
let shown: Entry[] = [];
let starred = new Set<string>();
let open: number | null = null;
let legendOn = false;

function link(href: string, text: string): HTMLAnchorElement {
  const a = document.createElement("a");
  a.href = href;
  a.textContent = text;
  return a;
}
const same = (a: string | undefined, b: string): boolean => a?.toLowerCase() === b.toLowerCase();

/* Stars: one per tree per visitor; the API decides, the page shows it at once */
const STAR =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 2.8l2.85 5.95 6.55.85-4.8 4.55 1.2 6.5L12 17.5l-5.8 3.15 1.2-6.5-4.8-4.55 6.55-.85z"/></svg>';
function starButton(e: Entry): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "star";
  b.dataset.login = e.login;
  b.innerHTML = STAR; // a fixed icon, no data in it
  b.append(document.createElement("span"));
  paintStar(b, e);
  b.addEventListener("click", () => void toggle(e.login));
  return b;
}
// Logins with a star request in flight. Not `disabled`: that would drop keyboard focus.
const busy = new Set<string>();
function paintStar(b: HTMLButtonElement, e: Entry): void {
  const on = starred.has(e.login.toLowerCase());
  b.setAttribute("aria-disabled", String(busy.has(e.login.toLowerCase())));
  b.setAttribute("aria-pressed", String(on));
  b.setAttribute("aria-label", `${on ? "Unstar" : "Star"} @${e.login}'s tree, ${e.stars} ${e.stars === 1 ? "star" : "stars"}`);
  b.title = on ? "Unstar" : "Star";
  const n = b.querySelector("span");
  if (n) n.textContent = String(e.stars);
}
/** Every star button for `login` (on its card and in the large view) shows its state. */
function syncStars(login: string): void {
  const e = all[findIndex(all, login)];
  document.querySelectorAll<HTMLButtonElement>("button.star").forEach((b) => {
    if (e && same(b.dataset.login, login)) paintStar(b, e);
  });
}
async function toggle(login: string): Promise<void> {
  const e = all[findIndex(all, login)];
  const key = login.toLowerCase();
  if (!e || busy.has(key)) return; // one request at a time per tree, so a double click can't race
  busy.add(key);
  const was = { on: starred.has(key), stars: e.stars };
  if (was.on) starred.delete(key);
  else starred.add(key);
  e.stars += was.on ? -1 : 1;
  syncStars(login);
  let failed = "";
  try {
    const r = await fetch(`${API}/v1/gallery/${encodeURIComponent(e.login)}/star`, { method: "POST" });
    const body: unknown = await r.json().catch(() => null);
    const b = body && typeof body === "object" ? (body as { stars?: unknown; starred?: unknown }) : {};
    if (r.ok && typeof b.stars === "number" && typeof b.starred === "boolean") {
      e.stars = b.stars;
      if (b.starred) starred.add(key);
      else starred.delete(key);
    } else {
      failed = r.status === 429 ? "That's a lot of stars from here for now. Try again later." : "That star didn't stick. Try again in a while.";
    }
  } catch {
    failed = "That star didn't stick. Try again in a while.";
  }
  if (failed) {
    e.stars = was.stars;
    if (was.on) starred.add(key);
    else starred.delete(key);
    status.textContent = failed;
  }
  busy.delete(key);
  syncStars(login);
}

/* The grid */
function card(e: Entry, i: number): HTMLElement {
  const a = document.createElement("article");
  a.className = "card plate";
  a.dataset.login = e.login;
  const who = document.createElement("span");
  who.className = "who";
  who.textContent = `@${e.login}`;
  const label = document.createElement("div");
  label.className = "card-label";
  if (!isGrown(e)) {
    // Shown before its first growth: no picture, style or star yet.
    const wait = document.createElement("p");
    wait.className = "waiting";
    wait.textContent = "Not grown yet";
    // Without a large view, Remove sits here, so its owner can still take it out.
    const remove = link(authUrl(e.login, "remove", false), "This is my tree: remove it");
    remove.className = "remove";
    wait.append(remove);
    label.append(who);
    a.append(wait, label);
    return a;
  }
  const openBtn = document.createElement("button");
  openBtn.type = "button";
  openBtn.className = "open";
  openBtn.setAttribute("aria-label", `Open @${e.login}'s tree, ${e.style}`);
  const img = document.createElement("img");
  img.loading = "lazy";
  img.decoding = "async";
  img.alt = "";
  img.dataset.i = String(i);
  img.src = svgUrl(e, theme(), false);
  openBtn.append(img);
  openBtn.addEventListener("click", () => openView(i));
  const sty = document.createElement("span");
  sty.className = "sty";
  sty.textContent = e.style;
  label.append(who, sty, starButton(e));
  a.append(openBtn, label);
  return a;
}
function query(): Query {
  const s = sort.value;
  return { search: search.value, style: style.value, language: lang.value, age: age.value, sort: s === "stars" || s === "az" ? s : "new" };
}
function render(): void {
  shown = filtered(all, query());
  grid.replaceChildren(...shown.map(card));
  if (!all.length) {
    status.replaceChildren("No trees on show yet. Grow yours and show it: ", link("index.html#yours", "See your own tree →"));
  } else if (!shown.length) {
    const clear = document.createElement("button");
    clear.type = "button";
    clear.className = "linklike";
    clear.textContent = "Clear the filters";
    clear.addEventListener("click", () => {
      search.value = style.value = lang.value = age.value = "";
      render();
      search.focus();
    });
    status.replaceChildren("No trees match. ", clear);
  } else {
    status.textContent = `${shown.length} of ${all.length} ${all.length === 1 ? "tree" : "trees"}`;
  }
}
for (const el of [search, style, lang, age, sort]) el.addEventListener("input", render);

/** The pictures follow the theme. */
function paint(): void {
  $("theme").setAttribute("aria-label", theme() === "dark" ? "Switch to light mode" : "Switch to dark mode");
  grid.querySelectorAll<HTMLImageElement>("img[data-i]").forEach((img) => {
    const e = shown[Number(img.dataset.i)];
    if (e && isGrown(e)) img.src = svgUrl(e, theme(), false);
  });
  if (open !== null) showView();
}
paint();

/* The large view: one tree, its legend, its star, and Remove for its owner */
const grownCount = (): number => shown.filter(isGrown).length;
function openView(i: number): void {
  const t = shown[i];
  if (!t || !isGrown(t)) return; // a tree not grown yet has nothing to show large
  open = i;
  showView();
  if (!dlg.open) dlg.showModal();
  const e = shown[i];
  if (e) history.replaceState(null, "", `${location.pathname}${location.search}#${encodeURIComponent(e.login)}`);
}
function showView(): void {
  const e = open === null ? undefined : shown[open];
  if (!e || !isGrown(e)) return;
  const head = document.createElement("div");
  head.className = "gview-head";
  const title = document.createElement("h2");
  title.id = "view-title";
  title.append(link(`https://github.com/${encodeURIComponent(e.login)}`, `@${e.login}`));
  const sty = document.createElement("span");
  sty.className = "sty";
  sty.textContent = e.style;
  const close = document.createElement("button");
  close.type = "button";
  close.className = "icon-btn";
  close.setAttribute("aria-label", "Close");
  close.textContent = "✕";
  close.addEventListener("click", () => dlg.close());
  head.append(title, sty, close);
  const img = document.createElement("img");
  img.src = svgUrl(e, theme(), legendOn);
  img.alt = `The bonsai grown from the public activity of @${e.login}, ${e.style}`;
  const art = document.createElement("div");
  art.className = "gview-art";
  art.append(navButton("Previous tree", "←", -1), img, navButton("Next tree", "→", 1));
  const foot = document.createElement("div");
  foot.className = "gview-foot";
  const check = document.createElement("label");
  check.className = "check";
  const box = document.createElement("input");
  box.type = "checkbox";
  box.className = "tick";
  box.checked = legendOn;
  box.addEventListener("change", () => {
    legendOn = box.checked;
    img.src = svgUrl(e, theme(), legendOn);
  });
  check.append(box, " Legend");
  const remove = link(authUrl(e.login, "remove", false), "This is my tree: remove it");
  remove.className = "remove";
  foot.append(check, starButton(e), remove);
  dlg.replaceChildren(head, art, foot);
}
function navButton(label: string, glyph: string, step: number): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "icon-btn gnav";
  b.setAttribute("aria-label", label);
  b.textContent = glyph;
  b.hidden = grownCount() < 2;
  b.addEventListener("click", () => move(step));
  return b;
}
function move(step: number): void {
  if (open === null || grownCount() < 2) return;
  // The next grown tree: the ones still waiting have no large view.
  let j = open;
  do j = (j + step + shown.length) % shown.length;
  while (!isGrown(shown[j]!));
  openView(j);
  dlg.querySelector<HTMLButtonElement>(step < 0 ? ".gnav" : ".gnav ~ .gnav")?.focus();
}
dlg.addEventListener("keydown", (ev) => {
  if (ev.key === "ArrowLeft") move(-1);
  else if (ev.key === "ArrowRight") move(1);
});
dlg.addEventListener("close", () => {
  const was = open === null ? undefined : shown[open];
  open = null;
  history.replaceState(null, "", location.pathname + location.search);
  if (was) grid.querySelector<HTMLElement>(`article[data-login="${CSS.escape(was.login)}"] .open`)?.focus();
});
// A click on the backdrop closes it, as Escape does.
dlg.addEventListener("click", (ev) => {
  if (ev.target === dlg) dlg.close();
});

/* Load: the list, then this visitor's stars; a removal result, and a #login deep link */
const removed = parseResult(location.search, "remove");
if (removed) history.replaceState(null, "", location.pathname + location.hash);
async function load(): Promise<void> {
  try {
    // Fresh after a sign-in round trip, so a tree just added or removed shows as it is now.
    const r = await fetch(`${API}/v1/gallery`, { cache: removed || location.hash ? "no-cache" : "default" });
    if (!r.ok) throw new Error(String(r.status));
    all = parseList(await r.json());
  } catch {
    status.textContent = "The exhibition can't be reached right now. Try again in a while.";
    return;
  }
  for (const l of languagesByCount(all)) lang.append(new Option(l, l));
  render();
  if (removed) status.textContent = resultText("remove", removed.flag, removed.login);
  const deep = decodeURIComponent(location.hash.slice(1));
  if (deep) {
    const i = findIndex(shown, deep);
    if (i >= 0) openView(i);
  }
  try {
    const r = await fetch(`${API}/v1/gallery/starred`, { cache: "no-store" });
    if (r.ok) {
      const fresh = parseStarred(await r.json());
      // A star clicked while this loaded keeps its own answer.
      for (const k of busy) {
        if (starred.has(k)) fresh.add(k);
        else fresh.delete(k);
      }
      starred = fresh;
    }
  } catch {}
  for (const e of all) syncStars(e.login);
}
void load();
