// Tests the fold arithmetic of the game view: the room the first screen gets
// above it, the gap before the board, and the height of the monitor beside it.
// Expected values are literals. An assertion written from the same formula as
// the code under test is true whatever that formula says.
//
//   node --test test/fold_fit_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import "../host/fun/fold-fit.js";

const { fit, monitorHeight } = globalThis.TEHAS_FOLD_FIT;

// A wide screen as the page lays it out: 88px of header, the room the fold keeps
// above the head panel (32px, the offset fitFold() measures at), the panel, then
// the stations and the doors under them. head is what the panel costs, so a
// taller panel moves the doors down with nothing else changed.
const lineTop = (head) => 88 + 32 + head;
const doorsBottom = (head) => lineTop(head) + 360 + 200;

// Where everything lands for one window and one panel. Room over the baseline
// pushes the doors down by the same amount, so what hangs off the doors is
// measured from there.
const place = (innerHeight, head) => {
  const bottom = doorsBottom(head);
  const { foldPad, foldGap } = fit(innerHeight, bottom);
  const moved = bottom + foldPad - 32;
  return { foldPad, foldGap, above: foldPad, below: innerHeight - moved, boardTop: moved + foldGap };
};

test("the offsets for a screen as the page lays it out are these numbers", () => {
  assert.equal(doorsBottom(190), 870);
  assert.equal(doorsBottom(320), 1000);
  // A screen taller than the window, and one that fills it exactly: no give.
  assert.deepEqual(fit(720, 870), { foldPad: 32, foldGap: 72 });
  assert.deepEqual(fit(902, 870), { foldPad: 32, foldGap: 72 });
  // Room over: half of it goes above the first screen, and the board keeps the
  // rest as air under the fold. The last case is where the give reaches its cap.
  assert.deepEqual(fit(918, 870), { foldPad: 40, foldGap: 72 });
  assert.deepEqual(fit(950, 870), { foldPad: 56, foldGap: 80 });
  assert.deepEqual(fit(1000, 870), { foldPad: 80, foldGap: 106 });
  assert.deepEqual(fit(1110, 870), { foldPad: 136, foldGap: 160 });
  assert.deepEqual(fit(1400, 870), { foldPad: 232, foldGap: 354 });
});

test("the first screen is centred: as much room below it as above it", () => {
  // Only windows where the give has not reached its cap. The spare is read on a
  // 16px grid and half of it goes above, so the two sides differ by less than
  // one 16px step, and never by giving the whole spare to the top.
  for (const head of [150, 190, 320]) {
    for (let innerHeight = doorsBottom(head) + 32; innerHeight <= 1200; innerHeight += 5) {
      const p = place(innerHeight, head);
      assert.ok(p.below >= p.above, `${p.above} above and only ${p.below} below in a ${innerHeight} window`);
      assert.ok(p.below - p.above < 16, `the sides differ by ${p.below - p.above}: the spare is not split`);
    }
  }
});

test("room over the first screen comes in steps of 8px and stops at 232px", () => {
  for (let innerHeight = 600; innerHeight <= 3000; innerHeight += 7) {
    const p = place(innerHeight, 190);
    assert.ok(p.foldPad >= 32 && p.foldPad <= 232, `${p.foldPad} of room above the first screen`);
    assert.equal((p.foldPad - 32) % 8, 0, `${p.foldPad} is off the 8px grid`);
    assert.ok(p.foldGap >= 72, `only ${p.foldGap} before the board`);
  }
});

test("the board starts under the fold with room to spare", () => {
  // Where the board lands when it gets the window plus the air under the fold.
  assert.equal(place(1000, 190).boardTop, 1024);
  assert.equal(place(1110, 190).boardTop, 1134);
  assert.equal(place(1400, 190).boardTop, 1424);
  assert.equal(place(1200, 320).boardTop, 1224);
  for (let innerHeight = 600; innerHeight <= 2400; innerHeight += 3) {
    for (const head of [150, 190, 320]) {
      const p = place(innerHeight, head);
      assert.ok(p.boardTop >= innerHeight + 24, `the board starts at ${p.boardTop} in a ${innerHeight} window`);
    }
  }
});

test("a taller head panel takes room off the fold and never adds any", () => {
  const innerHeight = 1400;
  let pad = null;
  for (const head of [150, 190, 230, 300, 420]) {
    const p = place(innerHeight, head);
    if (pad !== null) assert.ok(p.foldPad <= pad, `${head}px of panel gave ${p.foldPad} after ${pad}`);
    pad = p.foldPad;
  }
});

test("the monitor stands as tall as the stations and the doors beside it", () => {
  // The stations and the doors together stand 560px, whatever the panel above.
  for (const head of [150, 190, 320]) {
    assert.equal(monitorHeight(lineTop(head), doorsBottom(head)), 528);
  }
  assert.equal(monitorHeight(310, 870), 528);
  // An offset taller than the row leaves the monitor with nothing to stand on.
  assert.equal(monitorHeight(860, 870), 0);
  assert.equal(monitorHeight(900, 870), 0);
});
