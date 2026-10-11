// Tests build() of the Buzzword Buster wall: the pure step from the words to
// the tetrominoes of one round.
//
//   node --test test/brick_wall_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import "../host/fun/brick-wall.js";

const { build, COLS, ROWS, CELL, GAP, LEFT, TOP, MAX_WORDS, MAX_PIECES, MAX_LETTERS, COLORS } = globalThis.TEHAS_BRICK_WALL;

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
const piecesFor = (letters) => Math.ceil(letters / 4);

// The seven tetrominoes and their rotations, written out here rather than read
// from the wall, so the test cannot agree with a wrong implementation.
const TETROMINOES = (() => {
  const base = {
    I: [[0, 0], [1, 0], [2, 0], [3, 0]],
    O: [[0, 0], [1, 0], [0, 1], [1, 1]],
    T: [[0, 0], [1, 0], [2, 0], [1, 1]],
    S: [[1, 0], [2, 0], [0, 1], [1, 1]],
    Z: [[0, 0], [1, 0], [1, 1], [2, 1]],
    J: [[0, 0], [0, 1], [1, 1], [2, 1]],
    L: [[2, 0], [0, 1], [1, 1], [2, 1]],
  };
  const shapes = new Map();
  for (const [name, cells] of Object.entries(base)) {
    const turn = (list) => {
      const rows = Math.max(...list.map(([, r]) => r)) + 1;
      return list.map(([c, r]) => [rows - 1 - r, c]);
    };
    for (let rot = cells; ; rot = turn(rot)) {
      const key = shape(rot);
      if (shapes.has(key)) break;
      shapes.set(key, name);
    }
  }
  return shapes;
})();

// The cells of a piece as a shape: counted from its own top left corner.
function shape(cells) {
  const left = Math.min(...cells.map(([c]) => c));
  const top = Math.min(...cells.map(([, r]) => r));
  return JSON.stringify(cells.map(([c, r]) => [c - left, r - top]).sort((a, b) => a[1] - b[1] || a[0] - b[0]));
}
// The pieces of one word, in the order they were numbered.
function byWord(wall) {
  const map = new Map();
  for (const piece of wall) map.set(piece.wordKey, [...(map.get(piece.wordKey) || []), piece]);
  for (const pieces of map.values()) pieces.sort((a, b) => a.piece - b.piece);
  return map;
}

test("every piece is one of the seven tetrominoes, in some rotation", () => {
  const seen = new Set();
  for (const seed of SEEDS) {
    for (const piece of build(pool(40), seeded(seed))) {
      assert.equal(piece.cells.length, 4, `${piece.word.word} has ${piece.cells.length} cells`);
      assert.equal(new Set(piece.cells.map(String)).size, 4, "a piece covers one cell twice");
      const at = shape(piece.cells);
      assert.ok(TETROMINOES.has(at), `not a tetromino: ${at}`);
      assert.equal(TETROMINOES.get(at), piece.shape, `${piece.shape} is not the shape of its cells`);
      seen.add(piece.shape);
    }
  }
  assert.deepEqual([...seen].sort(), ["I", "J", "L", "O", "S", "T", "Z"], "not all seven shapes turn up");
});

test("no two pieces share a cell, and every cell is on the grid", () => {
  for (const seed of SEEDS) {
    const cells = build(pool(40), seeded(seed)).flatMap((p) => p.cells);
    assert.equal(new Set(cells.map(String)).size, cells.length, "two pieces on one cell");
    for (const [c, r] of cells) assert.ok(c >= 0 && c < COLS && r >= 0 && r < ROWS, `${c},${r} is off the grid`);
  }
});

test("the wall hangs from the top with no holes, and is a rectangle when the words fill it", () => {
  for (const seed of SEEDS) {
    const wall = build(pool(40), seeded(seed));
    const at = new Set(wall.flatMap((p) => p.cells).map(String));
    for (const cell of at) {
      const [c, r] = cell.split(",").map(Number);
      if (r > 0) assert.ok(at.has(c + "," + (r - 1)), `empty ${c},${r - 1} above a cell in use`);
    }
    // Words enough for the wall: every column filled to the same row, and every
    // cell of it part of a piece.
    const heights = new Map();
    for (const cell of at) {
      const [c, r] = cell.split(",").map(Number);
      heights.set(c, Math.max(heights.get(c) || 0, r + 1));
    }
    assert.equal(new Set(heights.values()).size, 1, `columns end at different rows: ${[...heights.values()]}`);
    assert.equal(at.size, wall.length * 4);
  }
});

test("a word takes ceil(letters / 4) pieces, joined edge to edge in one colour", () => {
  for (const seed of SEEDS.slice(0, 120)) {
    const wall = build(pool(40), seeded(seed));
    for (const pieces of byWord(wall).values()) {
      const word = pieces[0].word.word;
      assert.equal(pieces.length, piecesFor(word.length), `${word} is ${pieces.length} pieces`);
      assert.deepEqual(pieces.map((p) => p.piece), pieces.map((_, i) => i), "the pieces are not numbered in order");
      assert.equal(new Set(pieces.map((p) => p.color)).size, 1, `${word} is written in more than one colour`);
      for (let i = 1; i < pieces.length; i++) {
        const own = new Set(pieces.slice(0, i).flatMap((p) => p.cells.map(String)));
        assert.ok(pieces[i].cells.some(([c, r]) =>
          own.has((c - 1) + "," + r) || own.has((c + 1) + "," + r) || own.has(c + "," + (r - 1)) || own.has(c + "," + (r + 1))),
          `${word} piece ${i} touches no piece of its word`);
      }
    }
  }
});

test("the cells of a word spell it, in order, with blanks only at the end", () => {
  for (const seed of SEEDS.slice(0, 120)) {
    for (const pieces of byWord(build(pool(40), seeded(seed))).values()) {
      const word = pieces[0].word.word;
      const read = pieces.flatMap((p) => p.letters);
      assert.equal(read.length, pieces.length * 4, `${word} has ${read.length} cells for its letters`);
      assert.deepEqual(read.slice(0, word.length), [...word], `the cells of ${word} do not read ${word}`);
      assert.ok(read.slice(word.length).every((letter) => letter === null), `${word} has letters past its end`);
      for (const piece of pieces) {
        assert.equal(piece.letters.length, 4);
        const filled = piece.letters.filter((l) => l !== null);
        assert.deepEqual(piece.letters.slice(0, filled.length), filled, `${word} has a blank in the middle of a piece`);
      }
    }
  }
});

test("pieces of different words that touch have different colours", () => {
  let colours = new Set();
  for (const seed of SEEDS) {
    const wall = build(pool(40), seeded(seed));
    const at = new Map();
    for (const piece of wall) {
      assert.match(piece.color, /^#[0-9a-f]{3}$/);
      assert.ok(COLORS.includes(piece.color), `${piece.color} is not in the palette`);
      for (const cell of piece.cells) at.set(String(cell), piece);
    }
    for (const [cell, piece] of at) {
      const [c, r] = cell.split(",").map(Number);
      for (const [x, y] of [[c - 1, r], [c + 1, r], [c, r - 1], [c, r + 1]]) {
        const other = at.get(x + "," + y);
        if (other && other.wordKey !== piece.wordKey) {
          assert.notEqual(other.color, piece.color, `seed ${seed}: ${piece.word.word} and ${other.word.word} share ${piece.color}`);
        }
      }
    }
    colours = new Set([...colours, ...wall.map((p) => p.color)]);
  }
  assert.equal(colours.size, COLORS.length, "the wall does not use the palette");
});

test("every round shuffles the words, the shapes and the places", () => {
  const words = pool(40);
  const round = (seed) => build(words, seeded(seed));
  const told = (wall) => JSON.stringify(wall.map((p) => [p.word.word, p.shape, p.cells, p.color]));
  assert.equal(told(round(7)), told(round(7)), "the same seed gives a different wall");
  assert.equal(new Set(SEEDS.map((s) => told(round(s)))).size, SEEDS.length, "different seeds give the same wall");
  // Which words are on the wall changes, not only where they are.
  const picked = new Set(SEEDS.flatMap((s) => round(s).map((p) => p.word.word)));
  assert.equal(picked.size, new Set(words.map((w) => w.word)).size, "some word is never on the wall");
  // In one round too: not a wall of one shape.
  for (const seed of SEEDS.slice(0, 50)) {
    assert.ok(new Set(round(seed).map((p) => p.shape)).size >= 3, `seed ${seed} is nearly one shape`);
  }
  // With no seed given it is the page's own chance.
  assert.notEqual(told(build(words)), told(build(words)), "no seed and still the same wall");
  // The list it was given is left as it was.
  assert.deepEqual(words, pool(40), "the words handed over were changed");
});

test("the wall keeps its size however many words there are", () => {
  for (const n of [12, 13, 25, 100, 1000]) {
    for (const seed of SEEDS.slice(0, 60)) {
      const wall = build(pool(n), seeded(seed));
      const cells = wall.flatMap((p) => p.cells);
      assert.ok(wall.length <= MAX_PIECES, `${wall.length} pieces from ${n} words`);
      assert.ok(new Set(wall.map((p) => p.wordKey)).size <= MAX_WORDS, `${new Set(wall.map((p) => p.wordKey)).size} words from ${n}`);
      assert.ok(cells.length <= COLS * ROWS, "the wall is bigger than the grid");
      const cols = new Set(cells.map(([c]) => c)).size;
      const rows = new Set(cells.map(([, r]) => r)).size;
      assert.ok(cols <= COLS && rows <= ROWS, `a wall of ${cols} by ${rows}`);
    }
  }
  assert.equal(MAX_PIECES, COLS * ROWS / 4);
  // Not every round fills all sixteen columns: words that do not close a wall
  // that wide settle for a narrower one, which is what host/docs/tools.md says.
  const widths = new Set(SEEDS.map((seed) => new Set(build(pool(40), seeded(seed)).flatMap((p) => p.cells.map(([c]) => c))).size));
  assert.ok(widths.size > 1, `every round is the same width: ${[...widths]}`);
});

test("a few words are all placed, one word is a wall, and none is no wall", () => {
  for (const seed of SEEDS.slice(0, 50)) {
    for (const n of [1, 2, 5]) {
      const wall = build(pool(n), seeded(seed));
      assert.equal(new Set(wall.map((p) => p.wordKey)).size, n, `${n} words gave ${new Set(wall.map((p) => p.wordKey)).size} of them a place`);
      for (const pieces of byWord(wall).values()) assert.equal(pieces.length, piecesFor(pieces[0].word.word.length));
      const at = new Set(wall.flatMap((p) => p.cells).map(String));
      for (const cell of at) {
        const [c, r] = cell.split(",").map(Number);
        if (r > 0) assert.ok(at.has(c + "," + (r - 1)), `hole in a wall of ${n} words`);
      }
    }
  }
  assert.deepEqual(build([]), [], "no words and still a wall");
  assert.deepEqual(build(undefined), [], "no list at all and still a wall");
});

test("a word longer than the wall writes is left out, not packed past it", () => {
  // The page cuts every word to MAX_LETTERS (game-view.js). build holds that
  // line whatever it is handed: a word no wall was designed for goes on its own,
  // and does not empty a round of the words that do fit.
  assert.deepEqual(build([{ word: "x".repeat(200), count: 1 }]), [], "a word too long made a wall");
  for (const seed of SEEDS.slice(0, 40)) {
    const wall = build([{ word: "x".repeat(200), count: 9 }, ...pool(20)], seeded(seed));
    assert.ok(wall.length, `seed ${seed}: a long word left the round with no wall`);
    for (const piece of wall) assert.ok(piece.word.word.length <= MAX_LETTERS, `${piece.word.word} is longer than the wall writes`);
    for (const pieces of byWord(wall).values()) assert.equal(pieces.length, piecesFor(pieces[0].word.word.length));
  }
});

test("every cell has a square to draw and to hit, on the grid it is on", () => {
  for (const seed of SEEDS.slice(0, 60)) {
    for (const piece of build(pool(40), seeded(seed))) {
      assert.equal(piece.rects.length, 4, `${piece.word.word} draws ${piece.rects.length} squares for 4 cells`);
      for (const [i, cell] of piece.cells.entries()) {
        const [c, r] = cell;
        const rect = piece.rects[i];
        assert.deepEqual(rect, { x: LEFT + c * CELL, y: TOP + r * CELL, w: CELL - GAP, h: CELL - GAP });
        assert.equal(rect.w, rect.h, "a cell is not a square");
        assert.ok(rect.x >= LEFT && rect.x + rect.w <= LEFT + COLS * CELL - GAP, "outside the wall sideways");
        assert.ok(rect.y >= TOP && rect.y + rect.h <= TOP + ROWS * CELL - GAP, "outside the wall up or down");
      }
    }
  }
});
