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
  assert.deepEqual(answer, { text: "the rest of the log\n", size: 20 });
  assert.deepEqual(asked, [{
    url: "/live/console-7.txt",
    init: { headers: { Range: "bytes=1024-" }, cache: "no-store" },
  }]);
});

test("a 200 at the start of the file is its text too", async () => {
  fetchOf(200, "all of it");
  assert.deepEqual(await tailStep("console-7.txt", 0), { text: "all of it", size: 9 });
});

test("a 200 later on is a replaced file, not more text", async () => {
  fetchOf(200, "a new cycle's whole console");
  assert.equal(await tailStep("console-7.txt", 4096), "replaced");
});

test("a 416 is a console at its end", async () => {
  fetchOf(416);
  assert.equal(await tailStep("console-7.txt", 512), "ended");
});

test("any other answer, or no answer, is nothing", async () => {
  for (const status of [404, 500, 302, 0]) {
    fetchOf(status, "words nobody asked for");
    assert.equal(await tailStep("console-7.txt", 12), null, `status ${status}`);
  }
});

test("an empty piece is a size of nothing, which no page counts", async () => {
  fetchOf(206, "");
  assert.deepEqual(await tailStep("console-7.txt", 20), { text: "", size: 0 });
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
