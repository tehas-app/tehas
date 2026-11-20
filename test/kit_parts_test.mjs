// The pure parts of host/fun/kit/parts.js and host/fun/kit/island.js. What
// they draw is checked in a browser (test/browser/kit-check.mjs).
import "./kit_stub.mjs";
import { test } from "node:test";
import assert from "node:assert/strict";
import { ago, message, toMs } from "../host/fun/kit/parts.js";
import { attrName, eventFor, readProps } from "../host/fun/kit/island.js";

const NOW = Date.parse("2026-11-17T12:00:00Z");

test("ago words a time against now in the largest whole unit", () => {
  assert.equal(ago(NOW - 5 * 60000, NOW, "en"), "5 minutes ago");
  assert.equal(ago(NOW - 26 * 3600000, NOW, "en"), "yesterday");
  assert.equal(ago(NOW, NOW, "en"), "now");
  assert.equal(ago(NOW + 3 * 3600000, NOW, "en"), "in 3 hours");
  assert.equal(ago(NaN, NOW, "en"), "");
});

test("toMs takes ms or an ISO string", () => {
  assert.equal(toMs(NOW), NOW);
  assert.equal(toMs("2026-11-17T12:00:00Z"), NOW);
  assert.ok(Number.isNaN(toMs("not a time")));
});

test("message reads an error, an accessor of one, or anything else", () => {
  assert.equal(message(new Error("gone")), "gone");
  assert.equal(message(() => new Error("later")), "later");
  assert.equal(message("plain"), "plain");
  assert.equal(message(null), "");
});

function element(attrs) {
  return {
    localName: "tehas-factory-queue",
    hasAttribute: (n) => n in attrs,
    getAttribute: (n) => (n in attrs ? attrs[n] : null),
  };
}

test("readProps types the attributes it is told of, in kebab case", () => {
  const el = element({ src: "/live/status.json", limit: "10", "show-done": "" });
  assert.deepEqual(
    readProps(el, { src: String, limit: Number, showDone: Boolean, hidden: Boolean, gone: String }),
    { src: "/live/status.json", limit: 10, showDone: true, hidden: false },
  );
  assert.equal(attrName("showDoneItems"), "show-done-items");
});

test("an event is <element>:<verb>, bubbles, and carries plain JSON", () => {
  const e = eventFor(element({}), "pick", { number: 41, at: new Date(NOW), fn() {} });
  assert.equal(e.type, "tehas-factory-queue:pick");
  assert.equal(e.bubbles, true);
  assert.deepEqual(e.detail, { number: 41, at: "2026-11-17T12:00:00.000Z" });
  assert.equal(eventFor(element({}), "close").detail, null);
});
