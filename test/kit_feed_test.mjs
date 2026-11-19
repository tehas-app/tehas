// host/fun/kit/feed.js: one source per /live/ address per page, one poller,
// paused while the page is hidden, keeping the last good answer.
import { setHidden, listening } from "./kit_stub.mjs";
import { test, beforeEach, afterEach, mock } from "node:test";
import assert from "node:assert/strict";
import { createEffect, createRoot, flush, isPending } from "../host/fun/kit/vendor/solid.js";
import { feed } from "../host/fun/kit/feed.js";
import { resetPage } from "../host/fun/kit/state.js";

let calls;
let answer;
// The answers held back while answer.hold is set, each let go by land().
let held;
const land = () => held.shift()();

beforeEach(() => {
  calls = [];
  answer = { ok: true, body: '{"n":1}' };
  held = [];
  globalThis.fetch = async (src, init) => {
    calls.push({ src, init });
    if (answer.hold) {
      await new Promise((resolve, reject) => {
        held.push(resolve);
        init.signal.addEventListener("abort", () => reject(init.signal.reason));
      });
    }
    const { ok, body } = answer;
    if (!ok) return { ok: false, status: 503, statusText: "Service Unavailable" };
    return { ok: true, status: 200, text: async () => body };
  };
  mock.timers.enable({ apis: ["setTimeout"] });
});

afterEach(() => {
  resetPage();
  mock.timers.reset();
  setHidden(false);
});

const settle = async () => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  flush();
};

// An island reading a feed: what it saw, in order, as an effect sees it. An
// effect runs once the data is in; an error goes to its error handler, as it
// goes to <Errored> in a page.
function island(src, options) {
  return createRoot((dispose) => {
    const live = feed(src, options);
    const seen = [];
    createEffect(
      () => ({ n: live.data().n, pending: isPending(() => live.data()), error: live.error() }),
      {
        effect: (v) => {
          seen.push(v);
        },
        error: (e) => {
          seen.push({ thrown: e.message });
        },
      },
    );
    return { live, seen, dispose };
  });
}

test("two islands that name one address share one source and one fetch", async () => {
  const a = island("/live/status.json", { every: 1000 });
  const b = island("/live/status.json", { every: 1000 });
  await settle();
  assert.equal(a.live, b.live);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].init.cache, "no-store");
  assert.deepEqual(a.seen.at(-1), { n: 1, pending: false, error: null });
  assert.deepEqual(b.seen.at(-1), { n: 1, pending: false, error: null });
});

test("nothing is drawn until the first answer, then the data", async () => {
  const a = island("/live/status.json");
  flush();
  assert.deepEqual(a.seen, []);
  await settle();
  assert.deepEqual(a.seen.at(-1), { n: 1, pending: false, error: null });
});

test("one poller per address, at the shortest period asked for", async () => {
  island("/live/status.json", { every: 5000 });
  island("/live/status.json", { every: 1000 });
  island("/live/running.json");
  await settle();
  assert.equal(calls.length, 2);
  mock.timers.tick(1000);
  await settle();
  assert.deepEqual(
    calls.map((c) => c.src),
    ["/live/status.json", "/live/running.json", "/live/status.json"],
  );
});

test("a poll that changes the data is pending until it lands", async () => {
  const a = island("/live/status.json", { every: 1000 });
  await settle();
  answer = { ok: true, body: '{"n":2}' };
  a.live.refresh();
  flush();
  assert.deepEqual(a.seen.at(-1), { n: 1, pending: true, error: null });
  await settle();
  assert.deepEqual(a.seen.at(-1), { n: 2, pending: false, error: null });
});

test("a failed poll keeps the last good data and says why", async () => {
  const a = island("/live/status.json", { every: 1000 });
  await settle();
  answer = { ok: false };
  mock.timers.tick(1000);
  await settle();
  assert.deepEqual(a.seen.at(-1).n, 1);
  assert.match(a.seen.at(-1).error.message, /503/);
  answer = { ok: true, body: '{"n":1}' };
  mock.timers.tick(1000);
  await settle();
  assert.deepEqual(a.seen.at(-1), { n: 1, pending: false, error: null });
});

test("a first answer that fails is an error for <Errored>", async () => {
  answer = { ok: false };
  const a = island("/live/status.json");
  await settle();
  assert.match(a.seen.at(-1).thrown, /\/live\/status\.json: 503/);
  answer = { ok: true, body: '{"n":5}' };
  a.live.refresh();
  await settle();
  assert.deepEqual(a.seen.at(-1), { n: 5, pending: false, error: null });
});

test("the same bytes again change nothing downstream", async () => {
  const a = island("/live/status.json", { every: 1000 });
  await settle();
  const at = a.live.at();
  const data = a.live.data();
  mock.timers.tick(1000);
  await settle();
  assert.equal(calls.length, 2);
  assert.equal(a.live.at(), at);
  assert.equal(a.live.data(), data);
});

test("no poll while the page is hidden; one when it shows again", async () => {
  island("/live/status.json", { every: 1000 });
  await settle();
  setHidden(true);
  mock.timers.tick(5000);
  await settle();
  assert.equal(calls.length, 1);
  setHidden(false);
  await settle();
  assert.equal(calls.length, 2);
});

test("the feed stops when the last island that reads it is disposed", async () => {
  const a = island("/live/status.json", { every: 1000 });
  const b = island("/live/status.json", { every: 1000 });
  await settle();
  a.dispose();
  mock.timers.tick(1000);
  await settle();
  assert.equal(calls.length, 2, "b still polls");
  answer = { ...answer, hold: true };
  mock.timers.tick(1000);
  await settle();
  assert.equal(calls.length, 3);
  b.dispose();
  assert.equal(listening(), 0);
  assert.ok(calls[2].init.signal.aborted, "the fetch that is out is aborted");
  answer = { ok: true, body: '{"n":1}' };
  mock.timers.tick(5000);
  await settle();
  assert.equal(calls.length, 3);
  const c = island("/live/status.json");
  await settle();
  assert.notEqual(c.live, a.live);
  assert.equal(calls.length, 4);
});

test("one fetch is out at a time; the next falls due a period after it lands", async () => {
  const a = island("/live/status.json", { every: 1000 });
  await settle();
  answer = { ...answer, hold: true };
  mock.timers.tick(1000);
  await settle();
  assert.equal(calls.length, 2);
  mock.timers.tick(5000);
  await settle();
  assert.equal(calls.length, 2, "no poll while one is out");
  answer = { ok: true, body: '{"n":2}' };
  land();
  await settle();
  assert.deepEqual(a.seen.at(-1), { n: 2, pending: false, error: null });
  mock.timers.tick(999);
  await settle();
  assert.equal(calls.length, 2);
  mock.timers.tick(1);
  await settle();
  assert.equal(calls.length, 3);
});

test("a refresh while a poll is out polls once more when it lands", async () => {
  const a = island("/live/status.json");
  await settle();
  answer = { ...answer, hold: true };
  a.live.refresh();
  await settle();
  a.live.refresh();
  a.live.refresh();
  await settle();
  assert.equal(calls.length, 2);
  answer = { ok: true, body: '{"n":3}' };
  land();
  await settle();
  await settle();
  assert.equal(calls.length, 3);
  assert.deepEqual(a.seen.at(-1), { n: 3, pending: false, error: null });
});

test("a fetch with no answer in 30 s is aborted and counts as a failed poll", async () => {
  const a = island("/live/status.json", { every: 1000 });
  await settle();
  answer = { ...answer, hold: true };
  mock.timers.tick(1000);
  await settle();
  mock.timers.tick(30000);
  await settle();
  assert.equal(a.seen.at(-1).n, 1);
  assert.match(a.seen.at(-1).error.message, /status\.json: no answer in 30000 ms/);
  answer = { ok: true, body: '{"n":1}' };
  mock.timers.tick(1000);
  await settle();
  assert.equal(calls.length, 3);
  assert.deepEqual(a.seen.at(-1), { n: 1, pending: false, error: null });
});

test("the period is at least a second", async () => {
  island("/live/status.json", { every: 0 });
  await settle();
  mock.timers.tick(999);
  await settle();
  assert.equal(calls.length, 1);
  mock.timers.tick(1);
  await settle();
  assert.equal(calls.length, 2);
});

test("the period goes back up when the reader that asked for the short one leaves", async () => {
  island("/live/status.json", { every: 5000 });
  const fast = island("/live/status.json", { every: 1000 });
  await settle();
  mock.timers.tick(1000);
  await settle();
  assert.equal(calls.length, 2);
  fast.dispose();
  mock.timers.tick(4999);
  await settle();
  assert.equal(calls.length, 2);
  mock.timers.tick(1);
  await settle();
  assert.equal(calls.length, 3);
});

test("isPending is true during any poll, also one that brings the same bytes", async () => {
  const a = island("/live/status.json");
  await settle();
  a.live.refresh();
  flush();
  assert.deepEqual(a.seen.at(-1), { n: 1, pending: true, error: null });
  await settle();
  assert.deepEqual(a.seen.at(-1), { n: 1, pending: false, error: null });
});

test("a feed outside an island's root is refused", () => {
  assert.throws(() => feed("/live/status.json"), /inside an island's root/);
});
