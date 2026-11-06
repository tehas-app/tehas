// Tests line() of the footers' versions and the industry's alerts banner: the
// pure steps from live/health.json to what the two front pages show, and the
// banner drawn into a stub document.
//
//   node --test test/versions_test.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import "../host/fun/versions.js";

const { line, alerts, draw } = globalThis.TEHAS_VERSIONS;
const a = "a".repeat(40);
const health = (checks) => ({ status: "pass", checks });

test("a release host run by an industry names all three", () => {
  const h = health({
    platform: { release: "v0.24.6", newest: "v0.24.6", behind: 0 },
    rukas: { image: a, imageVersion: "v0.14.54", target: a, behind: false },
    industry: { version: "v0.9.0" },
  });
  assert.equal(line(h), "tehas v0.24.6 · pi-rukas v0.14.54 · industry v0.9.0");
});

test("a standalone factory leaves the industry out", () => {
  const h = health({ platform: { release: "v0.24.6" }, rukas: { imageVersion: "v0.14.54" } });
  assert.equal(line(h), "tehas v0.24.6 · pi-rukas v0.14.54");
});

test("a checkout host and an image of no known version are named by commit", () => {
  const h = health({ platform: { commit: "9b1b7dc11b73b93f7296b5f3131fcc6ef6b964e9", behind: 0 }, rukas: { image: a, imageVersion: null } });
  assert.equal(line(h), "tehas 9b1b7dc · pi-rukas aaaaaaa");
});

test("the pi-rukas named is the image's, not the target's or the clone's", () => {
  const h = health({ rukas: { imageVersion: "v0.14.49", targetVersion: "v0.14.53", clone: a, behind: true } });
  assert.equal(line(h), "pi-rukas v0.14.49");
});

test("values of another shape are left out, and no version at all is null", () => {
  const h = health({
    platform: { release: "<b>v1</b>" },
    rukas: { imageVersion: "v0.14.54 x", image: "javascript:1" },
    industry: { version: 9 },
  });
  assert.equal(line(h), null);
  for (const f of [null, undefined, 3, [], {}, { checks: null }]) assert.equal(line(f), null);
});

const NOW = Date.parse("2026-10-17T14:00:20Z");
const industry = (over) => health({ industry: { version: "v0.9.0", alertsTime: "2026-10-17T14:00:00Z", alertsStale: false, ...over } });
const alert = (kind, over = {}) => ({ kind, since: "2026-10-17T10:00:00Z", text: `${kind} text`, link: "https://industry.example/f/1", ...over });

test("each kind of alert is named by its word, its text, its age and its link", () => {
  const h = industry({ alerts: [alert("paused"), alert("frozen", { since: "2026-10-17T13:30:00Z" }), alert("held-by-one", { since: "2026-10-17T14:00:00Z" })] });
  assert.deepEqual(alerts(h, NOW), [
    { word: "Paused", text: "paused text", ago: "4 h ago", link: "https://industry.example/f/1" },
    { word: "Frozen", text: "frozen text", ago: "30 min ago", link: "https://industry.example/f/1" },
    { word: "Held by one ticket", text: "held-by-one text", ago: "just now", link: "https://industry.example/f/1" },
  ]);
});

test("a kind the page does not know is named by its own word, and a bad link or time is left out", () => {
  const h = industry({ alerts: [alert("flooded", { link: "javascript:alert(1)", since: "soon" }), alert("constructor"), { text: "no kind" }, null] });
  assert.deepEqual(alerts(h, NOW), [
    { word: "flooded", text: "flooded text", ago: "", link: null },
    { word: "constructor", text: "constructor text", ago: "4 h ago", link: "https://industry.example/f/1" },
  ]);
});

test("no industry, no file, an empty list, a stale file or an unreadable one is no alert", () => {
  for (const h of [
    health({}),
    health({ industry: { version: "v0.9.0", alerts: null } }),
    industry({ alerts: [] }),
    industry({ alerts: [alert("paused")], alertsStale: true }),
    health({ industry: { version: "v0.9.0", alerts: null, alertsUnreadable: true } }),
    industry({ alerts: { kind: "paused" } }),
    null,
  ]) assert.deepEqual(alerts(h, NOW), [], JSON.stringify(h));
});

// The least of a document: the elements the banner makes, as plain objects.
function stubDocument() {
  const make = (name) => ({
    name, className: "", textContent: "", href: "", hidden: false, attrs: {}, kids: [],
    setAttribute(k, v) { this.attrs[k] = v; },
    append(...n) { this.kids.push(...n); },
    replaceChildren(...n) { this.kids = n; },
  });
  return { createElement: make, banner: make("section") };
}

test("the banner shows an alert and is hidden for an empty list", () => {
  const doc = stubDocument();
  const el = doc.banner;
  el.hidden = true;
  draw(el, alerts(industry({ alerts: [alert("paused", { text: "<b>Paused</b> for 4 h." })] }), NOW), doc);
  assert.equal(el.hidden, false);
  const [p] = el.kids;
  assert.equal(p.className, "industry-alert");
  assert.deepEqual(p.kids.map((k) => [k.name, k.textContent]), [
    ["b", "Paused"], ["span", "<b>Paused</b> for 4 h."], ["small", "started 4 h ago"], ["a", "Open"],
  ]);
  assert.equal(p.kids[3].href, "https://industry.example/f/1");
  assert.equal(p.kids[3].attrs.rel, "noopener noreferrer");
  draw(el, alerts(industry({ alerts: [] }), NOW), doc);
  assert.equal(el.hidden, true);
  assert.deepEqual(el.kids, []);
});

test("both front pages have the banner at the top, hidden until there is an alert, and load this module", () => {
  for (const page of ["index.html", "business.html"]) {
    const html = readFileSync(new URL(`../host/fun/${page}`, import.meta.url), "utf8");
    const banner = html.match(/<section class="industry-alerts[^"]*" id="industry-alerts"[^>]*>/);
    assert.ok(banner, page);
    assert.match(banner[0], / hidden>$/, page);
    assert.ok(html.indexOf(banner[0]) > html.indexOf("</header>"), page);
    assert.ok(html.indexOf(banner[0]) < html.indexOf("<main"), page);
    assert.ok(html.includes('<script src="/fun/versions.js"></script>'), page);
  }
});
