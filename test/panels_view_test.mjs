// Tests view() of the panels: the pure step from live/panels.json to the items
// the two front pages put in their menu.
//
//   node --test test/panels_view_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import "../host/fun/panels.js";

const { view } = globalThis.TEHAS_PANELS;

const ORIGIN = "https://example.org";
const panel = (id, over = {}) => ({ id, label: `Label ${id}`, url: `https://${id}.example.net/x`, ...over });
const file = (panels) => ({ api: 1, panels });

test("no file, or not one, is no item", () => {
  for (const f of [null, undefined, 3, "x", [], {}, { panels: "x" }, { panels: {} }]) {
    assert.deepEqual(view(f, ORIGIN), []);
  }
});

test("each panel keeps its order, its label and its url", () => {
  const items = view(file([panel("support", { label: "Ask us" }), panel("status")]), ORIGIN);
  assert.deepEqual(items, [
    { id: "support", label: "Ask us", url: "https://support.example.net/x" },
    { id: "status", label: "Label status", url: "https://status.example.net/x" },
  ]);
});

test("a label that is HTML is kept as the text it is", () => {
  const [item] = view(file([panel("a", { label: "<img src=x onerror=1>" })]), ORIGIN);
  assert.equal(item.label, "<img src=x onerror=1>");
});

test("a control character in a label is a space, and the ends are trimmed", () => {
  assert.equal(view(file([panel("a", { label: " Ask\nus\t" })]), ORIGIN)[0].label, "Ask us");
});

test("a url that is not http or https is dropped", () => {
  for (const url of ["javascript:alert(1)", "JavaScript:alert(1)", "data:text/html,x", "ftp://a.example.net/", "file:///etc/passwd", "//a.example.net/", "x", ""]) {
    assert.deepEqual(view(file([panel("a", { url })]), ORIGIN), [], url);
  }
});

test("a url on the page's own origin is dropped, and a sibling name is not", () => {
  for (const url of ["https://example.org/", "https://EXAMPLE.org/x", "https://example.org:443/x"]) {
    assert.deepEqual(view(file([panel("a", { url })]), ORIGIN), [], url);
  }
  assert.equal(view(file([panel("a", { url: "https://support.example.org/" })]), ORIGIN).length, 1);
  assert.equal(view(file([panel("a", { url: "http://example.org/" })]), ORIGIN).length, 1);
});

test("a url with a user or a password, or too long, is dropped", () => {
  for (const url of ["https://u@a.example.net/", "https://u:p@a.example.net/", `https://a.example.net/${"x".repeat(491)}`]) {
    assert.deepEqual(view(file([panel("a", { url })]), ORIGIN), [], url);
  }
});

test("odd entries are left out, an id is kept once, and four is the most", () => {
  const items = view(file([
    null, "x", { id: "a" }, panel("A"), panel("a_b"), panel("x".repeat(33)),
    panel("a", { label: 3 }), panel("a", { label: "" }), panel("a", { label: "x".repeat(25) }), panel("a", { url: 7 }),
    panel("a"), panel("a", { label: "again" }), panel("b"), panel("c"), panel("d"), panel("e"),
  ]), ORIGIN);
  assert.deepEqual(items.map((i) => i.id), ["a", "b", "c", "d"]);
  assert.equal(items[0].label, "Label a");
});

const { merge } = globalThis.TEHAS_PANELS;
const item = (id, over = {}) => ({ id, label: `Label ${id}`, url: `https://${id}.example.net/x`, ...over });

test("the bridge's menu follows the file's items, and takes the place of one with its id", () => {
  const file = [item("a"), item("b")];
  assert.deepEqual(merge(file, [], ORIGIN), file);
  assert.deepEqual(merge(file, undefined, ORIGIN), file);
  assert.deepEqual(merge(file, [item("c")], ORIGIN).map((i) => i.id), ["a", "b", "c"]);
  const swapped = merge(file, [item("a", { label: "From the host", url: "http://host.example.net/a" })], ORIGIN);
  assert.deepEqual(swapped, [{ id: "a", label: "From the host", url: "http://host.example.net/a" }, file[1]]);
});

test("four is the most with the bridge's too, and a bridge item on the page's own origin is dropped", () => {
  const file = [item("a"), item("b"), item("c")];
  assert.deepEqual(merge(file, [item("d"), item("e")], ORIGIN).map((i) => i.id), ["a", "b", "c", "d"]);
  assert.deepEqual(merge([], [item("d", { url: "https://example.org/x" })], ORIGIN), []);
  assert.deepEqual(merge([], [item("d", { url: "not a url" })], ORIGIN), []);
});
