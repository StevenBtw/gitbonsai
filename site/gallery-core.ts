// The gallery's pure logic: what the API sends, what the page shows, the
// sign-in links and their results. Shared by gallery.ts and app.ts, and tested
// by test/gallery-core.test.mjs.

/** The API's base URL; build.sh puts the real one (or a local stub's) in. */
export const API = "@API@";

/** One tree in the gallery, as GET /v1/gallery lists it. */
export interface Entry {
  login: string;
  github_id: number;
  style: string;
  languages: string[];
  added: string;
  grown: string;
  stars: number;
  /** The tree's age in weeks; -1 when the API didn't say. */
  age_weeks: number;
}
export type Sort = "new" | "stars" | "az";
/** The page's filters; "" means any. */
export interface Query {
  search: string;
  style: string;
  language: string;
  /** An age bracket from ageBracket(), "" for any age. */
  age: string;
  sort: Sort;
}
export type AddFlag = "added" | "growing" | "mismatch" | "unavailable" | "cancelled";
export type RemoveFlag = "removed" | "mismatch" | "unavailable" | "cancelled";
type Action = "add" | "remove";
type Theme = "light" | "dark";

const LOGIN = /^[A-Za-z0-9](-?[A-Za-z0-9])*$/;
const ADD: readonly AddFlag[] = ["added", "growing", "mismatch", "unavailable", "cancelled"];
const REMOVE: readonly RemoveFlag[] = ["removed", "mismatch", "unavailable", "cancelled"];

/** A GitHub username from what a visitor typed: trimmed, one leading "@" dropped; null if it can't be one. */
export function cleanLogin(raw: string): string | null {
  const s = raw.trim().replace(/^@/, "").trim();
  return s.length <= 39 && LOGIN.test(s) ? s : null;
}

const isStrings = (v: unknown): v is string[] => Array.isArray(v) && v.every((x) => typeof x === "string");
function isEntry(v: unknown): v is Entry {
  if (!v || typeof v !== "object") return false;
  const e = v as Record<string, unknown>;
  return (
    typeof e.login === "string" && cleanLogin(e.login) === e.login &&
    typeof e.github_id === "number" &&
    typeof e.style === "string" &&
    isStrings(e.languages) &&
    typeof e.added === "string" &&
    typeof e.grown === "string" &&
    typeof e.stars === "number" && Number.isFinite(e.stars)
  );
}

/** The entries of a GET /v1/gallery body; anything malformed is left out. An
 *  entry without an age still shows, under "Any age" only. */
export function parseList(body: unknown): Entry[] {
  const list = body && typeof body === "object" ? (body as { entries?: unknown }).entries : undefined;
  if (!Array.isArray(list)) return [];
  return list.filter(isEntry).map((e) => {
    const a = (e as { age_weeks?: unknown }).age_weeks;
    return { ...e, age_weeks: typeof a === "number" && Number.isFinite(a) && a >= 0 ? Math.floor(a) : -1 };
  });
}

/** The age brackets, youngest first: [key, label]. The key is the first year of the bracket. */
export const AGES: readonly (readonly [string, string])[] = [
  ["0", "Under a year"],
  ["1", "1 to 3 years"],
  ["4", "4 to 7 years"],
  ["8", "8 to 11 years"],
  ["12", "12 to 17 years"],
  ["18", "18 years or more"],
];
/** The bracket a tree of `weeks` falls in; "" when its age isn't known. */
export function ageBracket(weeks: number): string {
  if (weeks < 0) return "";
  const years = Math.floor(weeks / 52);
  let key = "0";
  for (const [k] of AGES) if (years >= Number(k)) key = k;
  return key;
}

/** The logins this visitor starred (lowercased), from a GET /v1/gallery/starred body. */
export function parseStarred(body: unknown): Set<string> {
  const list = body && typeof body === "object" ? (body as { starred?: unknown }).starred : undefined;
  return new Set(Array.isArray(list) ? list.filter((x): x is string => typeof x === "string").map((x) => x.toLowerCase()) : []);
}

const byLogin = (a: Entry, b: Entry): number => a.login.toLowerCase().localeCompare(b.login.toLowerCase());
const newest = (a: Entry, b: Entry): number => b.added.localeCompare(a.added);

/** The entries the filters let through, in the chosen order; `all` is left as it was. */
export function view(all: Entry[], q: Query): Entry[] {
  const s = q.search.trim().replace(/^@/, "").toLowerCase();
  const kept = all.filter(
    (e) =>
      (!s || e.login.toLowerCase().startsWith(s)) &&
      (!q.style || e.style === q.style) &&
      (!q.language || e.languages.includes(q.language)) &&
      (!q.age || ageBracket(e.age_weeks) === q.age),
  );
  const order = q.sort === "az" ? byLogin : q.sort === "stars" ? (a: Entry, b: Entry) => b.stars - a.stars || newest(a, b) : newest;
  return kept.sort(order);
}

/** Every language in the gallery, the most common first, ties A to Z. */
export function languagesByCount(all: Entry[]): string[] {
  const n = new Map<string, number>();
  for (const e of all) for (const l of e.languages) n.set(l, (n.get(l) ?? 0) + 1);
  return [...n].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([l]) => l);
}

/** Where `login` is in `list`, ignoring case; -1 if it isn't. */
export function findIndex(list: Entry[], login: string): number {
  const l = login.toLowerCase();
  return list.findIndex((e) => e.login.toLowerCase() === l);
}

/** The result the sign-in sent the visitor back with, or null if the address holds none (or a forged one). */
export function parseResult(search: string, action: Action): { flag: AddFlag | RemoveFlag; login: string; legend: boolean } | null {
  const p = new URLSearchParams(search);
  const flag = p.get("gallery") ?? "";
  const allowed: readonly string[] = action === "add" ? ADD : REMOVE;
  if (!allowed.includes(flag)) return null;
  const raw = p.get("login") ?? "";
  // A refused or declined sign-in can come back without a login (an untrusted state, or no name asked for).
  const login = raw === "" && (flag === "unavailable" || flag === "cancelled") ? "" : cleanLogin(raw);
  if (login === null || (login !== raw)) return null;
  return { flag: flag as AddFlag | RemoveFlag, login, legend: p.get("legend") === "1" };
}

/** What the page says about a sign-in result. */
export function resultText(action: Action, flag: AddFlag | RemoveFlag, login: string): string {
  const at = `@${login}`;
  if (action === "add") {
    switch (flag) {
      case "added": return `${at} is in the exhibition. See it →`;
      case "growing": return `${at} is in the exhibition; its tree appears once it has grown.`;
      case "mismatch": return `You signed in with another GitHub account, so ${at} wasn't added.`;
      case "cancelled": return "Nothing was added; you can try again any time.";
      default: return "This tree can't be shown in the exhibition.";
    }
  }
  switch (flag) {
    case "removed": return `${at}'s tree is out of the exhibition.`;
    case "mismatch": return "You signed in with another GitHub account, so nothing was removed.";
    case "cancelled": return "Nothing was removed.";
    default: return "That tree couldn't be removed right now. Try again in a while.";
  }
}

/** Where the GitHub sign-in starts, for adding or removing `login`'s tree. An add
 *  without a login shows whoever signs in: the API takes the name from GitHub. */
export function authUrl(login: string, action: Action, legend: boolean): string {
  const who = login ? `login=${encodeURIComponent(login)}&` : "";
  return `${API}/v1/gallery/auth?${who}action=${action}&legend=${legend ? 1 : 0}`;
}

/** The URL of a gallery tree's picture; `?v=` changes whenever the tree is regrown. */
export function svgUrl(e: Entry, theme: Theme, legend: boolean): string {
  return `${API}/v1/gallery/${encodeURIComponent(e.login)}/${legend ? "legend-" : ""}${theme}.svg?v=${encodeURIComponent(e.grown)}`;
}
