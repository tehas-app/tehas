// Tests the models of the wiki page and the dashboard's wiki link
// (host/fun/factory/tehas-factory-wiki.model.js), and the page's fragment.
//
//   node --test test/wiki_view_test.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { wikiLink, wikiModel } from "../host/fun/factory/tehas-factory-wiki.model.js";

const NOW = Date.parse("2026-11-23T12:00:00Z");
const FILE = {
  api: 1,
  state: "idle",
  current: {
    id: "2026-11-21-68ffab1d",
    commit: "68ffab1d0123456789abcdef0123456789abcdef",
    describe: "v1.2.0-5-g68ffab1d",
    builtAt: "2026-11-23T09:00:00Z",
    url: "/wiki/2026-11-21-68ffab1d/",
  },
  main: { commit: "7c1e09a20123456789abcdef0123456789abcdef", commitDate: "2026-11-23T10:12:00+02:00" },
  behind: { commits: 5, days: 2, line: "5 commits and 2 days behind main" },
};

test("a build is said by its time, its commit, its release and how far behind main", () => {
  assert.deepEqual(wikiModel(FILE, NOW), {
    present: true,
    href: "/wiki/2026-11-21-68ffab1d/",
    id: "2026-11-21-68ffab1d",
    built: "Built 3 h ago at commit 68ffab1d (v1.2.0-5-g68ffab1d).",
    behind: "5 commits and 2 days behind main.",
    main: "Main is 7c1e09a2, 2026-11-23.",
  });
  assert.deepEqual(wikiLink(FILE), { href: "/wiki/2026-11-21-68ffab1d/", title: "2026-11-21-68ffab1d" });
});

test("no file, another api or no build gives no link", () => {
  for (const f of [null, "x", [], { api: 2 }]) {
    assert.equal(wikiModel(f, NOW).present, false);
    assert.equal(wikiLink(f), null);
  }
  const none = wikiModel({ api: 1, current: null, behind: null, main: null }, NOW);
  assert.deepEqual([none.present, none.href, none.built], [true, null, "No build yet."]);
});

test("an address of another form is no link", () => {
  for (const url of ["https://elsewhere.example/", "/wiki/current/", "/wiki/2026-11-21-68ffab1d/../x", "javascript:alert(1)", "/wiki/2026-11-21-68FFAB1D/"]) {
    const f = { ...FILE, current: { ...FILE.current, url } };
    assert.equal(wikiModel(f, NOW).href, null, url);
    assert.equal(wikiLink(f), null, url);
  }
});

test("the page loads the bridge and its one island, and the dashboard its link", () => {
  const frag = readFileSync(new URL("../host/docs/wiki.frag", import.meta.url), "utf8");
  assert.deepEqual(frag.split("\n").filter((l) => l.includes("<script")), [
    '<script src="/fun/host-bridge.js"></script>',
    '<script type="module" src="/fun/factory/tehas-factory-wiki.js"></script>',
  ]);
  assert.match(frag, /<tehas-factory-wiki data-tehas-island="contents" src="\/live\/wiki.json">/);
  const dash = readFileSync(new URL("../host/docs/dashboard.frag", import.meta.url), "utf8");
  assert.ok(dash.includes('<a id="wiki" class="muted" hidden>wiki</a>'));
});
