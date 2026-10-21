// The wall of Buzzword Buster: which words a round shows and the tetrominoes
// they are written on. See DESIGN.md, "Breakroom".
//
// build() has no side effects and draws nothing, so it is tested without a
// browser (test/brick_wall_test.mjs). game-view.js draws what it returns.
//
// The wall is a grid of square cells, one letter in each. A word takes a run of
// tetrominoes: one of the seven pieces for three or four letters, two for five
// to eight, three for nine or ten. A word longer than MAX_LETTERS is left out of
// the round, as the old wall left out a word too wide to write. The pieces of a
// word touch one another and share a colour; pieces of words that touch do not.
// The wall hangs from the top, so in every column the cells in use run from the
// top row down with no gap, as a well played stack does.
//
// A round picks the words first and the wall after them: at most MAX_WORDS of
// them, and the largest wall (columns by rows) whose pieces those words make up
// exactly, capped at MAX_PIECES pieces. With words enough the wall is therefore
// a rectangle with no holes, and it keeps that size however many words the page
// hands over. With a small pool every word goes on the wall and the lower edge
// may be uneven.
//
// build(words, random) returns one entry per piece:
//
//   word     the { word, count } it belongs to, as the page handed it over
//   wordKey  the same number for every piece of one word, another for each other
//   piece    which piece of its word this is, counting from 0
//   shape    the tetromino: "I", "O", "T", "S", "Z", "J" or "L"
//   color    the colour of its word, shared by every piece of it
//   cells    the four [column, row] cells, left to right and top to bottom
//   letters  the letter of each cell in that order, null where the word runs out
//   rects    one square per cell, in the order of the cells: { x, y, w, h }
//
// The letters of a word run through its pieces in order: the cells of one piece
// left to right and top to bottom, then the next piece. The last piece of a
// word of five, six, seven or ten letters has blank cells in it.
(() => {
  const COLS = 16, ROWS = 6;
  const CELL = 28, GAP = 4;
  const LEFT = 16, TOP = 24;
  const MAX_WORDS = 12;
  const MAX_PIECES = COLS * ROWS / 4;
  // Longest word a round writes. The page hands over no longer one (game-view.js
  // cuts every word to ten letters); taking the same limit here keeps a word that
  // no wall was designed for from emptying a whole round.
  const MAX_LETTERS = 10;
  // Theme tokens, painted by game-view.js through theme.js.
  const COLORS = ["piece-1", "piece-2", "piece-3", "piece-4"];
  // How much rougher than the flattest place a piece may take, in cells. Zero
  // leaves only the two flat shapes; one lets the other five in too, and is as
  // rough as the packer can be and still fill a wall on its own: at two or three
  // the lay fails often enough to leave a round empty. The flat shapes still win
  // most places, because a column-solid wall gives a T, an S or a Z a legal spot
  // about one time in ten.
  const SLOP = 1;

  const piecesFor = (letters) => Math.ceil(letters / 4);

  function shuffle(list, random) {
    for (let i = list.length - 1; i > 0; i--) {
      const j = Math.floor(random() * (i + 1));
      [list[i], list[j]] = [list[j], list[i]];
    }
    return list;
  }

  // The seven tetrominoes in each of their rotations: cells in reading order
  // from the piece's top left corner, its width, and for every column it covers
  // the first row and how many cells. In a tetromino the cells of one column are
  // always a run, which is all the placement needs to know about a column.
  const ROTATIONS = (() => {
    const base = {
      I: [[0, 0], [1, 0], [2, 0], [3, 0]],
      O: [[0, 0], [1, 0], [0, 1], [1, 1]],
      T: [[0, 0], [1, 0], [2, 0], [1, 1]],
      S: [[1, 0], [2, 0], [0, 1], [1, 1]],
      Z: [[0, 0], [1, 0], [1, 1], [2, 1]],
      J: [[0, 0], [0, 1], [1, 1], [2, 1]],
      L: [[2, 0], [0, 1], [1, 1], [2, 1]],
    };
    const turn = (cells) => {
      const rows = Math.max(...cells.map(([, r]) => r)) + 1;
      return cells.map(([c, r]) => [rows - 1 - r, c]);
    };
    const reading = (cells) => {
      const top = Math.min(...cells.map(([, r]) => r));
      const left = Math.min(...cells.map(([c]) => c));
      return cells.map(([c, r]) => [c - left, r - top]).sort((a, b) => a[1] - b[1] || a[0] - b[0]);
    };
    const out = [];
    for (const [name, cells] of Object.entries(base)) {
      const seen = new Set();
      for (let rot = cells; ; rot = turn(rot)) {
        const at = reading(rot);
        if (seen.has(JSON.stringify(at))) break;
        seen.add(JSON.stringify(at));
        const width = Math.max(...at.map(([c]) => c)) + 1;
        const columns = Array.from({ length: width }, (_, c) => {
          const rows = at.filter(([x]) => x === c).map(([, r]) => r);
          return { first: Math.min(...rows), count: rows.length };
        });
        out.push({ name, columns, width });
      }
    }
    return out;
  })();

  // The wall a given number of pieces fills exactly: as wide as the grid allows,
  // and no taller than it. null when those pieces make no such rectangle.
  function fit(pieces) {
    for (let cols = Math.min(COLS, pieces * 4); cols >= 4; cols--) {
      const rows = (pieces * 4) / cols;
      if (rows >= 2 && rows <= ROWS && Number.isInteger(rows)) return { cols, rows };
    }
    return null;
  }

  // The words of a round, and the wall they fill.
  //
  // The pool is shuffled and read from the front: at most MAX_WORDS words, each
  // one as it comes, until the wall is full at MAX_PIECES pieces. The wall is the
  // biggest one those words close exactly, and words later in the shuffle are
  // then added back while the wall stays closed, so a run that overshoots a
  // corner by one or two pieces loses as few words as it can. A pool small enough
  // for the wall is placed whole, and its wall may end uneven.
  function choose(pool, random) {
    const words = shuffle(pool.slice(), random);
    const need = words.map((w) => piecesFor(w.word.length));
    const whole = words.length <= MAX_WORDS && need.reduce((a, b) => a + b, 0) <= MAX_PIECES;
    if (whole) return { words, wall: { cols: COLS, rows: ROWS } };

    let best = { at: [], wall: null, pieces: 0 }, run = [], pieces = 0;
    for (let i = 0; i < words.length && run.length < MAX_WORDS && pieces + need[i] <= MAX_PIECES; i++) {
      run.push(i);
      pieces += need[i];
      const wall = fit(pieces);
      if (wall && pieces > best.pieces) best = { at: run.slice(), wall, pieces };
    }
    if (!best.wall) return { words: run.map((i) => words[i]), wall: { cols: COLS, rows: ROWS } };

    const taken = new Set(best.at);
    for (let i = 0; i < words.length && best.at.length < MAX_WORDS && best.pieces < MAX_PIECES; i++) {
      if (taken.has(i)) continue;
      const wall = fit(best.pieces + need[i]);
      if (!wall) continue;
      best = { at: [...best.at, i], wall, pieces: best.pieces + need[i] };
      taken.add(i);
    }
    return { words: best.at.map((i) => words[i]), wall: best.wall };
  }

  // Lay the words out, one piece at a time, from the top of the wall down.
  // heights[c] is how far column c is filled: a piece may only sit on the fill
  // of every column it covers, which is what keeps a column solid from the top.
  // A piece of a word that is not the first must touch one already down. Of the
  // places left, each shape offers the flattest one it has, and one of those is
  // taken by chance: a wall laid from only the flattest places is all I and O,
  // so a shape may be up to SLOP cells rougher than the flattest of them all.
  // Returns null when no place is left for a piece.
  function lay(words, wall, random) {
    const start = Math.floor((COLS - wall.cols) / 2);
    const stop = start + wall.cols;
    const heights = new Array(COLS).fill(0);
    const used = new Map();
    const pieces = [];
    for (let wi = 0; wi < words.length; wi++) {
      const own = new Set();
      for (let p = 0; p < piecesFor(words[wi].word.length); p++) {
        const options = [];
        for (const rot of ROTATIONS) {
          for (let col = start; col + rot.width <= stop; col++) {
            const top = heights[col] - rot.columns[0].first;
            if (top < 0) continue;
            const cells = [];
            let fits = true;
            for (let dc = 0; dc < rot.width && fits; dc++) {
              const run = rot.columns[dc];
              if (heights[col + dc] !== top + run.first || heights[col + dc] + run.count > wall.rows) { fits = false; break; }
              for (let r = 0; r < run.count; r++) cells.push([col + dc, top + run.first + r]);
            }
            if (!fits) continue;
            let touch = 0;
            const added = new Map();
            for (const [c, r] of cells) {
              added.set(c, (added.get(c) || 0) + 1);
              if (own.has((c - 1) + "," + r) || own.has((c + 1) + "," + r) || own.has(c + "," + (r - 1)) || own.has(c + "," + (r + 1))) touch++;
            }
            let rough = 0, closed = 0;
            for (let c = start; c < stop; c++) {
              const after = heights[c] + (added.get(c) || 0);
              if (after === wall.rows) closed++;
              if (c > start) rough += Math.abs(after - (heights[c - 1] + (added.get(c - 1) || 0)));
            }
            options.push({ rot, cells, touch, closed, rough });
          }
        }
        const legal = p === 0 ? options : options.filter((o) => o.touch > 0);
        if (!legal.length) return null;
        // The shape first and by chance, then the flattest place it has left.
        // Only shapes within SLOP of the flattest are offered, so the fill still
        // goes down as one front and leaves no slot a tetromino cannot fit.
        const chosen = new Map();
        for (const option of legal) {
          const best = chosen.get(option.rot);
          if (!best || option.rough < best.rough || (option.rough === best.rough && option.closed < best.closed)) chosen.set(option.rot, option);
        }
        const flattest = Math.min(...[...chosen.values()].map((o) => o.rough));
        const picks = shuffle([...chosen.values()].filter((o) => o.rough <= flattest + SLOP), random);
        // Of those, the shape that has been used least this round: a wall of one
        // shape is not a wall of tetrominoes.
        picks.sort((a, b) => (used.get(a.rot.name) || 0) - (used.get(b.rot.name) || 0));
        const spot = picks[0];
        used.set(spot.rot.name, (used.get(spot.rot.name) || 0) + 1);
        for (const [c, r] of spot.cells) {
          heights[c] += 1;
          own.add(c + "," + r);
        }
        pieces.push({ word: words[wi], wordKey: wi, piece: p, shape: spot.rot.name, cells: spot.cells.sort((a, b) => a[1] - b[1] || a[0] - b[0]) });
      }
    }
    return pieces;
  }

  // A colour for every word: words that touch never share one, and a word's
  // pieces all do. The busiest word is coloured first and a cornered colour is
  // tried again with the next one, which is enough for any wall: the words are
  // connected regions of a grid, and those always take four colours.
  function colour(words, pieces, random) {
    const at = new Map();
    for (const piece of pieces) for (const [c, r] of piece.cells) at.set(c + "," + r, piece.wordKey);
    const beside = words.map(() => new Set());
    for (const [key, wi] of at) {
      const [c, r] = key.split(",").map(Number);
      for (const [x, y] of [[c + 1, r], [c, r + 1]]) {
        const other = at.get(x + "," + y);
        if (other !== undefined && other !== wi) { beside[wi].add(other); beside[other].add(wi); }
      }
    }
    const order = words.map((_, wi) => wi).sort((a, b) => beside[b].size - beside[a].size);
    const color = new Array(words.length).fill(null);
    const fill = (i) => {
      if (i === order.length) return true;
      const wi = order[i];
      const used = new Set([...beside[wi]].map((n) => color[n]));
      for (const c of shuffle(COLORS.slice(), random)) {
        if (used.has(c)) continue;
        color[wi] = c;
        if (fill(i + 1)) return true;
      }
      color[wi] = null;
      return false;
    };
    return fill(0) ? color : null;
  }

  // words: [{ word, count }]. Returns the pieces of one round, in the order they
  // were laid: the words themselves shuffled, and their pieces one after
  // another. A wall that will not pack is tried again, and then with a smaller
  // wall, so a round is never empty because the packer ran out of room. Words
  // the wall cannot write are left out before any of it.
  function build(words, random = Math.random) {
    const pool = (Array.isArray(words) ? words : [])
      .filter((w) => w && typeof w.word === "string" && w.word.length > 0 && w.word.length <= MAX_LETTERS);
    if (!pool.length) return [];
    // A wall that will not pack is tried again with other shapes and other
    // places; a round that still will not pack loses the word that would not
    // fit, and only then falls back to laying the words out free.
    const plans = [choose(pool, random)];
    for (let cut = 1; cut < plans[0].words.length && plans.length < 4; cut++) {
      const rest = plans[0].words.slice(0, plans[0].words.length - cut);
      const wall = fit(rest.reduce((a, w) => a + piecesFor(w.word.length), 0));
      if (wall) plans.push({ words: rest, wall });
    }
    plans.push({ words: pool.slice(0, MAX_WORDS), wall: { cols: COLS, rows: ROWS } });
    for (const plan of plans) {
      for (let tries = 0; tries < 6; tries++) {
        const pieces = lay(plan.words, plan.wall, random);
        if (!pieces) continue;
        const colors = colour(plan.words, pieces, random);
        if (!colors) continue;
        return pieces.map((piece) => ({
          ...piece,
          color: colors[piece.wordKey],
          letters: piece.cells.map((_, at) => piece.word.word[4 * piece.piece + at] ?? null),
          rects: piece.cells.map(([c, r]) => ({ x: LEFT + c * CELL, y: TOP + r * CELL, w: CELL - GAP, h: CELL - GAP })),
        }));
      }
    }
    return [];
  }

  globalThis.TEHAS_BRICK_WALL = {
    build, COLS, ROWS, CELL, GAP, LEFT, TOP, MAX_WORDS, MAX_PIECES, MAX_LETTERS, COLORS,
  };
})();
