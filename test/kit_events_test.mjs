// host/fun/kit/events.js: the names of api/events.txt, and raise, which
// dispatches an envelope a page takes as tehas:<event>.
//
//   node --test test/kit_events_test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { events, names, raise } from "../host/fun/kit/events.js";

const api = (name) => new URL(`../api/${name}`, import.meta.url);
const sample = (name, side = "full") => JSON.parse(readFileSync(api(`event-${name.replace(".", "-")}.${side}.json`), "utf8"));

test("the names are exactly the lines of api/events.txt", () => {
  const lines = readFileSync(api("events.txt"), "utf8").split("\n").filter(Boolean);
  assert.deepEqual([...names], lines);
  assert.ok(Object.isFrozen(names));
});

test("raise dispatches every sample as tehas:<event>, not bubbling, the envelope as detail", () => {
  for (const name of names) {
    for (const side of ["full", "bare"]) {
      const envelope = sample(name, side);
      const heard = [];
      const on = (e) => heard.push(e);
      events.addEventListener(`tehas:${name}`, on);
      raise(envelope);
      events.removeEventListener(`tehas:${name}`, on);
      assert.equal(heard.length, 1, name);
      assert.equal(heard[0].detail, envelope);
      assert.equal(heard[0].bubbles, false);
    }
  }
});

test("a relayed envelope keeps its source", () => {
  const envelope = { ...sample("cycle.started"), source: "tehas-tehas" };
  let detail = null;
  const on = (e) => (detail = e.detail);
  events.addEventListener("tehas:cycle.started", on);
  raise(envelope);
  events.removeEventListener("tehas:cycle.started", on);
  assert.equal(detail.source, "tehas-tehas");
});

test("raise refuses another name, shape or when, and dispatches nothing", () => {
  const base = sample("cycle.ended");
  const gate = JSON.parse(readFileSync(api("event-cycle-started-before.full.json"), "utf8"));
  const bad = [
    null,
    [],
    { ...base, event: "ticket.reopened" },
    { ...base, when: "before" },
    gate,
    { ...base, api: 2 },
    { ...base, at: "2026-11-22T13:20:00.5Z" },
    { ...base, at: "2026-11-22T13:20:00+02:00" },
    { ...base, key: "a b" },
    { ...base, key: "" },
    { ...base, source: "Tehas Tehas" },
    { ...base, data: null },
    { ...base, extra: 1 },
  ];
  let heard = 0;
  const on = () => heard++;
  events.addEventListener("tehas:cycle.ended", on);
  for (const e of bad) assert.throws(() => raise(e), TypeError, JSON.stringify(e));
  events.removeEventListener("tehas:cycle.ended", on);
  assert.equal(heard, 0);
});
