// Tests the Energy use section: layout(), the pure step from live/energy.json
// to what is drawn, and the half that fetches and draws, against a page of
// stubs.
//
//   node --test test/energy_view_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { BANDS, COUNT_FRAME_MS, countFrames, HEIGHT, layout, rateText, SLOTS } from "../host/fun/energy-view.js";

const T = 1791526545; // a multiple of 15
const NOW = T * 1000 + 5000; // five seconds after the file was written

function file(over = {}) {
  return {
    generatedAt: new Date(T * 1000).toISOString(),
    source: "viiwork",
    sourceUrl: "https://github.com/janit/viiwork",
    periodSeconds: 15,
    factoryModels: ["model-large"],
    totals: { watts: 1190, kWh24h: 14.23, kWh30d: 448.12, hosts: 3, reporting: 2, energyHosts: 2 },
    hosts: [
      { name: "node-a", source: "dcmi", watts: 456, kWh24h: 12.79, kWh30d: 393.04, ramUsedMb: 10023, ramTotalMb: 64198 },
      { name: "node-b", source: "nvidia-smi", watts: 734, kWh24h: 1.44, kWh30d: 55.08, ramUsedMb: 52624, ramTotalMb: 253212 },
      { name: "node-c", source: null, watts: null, kWh24h: null, kWh30d: null, ramUsedMb: 3100, ramTotalMb: 15800 },
    ],
    models: [
      { name: "model-large", hosts: ["node-b"], engines: [{ version: "2.9.0", engine: "strata" }], slots: 1, busy: 1, queued: 2 },
      { name: "model-small", hosts: ["node-a", "node-b"], engines: [{ version: "2.9.0", engine: "llamacpp" }], slots: 6, busy: 2, queued: 0 },
    ],
    samples: [
      { t: T - 45, w: { "node-a": 500, "node-b": 250 } },
      { t: T - 15, w: { "node-a": 500 } },
      { t: T, w: { "node-a": 456, "node-b": 734 } },
    ],
    ...over,
  };
}

test("the headline and the rows of a fresh file", () => {
  const v = layout(file(), NOW);
  assert.deepEqual(v.state, "live");
  assert.deepEqual(v.scope, "2/3 HOSTS");
  assert.deepEqual([v.watts, v.kwh24, v.kwh30], ["1190", "14.2", "448.1"]);
  assert.deepEqual(v.rows, [
    { name: "node-a", band: 0, source: "DCMI", now: "456 W", day: "12.8 KWH", month: "393.0 KWH", ram: { filled: 2, percent: 16 } },
    { name: "node-b", band: 1, source: "NVIDIA-SMI", now: "734 W", day: "1.4 KWH", month: "55.1 KWH", ram: { filled: 3, percent: 21 } },
    // No reading: no band and dashes, never a zero.
    { name: "node-c", band: null, source: "-", now: "-", day: "-", month: "-", ram: { filled: 2, percent: 20 } },
  ]);
  assert.deepEqual(v.models, [
    { name: "model-large", hosts: "node-b", engines: "viiwork 2.9.0 (strata)", busy: "1/1", queued: "2" },
    { name: "model-small", hosts: "node-a, node-b", engines: "viiwork 2.9.0 (llamacpp)", busy: "2/6", queued: "0" },
  ]);
});

test("a column's place comes from its time, and a missed slot is empty", () => {
  const v = layout(file(), NOW);
  assert.deepEqual(v.columns.length, SLOTS);
  assert.deepEqual(v.scaleTop, 1500);
  assert.deepEqual([v.from, v.to], [T - 119 * 15, T]);
  // 136 pixels stand for 1500 W. Bands tile: one ends where the next begins.
  assert.deepEqual(v.columns[119], [{ band: 0, from: 0, to: 41 }, { band: 1, from: 41, to: 108 }]);
  assert.deepEqual(v.columns[118], [{ band: 0, from: 0, to: 45 }]);
  assert.deepEqual(v.columns[117], null);
  assert.deepEqual(v.columns[116], [{ band: 0, from: 0, to: 45 }, { band: 1, from: 45, to: 68 }]);
  assert.deepEqual(v.columns.filter(Boolean).length, 3);
});

test("the scale is the next 500 W above the largest total, at least 500", () => {
  const top = (w) => layout(file({ samples: [{ t: T, w: { "node-a": w } }] }), NOW).scaleTop;
  assert.deepEqual([top(0), top(499), top(500), top(501), top(3650)], [500, 500, 500, 1000, 4000]);
  assert.deepEqual(layout(file({ samples: [] }), NOW).scaleTop, 500);
  const full = layout(file({ samples: [{ t: T, w: { "node-a": 500 } }] }), NOW);
  assert.deepEqual(full.columns[119], [{ band: 0, from: 0, to: HEIGHT }]);
});

test("samples outside the window are not drawn", () => {
  const v = layout(file({ samples: [{ t: T - 120 * 15, w: { "node-a": 9000 } }, { t: T + 15, w: { "node-a": 9000 } }, { t: T - 119 * 15, w: { "node-a": 100 } }] }), NOW);
  assert.deepEqual(v.columns.filter(Boolean).length, 1);
  assert.deepEqual(v.columns[0], [{ band: 0, from: 0, to: 27 }]);
  assert.deepEqual(v.scaleTop, 500);
});

test("a file older than 45 seconds is stale: no present figures, the rest kept", () => {
  assert.deepEqual(layout(file(), T * 1000 + 45000).state, "live");
  const v = layout(file(), T * 1000 + 46000);
  assert.deepEqual(v.state, "stale");
  assert.deepEqual([v.watts, v.kwh24, v.kwh30], ["--", "14.2", "448.1"]);
  assert.deepEqual(v.rows.map((r) => r.now), ["-", "-", "-"]);
  assert.deepEqual(v.rows[0].day, "12.8 KWH");
  assert.deepEqual(v.rows[0].ram, { filled: 2, percent: 16 });
  assert.deepEqual(v.models.map((m) => [m.busy, m.queued]), [["-", "-"], ["-", "-"]]);
  assert.deepEqual(v.columns.filter(Boolean).length, 3);
  // A file with no usable time is stale too.
  assert.deepEqual(layout(file({ generatedAt: "soon" }), NOW).state, "stale");
});

test("fewer hosts with kWh than with power is said in the head", () => {
  const v = layout(file({ totals: { watts: 1190, kWh24h: 12.79, kWh30d: null, hosts: 3, reporting: 2, energyHosts: 1 } }), NOW);
  assert.deepEqual(v.scope, "2/3 HOSTS, KWH FROM 1");
  assert.deepEqual(v.kwh30, "--");
});

test("bands follow the rows, and the ninth is the first colour again", () => {
  const hosts = Array.from({ length: 10 }, (_, i) => ({ name: `node-${i}`, watts: 10 }));
  const w = Object.fromEntries(hosts.map((h) => [h.name, 10]));
  const v = layout(file({ hosts, samples: [{ t: T, w }] }), NOW);
  assert.deepEqual(v.rows.map((r) => r.band), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
  assert.deepEqual(v.columns[119].map((p) => p.band), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
  assert.deepEqual(BANDS.length, 8);
  // Every band is a theme token (test/theme_test.sh checks every theme sets it).
  assert.deepEqual(BANDS, Array.from({ length: 8 }, (_, i) => `band-${i + 1}`));
});

test("a machine that has left keeps its band in the window, above the rest", () => {
  const v = layout(file({ samples: [{ t: T - 15, w: { "node-a": 100, "node-z": 200, "node-gone": 100 } }, { t: T, w: { "node-a": 100 } }] }), NOW);
  // Rows are node-a, node-b, node-c (bands 0 to 2); then node-gone, node-z.
  assert.deepEqual(v.columns[118], [{ band: 0, from: 0, to: 27 }, { band: 3, from: 27, to: 54 }, { band: 4, from: 54, to: 109 }]);
  assert.deepEqual(v.rows.length, 3);
});

test("a long name is cut for the row only: its draw keeps its band", () => {
  const long = "n".repeat(70);
  const v = layout(file({ hosts: [{ name: long, watts: 100 }], samples: [{ t: T, w: { [long]: 100 } }] }), NOW);
  assert.deepEqual(v.rows.map((r) => [r.name.length, r.band]), [[64, 0]]);
  assert.deepEqual(v.columns[119].map((p) => p.band), [0]);
});

test("the meter: the draw against the scale, and no token line without counts", () => {
  // 1190 W of a 1500 W scale is eight blocks of ten.
  assert.deepEqual(layout(file(), NOW).meter, { watts: "1190", wattsNow: 1190, wattsFilled: 8, tokens: null, tokensNow: null, tokensFilled: 0 });
});

test("the meter: tokens are a rate over the last minute, full at the busiest minute", () => {
  const w = { "node-a": 456 };
  const samples = [
    { t: T - 300, w, k: { "node-a": 1000 } },
    { t: T - 285, w, k: { "node-a": 4000 } }, // 3000 in 15 s: 12000 a minute
    { t: T - 60, w, k: { "node-a": 4000 } },
    { t: T - 45, w, k: { "node-a": 4100, "node-b": 50 } }, // node-b is new: nothing yet
    { t: T - 30, w, k: { "node-a": 4200, "node-b": 150 } },
    { t: T - 15, w, k: { "node-a": 4200, "node-b": 150 } },
    { t: T, w, k: { "node-a": 4500, "node-b": 250 } },
  ];
  // The last minute: 100 + 200 + 0 + 400 = 700 tokens.
  const v = layout(file({ samples }), NOW);
  assert.deepEqual([v.meter.tokens, v.meter.tokensFilled], ["700", 1]);
  // A counter that went down has started again: what it shows now is new.
  samples[6] = { t: T, w, k: { "node-a": 400, "node-b": 250 } };
  assert.deepEqual(layout(file({ samples }), NOW).meter.tokens, "800");
  // From ten thousand on, in thousands.
  samples[6] = { t: T, w, k: { "node-a": 4200 + 23400, "node-b": 150 } };
  const busy = layout(file({ samples }), NOW).meter;
  assert.deepEqual([busy.tokens, busy.tokensNow, busy.tokensFilled], ["24K", 23700, 10]);
});

test("the meter: a changed reading counts to the new one in stepped frames", () => {
  assert.deepEqual(countFrames(0, 1190), [149, 298, 446, 595, 744, 893, 1041, 1190]);
  assert.deepEqual(countFrames(1190, 400, 4), [993, 795, 598, 400]);
  assert.deepEqual(countFrames(8, 3), [7, 7, 6, 6, 5, 4, 4, 3]);
  assert.deepEqual(countFrames(9000, 12000, 3).map(rateText), ["10K", "11K", "12K"]);
});

test("the meter: no rate from one sample, a missed slot or a stale file", () => {
  const w = { "node-a": 456 };
  const pair = [{ t: T - 15, w, k: { "node-a": 10 } }, { t: T, w, k: { "node-a": 70 } }];
  assert.deepEqual(layout(file({ samples: pair }), NOW).meter.tokens, "240");
  assert.deepEqual(layout(file({ samples: [pair[1]] }), NOW).meter.tokens, "--");
  // The newest sample is not this slot's: the rate would be an old one.
  assert.deepEqual(layout(file({ samples: [{ ...pair[0], t: T - 30 }, { ...pair[1], t: T - 15 }] }), NOW).meter.tokens, "--");
  const stale = layout(file({ samples: pair }), NOW + 60000).meter;
  assert.deepEqual(stale, { watts: "--", wattsNow: null, wattsFilled: 0, tokens: "--", tokensNow: null, tokensFilled: 0 });
  // Counts that are no numbers are no counts.
  const odd = [{ t: T - 15, w, k: { "node-a": "ten", toString: 5 } }, { t: T, w, k: { "node-a": -1, toString: 9 } }, { t: T, w, k: 7 }];
  assert.deepEqual(layout(file({ samples: odd.slice(0, 2) }), NOW).meter.tokens, "16");
});

// The ENGINES cell of a model whose entry carries these engines.
const enginesCell = (engines, over = {}) =>
  layout(file({ models: [{ name: "model-large", hosts: ["node-b"], slots: 1, busy: 1, queued: 0, ...(engines === undefined ? {} : { engines }) }], ...over }), NOW).models[0].engines;

test("the engines of a model: the mesh, its version and the engine", () => {
  assert.deepEqual(enginesCell([{ version: "2.9.0", engine: "strata" }]), "viiwork 2.9.0 (strata)");
  // A mesh in the middle of an upgrade, or two engines: joined as the hosts are.
  assert.deepEqual(
    enginesCell([{ version: "2.10.0", engine: "llamacpp" }, { version: "2.9.0", engine: "vllm" }]),
    "viiwork 2.10.0 (llamacpp), viiwork 2.9.0 (vllm)",
  );
  assert.deepEqual(enginesCell([{ version: null, engine: "strata" }]), "viiwork (strata)");
  assert.deepEqual(enginesCell([{ version: "2.9.0", engine: null }]), "viiwork 2.9.0");
});

test("the engine's name for people and its own version, where the mesh reports them", () => {
  const cell = (e) => enginesCell([{ version: "2.9.2", engine: "llamacpp", ...e }]);
  assert.deepEqual(cell({ engineName: "llama.cpp", engineVersion: "b11371" }), "viiwork 2.9.2 (llama.cpp b11371)");
  // An engine that reports no version of its own.
  assert.deepEqual(cell({ engineName: "Strata", engineVersion: null }), "viiwork 2.9.2 (Strata)");
  // No name: the id, as from a mesh that sends none.
  assert.deepEqual(cell({ engineName: null, engineVersion: "b11371" }), "viiwork 2.9.2 (llamacpp b11371)");
  assert.deepEqual(cell({ engineName: "", engineVersion: 7 }), "viiwork 2.9.2 (llamacpp)");
  assert.deepEqual(enginesCell([{ version: null, engine: null, engineName: "vLLM", engineVersion: "0.31.0" }]), "viiwork (vLLM 0.31.0)");
  // Its own version alone names no engine.
  assert.deepEqual(enginesCell([{ version: null, engine: null, engineName: null, engineVersion: "b1" }]), "-");
  assert.deepEqual(enginesCell([{ version: "2.9.2", engine: null, engineVersion: "b1" }]), "viiwork 2.9.2");
  // A mesh in the middle of an upgrade: the old machines send the id only.
  assert.deepEqual(
    enginesCell([{ version: "2.9.0", engine: "llamacpp" }, { version: "2.9.2", engine: "llamacpp", engineName: "llama.cpp", engineVersion: "b11371" }]),
    "viiwork 2.9.0 (llamacpp), viiwork 2.9.2 (llama.cpp b11371)",
  );
});

test("no engines is a dash: an empty array, and a file from the old sampler", () => {
  assert.deepEqual(enginesCell([]), "-");
  assert.deepEqual(enginesCell(undefined), "-");
  // Elements that say nothing, or are no engines at all.
  assert.deepEqual(enginesCell([{ version: null, engine: null }, null, 7, "strata", { version: 2, engine: ["x"] }]), "-");
  assert.deepEqual(enginesCell("strata"), "-");
});

test("the name of the mesh is the file's, not the page's", () => {
  assert.deepEqual(enginesCell([{ version: "1.0", engine: "strata" }], { source: "othermesh" }), "othermesh 1.0 (strata)");
  for (const source of [undefined, "", 7]) {
    assert.deepEqual(enginesCell([{ version: "1.0", engine: "strata" }], { source }), "1.0 (strata)");
  }
  // The engine is shown as the mesh reports it, cut like any other name.
  assert.deepEqual(enginesCell([{ version: "1.0", engine: "<b>" + "x".repeat(100) }]), `viiwork 1.0 (<b>${"x".repeat(61)})`);
});

test("the data source line: a link only for an https address", () => {
  const source = (over) => layout(file(over), NOW).source;
  assert.deepEqual(source({}), { name: "viiwork", url: "https://github.com/janit/viiwork" });
  for (const sourceUrl of [undefined, null, 7, "", "http://example.org/", "javascript:alert(1)", " https://example.org/", "HTTPS://example.org/"]) {
    assert.deepEqual(source({ sourceUrl }), { name: "viiwork", url: null });
  }
  for (const name of [undefined, null, "", 7, {}]) assert.deepEqual(source({ source: name }), null);
  assert.deepEqual(layout(null, NOW).source, null);
  // Stale figures still came from somewhere.
  assert.deepEqual(layout(file(), NOW + 60000).source, { name: "viiwork", url: "https://github.com/janit/viiwork" });
});

test("odd input does not break the page", () => {
  for (const bad of [null, 7, "x", [], {}, { hosts: "many", samples: 3, totals: [], models: {} }]) {
    const v = layout(bad, NOW);
    assert.deepEqual([v.state, v.scope, v.watts, v.rows.length, v.models.length, v.scaleTop], ["stale", "0/0 HOSTS", "--", 0, 0, 500]);
  }
  const v = layout(file({
    hosts: [null, { name: "" }, { name: "node-a", watts: -5, kWh24h: "12", ramUsedMb: 99, ramTotalMb: 0, source: 7 }, { name: "x".repeat(200), watts: Infinity, ramUsedMb: 900, ramTotalMb: 100 }],
    models: [null, { name: "model-odd", hosts: "node-a", slots: "two", busy: 1, queued: null }],
    samples: [null, { t: T, w: { "node-a": -5, constructor: 5, "node-b": "9", toString: 300 } }, { t: "now", w: {} }, { t: T - 15 }],
    totals: { watts: NaN, reporting: -1 },
  }), NOW);
  assert.deepEqual(v.rows.map((r) => [r.name.length, r.band, r.source, r.now, r.day, r.ram]), [
    [6, null, "-", "-", "-", null],
    [64, null, "-", "-", "-", { filled: 10, percent: 100 }],
  ]);
  assert.deepEqual(v.models, [{ name: "model-odd", hosts: "-", engines: "-", busy: "-", queued: "-" }]);
  // Only own, numeric, non-negative readings are stacked: "constructor" and "toString" here are departed hosts' names.
  assert.deepEqual(v.columns[119].length, 2);
  assert.deepEqual(v.watts, "--");
});

// The half that fetches and draws. It runs only where there is a document, so
// a case gives the module one: a page of stubs holding every element the
// script names except those listed, a file to read, and timers that are
// recorded instead of run, so nothing outlives the case. Each page gets a
// fresh copy of the module (the ?key on the import).
const VIEW = ["energy", "menu-energy", "floor-meter"];

async function page(missing, key, opt = {}) {
  const { motion = false, frames = 0, status = 200, ok = true, unreachable = false } = opt;
  const found = new Map(), errors = [];
  // Fills are counted against the element that asked for the context, so a
  // fill of a row's swatch cannot answer for a fill of the chart.
  const drawn = {};
  // Timers are held rather than run: the 15 s read never fires, and a case runs
  // the frames of a count itself, one at a time, and reads what each drew. A
  // count that reached its figure clears its own timer, which is recorded too.
  const timers = new Map();
  let lastTimer = 0, scheduled = 0, countTimers = 0, cleared = 0, reads = 0;
  const node = (id) => ({
    // hidden starts false, as it does on a real element, so a write to one the
    // page has stands against a page that was left alone.
    id, hidden: false, dataset: {}, textContent: "", className: "",
    setAttribute() {}, replaceChildren() {}, append() {},
    insertRow: () => node(`${id}-row`), insertCell: () => node(`${id}-cell`),
    getContext: () => ({ fillStyle: "", fillRect: () => { drawn[id] = (drawn[id] ?? 0) + 1; } }),
  });
  const note = (e) => errors.push(String(e.message ?? e));
  process.on("unhandledRejection", note);
  const real = Object.fromEntries(["document", "window", "fetch", "performance", "setInterval", "clearInterval"].map((k) => [k, globalThis[k]]));
  globalThis.document = {
    hidden: false,
    getElementById: (id) => {
      if (missing.includes(id)) return null;
      let el = found.get(id);
      if (!el) found.set(id, (el = node(id)));
      return el;
    },
    addEventListener: () => {},
    createElement: (tag) => node(tag),
  };
  globalThis.window = { matchMedia: () => ({ matches: !motion }) }; // motion: the meter may count
  globalThis.performance = { now: () => 0 };
  globalThis.setInterval = (step, ms) => {
    const count = ms === COUNT_FRAME_MS;
    scheduled++;
    if (count) countTimers++;
    timers.set(++lastTimer, { step, count });
    return lastTimer;
  };
  globalThis.clearInterval = (id) => {
    const t = timers.get(id);
    if (!t) return;
    timers.delete(id);
    if (t.count) cleared++;
  };
  globalThis.fetch = async () => {
    reads++;
    if (unreachable) throw new Error("no route to the host");
    return {
      status, ok, json: async () => file(),
      headers: { get: () => new Date(T * 1000).toUTCString() },
    };
  };
  const watts = () => found.get("meter-watts")?.textContent;
  const held = (count) => [...timers.values()].filter((t) => t.count === count).length;
  try {
    await import(`../host/fun/energy-view.js?${key}`);
    // The module does not await its own refresh, so the case waits for its
    // chain (read, json, draw) by turning the loop: 40 is far past the depth of
    // that chain. A case that ran out of turns would find no read and no draw,
    // which the reads assertion of every case below rules out.
    for (let i = 0; i < 40; i++) await new Promise((r) => setImmediate(r));
    const first = watts();
    const steps = [];
    for (let i = 0; i < frames; i++) {
      for (const { step, count } of [...timers.values()]) if (count) step();
      steps.push(watts());
    }
    return {
      errors, drawn, first, steps, reads, scheduled, countTimers, cleared,
      heldRefresh: held(false), heldCount: held(true),
      // The three elements the view shows or hides. An element the page does
      // not have reads "absent", which no element the page has can answer.
      hidden: VIEW.map((id) => (found.has(id) ? found.get(id).hidden : "absent")),
    };
  } finally {
    process.off("unhandledRejection", note);
    for (const [k, v] of Object.entries(real)) globalThis[k] = v;
  }
}

test("a page with its meter, its section and its link is shown and drawn", async () => {
  const p = await page([], "whole");
  assert.deepEqual(p.errors, []);
  assert.equal(p.reads, 1); // the file was read: what follows is a draw, not a wait
  assert.deepEqual(p.hidden, [false, false, false]); // all three shown: the file has data
  // The chart drew on its own canvas, and each host row drew its own swatch.
  assert.deepEqual(Object.keys(p.drawn).sort(), ["canvas", "energy-canvas"]);
  assert.ok(p.drawn["energy-canvas"] > 0);
  assert.deepEqual(p.drawn.canvas, 3);
  // The 15 s read is scheduled and held: a case never fires it.
  assert.deepEqual([p.scheduled, p.countTimers, p.heldRefresh], [1, 0, 1]);
});

// A page missing one of the three elements is left as it was written.
for (const drop of VIEW) {
  test(`a page with no ${drop} throws nothing and draws nothing`, async () => {
    const p = await page([drop], `no-${drop}`);
    assert.deepEqual(p.errors, []);
    assert.equal(p.reads, 1); // the read happened; the guard stopped the draw
    assert.equal(p.hidden[VIEW.indexOf(drop)], "absent"); // the element the page lacks
    assert.deepEqual(p.hidden.filter((v, i) => VIEW[i] !== drop), [false, false]); // the rest untouched
    assert.deepEqual(p.drawn, {}); // not the chart, not a swatch
  });
}

test("the meter counts to its figure in frames where motion is allowed", async () => {
  const p = await page([], "count", { motion: true, frames: 7 });
  assert.deepEqual(p.errors, []);
  assert.equal(p.reads, 1);
  // The 15 s read and one count: the watts line, because this file has no token
  // rate to count.
  assert.deepEqual([p.scheduled, p.countTimers], [2, 1]);
  // The first frame is part of the way to 1190 W, not the figure itself, and
  // the last frame is the figure. Seven more frames finish the eight.
  assert.deepEqual(p.first, "149");
  assert.deepEqual(p.steps, ["298", "446", "595", "744", "893", "1041", "1190"]);
  // The count that reached its figure cleared its own timer.
  assert.deepEqual([p.cleared, p.heldCount], [1, 0]);
});

test("reduced motion draws the figure at once and starts no count", async () => {
  const p = await page([], "calm");
  assert.deepEqual(p.errors, []);
  assert.equal(p.reads, 1);
  assert.deepEqual([p.scheduled, p.countTimers], [1, 0]); // no frames to run
  assert.deepEqual(p.first, "1190"); // the figure itself, at once
});

test("a read that fails stays inside the module", async () => {
  const p = await page([], "unreachable", { unreachable: true });
  assert.deepEqual(p.errors, []); // nothing escaped the read to the runner
  assert.equal(p.reads, 1);
  assert.deepEqual(p.hidden, [true, true, true]); // with no data the meter is hidden
  assert.deepEqual(p.drawn, {});
});

test("a file that is gone hides the meter", async () => {
  const p = await page([], "404", { status: 404, ok: false });
  assert.deepEqual(p.errors, []);
  assert.equal(p.reads, 1);
  assert.deepEqual(p.hidden, [true, true, true]); // the section, the link and the meter
  assert.deepEqual(p.drawn, {});
});
