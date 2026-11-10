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

test("what is not a set of labels, or not a set of options, says nothing", () => {
  // A page that reads its options out of an untrusted file may hand over
  // null, which is the table with planning, not a thrown TypeError.
  for (const options of [null, undefined, "planning"]) {
    assert.equal(stageOf(["tehas:plan"], false, options), "planning", String(options));
  }
  // What is not a list of labels is no label at all, whatever it reads like:
  // a string would otherwise answer includes() like a list of one.
  for (const labels of ["tehas:ready", null, undefined, 7, {}]) {
    assert.equal(stageOf(labels, false), null, String(labels));
  }
  // A list with something that is not a label in it still reads its labels.
  assert.equal(stageOf(["tehas:ready", 7, null], false), "line");
});

test("a label answers whatever its case, and a page may pass it as GitHub wrote it", () => {
  for (const planning of [true, false]) {
    const at = (labels) => stageOf(labels, false, { planning });
    assert.equal(at(["Tehas:Ready"]), "line");
    assert.equal(at(["NEEDS-HUMAN-ATTENTION"]), "stopped");
    assert.equal(at(["Tehas:Plan"]), planning ? "planning" : null);
    // What a label is not stays no stage, whatever its case.
    assert.equal(at(["Bug", "Help Wanted"]), null);
  }
});

// The pages name the steps their own way, so a step the table gains and a
// page does not name must draw nothing rather than the stage next to it. The
// two maps are the ones the pages read (business-view.js's LINE_KEY,
// game-data.js's FLOOR_STAGES), copied as they are written; the cases beside
// each page say the page really has that map, this one says what both must
// cover.
const LINE_KEY = { stopped: "stop", review: "review", building: "build", planning: "plan", line: "line", shipped: "ship" };
const FLOOR_STAGES = { stopped: "stopped", review: "review", building: "running", line: "ready" };
const PAGES = [
  // "asked" is the { planning } the page passes; "drawn" the names it has for
  // its own sake rather than as a step of the table. "shipped" is the column
  // of closed tickets, which both pages fill from the file's own list before
  // any label is read; "backlog" is the floor's one stage no label answers for.
  ["business-view.js", LINE_KEY, { asked: true, drawn: ["shipped"] }],
  ["game-data.js", FLOOR_STAGES, { asked: false, drawn: ["shipped", "backlog"] }],
];

test("each page names every step of the table it asks the table for", () => {
  // A step the table gains and a page does not name would be an unmapped row:
  // those tickets would be drawn as no stage at all, quietly. A step a page
  // names and does not ask for is its own stage, not the table's.
  for (const [page, names, { asked, drawn }] of PAGES) {
    const want = STAGE_ORDER.filter((step) => asked || step !== "planning");
    const need = want.filter((step) => !drawn.includes(step));
    const have = Object.keys(names).filter((step) => !drawn.includes(step));
    assert.deepEqual(have.sort(), [...need].sort(), page + " names the steps it reads");
  }
});
