// Tests the shape of the plain dashboard: host/docs/dashboard.frag holds each
// island of host/fun/factory/ with its server HTML, loads the host bridge
// before the islands and the kit's stylesheet, and every island is the three
// files of the contract. What the models make is test/dashboard_host_test.mjs;
// what the islands draw is test/browser/dashboard-check.mjs, in a browser.
//
//   node --test test/dashboard_view_test.mjs
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { test } from "node:test";

const FRAG = readFileSync(new URL("../host/docs/dashboard.frag", import.meta.url), "utf8");
const FACTORY = new URL("../host/fun/factory/", import.meta.url);
const read = (name) => readFileSync(new URL(name, FACTORY), "utf8");
const ISLANDS = ["state", "console", "issues", "readonly", "days", "previews", "host", "wikilink"];

test("the fragment loads the host bridge, then each island's element, and holds no script of its own", () => {
  const tags = FRAG.split("\n").filter((l) => l.includes("<script"));
  assert.deepEqual(tags, [
    '<script src="/fun/host-bridge.js"></script>',
    ...ISLANDS.map((n) => `<script type="module" src="/fun/factory/tehas-factory-${n}.js"></script>`),
  ]);
  assert.ok(FRAG.includes('<link rel="stylesheet" href="/fun/kit/kit.css">'));
  assert.ok(!FRAG.includes("style="));
});

test("each island stands in the fragment with its server HTML, laid out as its children", () => {
  for (const n of ISLANDS) {
    const open = [...FRAG.matchAll(new RegExp(`<tehas-factory-${n} ([^>]*)>`, "g"))];
    assert.ok(open.length > 0, n);
    for (const m of open) assert.match(m[1], /^data-tehas-island="contents" src="\/live\/[a-z]+\.json"/, n);
  }
  for (const list of ["queued", "awaiting", "stopped", "completed"]) {
    assert.ok(FRAG.includes(`src="/live/status.json" list="${list}"><section><h2>`), list);
    assert.ok(FRAG.includes(`<ul class="issues" id="${list}"></ul>`), list);
  }
  for (const id of ["now", "ro", "analytics", "previews", "host"]) assert.match(FRAG, new RegExp(`<section [^>]*id="${id}" hidden>`), id);
  assert.ok(FRAG.includes('<span id="state" class="pill idle">loading</span>'));
});

test("each island is an element, a model and a render module, and only the render module draws", () => {
  for (const n of ISLANDS) {
    const element = read(`tehas-factory-${n}.js`);
    assert.ok(element.includes('import { island } from "../kit/island.js";'), n);
    assert.ok(element.includes(`island("tehas-factory-${n}", {`), n);
    assert.ok(element.includes(`load: () => import("./tehas-factory-${n}.render.js"),`), n);
    assert.ok(existsSync(new URL(`tehas-factory-${n}.model.js`, FACTORY)), n);
    assert.ok(!read(`tehas-factory-${n}.model.js`).includes("kit/"), n);
    assert.ok(read(`tehas-factory-${n}.render.js`).includes("export const mount = view("), n);
  }
});

test("the islands read server data only through the kit's feeds; the console's tail is the one fetch", () => {
  for (const n of ISLANDS) {
    for (const part of ["", ".model", ".render"]) assert.ok(!read(`tehas-factory-${n}${part}.js`).includes("fetch("), n + part);
  }
  const tail = read("tehas-factory-tail.js");
  assert.ok(tail.includes('fetch("/live/" + file, { headers: { Range: "bytes=" + this.#offset + "-" }, cache: "no-store" })'));
  assert.ok(!/^import /m.test(tail));
  assert.ok(read("tehas-factory-console.render.js").includes('import "./tehas-factory-tail.js";'));
});
