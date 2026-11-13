// Tests the reading module of the host bridge (host/fun/host-bridge.js): what
// it makes of /host/bridge.json, the samples in api/, each rule's breach, text
// that is never markup, how long it keeps an answer, and that with no answer
// nothing on a page changes.
//
//   node --test test/host_bridge_test.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import "../host/fun/host-bridge.js";

const HB = globalThis.TEHAS_HOST_BRIDGE;
const { read, ticketHref, notesOf, chips, drawNotices, keeper } = HB;

const sample = (name) => readFileSync(new URL(`../api/host-bridge.${name}.json`, import.meta.url), "utf8");
const NOW = Date.parse("2026-11-08T12:00:20Z");
const full = () => JSON.parse(sample("full"));
const readOf = (v, now = NOW) => read(JSON.stringify(v), now);
const REPO = "acme/widget";
const ISSUE = (n) => `https://github.com/${REPO}/issues/${n}`;

test("the full sample gives every element", () => {
  const a = read(sample("full"), NOW);
  assert.equal(a.ticketLink, "/host/ticket/{n}");
  assert.deepEqual(a.notices, [
    { id: "n-1", level: "info", text: "Example notice.", link: "/host/notice/n-1" },
    { id: "n-2", level: "warn", text: "Example warning.", link: "https://host.example/notice/n-2" },
    { id: "n-3", level: "alert", text: "Example alert.", link: null },
  ]);
  assert.deepEqual(notesOf(a, 42), [{ text: "Example note", link: "/host/ticket/42" }, { text: "Second note", link: null }]);
  assert.deepEqual(notesOf(a, 7), [{ text: "Another note", link: "https://host.example/ticket/7" }]);
  assert.deepEqual(notesOf(a, 8), []);
  assert.deepEqual(a.menu, [{ id: "m-1", label: "Example page", url: "https://host.example/page" }]);
});

test("the bare sample is an answer with nothing to draw", () => {
  const a = read(sample("bare"), NOW);
  assert.deepEqual(a, { ticketLink: null, notices: [], notes: new Map(), menu: [] });
  assert.equal(ticketHref(a, REPO, ISSUE(42)), null);
});

test("not JSON, not an object, over 256 KiB, or a bad bridge or time is ignored whole", () => {
  for (const text of ["", "nope", "[]", "null", "1", '"x"', "{"]) assert.equal(read(text, NOW), null, text);
  for (const bridge of [undefined, 0, 2, "1", 1.5, null, true]) {
    const v = full();
    v.bridge = bridge;
    assert.equal(readOf(v), null, String(bridge));
  }
  for (const time of [undefined, 1, "noon", "2026-11-08", "2026-11-08 12:00:00Z", "2026-11-08T12:00:00", "2026-13-40T12:00:00Z"]) {
    const v = full();
    v.time = time;
    assert.equal(readOf(v), null, String(time));
  }
  const big = full();
  big.padding = "x".repeat(256 * 1024);
  assert.equal(readOf(big), null);
  assert.equal(read(123, NOW), null);
});

test("an answer more than 5 minutes old is ignored, and one as far ahead", () => {
  const t = Date.parse("2026-11-08T12:00:00Z");
  assert.ok(readOf(full(), t + 5 * 60 * 1000));
  assert.equal(readOf(full(), t + 5 * 60 * 1000 + 1), null);
  assert.ok(readOf(full(), t - 5 * 60 * 1000));
  assert.equal(readOf(full(), t - 5 * 60 * 1000 - 1), null);
  const offset = full();
  offset.time = "2026-11-08T14:00:00+02:00";
  assert.ok(readOf(offset));
});

test("unknown fields are ignored, at the top and in each element", () => {
  const v = full();
  v.later = { any: 1 };
  v.notices[0].later = [1];
  v.ticketNotes[0].later = "x";
  const a = readOf(v);
  assert.equal(a.notices.length, 3);
  assert.equal(notesOf(a, 42).length, 2);
  assert.equal(Object.hasOwn(a.notices[0], "later"), false);
});

test("the host is shown nowhere and not read", () => {
  const v = full();
  delete v.host;
  assert.ok(readOf(v));
});

test("a ticket link needs /host/, {n} once and at most 200 characters, else tickets keep GitHub", () => {
  for (const bad of ["https://host.example/{n}", "/hosts/{n}", "/host/ticket", "/host/{n}/{n}", "//host/{n}",
    "/host/" + "x".repeat(192) + "{n}", "/host/a b/{n}", "/host/\n{n}", 42, null]) {
    const v = full();
    v.ticketLink = bad;
    const a = readOf(v);
    assert.equal(a.ticketLink, null, String(bad));
    assert.equal(a.notices.length, 3);
  }
  const v = full();
  v.ticketLink = "/host/" + "x".repeat(191) + "{n}";
  assert.equal(readOf(v).ticketLink.length, 200);
});

test("a ticket of the repository links to the host; a pull request, another repository or a search keeps GitHub", () => {
  const a = read(sample("full"), NOW);
  assert.equal(ticketHref(a, REPO, ISSUE(42)), "/host/ticket/42");
  assert.equal(ticketHref(a, "Acme/Widget", ISSUE(42)), "/host/ticket/42");
  for (const url of [`https://github.com/${REPO}/pull/42`, "https://github.com/other/repo/issues/42",
    `https://github.com/${REPO}/issues?q=is%3Aopen`, `https://github.com/${REPO}/issues/42#x`,
    `http://github.com/${REPO}/issues/42`, `https://github.com/${REPO}/issues/0`, null, "", "https://lab-4.example/"]) {
    assert.equal(ticketHref(a, REPO, url), null, String(url));
  }
  for (const repo of [null, "", "nope", "a/b/c"]) assert.equal(ticketHref(a, repo, ISSUE(42)), null, String(repo));
});

test("each breach of a notice drops that notice alone", () => {
  const ok = { id: "ok", level: "info", text: "Fine." };
  const bad = [
    null, "x", [], { ...ok, id: "Big" }, { ...ok, id: "" }, { ...ok, id: "a".repeat(33) }, { ...ok, id: 1 },
    { ...ok, level: "loud" }, { ...ok, level: undefined }, { ...ok, text: "" }, { ...ok, text: "   " },
    { ...ok, text: "t".repeat(201) }, { ...ok, text: 5 }, { ...ok, link: "http://x.example/" },
    { ...ok, link: "javascript:alert(1)" }, { ...ok, link: "/other/x" }, { ...ok, link: "https://" },
    { ...ok, link: "https://u:p@x.example/" }, { ...ok, link: "https://x.example/" + "p".repeat(512) },
    { ...ok, link: "/host/a b" }, { ...ok, link: null },
  ];
  for (const n of bad) {
    const v = full();
    v.notices = [n, ok];
    assert.deepEqual(readOf(v).notices.map((x) => x.id), ["ok"], JSON.stringify(n));
  }
});

test("notices: at most 5, ids unique, control characters as spaces, 200 characters kept whole", () => {
  const v = full();
  v.notices = [
    { id: "a", level: "info", text: "one" }, { id: "a", level: "warn", text: "again" },
    { id: "b", level: "warn", text: "tab\there\nand a line" }, { id: "c", level: "alert", text: "é".repeat(200) },
    { id: "d", level: "info", text: "x" }, { id: "e", level: "info", text: "x" }, { id: "f", level: "info", text: "x" },
  ];
  const a = readOf(v);
  assert.deepEqual(a.notices.map((n) => n.id), ["a", "b", "c", "d", "e"]);
  assert.equal(a.notices[1].text, "tab here and a line");
  assert.equal(a.notices[2].text.length, 200);
});

test("each breach of a ticket note drops that note alone; at most 3 a ticket and 200 in all", () => {
  const ok = { issue: 5, text: "ok" };
  for (const n of [null, { ...ok, issue: 0 }, { ...ok, issue: 1000000000 }, { ...ok, issue: 1.5 }, { ...ok, issue: "5" },
    { ...ok, text: "" }, { ...ok, text: "t".repeat(61) }, { ...ok, link: "ftp://x" }, { ...ok, link: 1 }]) {
    const v = full();
    v.ticketNotes = [n, ok];
    assert.deepEqual(notesOf(readOf(v), 5), [{ text: "ok", link: null }], JSON.stringify(n));
  }
  const v = full();
  v.ticketNotes = [1, 2, 3, 4].map((i) => ({ issue: 9, text: `n${i}` }));
  assert.deepEqual(notesOf(readOf(v), 9).map((n) => n.text), ["n1", "n2", "n3"]);
  v.ticketNotes = Array.from({ length: 300 }, (_, i) => ({ issue: i + 1, text: "x" }));
  const a = readOf(v);
  assert.equal(a.notes.size, 200);
  assert.deepEqual(notesOf(a, 201), []);
  v.ticketNotes = { issue: 1, text: "not a list" };
  assert.equal(readOf(v).notes.size, 0);
});

// The least of a document: the elements the module makes, as plain objects.
test("each breach of a menu item drops that item alone; ids unique and four the most", () => {
  const m = (id, over = {}) => ({ id, label: `Page ${id}`, url: `https://${id}.host.example/p`, ...over });
  const v = full();
  v.menu = [
    m("a"), m("a"), m("Big"), m("b", { label: "" }), m("c", { label: "x".repeat(25) }),
    m("d", { url: "javascript:alert(1)" }), m("e", { url: "/host/page" }), m("f", { url: "https://u:p@f.host.example/" }),
    m("g", { url: `https://g.host.example/${"x".repeat(500)}` }), m("h", { url: "https://h.host.example/a b" }),
    m("i", { label: " Two\nlines " }), m("j", { url: "http://j.host.example/p" }), m("k"), m("l"),
  ];
  const a = readOf(v);
  assert.deepEqual(a.menu.map((i) => i.id), ["a", "i", "j", "k"]);
  assert.equal(a.menu[1].label, "Two lines");
  assert.equal(a.menu[2].url, "http://j.host.example/p");
  v.menu = "x";
  assert.deepEqual(readOf(v).menu, []);
});

test("an answer that still carries tokens is read whole, and the tokens are ignored", () => {
  const v = full();
  v.tokens = {
    days: [{ date: "2026-11-08", requests: 412, input: 9120000, output: 81000, cached: 8700000 }],
    models: [{ model: "example-model", requests: 412, input: 9120000, output: 81000, cached: 8700000 }],
    unknown: 3,
  };
  const a = readOf(v);
  assert.deepEqual(a, readOf(full()));
  assert.equal("tokens" in a, false);
  for (const tokens of [null, [], "x", 1, { days: "x" }]) {
    v.tokens = tokens;
    assert.deepEqual(readOf(v), readOf(full()), String(tokens));
  }
  assert.equal("drawTokens" in HB, false);
});

function stubDocument() {
  const make = (name) => ({
    name, className: "", textContent: "", href: "", hidden: false, attrs: {}, kids: [],
    setAttribute(k, v) { this.attrs[k] = v; },
    append(...n) { this.kids.push(...n); },
    replaceChildren(...n) { this.kids = n; },
  });
  return { createElement: make, el: make("section") };
}

test("text is set as text, never markup, and a host link opens in the same tab", () => {
  const v = full();
  v.notices = [{ id: "x", level: "alert", text: "<img src=x onerror=alert(1)>", link: "/host/n" },
    { id: "y", level: "info", text: "<b>b</b>", link: "https://host.example/y" }];
  v.ticketNotes = [{ issue: 3, text: "<script>1</script>", link: "/host/t/3" }, { issue: 3, text: "plain" }];
  const a = readOf(v);
  const doc = stubDocument();
  drawNotices(doc.el, a.notices, doc);
  assert.equal(doc.el.hidden, false);
  const [p, q] = doc.el.kids;
  assert.equal(p.className, "industry-alert host-notice");
  assert.equal(p.attrs["data-level"], "alert");
  assert.deepEqual(p.kids.map((k) => [k.name, k.textContent]), [["b", "Alert"], ["span", "<img src=x onerror=alert(1)>"], ["a", "Open"]]);
  assert.equal(p.kids[2].href, "/host/n");
  assert.equal(p.kids[2].attrs.target, undefined);
  assert.equal(q.kids[2].attrs.target, "_blank");
  assert.equal(q.kids[2].attrs.rel, "noopener noreferrer");
  const box = chips(doc, notesOf(a, 3));
  assert.equal(box.className, "host-notes");
  assert.deepEqual(box.kids.map((k) => [k.name, k.className, k.textContent, k.href]),
    [["a", "host-note", "<script>1</script>", "/host/t/3"], ["span", "host-note", "plain", ""]]);
  assert.equal(box.kids[0].attrs.target, undefined);
  for (const f of ["innerHTML", "outerHTML", "insertAdjacentHTML", "document.write"]) {
    assert.ok(!readFileSync(new URL("../host/fun/host-bridge.js", import.meta.url), "utf8").includes(f), f);
  }
});

test("with no answer nothing changes: no link, no chip, and the notices stay hidden and empty", () => {
  assert.equal(HB.current(), null);
  assert.equal(HB.ticket(REPO, ISSUE(42)), null);
  assert.equal(HB.notes(stubDocument(), 42), null);
  assert.equal(ticketHref(null, REPO, ISSUE(42)), null);
  assert.equal(chips(stubDocument(), notesOf(null, 42)), null);
  const doc = stubDocument();
  doc.el.hidden = true;
  drawNotices(doc.el, [], doc);
  assert.equal(doc.el.hidden, true);
  assert.deepEqual(doc.el.kids, []);
});

// A fetch that answers from a list of responses, and a clock to move.
function harness(responses) {
  let t = NOW;
  const seen = [];
  const asked = [];
  const k = keeper({
    fetch: async (path, opts) => {
      asked.push([path, opts.cache, Boolean(opts.signal)]);
      const r = responses.shift();
      if (r instanceof Error) throw r;
      return {
        status: r.status, ok: r.status >= 200 && r.status < 300,
        headers: { get: (h) => (r.headers || {})[h] ?? null },
        text: async () => r.body,
      };
    },
    now: () => t,
    changed: (a) => seen.push(a),
  });
  return { k, seen, asked, move: (ms) => { t += ms; } };
}
const ok = () => ({ status: 200, body: sample("full") });

test("the keeper asks for /host/bridge.json without the cache, and a 404 is no host", async () => {
  const h = harness([{ status: 404, body: "" }]);
  await h.k.ask();
  assert.deepEqual(h.asked, [["/host/bridge.json", "no-store", true]]);
  assert.equal(h.k.current(), null);
  assert.deepEqual(h.seen, []);
});

test("the keeper keeps the last good answer for 5 minutes through failed asks, then drops it", async () => {
  const h = harness([ok(), new Error("down"), { status: 502, body: "" }, { status: 200, body: "nope" }, new Error("down")]);
  await h.k.ask();
  const first = h.k.current();
  assert.ok(first);
  h.move(60 * 1000);
  await h.k.ask();
  h.move(60 * 1000);
  await h.k.ask();
  h.move(60 * 1000);
  await h.k.ask();
  assert.equal(h.k.current(), first);
  h.move(2 * 60 * 1000 + 1);
  await h.k.ask();
  assert.equal(h.k.current(), null);
  assert.deepEqual(h.seen, [first, null]);
});

test("a 404 drops a kept answer at once, and an answer too big by its length is not read", async () => {
  const h = harness([ok(), { status: 404, body: "" }, { status: 200, body: sample("full"), headers: { "Content-Length": String(300 * 1024) } }]);
  await h.k.ask();
  assert.ok(h.k.current());
  await h.k.ask();
  assert.equal(h.k.current(), null);
  await h.k.ask();
  assert.equal(h.k.current(), null);
});

test("the answer's age is judged by the server's clock when it gives one", async () => {
  const h = harness([{ status: 200, body: sample("full"), headers: { Date: "Sun, 08 Nov 2026 12:01:00 GMT" } }]);
  h.move(24 * 3600 * 1000);
  await h.k.ask();
  assert.ok(h.k.current());
});

test("the pages read the answer only through the module, and only when it is there", () => {
  for (const f of ["business-view.js", "game-view.js", "epics-view.js", "dashboard-view.js"]) {
    const src = readFileSync(new URL(`../host/fun/${f}`, import.meta.url), "utf8");
    assert.ok(!src.includes("/host/bridge.json"), f);
    assert.ok(src.includes("globalThis.TEHAS_HOST_BRIDGE || null"), f);
  }
});
