// What the plain dashboard asks the server for, with a stubbed fetch and
// fake timers: the host island's two periods (hostFeed of
// host/fun/factory/tehas-factory-host.render.js) and the console's tail
// (host/fun/factory/tehas-factory-tail.js), on a minimal element.
//
//   node --test test/dashboard_feeds_test.mjs
import { setHidden } from "./kit_stub.mjs";
import { test, beforeEach, afterEach, mock } from "node:test";
import assert from "node:assert/strict";
import { createRoot, flush } from "../host/fun/kit/vendor/solid.js";
import { resetPage } from "../host/fun/kit/state.js";
import { hostFeed } from "../host/fun/factory/tehas-factory-host.render.js";

// What a custom element needs of the DOM: attributes, the <pre>, the note
// and the checkbox inside it.
const defined = new Map();
globalThis.customElements = { get: (n) => defined.get(n), define: (n, c) => defined.set(n, c) };
globalThis.HTMLElement = class {
  constructor() {
    this.attrs = new Map();
    this.isConnected = false;
    this.pre = { textContent: "", scrollTop: 0, scrollHeight: 0, append(t) { this.textContent += t; this.scrollHeight = this.textContent.length; } };
    this.note = { textContent: "" };
    this.follow = { checked: true };
  }
  getAttribute(n) {
    return this.attrs.has(n) ? this.attrs.get(n) : null;
  }
  hasAttribute(n) {
    return this.attrs.has(n);
  }
  setAttribute(n, v) {
    const before = this.getAttribute(n);
    this.attrs.set(n, v);
    if (this.constructor.observedAttributes?.includes(n)) this.attributeChangedCallback(n, before, v);
  }
  querySelector(sel) {
    return { pre: this.pre, ".console-bar .muted": this.note, "input[type=checkbox]": this.follow }[sel] ?? null;
  }
};
await import("../host/fun/factory/tehas-factory-tail.js");
const Tail = defined.get("tehas-factory-tail");

let calls;
let answer;
let held;

beforeEach(() => {
  calls = [];
  held = [];
  answer = () => ({ ok: true, status: 200, text: async () => '{"generatedAt":"x"}' });
  globalThis.fetch = async (src, init) => {
    calls.push({ src, init });
    const a = answer(src, init);
    if (a.hold) await new Promise((resolve) => held.push(resolve));
    return a;
  };
  mock.timers.enable({ apis: ["setTimeout", "setInterval"] });
});

afterEach(() => {
  resetPage();
  mock.timers.reset();
  setHidden(false);
});

const settle = async () => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
  flush();
  for (let i = 0; i < 8; i++) await Promise.resolve();
};
const after = async (ms) => {
  mock.timers.tick(ms);
  await settle();
};
const missing = () => ({ ok: false, status: 404, statusText: "Not Found" });
const host = () => createRoot((dispose) => (hostFeed("/live/top.json"), dispose));

test("the host file is asked for every ten seconds while it is there", async () => {
  host();
  await settle();
  assert.equal(calls.length, 1);
  await after(10000);
  assert.equal(calls.length, 2);
  await after(10000);
  assert.equal(calls.length, 3);
});

test("a host with no file is asked once a minute, not every ten seconds", async () => {
  answer = missing;
  host();
  await settle();
  assert.equal(calls.length, 1);
  await after(50000);
  assert.equal(calls.length, 1);
  await after(10000);
  assert.equal(calls.length, 2);
});

test("a file that comes after a first miss is then read every ten seconds", async () => {
  answer = missing;
  host();
  await settle();
  answer = () => ({ ok: true, status: 200, text: async () => '{"n":1}' });
  await after(60000);
  assert.equal(calls.length, 2);
  await after(10000);
  assert.equal(calls.length, 3);
});

test("a file that goes away is asked for once a minute again", async () => {
  host();
  await settle();
  answer = missing;
  await after(10000);
  assert.equal(calls.length, 2);
  await after(50000);
  assert.equal(calls.length, 2);
  await after(10000);
  assert.equal(calls.length, 3);
});

test("the host file is not asked for while the page is hidden", async () => {
  host();
  await settle();
  setHidden(true);
  await after(60000);
  assert.equal(calls.length, 1);
  setHidden(false);
  await settle();
  assert.equal(calls.length, 2);
});

test("the island gone, nothing is asked for", async () => {
  const dispose = host();
  await settle();
  dispose();
  await after(120000);
  assert.equal(calls.length, 1);
});

// A console file of `text` as the server answers a Range request.
const bytes = (s) => new TextEncoder().encode(s);
function served(files) {
  return (src, init) => {
    const body = bytes(files[src.replace("/live/", "")] ?? "");
    const from = Number(/bytes=(\d+)-/.exec(init.headers.Range)[1]);
    if (from > body.length) return { status: 416, arrayBuffer: async () => new ArrayBuffer(0) };
    return { status: 206, arrayBuffer: async () => body.slice(from).buffer };
  };
}
function tail(file, live = true) {
  const el = new Tail();
  el.attrs.set("file", file);
  if (live) el.attrs.set("live", "");
  el.isConnected = true;
  el.connectedCallback();
  return el;
}

test("the tail asks only for the bytes it has not read, and appends them", async () => {
  const files = { "console-41.txt": "one\n" };
  answer = served(files);
  const el = tail("console-41.txt");
  await settle();
  assert.equal(el.pre.textContent, "one\n");
  assert.equal(calls[0].init.headers.Range, "bytes=0-");
  assert.equal(el.note.textContent, "live");
  files["console-41.txt"] = "one\ntwo\n";
  await after(2000);
  assert.equal(calls[1].init.headers.Range, "bytes=4-");
  assert.equal(el.pre.textContent, "one\ntwo\n");
  assert.equal(el.pre.scrollTop, el.pre.scrollHeight, "it follows the output");
  el.follow.checked = false;
  files["console-41.txt"] += "three\n";
  el.pre.scrollTop = 0;
  await after(2000);
  assert.equal(el.pre.scrollTop, 0, "not while follow is off");
});

test("a whole file where a range was asked for is a new cycle's: the text starts again", async () => {
  answer = served({ "console-41.txt": "old run\n" });
  const el = tail("console-41.txt");
  await settle();
  answer = () => ({ status: 200, arrayBuffer: async () => bytes("new\n").buffer });
  await after(2000);
  assert.equal(el.pre.textContent, "");
  answer = served({ "console-41.txt": "new\n" });
  await after(2000);
  assert.equal(calls.at(-1).init.headers.Range, "bytes=0-");
  assert.equal(el.pre.textContent, "new\n");
});

test("an answer for the file shown before another was chosen is dropped", async () => {
  const files = { "console-41.txt": "forty-one\n", "console-36.txt": "thirty-six\n" };
  const serve = served(files);
  answer = (src, init) => ({ ...serve(src, init), hold: src.endsWith("41.txt") });
  const el = tail("console-41.txt");
  await settle();
  el.setAttribute("file", "console-36.txt");
  held.shift()();
  await settle();
  assert.equal(el.pre.textContent, "");
  answer = serve;
  await after(2000);
  assert.equal(el.pre.textContent, "thirty-six\n");
});

test("past 400000 characters the oldest go, and 300000 stay", async () => {
  const files = { "console-41.txt": "a".repeat(399999) };
  answer = served(files);
  const el = tail("console-41.txt");
  await settle();
  assert.equal(el.pre.textContent.length, 399999);
  files["console-41.txt"] += "bc";
  await after(2000);
  assert.equal(el.pre.textContent.length, 300000);
  assert.ok(el.pre.textContent.endsWith("abc"));
});

test("a name that is not a console file's is not asked for", async () => {
  answer = served({});
  tail("../status.json");
  tail("");
  await after(2000);
  assert.equal(calls.length, 0);
});

test("a tail that cannot reach the file says so; an ended one says ended", async () => {
  answer = () => {
    throw new Error("offline");
  };
  const el = tail("console-41.txt");
  await settle();
  assert.equal(el.note.textContent, "console unavailable");
  answer = served({ "console-36.txt": "done\n" });
  const ended = tail("console-36.txt", false);
  await settle();
  assert.equal(ended.note.textContent, "ended");
});

test("a tail taken off the page stops asking", async () => {
  answer = served({ "console-41.txt": "x" });
  const el = tail("console-41.txt");
  await settle();
  el.disconnectedCallback();
  await after(10000);
  assert.equal(calls.length, 1);
});
