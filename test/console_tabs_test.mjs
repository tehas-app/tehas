// Tests view() of the console's tabs: the pure step from live/running.json
// to which cycles can be shown and which one is.
//
//   node --test test/console_tabs_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import "../host/fun/console-tabs.js";

const { view, consoleFile } = globalThis.TEHAS_CONSOLE_TABS;

const cycle = (issue, over = {}) => ({
  issue, running: true, status: "running", step: "develop", statusText: "",
  startedAt: 1000, updatedAt: 2000, console: `console-${issue}.txt`, ...over,
});
const ended = (issue, status = "handoff", over = {}) => cycle(issue, { running: false, status, ...over });
const issues = (v) => v.tabs.map((t) => t.issue);

test("no file, or nothing in it, is no tab and nothing shown", () => {
  for (const now of [null, undefined, {}, { cycles: [], last: null }]) {
    const v = view(now, null);
    assert.deepEqual(v.tabs, []);
    assert.equal(v.shown, null);
    assert.equal(v.running, 0);
    assert.equal(v.failure, null);
  }
});

test("one running cycle is one tab, and it is shown", () => {
  const v = view({ cycles: [cycle(12)], last: null }, null);
  assert.deepEqual(issues(v), [12]);
  assert.equal(v.shown.issue, 12);
  assert.equal(v.shown.running, true);
  assert.equal(v.shown.cycle.console, "console-12.txt");
  assert.equal(v.running, 1);
});

test("two running cycles are two tabs in the order given, the first shown", () => {
  const v = view({ cycles: [cycle(12), cycle(14)], last: null }, null);
  assert.deepEqual(issues(v), [12, 14]);
  assert.equal(v.shown.issue, 12);
  assert.equal(v.running, 2);
});

test("the selected tab is shown", () => {
  const v = view({ cycles: [cycle(12), cycle(14)], last: null }, 14);
  assert.equal(v.shown.issue, 14);
});

test("a selection that is no longer listed falls back to the first tab", () => {
  const v = view({ cycles: [cycle(12), cycle(14)], last: null }, 99);
  assert.equal(v.shown.issue, 12);
});

test("the last cycle is a tab after the running ones, marked as ended", () => {
  const v = view({ cycles: [cycle(12)], last: ended(7) }, null);
  assert.deepEqual(issues(v), [12, 7]);
  assert.deepEqual(v.tabs.map((t) => t.running), [true, false]);
  assert.equal(view({ cycles: [cycle(12)], last: ended(7) }, 7).shown.issue, 7);
});

test("with nothing running the last cycle is the one tab", () => {
  const v = view({ cycles: [], last: ended(7) }, null);
  assert.deepEqual(issues(v), [7]);
  assert.equal(v.shown.running, false);
  assert.equal(v.running, 0);
});

test("an issue that is running again has one tab, the running one", () => {
  const v = view({ cycles: [cycle(7)], last: ended(7, "aborted") }, null);
  assert.deepEqual(issues(v), [7]);
  assert.equal(v.shown.running, true);
});

test("the same issue listed twice is one tab", () => {
  assert.deepEqual(issues(view({ cycles: [cycle(12), cycle(12)], last: null }, null)), [12]);
});

test("a last cycle that ended badly is the failure until a cycle starts after it", () => {
  const bad = ended(7, "aborted", { updatedAt: 3000 });
  assert.deepEqual(view({ cycles: [cycle(12, { startedAt: 2500 })], last: bad }, null).failure, { issue: 7, status: "aborted" });
  assert.equal(view({ cycles: [cycle(12, { startedAt: 3000 })], last: bad }, null).failure, null);
  assert.equal(view({ cycles: [cycle(12, { startedAt: 3500 })], last: bad }, null).failure, null);
  assert.deepEqual(view({ cycles: [], last: ended(7, "lost") }, null).failure, { issue: 7, status: "lost" });
  assert.equal(view({ cycles: [], last: ended(7, "merged") }, null).failure, null);
  assert.equal(view({ cycles: [], last: ended(7, "handoff") }, null).failure, null);
});

test("a long status is cut, as it is drawn on the floor", () => {
  assert.equal(view({ cycles: [], last: ended(7, "x".repeat(100)) }, null).failure.status.length, 40);
});

test("the one object an earlier version wrote is read: a running cycle", () => {
  const v = view(cycle(12), null);
  assert.deepEqual(issues(v), [12]);
  assert.equal(v.running, 1);
  assert.equal(v.failure, null);
});

test("the one object an earlier version wrote is read: an ended cycle", () => {
  const v = view(ended(12, "aborted"), null);
  assert.deepEqual(issues(v), [12]);
  assert.equal(v.running, 0);
  assert.deepEqual(v.failure, { issue: 12, status: "aborted" });
});

test("what is not a cycle is left out, and nothing throws", () => {
  const junk = [null, 7, "12", [], {}, { issue: "12" }, { issue: 0 }, { issue: -3 }, { issue: 1.5 }, { issue: null }];
  const v = view({ cycles: [...junk, cycle(12)], last: "nope" }, null);
  assert.deepEqual(issues(v), [12]);
  for (const now of [7, "text", [], [cycle(12)], { cycles: "many" }, { cycles: null, last: 3 }]) {
    assert.deepEqual(view(now, null).tabs, []);
  }
});

test("a console file is a plain file name or nothing", () => {
  assert.equal(consoleFile(cycle(12)), "console-12.txt");
  for (const name of ["../secret", "a/b.txt", "", 7, null, undefined, "console 12.txt"]) {
    assert.equal(consoleFile(cycle(12, { console: name })), null);
  }
});
