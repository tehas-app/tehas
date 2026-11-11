// Tests stageOf() of host/fun/stages.js: the one label table the factory floor
// and the business page ask, so one ticket cannot read as two stages. The two
// settings are `planning` (does the plan stage count) and the names each page
// gives a closed ticket and a ticket that carries no tehas label.
//
//   node --test test/stages_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { LABEL_STAGES, stageOf } from "../host/fun/stages.js";

// The two pages, as their callers ask them: the stage each page has for a closed
// ticket, and for one that carries none of these labels. The names the table
// itself answers with - stopped, review, running, plan, ready - are the labels'
// own; a page that calls a stage otherwise maps it (host/fun/business-view.js).
const FLOOR = { shipped: "shipped", none: "backlog" };
const BOARD = { planning: true, shipped: "ship", none: "ship" };

test("the table is ordered, and the first label that matches answers", () => {
  assert.deepEqual(LABEL_STAGES, [
    ["needs-human-attention", "stopped"],
    ["tehas:awaiting-merge", "review"],
    ["tehas:running", "running"],
    ["tehas:plan", "plan"],
    ["tehas:planning", "plan"],
    ["tehas:ready", "ready"],
  ]);
  // A ticket keeps the place of the earlier row, whichever order its labels
  // come in: a cycle that is building is not also waiting for a review, and a
  // planned ticket is not yet on the line.
  for (const [better, worse] of [
    ["needs-human-attention", "tehas:ready"],
    ["tehas:awaiting-merge", "tehas:running"],
    ["tehas:running", "tehas:ready"],
    ["tehas:plan", "tehas:ready"],
  ]) {
    for (const labels of [[better, worse], [worse, better]]) {
      const want = LABEL_STAGES.find(([label]) => label === better)[1];
      assert.equal(stageOf(labels, false, BOARD), want, labels.join(" + "));
    }
  }
});

test("a label from anywhere else says nothing", () => {
  assert.equal(stageOf([], false, FLOOR), "backlog");
  assert.equal(stageOf(["bug", "help wanted"], false, FLOOR), "backlog");
  assert.equal(stageOf([], false), null);
});

// Both callers normalise their file first (host/fun/business-view.js, the
// normalize() of host/fun/game-data.js), so this is the shared function's own
// bound: a ticket whose labels are missing reads as an unlabelled one, which is
// what the two pages answer for that file, rather than throwing.
test("labels that are not a list name no stage", () => {
  for (const labels of [undefined, null, "tehas:ready", {}, 0]) {
    assert.equal(stageOf(labels, false, FLOOR), "backlog", String(labels));
    assert.equal(stageOf(labels, false, { planning: true, shipped: "ship", none: null }), null, String(labels));
    // A shut ticket has left the line whatever its labels say, list or none.
    assert.equal(stageOf(labels, true, FLOOR), "shipped", String(labels));
  }
});

test("a closed ticket has left the line whatever its labels say", () => {
  assert.equal(stageOf(["tehas:running"], true, FLOOR), "shipped");
  assert.equal(stageOf(["tehas:ready"], true, BOARD), "ship");
  // Each page names the stage in its own words, and the answer does not wait
  // for a label.
  assert.equal(stageOf([], true, { shipped: "gone" }), "gone");
});

test("the worst label answers first, whichever order the ticket lists them in", () => {
  for (const labels of [["tehas:ready", "needs-human-attention"], ["needs-human-attention", "tehas:ready"]]) {
    assert.equal(stageOf(labels, false, FLOOR), "stopped");
    assert.equal(stageOf(labels, false, BOARD), "stopped");
  }
});

test("planning is a stage on the business page and not on the factory floor", () => {
  // With planning on, a ticket that is being planned stands in the Planning
  // column; with it off the line has no place for it, so tehas:ready wins.
  const both = ["tehas:plan", "tehas:ready"];
  assert.equal(stageOf(both, false, { planning: true, shipped: "ship", none: null }), "plan");
  assert.equal(stageOf(both, false, { planning: false, shipped: "ship", none: null }), "ready");
  assert.equal(stageOf(both, false, FLOOR), "ready");
  assert.equal(stageOf(both, false, BOARD), "plan");
  // tehas writes both words for the same stage, and both count.
  assert.equal(stageOf(["tehas:planning"], false, BOARD), "plan");
  assert.equal(stageOf(["tehas:planning"], false, FLOOR), "backlog");
});
