// Tests the label table of host/fun/stages.js: which stage a ticket stands at
// by the labels tehas writes, in the order both pages read them, with planning
// a stage of the line and no stage of the factory floor. The two pages that
// read it are tested by their own files: this one says what they agree on.
//
//   node --test test/stages_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { STAGE_LABELS, STAGE_ORDER, stageOf } from "../host/fun/stages.js";

const BOTH = (labels, closed) => stageOf(labels, closed);
const LINE = (labels, closed) => stageOf(labels, closed, { planning: true });
const FLOOR = (labels, closed) => stageOf(labels, closed, { planning: false });

test("the table names the labels in the order they answer", () => {
  assert.deepEqual(STAGE_ORDER, ["stopped", "review", "building", "planning", "line"]);
  assert.deepEqual(STAGE_LABELS, {
    stopped: ["needs-human-attention"],
    review: ["tehas:awaiting-merge"],
    building: ["tehas:running"],
    planning: ["tehas:plan", "tehas:planning"],
    line: ["tehas:ready"],
  });
  assert.deepEqual(Object.keys(STAGE_LABELS), STAGE_ORDER);
});

test("a closed ticket has left the line whatever its labels say", () => {
  for (const labels of [[], ["tehas:ready"], ["needs-human-attention", "tehas:plan"]]) {
    assert.equal(BOTH(labels, true), "shipped", labels.join(","));
    assert.equal(FLOOR(labels, true), "shipped", labels.join(","));
  }
});

test("one label of tehas names one stage, on either page", () => {
  for (const planning of [true, false]) {
    const at = (labels) => stageOf(labels, false, { planning });
    assert.equal(at(["needs-human-attention"]), "stopped");
    assert.equal(at(["tehas:awaiting-merge"]), "review");
    assert.equal(at(["tehas:running"]), "building");
    assert.equal(at(["tehas:ready"]), "line");
    // A label tehas does not write for the line is no stage at all.
    assert.equal(at(["bug", "help wanted"]), null);
    assert.equal(at([]), null);
  }
});

test("the label that stands first in the table answers first", () => {
  const labels = ["tehas:ready", "tehas:plan", "tehas:running", "tehas:awaiting-merge", "needs-human-attention"];
  assert.equal(BOTH(labels, false), "stopped");
  assert.equal(BOTH(["tehas:ready", "tehas:awaiting-merge"], false), "review");
  assert.equal(BOTH(["tehas:ready", "tehas:running"], false), "building");
  // The order is the same with planning off: a stopped ticket is stopped on the
  // floor as it is on the line.
  assert.equal(FLOOR(labels, false), "stopped");
});

test("planning is a stage of the line, and no stage of the floor", () => {
  for (const label of ["tehas:plan", "tehas:planning"]) {
    assert.equal(LINE([label], false), "planning");
    assert.equal(FLOOR([label], false), null);
  }
});

test("a ticket with both tehas:plan and tehas:ready is planned on the line and ready on the floor", () => {
  assert.equal(LINE(["tehas:plan", "tehas:ready"], false), "planning");
  assert.equal(FLOOR(["tehas:plan", "tehas:ready"], false), "line");
  // With planning off nothing else the ticket carries counts either.
  assert.equal(FLOOR(["tehas:planning"], false), null);
});

test("planning is on unless the page says otherwise", () => {
  assert.equal(stageOf(["tehas:plan"], false), "planning");
  assert.equal(stageOf(["tehas:plan"], false, {}), "planning");
});
