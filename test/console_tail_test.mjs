// Tests the console tail of host/fun/console-tail.js: the one Range-request
// step the plain dashboard, the business page and the factory floor share, and
// the cut that keeps a long cycle's console from eating the page. The fetch is
// a stub that answers one status at a time; a page's own guards - whether it
// asks while hidden, whether an ended cycle has been read to its end - stay in
// the test of the page that has them.
//
//   node --test test/console_tail_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { appendCapped, CAP, KEEP, tailStep } from "../host/fun/console-tail.js";

const asked = [];
function fetchOf(status, body = "") {
  globalThis.fetch = async (url, init) => {
    asked.push({ url, init });
    if (status === 0) throw new Error("no route to the host");
    return {
      status,
      ok: status >= 200 && status < 300,
      arrayBuffer: async () => {
        const bytes = new TextEncoder().encode(body);
        return bytes.buffer;
      },
    };
  };
}

test("a 206 is the file's text from the offset on, with its size", async () => {
  asked.length = 0;
  fetchOf(206, "the rest of the log\n");
  const answer = await tailStep("console-7.txt", 1024);
  assert.deepEqual(answer, { kind: "chunk", text: "the rest of the log\n", size: 20 });
  assert.deepEqual(asked, [{
    url: "/live/console-7.txt",
    init: { headers: { Range: "bytes=1024-" }, cache: "no-store", signal: asked[0].init.signal },
  }]);
  // The request carries a deadline of its own, so one that never answers
  // cannot hold the page's console forever (host-bridge.js keeps one too).
  assert.equal(asked[0].init.signal.aborted, false);
});

test("a 200 at the start of the file is its text too", async () => {
  fetchOf(200, "all of it");
  assert.deepEqual(await tailStep("console-7.txt", 0), { kind: "chunk", text: "all of it", size: 9 });
});

test("a 200 later on is a replaced file, not more text", async () => {
  fetchOf(200, "a new cycle's whole console");
  assert.deepEqual(await tailStep("console-7.txt", 4096), { kind: "replaced" });
});

test("a 416 is a console at its end", async () => {
  fetchOf(416);
  assert.deepEqual(await tailStep("console-7.txt", 512), { kind: "ended" });
});

test("an answer the page ignores is none", async () => {
  for (const status of [404, 500, 302]) {
    fetchOf(status, "words nobody asked for");
    assert.deepEqual(await tailStep("console-7.txt", 12), { kind: "none" }, `status ${status}`);
  }
});

test("no answer at all is failed, which a page says beside its console", async () => {
  fetchOf(0, "nothing arrives");
  assert.deepEqual(await tailStep("console-7.txt", 12), { kind: "failed" });
});

test("a request that never answers is failed, and the page may ask again", async () => {
  // The clock of this case only: tailStep's deadline is turned by hand.
  const real = { set: globalThis.setTimeout, clear: globalThis.clearTimeout };
  let due = null;
  globalThis.setTimeout = (fn) => { due = fn; return 1; };
  globalThis.clearTimeout = () => { due = null; };
  try {
    globalThis.fetch = (_url, init) => new Promise((_resolve, reject) => {
      init.signal.addEventListener("abort", () => reject(new Error("the request was stopped")));
    });
    const asked = tailStep("console-7.txt", 0);
    assert.ok(due, "nothing was left to fire the deadline");
    due();
    assert.deepEqual(await asked, { kind: "failed" });
    assert.equal(due, null, "the deadline outlived the request");
  } finally {
    Object.assign(globalThis, real);
  }
});

test("a name that is not a plain file name asks for nothing", async () => {
  asked.length = 0;
  fetchOf(206, "the file it should never have been asked for");
  for (const name of ["../etc/passwd", "a/b.txt", "console 7.txt", "", "", 7, null, undefined]) {
    assert.deepEqual(await tailStep(name, 0), { kind: "none" }, String(name));
  }
  assert.deepEqual(asked, [], "a name of that shape reached the network");
});

test("an empty piece is a size of nothing, which no page counts", async () => {
  fetchOf(206, "");
  assert.deepEqual(await tailStep("console-7.txt", 20), { kind: "chunk", text: "", size: 0 });
});

function pre() {
  return { text: "", textContent: "", appended: [], append(t) { this.appended.push(t); this.textContent += t; } };
}

test("the console keeps the newest of itself once it is long", () => {
  assert.deepEqual([CAP, KEEP], [400000, 300000]);
  const box = pre();
  appendCapped(box, "first line\n");
  assert.deepEqual(box.appended, ["first line\n"]);
  assert.equal(box.textContent, "first line\n");

  appendCapped(box, "x".repeat(CAP - box.textContent.length + 1));
  assert.equal(box.textContent.length, KEEP, "the cut left the wrong number of characters");
  // What is kept is the newest of it: the tail of what was appended.
  assert.equal(box.textContent, "x".repeat(KEEP));
});

test("a console that never passes the bound keeps every word", () => {
  const box = pre();
  for (let i = 0; i < 3; i++) appendCapped(box, `line ${i}\n`);
  assert.equal(box.textContent, "line 0\nline 1\nline 2\n");
});
