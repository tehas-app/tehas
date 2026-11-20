// host/fun/kit/press.js: one request with headers, a JSON body and a rule
// for its status, and the pattern host/docs/frontend.md gives for it: a press
// inside Solid's action() with createOptimistic and createOptimisticStore.
//
//   node --test test/kit_press_test.mjs
import "./kit_stub.mjs";
import { test, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import {
  action,
  createEffect,
  createOptimistic,
  createOptimisticStore,
  createRoot,
  flush,
} from "../host/fun/kit/vendor/solid.js";
import { PressError, press } from "../host/fun/kit/press.js";
import { keyOf } from "../host/fun/kit/request.js";
import { feed } from "../host/fun/kit/feed.js";
import { pageStore, resetPage } from "../host/fun/kit/state.js";

let calls;
let answer;

beforeEach(() => {
  calls = [];
  answer = () => ({ status: 200, body: '{"ok":true}' });
  globalThis.fetch = async (src, init) => {
    calls.push({ src, init });
    const a = answer(src, init);
    await new Promise((r) => setTimeout(r, 1));
    return {
      ok: a.status >= 200 && a.status < 300,
      status: a.status,
      statusText: a.statusText ?? "",
      headers: new Headers(a.headers ?? {}),
      text: async () => a.body ?? "",
    };
  };
});

afterEach(() => resetPage());

const settle = async () => {
  for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 1));
  flush();
};

test("a press posts JSON with its headers and gives the answer", async () => {
  answer = () => ({ status: 201, body: '{"id":3}', headers: { Date: "Thu, 19 Nov 2026 10:00:00 GMT", "X-Session": "s2" } });
  const got = await press("/api/tickets", { headers: () => ({ "X-Session": "s1" }), body: { title: "a" }, expose: ["X-Session"] });
  assert.equal(calls[0].init.method, "POST");
  assert.equal(calls[0].init.body, '{"title":"a"}');
  assert.deepEqual(calls[0].init.headers, { "X-Session": "s1", "Content-Type": "application/json" });
  assert.deepEqual(got, { status: 201, ok: true, data: { id: 3 }, headers: { "x-session": "s2" }, date: Date.parse("Thu, 19 Nov 2026 10:00:00 GMT") });
});

test("a string body goes as it is, with the caller's content type", async () => {
  await press("/api/note", { method: "PUT", body: "words", headers: { "content-type": "text/plain" }, credentials: "include" });
  assert.equal(calls[0].init.method, "PUT");
  assert.equal(calls[0].init.body, "words");
  assert.deepEqual(calls[0].init.headers, { "content-type": "text/plain" });
  assert.equal(calls[0].init.credentials, "include");
});

test("a form, a search, a blob or bytes go as they are, with no JSON content type", async () => {
  const form = new FormData();
  form.set("title", "a");
  const bodies = [form, new URLSearchParams("a=1"), new Blob(["x"]), new Uint8Array([1]), new ArrayBuffer(1)];
  for (const body of bodies) await press("/api/form", { body });
  assert.deepEqual(calls.map((c) => c.init.body), bodies);
  assert.deepEqual(calls.map((c) => c.init.headers), [{}, {}, {}, {}, {}]);
});

test("arrays and plain values go as JSON; another kind of object is refused", async () => {
  await press("/api/a", { body: [1, 2] });
  await press("/api/a", { body: Object.assign(Object.create(null), { a: 1 }) });
  await press("/api/a", { body: 3 });
  assert.deepEqual(calls.map((c) => c.init.body), ["[1,2]", '{"a":1}', "3"]);
  for (const body of [new Map([["a", 1]]), () => 1, new ReadableStream()]) {
    await assert.rejects(press("/api/a", { body }), /\/api\/a: a body press cannot send/);
  }
  assert.equal(calls.length, 3);
});

test("an empty answer is null; parse text gives the text", async () => {
  answer = () => ({ status: 204 });
  assert.equal((await press("/api/a")).data, null);
  answer = () => ({ status: 200, body: "done" });
  assert.equal((await press("/api/a", { parse: "text" })).data, "done");
});

test("a status that is not 2xx rejects with its status and body", async () => {
  answer = () => ({ status: 409, statusText: "Conflict", body: '{"why":"taken"}' });
  await assert.rejects(press("/api/pick"), (e) => {
    assert.ok(e instanceof PressError);
    assert.equal(e.message, "/api/pick: 409 Conflict");
    assert.equal(e.status, 409);
    assert.deepEqual(e.data, { why: "taken" });
    return true;
  });
});

test("an accepted status resolves with its value", async () => {
  answer = () => ({ status: 404, body: "This factory plans no releases." });
  const got = await press("/api/plan", { method: "GET", accept: { 404: { releases: [] } } });
  assert.deepEqual(got.data, { releases: [] });
  assert.equal(got.ok, false);
  assert.equal(got.status, 404);
});

test("keyOf names a source by the options that make it its own", () => {
  const f = () => ({});
  assert.equal(keyOf("feed", "/a", { every: 5 }, ["headers"]), "feed:/a");
  assert.equal(keyOf("feed", "/a", { headers: f }, ["headers"]), keyOf("feed", "/a", { headers: f }, ["headers"]));
  assert.notEqual(keyOf("feed", "/a", { headers: f }, ["headers"]), keyOf("feed", "/a", { headers: () => ({}) }, ["headers"]));
});

// The pattern of the manual: the guess shows at once, a failure reverts it,
// and a success holds it until the feed brings the truth.
test("an optimistic value over a feed: shown at once, kept on success, rolled back on failure", async () => {
  let count = 1;
  answer = (src) => (src === "/live/votes.json" ? { status: 200, body: JSON.stringify({ count }) } : { status: 200, body: "{}" });
  const seen = [];
  const vote = createRoot(() => {
    const live = feed("/live/votes.json");
    const [shown, setShown] = createOptimistic(() => live.data().count);
    createEffect(
      () => shown(),
      (v) => {
        seen.push(v);
      },
    );
    return action(function* () {
      setShown((n) => n + 1);
      yield press("/api/vote");
      count += 1;
      live.refresh();
    });
  });
  await settle();
  const ok = vote();
  flush();
  assert.equal(seen.at(-1), 2, "the guess shows before the answer");
  await ok;
  await settle();
  assert.deepEqual(seen, [1, 2], "no step back to the old value");
  answer = (src) => (src === "/live/votes.json" ? { status: 200, body: JSON.stringify({ count }) } : { status: 500, statusText: "Internal Server Error" });
  const failed = vote();
  flush();
  assert.equal(seen.at(-1), 3);
  await assert.rejects(failed, /500/);
  await settle();
  assert.deepEqual(seen, [1, 2, 3, 2], "the guess is rolled back");
});

test("an optimistic store over a page store: the answer's truth is written after the yield", async () => {
  const useList = pageStore("press-list", {
    state: () => ({ items: [] }),
    actions: (set) => ({
      add(item) {
        set((d) => {
          d.items.push(item);
        });
      },
    }),
  });
  answer = () => ({ status: 201, body: '{"id":10,"title":"saved"}' });
  const seen = [];
  const add = createRoot(() => {
    const list = useList();
    const [shown, setShown] = createOptimisticStore(() => ({ items: list.state.items.map((i) => i.title) }), { items: [] });
    createEffect(
      () => shown.items.join(),
      (v) => {
        seen.push(v);
      },
    );
    return action(function* (title) {
      setShown((d) => {
        d.items.push(`${title}…`);
      });
      const saved = yield press("/api/items", { body: { title } });
      list.actions.add(saved.data);
    });
  });
  await settle();
  const ok = add("first");
  flush();
  assert.equal(seen.at(-1), "first…");
  await ok;
  await settle();
  assert.equal(seen.at(-1), "saved");
  answer = () => ({ status: 503 });
  const failed = add("second");
  flush();
  assert.equal(seen.at(-1), "saved,second…");
  await assert.rejects(failed, /503/);
  await settle();
  assert.equal(seen.at(-1), "saved");
});
