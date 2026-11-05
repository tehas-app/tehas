// Tests the pure half of host/fun/dashboard-view.js, the plain dashboard's
// script: what the state pill, the shown cycle and a list of issues say, from
// the files as they are. No document at all: importing the module draws nothing.
//
//   node --test test/dashboard_view_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { MAX, dur, issueRows, previewWhy, render, view } from "../host/fun/dashboard-view.js";

const status = (over = {}) => ({
  running: [], queued: [], awaiting: [], stopped: [], completed: [], paused: false, noResources: false, ...over,
});
const tabs = (over = {}) => ({ tabs: [], shown: null, running: 0, ...over });

test("importing the module with no document draws nothing and needs nothing", () => {
  assert.equal(typeof globalThis.document, "undefined");
  assert.equal(MAX, 10);
});

test("the pill says idle, working, paused or out of resources", () => {
  assert.deepEqual(view(status(), tabs(), 0).pill, { className: "pill idle", text: "idle" });
  assert.deepEqual(view(status({ noResources: true }), tabs(), 0).pill, { className: "pill pause", text: "no resources" });
  const two = [{ issue: 4, running: true }, { issue: 9, running: true }];
  assert.deepEqual(view(status(), tabs({ running: 1, tabs: two.slice(0, 1) }), 0).pill, { className: "pill run", text: "working" });
  assert.deepEqual(view(status(), tabs({ running: 2, tabs: two }), 0).pill, { className: "pill run", text: "working on 2" });
  // A paused factory still finishes what it has, and says so.
  assert.deepEqual(view(status({ paused: true }), tabs({ running: 2, tabs: two }), 0).pill,
    { className: "pill pause", text: "paused, finishing #4, #9" });
  assert.deepEqual(view(status({ paused: true }), tabs(), 0).pill, { className: "pill pause", text: "paused" });
});

test("all-hands is reported only when the status says true", () => {
  assert.equal(view(status({ allHands: true }), tabs(), 0).allHands, true);
  for (const allHands of [false, null, undefined, "true", 1]) assert.equal(view(status({ allHands }), tabs(), 0).allHands, false);
});

test("the shown cycle has a heading and a step, running or ended", () => {
  const none = view(status(), tabs(), 0);
  assert.deepEqual([none.shown, none.running, none.title, none.step], [null, null, "", ""]);

  const cycle = { issue: 12, step: 3, statusText: "review", startedAt: 1000, updatedAt: 1300 };
  const s = status({ queued: [{ number: 12, title: "Fix the thing" }] });
  const live = view(s, tabs({ running: 1, shown: { issue: 12, running: true, cycle } }), 1000 + 3 * 3600 + 120);
  assert.equal(live.running, cycle);
  assert.equal(live.title, "Working on #12  Fix the thing");
  assert.equal(live.step, "review   (3 h 2 min so far)");

  const last = { ...cycle, status: "parked" };
  const ended = view(status(), tabs({ shown: { issue: 12, running: false, cycle: last } }), 99999);
  assert.equal(ended.running, null);
  assert.equal(ended.shown, last);
  assert.equal(ended.title, "Last cycle: #12");
  assert.equal(ended.step, "ended as parked at step 3 after 5 min");
});

test("a list of issues has at most ten rows and counts the rest", () => {
  const now = Date.parse("2026-11-05T12:00:00Z");
  const items = Array.from({ length: 12 }, (_, i) => ({
    number: i + 1, title: "t" + i, url: "u" + i, updatedAt: "2026-11-05T11:58:00Z", closedAt: "2026-11-05T12:00:00Z",
  }));
  const v = issueRows(items, "closedAt", now);
  assert.equal(v.count, 12);
  assert.equal(v.rows.length, MAX);
  assert.equal(v.more, 12);
  assert.deepEqual(v.rows[0], { number: 1, title: "t0", url: "u0", when: "just now" });
  assert.equal(issueRows(items.slice(0, 2), "updatedAt", now).more, 0);
  assert.deepEqual(issueRows([], "updatedAt", now), { count: 0, rows: [], more: 0 });
});

test("a preview that does not serve says why, with the reason from its state", () => {
  assert.equal(previewWhy({ state: "building" }), "building");
  assert.equal(previewWhy({ state: "build-failed", reason: "  the build failed " }), "unable to create: the build failed");
  assert.equal(previewWhy({ state: "odd" }), "not serving");
  assert.equal(previewWhy({ state: "toString" }), "not serving");
});

test("durations are minutes, then hours and minutes", () => {
  assert.deepEqual([dur(59), dur(61), dur(3600), dur(3720)], ["0 min", "1 min", "1 h 0 min", "1 h 2 min"]);
});

test("render puts children in only when the key changes", () => {
  let made = 0;
  const node = { dataset: {}, kids: [], replaceChildren(...k) { this.kids = k; } };
  const make = () => { made++; return ["a", "b"]; };
  assert.equal(render(node, "k1", make), true);
  assert.equal(render(node, "k1", make), false);
  assert.equal(made, 1);
  assert.deepEqual(node.kids, ["a", "b"]);
  assert.equal(render(node, "k2", () => ["c"]), true);
  assert.deepEqual(node.kids, ["c"]);
});
