// Tests figures() of the work order: the pure step from live/orders.json to
// what each ticket's runs spent.
//
//   node --test test/work_orders_view_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import "../host/fun/work-orders.js";

const { figures } = globalThis.TEHAS_ORDERS;

const file = (issues) => ({ orders: 1, generatedAt: "2026-10-29T10:00:00Z", issues });

test("no file, or not one, is no figures", () => {
  for (const f of [null, undefined, 3, [], {}, { orders: 2, issues: {} }, { orders: 1, issues: [] }]) {
    assert.equal(figures(f), null);
  }
});

test("each issue gives its runs, tokens and seconds", () => {
  const m = figures(file({ 962: { runs: 3, seconds: 5400, tokens: 60800000 }, 7: { runs: 1 } }));
  assert.deepEqual(m.get(962), { runs: 3, tokens: 60800000, seconds: 5400 });
  assert.deepEqual(m.get(7), { runs: 1, tokens: null, seconds: null });
  assert.equal(m.size, 2);
});

test("a key that is no issue number, or a figure that is no count, says nothing", () => {
  const m = figures(file({ "0": { seconds: 1 }, "-3": { seconds: 1 }, x: { seconds: 1 }, "05": { seconds: 1 },
    8: { seconds: -1, tokens: "9" }, 9: null }));
  assert.deepEqual([...m.keys()], [8]);
  assert.deepEqual(m.get(8), { runs: null, tokens: null, seconds: null });
});
