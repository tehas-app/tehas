// Tests build() of the Buzzword Buster wall: the pure step from the words to
// the blocks of one round.
//
//   node --test test/brick_wall_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import "../host/fun/brick-wall.js";

const { build, COLS, ROWS, CELL_W, CELL_H, GAP, LEFT, TOP, MAX_BLOCKS, MAX_CELLS } = globalThis.TEHAS_BRICK_WALL;

// The same numbers every run: a round here is named by its seed.
function seeded(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const SEEDS = Array.from({ length: 300 }, (_, i) => i + 1);
// Words of 3 to 10 letters, the lengths the page hands over.
const pool = (n) => Array.from({ length: n }, (_, i) => ({ word: "w".repeat(2 + (i % 8)) + i % 10, count: n - i }));
// The shape of a block whatever its place: its cells, counted from its first.
const shape = (b) => JSON.stringify(b.cells.map(([c, r]) => [c - b.cells[0][0], r - b.cells[0][1]]));

test("the wall is never overloaded, however many words there are", () => {
  for (const n of [12, 25, 100, 1000]) {
    for (const seed of SEEDS.slice(0, 60)) {
      const wall = build(pool(n), seeded(seed));
      assert.ok(wall.length <= MAX_BLOCKS, `${wall.length} blocks`);
      assert.ok(wall.flatMap((b) => b.cells).length <= MAX_CELLS, "more than half the cells");
      assert.ok(wall.length >= 8, `only ${wall.length} blocks from ${n} words`);
    }
  }
  assert.equal(MAX_CELLS, COLS * ROWS / 2);
});

test("no two blocks share a cell, and every cell is on the grid", () => {
  for (const seed of SEEDS) {
    const cells = build(pool(40), seeded(seed)).flatMap((b) => b.cells);
    assert.equal(new Set(cells.map(String)).size, cells.length);
    for (const [c, r] of cells) assert.ok(c >= 0 && c < COLS && r >= 0 && r < ROWS, `${c},${r}`);
  }
});

test("a block is drawn where its cells are, in one piece, with room for its word", () => {
  for (const seed of SEEDS) {
    for (const b of build(pool(40), seeded(seed))) {
      const [row, ...grown] = b.rects;
      // The word, 8 pixels a letter, with 4 clear at each end.
      assert.ok(b.word.word.length * 8 + 8 <= row.w, `${b.word.word} in ${row.w}`);
      assert.equal(b.rects.length, 1 + b.cells.filter(([, r]) => r !== b.cells[0][1]).length);
      for (const r of b.rects) {
        assert.ok(r.x >= LEFT && r.x + r.w <= LEFT + COLS * CELL_W - GAP, "outside the wall sideways");
        assert.ok(r.y >= TOP && r.y + r.h <= TOP + ROWS * CELL_H - GAP, "outside the wall up or down");
        assert.equal([r.x, r.y, r.w, r.h].every((v) => v % 4 === 0), true);
      }
      // A grown cell touches the row above or below it and stays in its columns.
      for (const g of grown) {
        assert.ok(g.y + g.h === row.y || g.y === row.y + row.h, "a gap inside a block");
        assert.ok(g.x >= row.x && g.x + g.w <= row.x + row.w);
      }
      // One rectangle per cell: the row covers its cells, each grown one its own.
      const drawn = new Set(b.rects.flatMap((r) => {
        const cols = Math.round((r.w + GAP) / CELL_W);
        const top = Math.round((r.y + (r.h > CELL_H - GAP && r.y < row.y ? 0 : r.h - (CELL_H - GAP)) - TOP) / CELL_H);
        return Array.from({ length: cols }, (_, i) => `${(r.x - LEFT) / CELL_W + i},${top}`);
      }));
      assert.deepEqual([...drawn].sort(), b.cells.map(String).sort());
    }
  }
});

test("blocks vary in width and in shape", () => {
  const blocks = SEEDS.flatMap((seed) => build(pool(40), seeded(seed)));
  const widths = new Set(blocks.map((b) => b.rects[0].w));
  const shapes = new Set(blocks.map(shape));
  assert.deepEqual([...widths].sort((a, b) => a - b), [2, 3, 4].map((n) => n * CELL_W - GAP));
  assert.ok(shapes.size >= 20, `${shapes.size} shapes`);
  // Plain rows and grown blocks both, and blocks grown up, down and both ways.
  const rows = (b) => new Set(b.cells.map(([, r]) => r - b.cells[0][1]));
  assert.ok(blocks.some((b) => b.rects.length === 1));
  for (const want of [[-1, 0], [0, 1], [-1, 0, 1]]) {
    assert.ok(blocks.some((b) => String([...rows(b)].sort()) === String(want)), `no block over rows ${want}`);
  }
  // In one round too: not twelve of a kind.
  for (const seed of SEEDS.slice(0, 50)) {
    assert.ok(new Set(build(pool(40), seeded(seed)).map(shape)).size >= 3);
  }
});

test("every round shuffles the words, the shapes and the places", () => {
  const words = pool(40);
  const round = (seed) => build(words, seeded(seed));
  const told = (wall) => JSON.stringify(wall.map((b) => [b.word.word, b.cells]));
  assert.equal(told(round(7)), told(round(7)));
  assert.equal(new Set(SEEDS.map((s) => told(round(s)))).size, SEEDS.length);
  // Which words are on the wall changes, not only their order.
  const picked = new Set(SEEDS.flatMap((s) => round(s).map((b) => b.word.word)));
  assert.equal(picked.size, new Set(words.map((w) => w.word)).size);
  // With no seed given it is the page's own chance.
  assert.notEqual(told(build(words)), told(build(words)));
  // The list it was given is left as it was.
  assert.deepEqual(words, pool(40));
});

test("blocks that touch have different colours", () => {
  for (const seed of SEEDS) {
    const wall = build(pool(40), seeded(seed));
    const colour = new Map(wall.flatMap((b, i) => b.cells.map((cell) => [String(cell), [i, b.color]])));
    for (const [i, b] of wall.entries()) {
      for (const [c, r] of b.cells) {
        for (const beside of [[c - 1, r], [c + 1, r], [c, r - 1], [c, r + 1]]) {
          const other = colour.get(String(beside));
          if (other && other[0] !== i) assert.notEqual(other[1], b.color, `seed ${seed}: ${b.word.word} at ${c},${r}`);
        }
      }
    }
    assert.ok(new Set(wall.map((b) => b.color)).size >= 3, "a wall of one or two colours");
  }
});

test("each word is on the wall once, with its count, in a colour of the palette", () => {
  for (const seed of SEEDS.slice(0, 50)) {
    const words = pool(40), wall = build(words, seeded(seed));
    assert.equal(new Set(wall.map((b) => b.word)).size, wall.length);
    for (const b of wall) assert.ok(words.some((w) => w.word === b.word.word && w.count === b.word.count));
    for (const b of wall) assert.match(b.color, /^#[0-9a-f]{3}$/);
  }
});

test("a few words are all placed, and none is no wall", () => {
  for (const seed of SEEDS.slice(0, 50)) {
    assert.equal(build(pool(5), seeded(seed)).length, 5);
    assert.equal(build(pool(1), seeded(seed)).length, 1);
  }
  assert.deepEqual(build([]), []);
  // A word too long for the wall is left out, not drawn past its edge.
  assert.deepEqual(build([{ word: "x".repeat(200), count: 1 }]), []);
});
