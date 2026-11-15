// Tests line() of the footers' versions and the host messages banner: the
// pure steps from live/health.json to what the two front pages show, and the
// banner drawn into a stub document.
//
//   node --test test/versions_test.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import "../host/fun/versions.js";

const { line, messages, draw } = globalThis.TEHAS_VERSIONS;
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

const message = (id, level, over = {}) => ({ id, level, text: `${id} text`, link: null, at: "2026-10-17T13:00:00Z", ...over });

test("a health with only host messages draws one line for each and shows the banner", () => {
  const h = health({ hostMessages: { list: [
    message("move", "alert", { link: "https://example.com/move" }),
    message("disk", "warn", { link: "javascript:alert(1)", at: "soon" }),
    message("note", "info", { text: "<b>Read</b> the notes." }),
    message("odd", "loud"),
  ] } });
  assert.deepEqual(messages(h, NOW), [
    { word: "Alert", text: "move text", ago: "60 min ago", link: "https://example.com/move" },
    { word: "Warning", text: "disk text", ago: "", link: null },
    { word: "Notice", text: "<b>Read</b> the notes.", ago: "60 min ago", link: null },
  ]);
  const doc = stubDocument();
  const el = doc.banner;
  el.hidden = true;
  draw(el, messages(h, NOW), doc);
  assert.equal(el.hidden, false);
  assert.equal(el.kids.length, 3);
  assert.equal(el.kids[0].className, "host-message");
  assert.deepEqual(el.kids[0].kids.map((k) => [k.name, k.textContent]), [
    ["b", "Alert"], ["span", "move text"], ["small", "set 60 min ago"], ["a", "Open"],
  ]);
  assert.equal(el.kids[0].kids[3].href, "https://example.com/move");
  assert.equal(el.kids[0].kids[3].attrs.rel, "noopener noreferrer");
  assert.deepEqual(el.kids[1].kids.map((k) => [k.name, k.textContent]), [["b", "Warning"], ["span", "disk text"]]);
  assert.deepEqual(el.kids[2].kids[1].textContent, "<b>Read</b> the notes.");
});

test("an empty list, no list or an unreadable file is no message, and the banner hides", () => {
  for (const h of [
    health({ hostMessages: { list: [] } }),
    health({ hostMessages: { list: null, unreadable: true } }),
    health({}),
    null,
  ]) assert.deepEqual(messages(h, NOW), [], JSON.stringify(h));
  const doc = stubDocument();
  const el = doc.banner;
  const h = health({ hostMessages: { list: [] } });
  draw(el, messages(h, NOW), doc);
  assert.equal(el.hidden, true);
  assert.deepEqual(el.kids, []);
});

test("both front pages have the banner at the top, hidden until there is a message, and load this module", () => {
  for (const page of ["index.html", "business.html"]) {
    const html = readFileSync(new URL(`../host/fun/${page}`, import.meta.url), "utf8");
    const banner = html.match(/<section class="host-messages[^"]*" id="host-messages"[^>]*>/);
    assert.ok(banner, page);
    assert.match(banner[0], / hidden>$/, page);
    assert.ok(html.indexOf(banner[0]) > html.indexOf("</header>"), page);
    assert.ok(html.indexOf(banner[0]) < html.indexOf("<main"), page);
    assert.ok(html.includes('<script src="/fun/versions.js"></script>'), page);
    assert.ok(!html.includes("industry-alerts"), page);
  }
});
