// host/fun/kit/stream.js: one connection per address and options per page,
// over fetch with headers, reading text/event-stream and NDJSON, connecting
// again with a backoff.
//
//   node --test test/kit_stream_test.mjs
import { setHidden, listening } from "./kit_stub.mjs";
import { test, beforeEach, afterEach, mock } from "node:test";
import assert from "node:assert/strict";
import { createRoot, flush } from "../host/fun/kit/vendor/solid.js";
import { stream } from "../host/fun/kit/stream.js";
import { pageStore, resetPage } from "../host/fun/kit/state.js";

// A response body the test writes into, chunk by chunk.
function pipe() {
  const queue = [];
  let wake = null;
  const p = {
    cancelled: false,
    push(text) {
      queue.push({ value: new TextEncoder().encode(text), done: false });
      wake?.();
    },
    end() {
      queue.push({ done: true });
      wake?.();
    },
    fail(e) {
      queue.push({ error: e });
      wake?.();
    },
    body: {
      getReader: () => ({
        async read() {
          while (!queue.length) await new Promise((r) => (wake = r));
          wake = null;
          const next = queue.shift();
          if (next.error) throw next.error;
          return next;
        },
        cancel: async () => {
          p.cancelled = true;
          queue.push({ done: true });
          wake?.();
        },
      }),
    },
  };
  return p;
}

let calls;
let answer;

beforeEach(() => {
  calls = [];
  // Each connection gets the next answer: a pipe, or { status }.
  answer = () => pipe();
  globalThis.fetch = async (src, init) => {
    const a = answer(src, init);
    calls.push({ src, init, a });
    if (a instanceof Error) throw a;
    const headers = new Headers(a.type ? { "Content-Type": a.type } : {});
    if (a.body) {
      const status = a.status ?? 200;
      return { ok: status < 300, status, statusText: a.statusText ?? "", headers, body: a.body };
    }
    return { ok: a.status < 300, status: a.status, statusText: a.statusText ?? "" };
  };
  mock.timers.enable({ apis: ["setTimeout", "Date"] });
});

afterEach(() => {
  resetPage();
  mock.timers.reset();
  setHidden(false);
});

const settle = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
  flush();
};
const after = async (ms) => {
  mock.timers.tick(ms);
  await settle();
};

function reader(src, options = {}) {
  return createRoot((dispose) => {
    const events = [];
    const live = stream(src, { on: (e) => events.push(e), ...options });
    return { live, events, dispose };
  });
}

test("server-sent events reach the callback, data parsed as JSON", async () => {
  const r = reader("/api/events", { headers: { "X-Session": "s1" } });
  await settle();
  assert.equal(r.live.state(), "open");
  assert.deepEqual(calls[0].init.headers, { "X-Session": "s1" });
  const p = calls[0].a;
  p.push('event: ticket\nid: 7\ndata: {"n":');
  p.push("1}\n\n: a comment\r\ndata: [1,\r\ndata: 2]\r\n\r\n");
  await settle();
  assert.deepEqual(r.events, [
    { type: "ticket", data: { n: 1 }, id: "7" },
    { type: "message", data: [1, 2], id: "7" },
  ]);
});

test("NDJSON lines reach the callback, the last one even with no newline", async () => {
  const r = reader("/api/live.ndjson", { format: "ndjson" });
  await settle();
  const p = calls[0].a;
  p.push('{"n":1}\n\n{"n"');
  p.push(':2}\n{"n":3}');
  p.end();
  await settle();
  assert.deepEqual(r.events.map((e) => e.data), [{ n: 1 }, { n: 2 }, { n: 3 }]);
  assert.equal(r.events[0].type, "message");
});

test("parse text leaves each event's data a string", async () => {
  const r = reader("/api/events", { parse: "text" });
  await settle();
  calls[0].a.push("data: plain words\n\n");
  await settle();
  assert.equal(r.events[0].data, "plain words");
});

test("an event that is not JSON is skipped and said, and the stream stays open", async () => {
  const r = reader("/api/events");
  await settle();
  calls[0].a.push("data: not json\n\ndata: 2\n\n");
  await settle();
  assert.deepEqual(r.events.map((e) => e.data), [2]);
  assert.match(r.live.error().message, /not JSON/);
  assert.equal(r.live.state(), "open");
});

test("events feed a page store through its action", async () => {
  const useTickets = pageStore("tickets", {
    state: () => ({ list: [] }),
    actions: (set) => ({
      apply(e) {
        set((d) => {
          d.list.push(e.data.n);
        });
      },
    }),
  });
  const { a, b } = createRoot(() => {
    const t = useTickets();
    // Two islands wire the same action: each event lands once.
    const a = createRoot(() => stream("/api/events", { on: t.actions.apply }));
    const b = createRoot(() => stream("/api/events", { on: t.actions.apply }));
    return { a, b, t };
  });
  assert.equal(a, b);
  await settle();
  calls[0].a.push('data: {"n":4}\n\ndata: {"n":5}\n\n');
  await settle();
  assert.equal(calls.length, 1);
  assert.deepEqual([...createRoot(() => useTickets()).state.list], [4, 5]);
});

test("a dropped connection is asked again with a growing wait, sending Last-Event-ID", async () => {
  answer = () => ({ status: 503, statusText: "Service Unavailable" });
  const r = reader("/api/events");
  await settle();
  assert.equal(r.live.state(), "error");
  assert.equal(r.live.status(), 503);
  assert.match(r.live.error().message, /503/);
  await after(999);
  assert.equal(calls.length, 1);
  await after(1);
  assert.equal(calls.length, 2);
  await after(1999);
  assert.equal(calls.length, 2, "the second wait is two seconds");
  answer = () => pipe();
  await after(1);
  assert.equal(calls.length, 3);
  assert.equal(r.live.state(), "open");
  assert.equal(r.live.error(), null);
  calls[2].a.push("id: 9\ndata: 1\n\n");
  calls[2].a.end();
  await settle();
  assert.equal(r.live.state(), "connecting");
  await after(1000);
  assert.equal(calls.length, 4, "an open connection resets the wait");
  assert.equal(calls[3].init.headers["Last-Event-ID"], "9");
});

test("an id counts only once its event is dispatched, so a drop in mid-event loses nothing", async () => {
  const r = reader("/api/events", { parse: "text" });
  await settle();
  calls[0].a.push("id: 7\ndata: seven\n\nid: 8\ndata: eig");
  calls[0].a.end();
  await settle();
  assert.deepEqual(r.events.map((e) => [e.id, e.data]), [["7", "seven"]]);
  await after(1000);
  assert.equal(calls.length, 2);
  assert.equal(calls[1].init.headers["Last-Event-ID"], "7");
});

test("an event with no closing blank line at the end of the stream is dropped", async () => {
  const r = reader("/api/events", { parse: "text" });
  await settle();
  calls[0].a.push("data: complete\n\ndata: partial\n");
  calls[0].a.end();
  await settle();
  assert.deepEqual(r.events.map((e) => e.data), ["complete"]);
});

test("a server retry field cannot make the wait shorter than the first wait", async () => {
  // Each connection says retry: 0 and ends at once.
  answer = () => {
    const p = pipe();
    p.push("retry: 0\n\n");
    p.end();
    return p;
  };
  reader("/api/events");
  await settle();
  assert.equal(calls.length, 1);
  for (let i = 0; i < 5; i++) await after(0);
  assert.equal(calls.length, 1, "no reconnect storm");
  await after(1000);
  assert.equal(calls.length, 2);
});

test("a server retry field can make the wait longer, up to max", async () => {
  answer = () => {
    const p = pipe();
    p.push("retry: 60000\n\n");
    p.end();
    return p;
  };
  reader("/api/events", { max: 5000 });
  await settle();
  await after(4999);
  assert.equal(calls.length, 1);
  await after(1);
  assert.equal(calls.length, 2);
});

test("the wait stops growing at max", async () => {
  answer = () => new Error("offline");
  reader("/api/events", { retry: 1000, max: 3000 });
  await settle();
  await after(1000);
  await after(2000);
  await after(3000);
  await after(3000);
  assert.equal(calls.length, 5);
});

test("a function for the headers is read on each connect", async () => {
  let session = "a";
  answer = () => ({ status: 500 });
  reader("/api/events", { headers: () => ({ "X-Session": session }) });
  await settle();
  session = "b";
  await after(1000);
  assert.deepEqual(calls.map((c) => c.init.headers["X-Session"]), ["a", "b"]);
});

test("a 204 stops the stream; reconnect starts it again", async () => {
  answer = () => ({ status: 204 });
  const r = reader("/api/events");
  await settle();
  assert.equal(r.live.state(), "closed");
  await after(60000);
  assert.equal(calls.length, 1);
  answer = () => pipe();
  r.live.reconnect();
  await settle();
  assert.equal(r.live.state(), "open");
});

test("while hidden an open stream stays, and a reconnect waits for the page to show", async () => {
  const r = reader("/api/events");
  await settle();
  setHidden(true);
  await settle();
  assert.equal(r.live.state(), "open");
  calls[0].a.end();
  await settle();
  await after(5000);
  assert.equal(calls.length, 1);
  setHidden(false);
  await settle();
  assert.equal(calls.length, 2);
  assert.equal(r.live.state(), "open");
});

test("hidden close drops the connection while hidden and opens it on show", async () => {
  const r = reader("/api/events", { hidden: "close" });
  await settle();
  setHidden(true);
  await settle();
  assert.equal(r.live.state(), "closed");
  assert.ok(calls[0].init.signal.aborted);
  assert.ok(calls[0].a.cancelled);
  setHidden(false);
  await settle();
  assert.equal(calls.length, 2);
  assert.equal(r.live.state(), "open");
});

test("readers with other options get a connection of their own", async () => {
  const a = reader("/api/events");
  const b = reader("/api/events", { format: "ndjson" });
  const c = reader("/api/events", { headers: { "X-Session": "c" } });
  await settle();
  assert.notEqual(a.live, b.live);
  assert.notEqual(a.live, c.live);
  assert.equal(calls.length, 3);
});

test("the connection closes when the last reader is disposed", async () => {
  const a = reader("/api/events");
  const b = reader("/api/events");
  await settle();
  assert.equal(calls.length, 1);
  a.dispose();
  calls[0].a.push("data: 1\n\n");
  await settle();
  assert.equal(a.events.length, 0, "a reader gone gets nothing");
  assert.equal(b.events.length, 1);
  b.dispose();
  assert.ok(calls[0].init.signal.aborted);
  assert.equal(listening(), 0);
  await after(60000);
  assert.equal(calls.length, 1);
});

test("an answer that does not come in 30 s is an error and is asked again", async () => {
  answer = () => ({ hold: true });
  globalThis.fetch = (src, init) => {
    calls.push({ src, init });
    return new Promise((_r, reject) => init.signal.addEventListener("abort", () => reject(init.signal.reason)));
  };
  const r = reader("/api/events");
  await after(30000);
  assert.equal(r.live.state(), "error");
  assert.match(r.live.error().message, /no answer in 30000 ms/);
  await after(1000);
  assert.equal(calls.length, 2);
});

test("a stream outside an island's root is refused", () => {
  assert.throws(() => stream("/api/events"), /inside an island's root/);
});

// An answer with a body the stream does not read as events.
function refused(status, body, type = "application/json") {
  const p = pipe();
  if (body !== undefined) p.push(body);
  p.end();
  return { status, statusText: "", body: p.body, type };
}

test("a refused answer's error carries its body, as JSON or as text", async () => {
  answer = () => refused(503, '{"error":"The board is resting."}');
  const r = reader("/api/events");
  await settle();
  assert.equal(r.live.state(), "error");
  assert.equal(r.live.error().name, "StreamError");
  assert.equal(r.live.error().status, 503);
  assert.deepEqual(r.live.error().data, { error: "The board is resting." });
  answer = () => refused(502, "<p>Bad gateway</p>", "text/html");
  await after(1000);
  assert.equal(r.live.error().data, "<p>Bad gateway</p>");
});

test("a refused answer's body is cut at 64 KiB", async () => {
  answer = () => refused(500, "x".repeat(70000), "text/plain");
  const r = reader("/api/events");
  await settle();
  assert.equal(r.live.error().data.length, 65536);
});

test("end lists the statuses that end the stream for good", async () => {
  answer = () => refused(401, '{"error":"Sign in."}');
  const r = reader("/api/events", { end: [401, 403] });
  await settle();
  assert.equal(r.live.state(), "closed");
  assert.equal(r.live.status(), 401);
  assert.deepEqual(r.live.error().data, { error: "Sign in." });
  await after(60000);
  assert.equal(calls.length, 1);
  answer = () => pipe();
  r.live.reconnect();
  await settle();
  assert.equal(r.live.state(), "open");
});

test("a status end does not list is asked again as before", async () => {
  answer = () => refused(503, "");
  const r = reader("/api/events", { end: [401] });
  await settle();
  assert.equal(r.live.state(), "error");
  await after(1000);
  assert.equal(calls.length, 2);
});

test("end as a function ends on any status or content type it refuses, as an EventSource does", async () => {
  const eventSource = (status, type) => status !== 200 || type !== "text/event-stream";
  answer = () => refused(200, "<html></html>", "text/html; charset=utf-8");
  const a = reader("/api/a", { end: eventSource });
  answer = () => refused(500, "down", "text/plain");
  const b = reader("/api/b", { end: eventSource });
  answer = () => {
    const p = pipe();
    p.type = "text/event-stream; charset=utf-8";
    return p;
  };
  const c = reader("/api/c", { end: eventSource });
  await settle();
  assert.equal(a.live.state(), "closed");
  assert.equal(a.live.error().data, "<html></html>");
  assert.equal(b.live.state(), "closed");
  assert.equal(b.live.status(), 500);
  assert.equal(c.live.state(), "open");
  await after(60000);
  assert.equal(calls.length, 3);
});

test("grow fixed keeps the first wait, up to max", async () => {
  answer = () => new Error("offline");
  reader("/api/events", { retry: 3000, grow: "fixed" });
  await settle();
  await after(3000);
  await after(3000);
  await after(3000);
  assert.equal(calls.length, 4);
});

test("the waits a page had: 2 s, then 4 s, and 1 s after a clean end", async () => {
  answer = () => ({ status: 503 });
  reader("/api/events", { retry: 2000, ended: 1000 });
  await settle();
  await after(1999);
  assert.equal(calls.length, 1);
  await after(1);
  assert.equal(calls.length, 2);
  await after(3999);
  assert.equal(calls.length, 2);
  answer = () => pipe();
  await after(1);
  assert.equal(calls.length, 3);
  calls[2].a.end();
  await settle();
  await after(999);
  assert.equal(calls.length, 3);
  await after(1);
  assert.equal(calls.length, 4);
});

test("ended 0 asks again at once after a clean end, and a failure still backs off", async () => {
  reader("/api/events", { ended: 0 });
  await settle();
  calls[0].a.push("data: 1\n\n");
  calls[0].a.end();
  await settle();
  await after(0);
  assert.equal(calls.length, 2);
  answer = () => ({ status: 503 });
  calls[1].a.push("data: 2\n\n");
  calls[1].a.end();
  await settle();
  await after(0);
  assert.equal(calls.length, 3);
  await after(1999);
  assert.equal(calls.length, 3, "the failure after a clean end waits the second step");
  await after(1);
  assert.equal(calls.length, 4);
});

test("hidden live asks again while the page is hidden", async () => {
  const r = reader("/api/events", { hidden: "live" });
  await settle();
  setHidden(true);
  await settle();
  calls[0].a.end();
  await settle();
  await after(1000);
  assert.equal(calls.length, 2);
  assert.equal(r.live.state(), "open");
});

test("hidden live connects on a page that starts hidden", async () => {
  setHidden(true);
  const r = reader("/api/events", { hidden: "live" });
  await settle();
  assert.equal(calls.length, 1);
  assert.equal(r.live.state(), "open");
});

test("the new options make a connection of their own", async () => {
  const a = reader("/api/events");
  const b = reader("/api/events", { end: [401] });
  const c = reader("/api/events", { grow: "fixed" });
  const d = reader("/api/events", { ended: 0 });
  await settle();
  assert.equal(new Set([a.live, b.live, c.live, d.live]).size, 4);
});

test("clean ends in a row with no event back off, even with ended 0", async () => {
  answer = () => {
    const p = pipe();
    p.end();
    return p;
  };
  reader("/api/events", { ended: 0 });
  await settle();
  await after(0);
  assert.equal(calls.length, 2, "the first quiet end is asked again at once");
  for (let i = 0; i < 5; i++) await after(0);
  assert.equal(calls.length, 2, "no loop");
  await after(1000);
  assert.equal(calls.length, 3);
  await after(1999);
  assert.equal(calls.length, 3);
  await after(1);
  assert.equal(calls.length, 4, "and the wait grows");
  answer = () => {
    const p = pipe();
    p.push("data: 1\n\n");
    p.end();
    return p;
  };
  await after(4000);
  assert.equal(calls.length, 5);
  await after(0);
  assert.equal(calls.length, 6, "an end that brought an event waits ended again");
});

test("a server retry field replaces ended, as an EventSource waits it after an end", async () => {
  answer = () => {
    const p = pipe();
    p.push("retry: 5000\ndata: 1\n\n");
    p.end();
    return p;
  };
  reader("/api/events", { retry: 3000, grow: "fixed", ended: 3000 });
  await settle();
  await after(4999);
  assert.equal(calls.length, 1);
  await after(1);
  assert.equal(calls.length, 2);
});

test("a refused body that stalls is read for 2 s; the error keeps what came and the wait counts from the answer", async () => {
  let p;
  answer = () => {
    p = pipe();
    p.push('{"error":"Rest');
    return { status: 503, statusText: "", body: p.body, type: "application/json" };
  };
  const r = reader("/api/events", { retry: 3000 });
  await settle();
  assert.equal(r.live.state(), "connecting");
  await after(2000);
  assert.equal(r.live.state(), "error");
  assert.equal(r.live.error().name, "StreamError");
  assert.equal(r.live.error().status, 503);
  assert.equal(r.live.error().data, '{"error":"Rest');
  assert.ok(p.cancelled);
  await after(999);
  assert.equal(calls.length, 1);
  await after(1);
  assert.equal(calls.length, 2, "3 s after the answer came");
});

test("a refused body is cut at 64 KiB in bytes, not characters", async () => {
  answer = () => refused(500, "\u00e9".repeat(40000), "text/plain");
  const r = reader("/api/events");
  await settle();
  assert.equal(r.live.error().data.length, 32768);
});

test("by default a break after the stream opened is a failure, and a clean end counts as a try", async () => {
  reader("/api/events", { retry: 2000, ended: 1000 });
  await settle();
  calls[0].a.push("data: 1\n\n");
  calls[0].a.fail(new TypeError("network error"));
  await settle();
  await after(1999);
  assert.equal(calls.length, 1, "a break waits the first step");
  await after(1);
  assert.equal(calls.length, 2);
  answer = () => new Error("offline");
  calls[1].a.end();
  await settle();
  await after(1000);
  assert.equal(calls.length, 3);
  await after(3999);
  assert.equal(calls.length, 3, "a failure after a clean end waits the second step");
  await after(1);
  assert.equal(calls.length, 4);
});

test("broken end: a break after the stream opened waits as a clean end", async () => {
  const r = reader("/api/events", { retry: 2000, ended: 1000, broken: "end" });
  await settle();
  calls[0].a.push("data: 1\n\n");
  calls[0].a.fail(new TypeError("network error"));
  await settle();
  assert.equal(r.live.state(), "error");
  assert.match(r.live.error().message, /network error/);
  await after(999);
  assert.equal(calls.length, 1);
  await after(1);
  assert.equal(calls.length, 2, "1 s after a break");
});

test("broken end: the ticket page's waits, 1 s after an end, then 2 s and 4 s, then 1 s", async () => {
  reader("/api/events", { retry: 2000, ended: 1000, broken: "end" });
  await settle();
  answer = () => new Error("offline");
  calls[0].a.push("data: 1\n\n");
  calls[0].a.end();
  await settle();
  await after(999);
  assert.equal(calls.length, 1);
  await after(1);
  assert.equal(calls.length, 2, "1 s after an end");
  await after(1999);
  assert.equal(calls.length, 2);
  await after(1);
  assert.equal(calls.length, 3, "2 s after the first failure");
  answer = () => {
    const p = pipe();
    p.push("data: 2\n\n");
    p.end();
    return p;
  };
  await after(3999);
  assert.equal(calls.length, 3);
  await after(1);
  assert.equal(calls.length, 4, "4 s after the second");
  await after(999);
  assert.equal(calls.length, 4);
  await after(1);
  assert.equal(calls.length, 5, "1 s after the next end");
});

test("broken end with no ended set waits the first step after an end or a break, and a failure starts over", async () => {
  reader("/api/events", { broken: "end" });
  await settle();
  calls[0].a.fail(new TypeError("network error"));
  await settle();
  await after(1000);
  assert.equal(calls.length, 2);
  answer = () => new Error("offline");
  calls[1].a.end();
  await settle();
  await after(1000);
  assert.equal(calls.length, 3);
  await after(999);
  assert.equal(calls.length, 3);
  await after(1);
  assert.equal(calls.length, 4, "the first failure after an end waits the first step");
});

test("broken makes a connection of its own", async () => {
  const a = reader("/api/events");
  const b = reader("/api/events", { broken: "end" });
  await settle();
  assert.notEqual(a.live, b.live);
});
