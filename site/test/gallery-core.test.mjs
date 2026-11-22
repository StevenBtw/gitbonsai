// Tests for gallery-core.ts, run on its compiled output: npm test --prefix site
import test from "node:test";
import assert from "node:assert/strict";
import * as g from "../.test-out/gallery-core.js";

const e = (login, over = {}) => ({
  login,
  github_id: login.length,
  style: "moyogi",
  languages: ["Rust"],
  added: "2026-11-01T00:00:00Z",
  grown: "2026-11-01T00:00:00Z",
  stars: 0,
  age_weeks: 300,
  ...over,
});

test("cleanLogin trims, drops one @ and validates", () => {
  assert.equal(g.cleanLogin("  @StevenBtw "), "StevenBtw");
  for (const bad of ["", "@", "a--b", "-a", "a-", "x".repeat(40), "<img>", "a b"]) {
    assert.equal(g.cleanLogin(bad), null, bad);
  }
  assert.equal(g.cleanLogin("x".repeat(39)), "x".repeat(39));
});

test("parseList reads entries and drops malformed ones", () => {
  for (const bad of [null, 42, {}, { entries: 3 }, "x", []]) assert.deepEqual(g.parseList(bad), []);
  const good = e("tj");
  assert.equal(g.parseList({ entries: [good, { ...good, login: undefined }] }).length, 1);
  assert.equal(g.parseList({ entries: [{ ...good, stars: "3" }] }).length, 0);
  assert.equal(g.parseList({ entries: [{ ...good, languages: [1] }] }).length, 0);
  assert.equal(g.parseList({ entries: [{ ...good, languages: [] }] }).length, 1);
});

test("parseStarred lowercases and tolerates junk", () => {
  assert.deepEqual([...g.parseStarred({ starred: ["StevenBtw"] })], ["stevenbtw"]);
  assert.equal(g.parseStarred({}).size, 0);
  assert.equal(g.parseStarred(null).size, 0);
  assert.equal(g.parseStarred({ starred: [1, "tj"] }).size, 1);
});

test("view filters and sorts without mutating", () => {
  const all = [
    e("stevenbtw", { style: "fukinagashi", languages: ["Rust", "Python"], added: "2026-11-03T00:00:00Z", stars: 2 }),
    e("tj", { style: "moyogi", languages: ["Go"], added: "2026-11-02T00:00:00Z", stars: 5 }),
    e("Anna", { style: "moyogi", languages: ["Rust"], added: "2026-11-01T00:00:00Z", stars: 5 }),
  ];
  const copy = structuredClone(all);
  const base = { search: "", style: "", language: "", age: "", sort: "new" };
  assert.deepEqual(g.view(all, { ...base, search: "STEV" }).map((x) => x.login), ["stevenbtw"]);
  assert.deepEqual(g.view(all, { ...base, style: "moyogi" }).map((x) => x.login), ["tj", "Anna"]);
  assert.deepEqual(g.view(all, { ...base, language: "Rust" }).map((x) => x.login), ["stevenbtw", "Anna"]);
  assert.deepEqual(g.view(all, base).map((x) => x.login), ["stevenbtw", "tj", "Anna"]);
  assert.deepEqual(g.view(all, { ...base, sort: "stars" }).map((x) => x.login), ["tj", "Anna", "stevenbtw"]);
  assert.deepEqual(g.view(all, { ...base, sort: "az" }).map((x) => x.login), ["Anna", "stevenbtw", "tj"]);
  assert.deepEqual(all, copy);
});

test("languagesByCount: most common first, ties A to Z", () => {
  assert.deepEqual(g.languagesByCount([e("a", { languages: ["Rust", "Go"] }), e("b", { languages: ["Go"] })]), ["Go", "Rust"]);
});

test("findIndex is case-insensitive", () => {
  const list = [e("tj"), e("stevenbtw")];
  assert.equal(g.findIndex(list, "STEVENBTW"), 1);
  assert.equal(g.findIndex(list, "nobody"), -1);
});

test("parseResult whitelists flags per action and validates the login", () => {
  assert.deepEqual(g.parseResult("?gallery=added&login=tj", "add"), { flag: "added", login: "tj", legend: false });
  assert.deepEqual(g.parseResult("?gallery=added&login=tj&legend=1", "add"), { flag: "added", login: "tj", legend: true });
  assert.equal(g.parseResult("?gallery=removed&login=tj", "add"), null);
  assert.deepEqual(g.parseResult("?gallery=cancelled&login=tj", "add")?.flag, "cancelled");
  assert.deepEqual(g.parseResult("?gallery=cancelled&login=tj", "remove")?.flag, "cancelled");
  assert.equal(g.parseResult("?gallery=pwned&login=tj", "add"), null);
  assert.equal(g.parseResult("?gallery=added&login=<img>", "add"), null);
  assert.equal(g.parseResult("", "add"), null);
  assert.deepEqual(g.parseResult("?gallery=unavailable&login=", "add"), { flag: "unavailable", login: "", legend: false });
  assert.equal(g.parseResult("?gallery=added&login=", "add"), null);
  // A sign-in declined before anyone was named comes back without a login too.
  assert.deepEqual(g.parseResult("?gallery=cancelled&login=", "add"), { flag: "cancelled", login: "", legend: false });
});

test("resultText uses the fixed copy", () => {
  assert.equal(g.resultText("add", "mismatch", "tj"), "You signed in with another GitHub account, so @tj wasn't added.");
  assert.equal(g.resultText("add", "unavailable", ""), "This tree can't be shown in the exhibition.");
  assert.equal(g.resultText("remove", "removed", "tj"), "@tj's tree is out of the exhibition.");
  for (const s of [g.resultText("add", "added", "tj"), g.resultText("add", "growing", "tj")]) assert.ok(!s.includes("—"));
});

test("authUrl and svgUrl", () => {
  assert.ok(g.authUrl("tj", "add", true).endsWith("/v1/gallery/auth?login=tj&action=add&legend=1"));
  // No username typed: the sign-in itself says whose tree it is.
  assert.ok(g.authUrl("", "add", false).endsWith("/v1/gallery/auth?action=add&legend=0"));
  assert.ok(g.svgUrl(e("tj", { grown: "2026-11-21T09:12:00Z" }), "dark", true).endsWith("/v1/gallery/tj/legend-dark.svg?v=2026-11-21T09%3A12%3A00Z"));
  assert.ok(g.svgUrl(e("tj"), "light", false).endsWith("/v1/gallery/tj/light.svg?v=2026-11-01T00%3A00%3A00Z"));
});

test("ageBracket buckets whole years, with 18+ at the top", () => {
  const y = (n) => n * 52;
  assert.equal(g.ageBracket(0), "0");
  assert.equal(g.ageBracket(51), "0");
  assert.equal(g.ageBracket(y(1)), "1");
  assert.equal(g.ageBracket(y(3) + 51), "1");
  assert.equal(g.ageBracket(y(4)), "4");
  assert.equal(g.ageBracket(y(8)), "8");
  assert.equal(g.ageBracket(y(11) + 51), "8");
  assert.equal(g.ageBracket(y(12)), "12");
  assert.equal(g.ageBracket(y(17) + 51), "12");
  assert.equal(g.ageBracket(y(18)), "18");
  assert.equal(g.ageBracket(y(40)), "18");
  assert.equal(g.ageBracket(-1), "");
});

test("the age filter, and entries without an age", () => {
  const all = [e("old", { age_weeks: 20 * 52 }), e("young", { age_weeks: 10 }), e("mid", { age_weeks: 5 * 52 })];
  const base = { search: "", style: "", language: "", age: "", sort: "az" };
  assert.deepEqual(g.view(all, { ...base, age: "18" }).map((x) => x.login), ["old"]);
  assert.deepEqual(g.view(all, { ...base, age: "0" }).map((x) => x.login), ["young"]);
  assert.deepEqual(g.view(all, { ...base, age: "4" }).map((x) => x.login), ["mid"]);
  const { age_weeks, ...noAge } = e("x");
  const kept = g.parseList({ entries: [noAge] });
  assert.equal(g.parseList({ entries: [{ ...noAge, age_weeks: null }] })[0].age_weeks, -1);
  assert.equal(kept.length, 1);
  assert.equal(kept[0].age_weeks, -1);
  assert.deepEqual(g.view(kept, { ...base, age: "0" }), []);
  assert.equal(g.view(kept, base).length, 1);
});

// A tree shown before it has grown, exactly as GET /v1/gallery sends it.
const waitingEntry = (login, added = "2026-11-10T00:00:00Z") => ({
  login, github_id: 7, added, example: false, stars: 0, grown: null, style: null, languages: [], age_weeks: null,
});

test("a waiting entry parses, and a bad one is still dropped", () => {
  const [w] = g.parseList({ entries: [waitingEntry("sprout")] });
  assert.equal(w.login, "sprout");
  assert.equal(w.grown, null);
  assert.equal(w.style, null);
  assert.equal(w.age_weeks, -1);
  assert.equal(g.isGrown(w), false);
  assert.equal(g.isGrown(e("tj")), true);
  // The guard holds what its type promises, even for an entry that never went through parseList.
  assert.equal(g.isGrown(e("tj", { style: null })), false);
  for (const bad of [{ grown: 5 }, { style: 3 }, { grown: "", style: null }, { grown: null, style: "moyogi" }, { languages: null }]) {
    assert.deepEqual(g.parseList({ entries: [{ ...waitingEntry("sprout"), ...bad }] }), [], JSON.stringify(bad));
  }
});

test("waiting entries leave a style or language filter, and sort by added", () => {
  const all = [e("tj", { added: "2026-11-01T00:00:00Z" }), ...g.parseList({ entries: [waitingEntry("sprout", "2026-11-05T00:00:00Z")] })];
  const base = { search: "", style: "", language: "", age: "", sort: "new" };
  assert.deepEqual(g.view(all, { ...base, style: "moyogi" }).map((x) => x.login), ["tj"]);
  assert.deepEqual(g.view(all, { ...base, language: "Rust" }).map((x) => x.login), ["tj"]);
  assert.deepEqual(g.view(all, base).map((x) => x.login), ["sprout", "tj"]);
  assert.deepEqual(g.view(all, { ...base, search: "spr" }).map((x) => x.login), ["sprout"]);
});
