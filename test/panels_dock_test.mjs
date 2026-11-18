// Tests the page half of host/fun/panels.js with the host bridge: an item of
// the bridge's menu is drawn in the front pages' menu and a press opens its
// page in the dock, framed as a panel is, not in this tab. An item that asks
// for a page of its own is a plain link to a new tab, with no dock or frame. A
// stub document stands for the page; see "The host bridge" in
// host/docs/api.md.
//
//   node --test test/panels_dock_test.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

function node(tag) {
  const n = {
    tag, className: "", textContent: "", hidden: false, kids: [], attrs: {}, listeners: {}, parent: null,
    setAttribute(k, v) { this.attrs[k] = String(v); },
    getAttribute(k) { return this.attrs[k] ?? null; },
    addEventListener(type, f) { (this.listeners[type] ||= []).push(f); },
    append(...kids) { for (const k of kids) { k.parent = this; this.kids.push(k); } },
    remove() { if (this.parent) this.parent.kids = this.parent.kids.filter((k) => k !== this); this.parent = null; },
    focus() {},
    all() { return this.kids.flatMap((k) => [k, ...k.all()]); },
    querySelectorAll(sel) { return this.all().filter((k) => k.className === sel.slice(1)); },
    querySelector(sel) { return this.querySelectorAll(sel)[0] || null; },
  };
  return n;
}

function press(link, over = {}) {
  let prevented = false;
  const e = { button: 0, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, ...over,
    preventDefault() { prevented = true; } };
  for (const f of link.listeners.click) f(e);
  return prevented;
}

const nav = node("nav");
const body = node("body");
const made = [];
globalThis.document = {
  body,
  createElement(tag) { const n = node(tag); made.push(n); return n; },
  querySelector: (sel) => (sel === ".topbar nav" ? nav : null),
  addEventListener() {},
};
globalThis.location = { origin: "https://factory.example" };
globalThis.fetch = async () => ({ ok: false, status: 404, json: async () => ({}) });

const sample = JSON.parse(readFileSync(new URL("../api/host-bridge.full.json", import.meta.url), "utf8"));
let changed = null;
globalThis.TEHAS_HOST_BRIDGE = {
  current: () => ({ ticketLink: null, notes: new Map(), menu: sample.menu }),
  onChange: (f) => { changed = f; },
};
await import("../host/fun/dom.js");
await import("../host/fun/panels.js");
await new Promise((r) => setTimeout(r, 0));

const links = () => nav.querySelectorAll(".panel-link");
const dock = () => body.querySelector(".panel-dock");

test("the bridge's menu item is a link in the menu that names the dock, with no frame until a press", () => {
  const [item, page] = sample.menu;
  assert.deepEqual(links().map((l) => [l.textContent, l.href]), [[item.label, item.url], [page.label, page.url]]);
  assert.equal(links()[0].attrs["aria-controls"], "panel-dock");
  assert.equal(links()[0].attrs["aria-expanded"], "false");
  assert.equal(made.filter((n) => n.tag === "iframe").length, 0);
  assert.equal(dock(), null);
});

test("a press opens the item's page in the dock, sandboxed, and not in this tab", () => {
  const [item] = sample.menu;
  const link = links()[0];
  assert.equal(press(link), true);
  assert.equal(dock().hidden, false);
  assert.equal(dock().querySelector(".panel-dock-title").textContent, item.label);
  const frames = made.filter((n) => n.tag === "iframe");
  assert.equal(frames.length, 1);
  assert.equal(frames[0].src, item.url);
  assert.ok(frames[0].attrs.sandbox.includes("allow-scripts"));
  assert.ok(!frames[0].attrs.sandbox.includes("allow-top-navigation"));
  assert.equal(link.attrs["aria-expanded"], "true");
  assert.equal(press(link), true);
  assert.equal(dock().hidden, true);
});

test("a press with a modifier or another button is left to the browser, as for a panel", () => {
  const link = links()[0];
  for (const over of [{ ctrlKey: true }, { metaKey: true }, { shiftKey: true }, { altKey: true }, { button: 1 }]) {
    assert.equal(press(link, over), false, JSON.stringify(over));
  }
  assert.equal(dock().hidden, true);
});

test("a page item is a plain link to a new tab: a press is left to the browser and makes no dock or frame", () => {
  const link = links()[1];
  assert.equal(link.target, "_blank");
  assert.equal(link.rel, "noopener noreferrer");
  assert.equal(link.attrs["aria-controls"], undefined);
  assert.equal(link.attrs["aria-expanded"], undefined);
  assert.equal((link.listeners.click || []).length, 0);
  assert.equal(dock().hidden, true);
});

test("an item that turns from the dock to a page closes the dock and drops its frame", () => {
  const [item] = sample.menu;
  press(links()[0]);
  assert.equal(dock().hidden, false);
  changed({ menu: [{ ...item, open: "page" }] });
  assert.equal(dock().hidden, true);
  assert.equal(dock().querySelector(".panel-dock-body").kids.length, 0);
  assert.equal(links()[0].attrs["aria-controls"], undefined);
  changed({ menu: sample.menu });
});

test("an item that goes from the answer goes from the menu, and a dock showing it closes", () => {
  press(links()[0]);
  assert.equal(dock().hidden, false);
  changed(null);
  assert.equal(links().length, 0);
  assert.equal(dock().hidden, true);
  assert.equal(dock().querySelector(".panel-dock-body").kids.length, 0);
});
