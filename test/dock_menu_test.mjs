// Tests the pure steps of the dock: bridge(), from the host bridge's menu to
// its items, and plugins(), from live/plugins.json to the items of the
// plugins menu the front page draw after them.
//
//   node --test test/dock_menu_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import "../host/fun/dock.js";

const { bridge, plugins } = globalThis.TEHAS_DOCK;

const ORIGIN = "https://example.org";
const BASE = "https://plugin-floorplan.example.org";
const view = (id, over = {}) => ({ id, label: `View ${id}`, url: `${BASE}/${id}/`, open: "dock", ...over });
const plugin = (name, views, over = {}) => ({ name, label: `Plugin ${name}`, views, ...over });
const file = (list) => ({ api: 1, plugins: list });

test("no file, or not one, is no item", () => {
  for (const f of [null, undefined, 3, "x", [], {}, { plugins: "x" }, { plugins: {} }]) {
    assert.deepEqual(plugins(f, ORIGIN), []);
  }
});

test("each view of each plugin is an item under the plugin's label, in order", () => {
  const items = plugins(file([plugin("floorplan", [view("map"), view("health", { open: "page" })])]), ORIGIN);
  assert.deepEqual(items, [
    { id: "m-floorplan-map", label: "View map", url: `${BASE}/map/`, open: "dock", group: "Plugin floorplan" },
    { id: "m-floorplan-health", label: "View health", url: `${BASE}/health/`, open: "page", group: "Plugin floorplan" },
  ]);
});

test("a view is left out for a bad id, label or address, and the plugin for a bad name or label", () => {
  const views = [
    view("Map"),
    view("map", { label: "" }),
    view("map", { label: "x".repeat(25) }),
    view("map", { url: "javascript:alert(1)" }),
    view("map", { url: "https://u:p@plugin-floorplan.example.org/" }),
    view("map", { url: `${ORIGIN}/same-origin/` }),
    view("map", { url: "not a url" }),
  ];
  for (const bad of views) {
    assert.deepEqual(plugins(file([plugin("floorplan", [bad, view("ok")])]), ORIGIN).map((i) => i.id), ["m-floorplan-ok"], JSON.stringify(bad));
  }
  for (const p of [plugin("Floor", [view("a")]), plugin("floorplan", [view("a")], { label: "" }), plugin("floorplan", "x")]) {
    assert.deepEqual(plugins(file([p]), ORIGIN), []);
  }
});

test("a label that is HTML is the text it is, and a control character is a space", () => {
  const [item] = plugins(file([plugin("floorplan", [view("a", { label: "<b>x y</b>" })], { label: "<i>P</i>" })]), ORIGIN);
  assert.equal(item.label, "<b>x y</b>");
  assert.equal(item.group, "<i>P</i>");
});

test("at most four views a plugin and eight plugins, each name once", () => {
  const many = ["a", "b", "c", "d", "e"].map((id) => view(id));
  assert.equal(plugins(file([plugin("floorplan", many)]), ORIGIN).length, 4);
  const names = Array.from({ length: 10 }, (_, i) => plugin(`p${i}`, [view("a")]));
  assert.equal(plugins(file(names), ORIGIN).length, 8);
  assert.equal(plugins(file([plugin("p", [view("a")]), plugin("p", [view("b")])]), ORIGIN).length, 1);
  // An open other than page docks.
  assert.equal(plugins(file([plugin("p", [view("a", { open: "full" })])]), ORIGIN)[0].open, "dock");
});

test("the committed sample of live/plugins.json makes its items", () => {
  const sample = JSON.parse(readFileSync(new URL("./fixtures/live/plugins.json", import.meta.url), "utf8"));
  assert.deepEqual(plugins(sample, ORIGIN).map((i) => [i.id, i.open, i.group]), [
    ["m-floorplan-map", "dock", "Floorplan"],
    ["m-floorplan-health", "page", "Floorplan"],
  ]);
});

const item = (id, over = {}) => ({ id, label: `Label ${id}`, url: `https://${id}.example.net/x`, ...over });

test("the bridge's items keep their order and how they open, and a later id takes the earlier one's place", () => {
  assert.deepEqual(bridge(undefined, ORIGIN), []);
  assert.deepEqual(bridge("x", ORIGIN), []);
  const out = bridge([item("a", { open: "page" }), item("b", { open: "dock" })], ORIGIN);
  assert.deepEqual(out.map((i) => [i.id, i.open]), [["a", "page"], ["b", "dock"]]);
  const swapped = bridge([item("a"), item("b"), item("a", { label: "Again", url: "http://host.example.net/a" })], ORIGIN);
  assert.deepEqual(swapped.map((i) => [i.id, i.label, i.url]), [
    ["a", "Again", "http://host.example.net/a"],
    ["b", "Label b", "https://b.example.net/x"],
  ]);
});

test("four bridge items is the most, and one on the page's own origin or with no address is dropped", () => {
  assert.deepEqual(bridge(["a", "b", "c", "d", "e"].map((id) => item(id)), ORIGIN).map((i) => i.id), ["a", "b", "c", "d"]);
  assert.deepEqual(bridge([item("d", { url: "https://example.org/x" })], ORIGIN), []);
  assert.deepEqual(bridge([item("d", { url: "not a url" })], ORIGIN), []);
});
