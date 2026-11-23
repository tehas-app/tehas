// Tests stageOf() of host/fun/stages.js: the label table the factory floor
// asks. A caller names the stage of a closed ticket and of one that carries
// none of the table's labels.
//
//   node --test test/stages_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { LABEL_STAGES, stageOf } from "../host/fun/stages.js";

// The floor, as game-data.js asks.
const FLOOR = { shipped: "shipped", none: "idea" };

test("the table is ordered, and the first label that matches answers", () => {
  assert.deepEqual(LABEL_STAGES, [
    ["needs-human-attention", "stopped"],
    ["tehas:awaiting-merge", "review"],
    ["tehas:running", "running"],
    ["tehas:ready", "ready"],
  ]);
  // A ticket keeps the place of the earlier row, whichever order its labels
  // come in: a cycle that is building is not also waiting for a review.
  for (const [better, worse] of [
    ["needs-human-attention", "tehas:ready"],
    ["tehas:awaiting-merge", "tehas:running"],
    ["tehas:running", "tehas:ready"],
  ]) {
    for (const labels of [[better, worse], [worse, better]]) {
      const want = LABEL_STAGES.find(([label]) => label === better)[1];
      assert.equal(stageOf(labels, false, FLOOR), want, labels.join(" + "));
    }
  }
});

test("a label from anywhere else says nothing", () => {
  assert.equal(stageOf([], false, FLOOR), "idea");
  assert.equal(stageOf(["bug", "help wanted"], false, FLOOR), "idea");
  assert.equal(stageOf([], false), null);
});

// A caller normalises its file first (the normalize() of host/fun/game-data.js),
// so this is the function's own bound: a ticket whose labels are missing reads
// as an unlabelled one, rather than throwing.
test("labels that are not a list name no stage", () => {
  for (const labels of [undefined, null, "tehas:ready", {}, 0]) {
    assert.equal(stageOf(labels, false, FLOOR), "idea", String(labels));
    assert.equal(stageOf(labels, true, FLOOR), "shipped", String(labels));
  }
});

test("a closed ticket has left the line whatever its labels say", () => {
  assert.equal(stageOf(["tehas:running"], true, FLOOR), "shipped");
  assert.equal(stageOf([], true, { shipped: "gone" }), "gone");
});

test("the plan labels name no stage, so tehas:ready wins a ticket that carries both", () => {
  assert.equal(stageOf(["tehas:plan", "tehas:ready"], false, FLOOR), "ready");
  assert.equal(stageOf(["tehas:planning"], false, FLOOR), "idea");
});
