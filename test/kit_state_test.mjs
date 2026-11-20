// host/fun/kit/state.js: named page stores shared by the islands of a page.
import "./kit_stub.mjs";
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { createRoot, createMemo, flush } from "../host/fun/kit/vendor/solid.js";
import { pageStore, resetPage, shared } from "../host/fun/kit/state.js";

const useBoard = pageStore("board", {
  state: () => ({ picked: null, seen: [] }),
  actions: (set) => ({
    pick(n) {
      set((d) => {
        d.picked = n;
        d.seen.push(n);
      });
    },
  }),
});

// An island: a root of its own that reads the store, as a render module does.
function island() {
  return createRoot((dispose) => {
    const board = useBoard();
    const picked = createMemo(() => board.state.picked);
    return { board, picked, dispose };
  });
}

beforeEach(() => resetPage());

test("two islands get the same store, and one's action is the other's read", () => {
  const a = island();
  const b = island();
  assert.equal(a.board, b.board);
  assert.equal(b.picked(), null);
  a.board.actions.pick(41);
  flush();
  assert.equal(b.picked(), 41);
  assert.deepEqual([...b.board.state.seen], [41]);
});

test("a write lands on the next microtask, or at flush()", async () => {
  const a = island();
  a.board.actions.pick(7);
  assert.equal(a.board.state.picked, null);
  await Promise.resolve();
  flush();
  assert.equal(a.board.state.picked, 7);
});

test("the store outlives the island that made it", () => {
  const a = island();
  const b = island();
  a.board.actions.pick(3);
  a.dispose();
  b.board.actions.pick(4);
  flush();
  assert.equal(b.picked(), 4);
  const c = island();
  assert.equal(c.board, b.board);
  assert.deepEqual([...c.board.state.seen], [3, 4]);
});

test("the store has no setter: actions are the only write", () => {
  const { board } = island();
  assert.deepEqual(Object.keys(board).sort(), ["actions", "state"]);
  assert.ok(Object.isFrozen(board) && Object.isFrozen(board.actions));
});

test("a second definition of a name with other actions is refused", () => {
  const same = pageStore("board", {
    state: () => ({ picked: 1 }),
    actions: () => ({ pick() {} }),
  });
  const other = pageStore("board", { state: () => ({}), actions: () => ({ drop() {} }) });
  island();
  createRoot(() => {
    assert.equal(same(), useBoard());
    assert.throws(() => other(), /defined twice/);
  });
});

test("a second definition whose actions read the state while made gets the store", () => {
  // The actions of both read the state as they are made, as a store that
  // keeps a count from its start does. The second one is compared with the
  // first by its actions' names, made against the store's own state.
  const spec = () => ({
    state: () => ({ items: [3] }),
    actions: (set, state) => {
      const start = state.items.length;
      return { add() { set((d) => { d.items.push(start); }); } };
    },
  });
  const one = pageStore("counted", spec());
  const two = pageStore("counted", spec());
  createRoot(() => {
    const store = one();
    assert.equal(two(), store);
    store.actions.add();
    flush();
    assert.deepEqual([...store.state.items], [3, 1]);
  });
  const other = pageStore("counted", {
    state: () => ({ items: [] }),
    actions: (set, state) => ({ [`drop${state.items.length}`]() {} }),
  });
  createRoot(() => assert.throws(() => other(), /defined twice/));
});

test("shared() makes a value once per key, and resetPage() disposes it", () => {
  let made = 0;
  const one = shared("k", () => ++made);
  assert.equal(shared("k", () => ++made), one);
  resetPage();
  assert.equal(shared("k", () => ++made), 2);
});
