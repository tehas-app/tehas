// Tests view() of the epics panel: the pure step from live/epics.json to the
// epics the two front pages draw.
//
//   node --test test/epics_view_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import "../host/fun/epics-view.js";

const { view } = globalThis.TEHAS_EPICS;

const sub = (number, state = "open", title = `Sub ${number}`) => ({ number, title, url: "ignored", state });
const epic = (number, subIssues, over = {}) => ({
  number, title: `Epic ${number}`, url: "ignored", total: subIssues.length,
  completed: subIssues.filter((s) => s?.state === "closed").length, more: 0, subIssues, ...over,
});
const file = (epics, repo = "acme/widget") => ({ generatedAt: "2026-10-29T10:00:00Z", repo, epics });

test("no file, or not one, is nothing to draw", () => {
  for (const f of [null, undefined, 3, [], {}, { epics: "x" }]) assert.equal(view(f), null);
});

test("an empty list is a file with no epics", () => {
  assert.deepEqual(view(file([])).epics, []);
});

test("each epic keeps its order, its counts and its sub-issues in theirs", () => {
  const v = view(file([epic(9, [sub(12, "closed"), sub(10)]), epic(4, [sub(5)])]));
  assert.deepEqual(v.epics.map((e) => e.number), [9, 4]);
  const e = v.epics[0];
  assert.equal(e.title, "Epic 9");
  assert.deepEqual([e.total, e.completed, e.more], [2, 1, 0]);
  assert.deepEqual(e.subs.map((s) => [s.number, s.closed]), [[12, true], [10, false]]);
});

test("every link is made from the repository and a number, not taken from the file", () => {
  const v = view(file([epic(9, [sub(12)], { url: "javascript:alert(1)" })]));
  assert.equal(v.epics[0].url, "https://github.com/acme/widget/issues/9");
  assert.equal(v.epics[0].subs[0].url, "https://github.com/acme/widget/issues/12");
  const odd = view(file([epic(9, [sub(12)])], "acme/widget/../x\"><b>"));
  assert.equal(odd.epics[0].url, null);
  assert.equal(odd.epics[0].subs[0].url, null);
});

test("sub-issues not listed are counted as more", () => {
  const v = view(file([epic(9, [sub(12, "closed")], { total: 5, completed: 3 })]));
  assert.deepEqual([v.epics[0].total, v.epics[0].completed, v.epics[0].more], [5, 3, 4]);
});

test("odd entries are left out and odd counts are bounded", () => {
  const v = view(file([
    null, { title: "no number" }, { number: -1 }, { number: 1.5 },
    epic(9, [sub(1), null, { number: "x" }], { total: -3, completed: 99 }),
    epic(9, [sub(2)]),
    { number: 7, title: 42 },
  ]));
  assert.deepEqual(v.epics.map((e) => e.number), [9, 7]);
  const nine = v.epics[0];
  assert.deepEqual(nine.subs.map((s) => s.number), [1]);
  assert.deepEqual([nine.total, nine.completed, nine.more], [1, 1, 0]);
  assert.equal(v.epics[1].title, "");
  assert.deepEqual(v.epics[1].subs, []);
});
