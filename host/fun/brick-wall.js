// The wall of Buzzword Buster: which words a round shows and the shape of the
// block each one is written on. See DESIGN.md, "Breakroom".
//
// build() has no side effects and draws nothing, so it is tested without a
// browser (test/brick_wall_test.mjs). game-view.js draws what it returns.
//
// The wall is laid on a grid of cells. A block is a row of cells wide enough
// for its word, and most blocks grow one or two cells above or below that row,
// as the pieces of a falling-block game do. Every round shuffles the words,
// the shapes and the places. Blocks that touch never have the same colour, so
// each shape can be told from its neighbours. The wall is never full: it stops at a fixed
// number of blocks and of cells, however many words there are, and the words
// left out wait for another round.
(() => {
  const COLS = 14, ROWS = 6;
  const CELL_W = 32, CELL_H = 24, GAP = 4;
  const LEFT = 16, TOP = 24;
  const MAX_BLOCKS = 12;
  const MAX_CELLS = COLS * ROWS / 2;
  const COLORS = ["#fc6", "#ea3", "#7cb", "#ccd"];
  // A letter is 8 pixels wide, and a word keeps 4 pixels clear at each end.
  const cellsFor = (text) => Math.max(2, Math.ceil((text.length * 8 + 8 + GAP) / CELL_W));

  function shuffle(list, random) {
    for (let i = list.length - 1; i > 0; i--) {
      const j = Math.floor(random() * (i + 1));
      [list[i], list[j]] = [list[j], list[i]];
    }
    return list;
  }

  // The cells a block of n cells in a row may grow: [column, row above (-1) or
  // below (1)]. An L, a J, a T, an S, a Z and a thick end, and their mirrors.
  function grown(n) {
    const end = n - 1, shapes = [];
    for (const row of [-1, 1]) {
      shapes.push([[0, row]], [[end, row]], [[0, row], [end, -row]]);
      shapes.push([[0, row], [1, row]], [[end - 1, row], [end, row]]);
      for (let c = 1; c < end; c++) shapes.push([[c, row]]);
    }
    return shapes;
  }

  // The first free place for a shape, from the top row down, so the wall hangs
  // from the top of the screen and the rows near the paddle stay clear. A
  // place counts only if a colour is left that no block beside it has.
  // taken: the colour of every cell in use, by "column,row".
  function place(taken, n, extra, random) {
    const columns = shuffle(Array.from({ length: COLS - n + 1 }, (_, c) => c), random);
    for (let row = 0; row < ROWS; row++) {
      for (const col of columns) {
        const cells = [];
        for (let c = 0; c < n; c++) cells.push([col + c, row]);
        for (const [c, r] of extra) cells.push([col + c, row + r]);
        if (!cells.every(([c, r]) => r >= 0 && r < ROWS && !taken.has(c + "," + r))) continue;
        const beside = new Set(cells.flatMap(([c, r]) =>
          [[c - 1, r], [c + 1, r], [c, r - 1], [c, r + 1]].map(([x, y]) => taken.get(x + "," + y))));
        const free = COLORS.filter((color) => !beside.has(color));
        if (free.length) return { col, row, cells, color: free[Math.floor(random() * free.length)] };
      }
    }
    return null;
  }

  // words: [{ word, count }]. Returns the blocks of one round, each with its
  // word, its colour, the cells it takes and the rectangles to draw and to
  // hit: the first is the row the word is written on.
  function build(words, random = Math.random) {
    const taken = new Map(), blocks = [];
    let used = 0;
    for (const word of shuffle(words.slice(), random)) {
      if (blocks.length >= MAX_BLOCKS) break;
      const least = cellsFor(String(word.word));
      if (least > COLS) continue;
      // Wider than it has to be now and then, and one grown shape picked by
      // chance. If neither has room, the plain row of the least width.
      const wide = Math.min(COLS, least + (random() < 0.3 ? 1 : 0));
      const shapes = grown(wide);
      const tries = [[wide, random() < 0.25 ? [] : shapes[Math.floor(random() * shapes.length)]], [wide, []], [least, []]];
      for (const [n, extra] of tries) {
        if (used + n + extra.length > MAX_CELLS) continue;
        const spot = place(taken, n, extra, random);
        if (!spot) continue;
        for (const [c, r] of spot.cells) taken.set(c + "," + r, spot.color);
        used += spot.cells.length;
        const x = LEFT + spot.col * CELL_W, y = TOP + spot.row * CELL_H;
        const rects = [{ x, y, w: n * CELL_W - GAP, h: CELL_H - GAP }];
        // A grown cell reaches across the gap to the row it grows from.
        for (const [c, r] of extra) {
          rects.push({ x: x + c * CELL_W, y: r < 0 ? y - CELL_H : y + CELL_H - GAP, w: CELL_W - GAP, h: CELL_H });
        }
        blocks.push({ word, color: spot.color, cells: spot.cells, rects });
        break;
      }
    }
    return blocks;
  }

  globalThis.TEHAS_BRICK_WALL = { build, COLS, ROWS, CELL_W, CELL_H, GAP, LEFT, TOP, MAX_BLOCKS, MAX_CELLS };
})();
