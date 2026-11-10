// Runs the plain dashboard's own script against a page of stubs, to see the
// half that shows the host: the list it draws from live/top.json, and when it
// asks for that file; and the "Last 10 days" section it draws from
// live/analytics.json. The fragment host/docs/dashboard.frag holds the
// markup and loads host/fun/dashboard-view.js; this case imports that module
// as the page would. No browser and no server: the document, the fetches and
// the timers are stubs, and the file is the one host_view_test.mjs describes.
//
//   node --test test/dashboard_host_test.mjs
import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { test } from "node:test";

// The module, imported once for the whole file and with no query string: every
// page below is a call of its start() against that one copy, so state the
// module kept outside start() would show up from one page in the next. No
// document is installed yet, so the import itself draws nothing.
import * as view from "../host/fun/dashboard-view.js";

// The files the module reads, and so the only ones the stub fetch answers. The
// console's file is not among them: the console half is stubbed out (below).
const ANSWERED = ["/live/previews.json", "/live/top.json", "/live/analytics.json", "/live/status.json", "/live/running.json"];
// The stub's surface, pinned by a case so that it stays this small.
const GLOBALS = ["document", "fetch", "setInterval", "setTimeout", "clearInterval", "TEHAS_CONSOLE_TABS"];
const NODE = ["name", "hidden", "dataset", "style", "attrs", "textContent", "className", "href", "kids",
  "setAttribute", "append", "replaceChildren"];

const T = 1791526545;

function file(over = {}) {
  return {
    generatedAt: new Date(T * 1000).toISOString(),
    host: {
      cpus: 4,
      load: [0.1, 0.2, 0.3],
      cpuPercent: 12.5,
      memory: { totalMb: 6400, usedMb: 1600, percent: 25 },
      processes: [
        { pid: 300, name: "pi-rukas", cpuPercent: 40, memPercent: 7.32, upSeconds: 989 },
        { pid: 200, name: "tehas-cycle", cpuPercent: 2, memPercent: 4.9, upSeconds: 61 },
        { pid: 100, name: "systemd", cpuPercent: null, memPercent: 1.8, upSeconds: 86500 },
      ],
    },
    units: [
      { id: "tehas-dispatch.timer", activeState: "active", unitFileState: "enabled", next: "10:00:00" },
      { id: "tehas-cycle@12.service", activeState: "active", unitFileState: "", next: "" },
    ],
    ...over,
  };
}

function status() {
  return {
    generatedAt: new Date(T * 1000).toISOString(),
    links: { all: "https://github.com/acme/widget/issues", queued: "q", awaiting: "a", stopped: "s", completed: "c" },
    running: [], queued: [], awaiting: [], stopped: [], completed: [], paused: false, noResources: false,
  };
}

async function page(opt = {}) {
  const { top = 200, unreachable = false } = opt;
  const byId = new Map(), listeners = {}, timers = [], errors = [], asked = [], chosen = [];
  let hidden = false, topReads = 0, analyticsReads = 0, keyed = 0, choose = null;
  const node = (name) => {
    const el = { name, hidden: false, dataset: {}, style: {}, attrs: {}, textContent: "", className: "", href: "", kids: [] };
    el.setAttribute = (k, v) => { el.attrs[k] = String(v); };
    el.append = (...kids) => { el.kids.push(...kids); };
    el.replaceChildren = (...kids) => { el.kids = kids; };
    return el;
  };
  const note = (e) => errors.push(String(e.message ?? e));
  const missing = (status = 404) => ({ ok: false, status, json: async () => ({}), headers: { get: () => "" } });
  const stubs = {
    document: {
      get hidden() { return hidden; },
      getElementById: (id) => { let el = byId.get(id); if (!el) byId.set(id, (el = node(id))); return el; },
      addEventListener: (name, fn) => { (listeners[name] ??= []).push(fn); },
      createElement: (tag) => node(tag),
      createElementNS: (_ns, tag) => node(tag),
    },
    // The tabs are the plain dashboard's other module. Here they say nothing is
    // running, so what a case sees is the host half. view() keeps the cycle it
    // was asked to show and keys() the page's own select(), so a case can choose
    // a tab and see what the page asked for next. consoleFile names no file, so
    // the console's tail() is never reached: reaching it would take arrayBuffer,
    // TextDecoder, checked and scrollTop, and a stub that grows to fit one case
    // stops saying anything about the page.
    TEHAS_CONSOLE_TABS: {
      view: (_now, selected) => { chosen.push(selected); return { running: 0, shown: null, cycles: [], tabs: [] }; },
      render: () => {},
      keys: (_el, fn) => { keyed++; choose = fn; },
      consoleFile: () => null,
    },
    fetch: async (url) => {
      asked.push(url);
      if (!ANSWERED.some((f) => url.endsWith(f))) {
        // The module swallows a failed fetch, so a file nobody answers would
        // pass unseen: it is an error of the case instead.
        errors.push("asked for " + url + ", which the stub does not answer");
        return missing();
      }
      if (url.endsWith("/live/top.json")) topReads++;
      if (url.endsWith("/live/analytics.json")) analyticsReads++;
      if (unreachable) throw new Error("no route to the host");
      if (url.endsWith("/live/top.json")) {
        return top === 200
          ? { ok: true, status: 200, json: async () => (opt.topFile || file)(), headers: { get: () => new Date(T * 1000).toUTCString() } }
          : missing(top);
      }
      if (url.endsWith("/live/analytics.json")) {
        return opt.analytics ? { ok: true, status: 200, json: async () => opt.analytics, headers: { get: () => "" } } : missing();
      }
      // No previews and no running cycle on this host: the host section is what
      // these cases are about.
      if (url.endsWith("/live/previews.json") || url.endsWith("/live/running.json")) return missing();
      return { ok: true, status: 200, json: async () => ({ ...status(), ...opt.status }), headers: { get: () => "" } };
    },
    // The page's own clock: the refresh it schedules is turned by a case, so no
    // timer outlives one.
    setInterval: (fn, ms) => { timers.push({ fn, ms }); return 0; },
    setTimeout: (fn) => { timers.push({ fn, ms: 0 }); return 0; },
    clearInterval: () => {},
  };
  const settle = async () => { for (let i = 0; i < 60; i++) await new Promise((r) => setImmediate(r)); };
  // Each turn of a page puts its own stubs in place and takes them away again,
  // so two pages can be turned one after the other.
  const turn = async (fn) => {
    const real = Object.fromEntries(GLOBALS.map((k) => [k, globalThis[k]]));
    Object.assign(globalThis, stubs);
    process.on("unhandledRejection", note);
    try {
      await fn();
      await settle();
    } finally {
      process.off("unhandledRejection", note);
      for (const [k, v] of Object.entries(real)) globalThis[k] = v;
    }
  };
  // A case can load the page the way the browser does instead: a fresh copy of
  // the module imported with the stubs in place, so only its own last line,
  // the probe-guarded start(), can draw it.
  await turn(() => (opt.entry ? import("../host/fun/dashboard-view.js?entry=" + opt.entry) : view.start()));
  // A page that ran none of the module fails here, whatever its case asserts:
  // the module set up its tabs once and asked for the queue.
  assert.equal(keyed, 1, "the module did not start on this page");
  assert.ok(asked.includes("/live/status.json"), "the module asked for nothing on this page");
  const text = (id) => byId.get(id)?.textContent;
  // The text of each row of one list: its cells joined as the page shows them.
  const items = (id) => (byId.get(id)?.kids ?? []).map((li) => li.kids
    .map((k) => (typeof k === "string" ? k : k.textContent)).join(""));
  return {
    errors, text, items, topReads: () => topReads, analyticsReads: () => analyticsReads,
    asked: () => [...asked],
    chosen: () => [...chosen],
    node: (id) => byId.get(id),
    periods: () => timers.map((t) => t.ms),
    hidden: (id = "host") => byId.get(id)?.hidden,
    hide: () => { hidden = true; },
    show: () => { hidden = false; },
    surface: () => ({ globals: Object.keys(stubs), node: Object.keys(node("probe")), tabs: stubs.TEHAS_CONSOLE_TABS }),
    choose: (issue) => turn(() => choose(issue)),
    tick: (n) => turn(async () => { for (let i = 0; i < n; i++) { for (const t of timers) await t.fn(); await settle(); } }),
    fire: (name) => turn(async () => { for (const fn of listeners[name] ?? []) await fn(); }),
  };
}

test("the plain dashboard lists the host it is given", async () => {
  const p = await page();
  assert.deepEqual(p.errors, []);
  assert.equal(p.topReads(), 1);
  assert.equal(p.hidden(), false);
  // One row per process and one per unit: the same figures the panel draws, from
  // the same function.
  assert.deepEqual(p.items("host-list"), [
    "pi-rukas pid 300  cpu 40%  mem 7.3%  up 16 min",
    "tehas-cycle pid 200  cpu 2%  mem 4.9%  up 1 min",
    "systemd pid 100  cpu --  mem 1.8%  up 1 d",
  ]);
  assert.deepEqual(p.items("unit-list"), [
    "tehas-dispatch.timer enabled  active  next 10:00:00",
    "tehas-cycle@12.service -  active",
  ]);
  assert.deepEqual([p.text("n-host"), p.text("n-units")], ["(3)", "(2)"]);
  assert.deepEqual(p.text("host-note"),
    "4 CPUs, load 0.10 0.20 0.30, 12.5% of the cpus in use, memory 1600 of 6400 MB, snapshot written just now.");
});

test("importing the module on its page draws the page with no call of start()", async () => {
  const p = await page({ entry: "auto" });
  assert.deepEqual(p.errors, []);
  assert.equal(p.topReads(), 1);
  assert.equal(p.items("host-list").length, 3);
});

test("a host that publishes no snapshot has no host section", async () => {
  const p = await page({ top: 404 });
  assert.deepEqual(p.errors, []);
  assert.equal(p.hidden(), true);
  assert.deepEqual(p.items("host-list"), []);
});

test("the dashboard asks for the host file only while the page is shown", async () => {
  const p = await page();
  assert.equal(p.topReads(), 1);
  p.hide();
  await p.tick(3); // three marks of the refresh it schedules
  assert.equal(p.topReads(), 1); // nothing on screen wants a fresh answer
  p.show();
  await p.fire("visibilitychange");
  assert.equal(p.topReads(), 2); // coming back asks at once
});

test("a file that is not there is not asked for at every refresh", async () => {
  const p = await page({ top: 404 });
  assert.equal(p.topReads(), 1);
  await p.tick(5);
  assert.equal(p.topReads(), 1); // once a minute, not at every period
  await p.fire("visibilitychange");
  assert.equal(p.topReads(), 2);
});

test("a host that cannot be reached keeps what was on screen", async () => {
  const p = await page({ unreachable: true });
  assert.deepEqual(p.errors, []); // the script caught it rather than rejecting
  assert.equal(p.hidden(), true); // nothing was ever drawn
});

test("all-hands is shown beside the state only while the snapshot says so", async () => {
  const on = await page({ top: 404, status: { allHands: true } });
  assert.deepEqual(on.errors, []);
  assert.equal(on.hidden("allhands"), false);
  for (const allHands of [false, null, undefined]) {
    const off = await page({ top: 404, status: { allHands } });
    assert.equal(off.hidden("allhands"), true);
  }
});

// A report as the analytics program prints it, cut short: two days, one
// reason, one step, two version pairs.
function report() {
  return {
    report: 1, from: 1792022400, to: 1792843200,
    shipped: { n: 2 },
    runs: { n: 4, pr: 2, merged: 0, handoff: 1, parked: 0, failed: 1, refused: 0, stopped: 0 },
    firstPassYield: { n: 2, share: 0.5 },
    leadTime: { n: 2, median: 3600, p90: 7200 },
    cycleTime: { n: 4, median: 600, p90: 900 },
    machineTime: { n: 4, seconds: 2400, perShipped: 1200 },
    tokens: { n: 4, total: 4000, input: 3000, output: 500, cacheRead: 500, perShipped: 2000 },
    availability: { n: 10, seconds: 3600, share: 0.9 },
    daily: [
      { day: "2026-10-23", shipped: 1, runs: 2, failed: 1, minutes: 20 },
      { day: "2026-10-24", shipped: 1, runs: 2, failed: 0, minutes: 20 },
    ],
    stopReasons: { n: 2, top: [{ reason: "timeout", n: 2 }] },
    stations: { n: 4, steps: [{ step: "explore", n: 4, passed: 3, share: 0.75 }] },
    versions: [
      { tehas: "v0.7.22", rukas: "0.13.0", n: 3, prShare: 0.667, medianSeconds: 600, medianTokens: 1000 },
      { tehas: "v0.7.21", rukas: "0.12.80", n: 1 },
    ],
  };
}

test("the last ten days are drawn from the report", async () => {
  const p = await page({ top: 404, analytics: report() });
  assert.deepEqual(p.errors, []);
  assert.equal(p.analyticsReads(), 1);
  assert.equal(p.hidden("analytics"), false);
  assert.equal(p.text("analytics-note"), "2026-10-23 to 2026-10-24, UTC");
  const figures = p.node("an-figures").kids.map((box) => box.kids.slice(0, 2).map((k) => k.textContent).join(" "));
  assert.deepEqual(figures, ["Shipped 2", "Runs 4", "First-pass yield 50%", "Lead time 60 min", "Cycle time 10 min",
    "Machine time 40 min", "Tokens 4k", "Availability 90%"]);
  // Two days of bars, then the line of cycle-minutes.
  const chart = p.node("an-chart").kids;
  assert.equal(chart.length, 3);
  assert.deepEqual(chart[0].kids[0].kids.map((b) => [b.className, b.style.height, b.textContent]),
    [["shipped", "100%", "1"], ["failed", "100%", "1"]]);
  assert.equal(chart[2].kids[0].attrs.points, "5,0.0 15,0.0");
  assert.deepEqual(p.items("an-reasons"), ["2timeout"]);
  assert.deepEqual(p.items("an-steps"), ["explore  75%, 3 of 4"]);
  const rows = p.node("an-versions").kids.map((tr) => tr.kids.map((td) => td.textContent));
  assert.deepEqual(rows, [["v0.7.22", "0.13.0", "3", "66.7%", "10 min", "1k"], ["v0.7.21", "0.12.80", "1", "", "", ""]]);
});

test("a host without the report has no ten-day section, and is asked once a minute", async () => {
  const p = await page({ top: 404 });
  assert.deepEqual(p.errors, []);
  assert.equal(p.hidden("analytics"), true);
  assert.equal(p.analyticsReads(), 1);
  await p.tick(5);
  assert.equal(p.analyticsReads(), 1);
  await p.fire("visibilitychange");
  assert.equal(p.analyticsReads(), 2);
});

// What follows pins the harness itself: that it runs the module it means to,
// that it reaches what it claims to and no more, and that one copy of the
// module can draw two pages.

const ROOT = new URL("../", import.meta.url);
const SOURCE = readFileSync(new URL("host/fun/dashboard-view.js", ROOT), "utf8");

test("the harness has the module itself, and the module draws only when started", () => {
  // An import that resolved to nothing, or to some other file, has no start().
  assert.deepEqual(Object.keys(view), ["start"]);
  assert.equal(typeof view.start, "function");
  // Importing with no document drew nothing and left no page behind.
  assert.equal(typeof globalThis.document, "undefined");
  assert.deepEqual(globalThis.TEHAS_DASHBOARD, { probe: "state" });
});

test("the module's imports are each exercised, by a page or by their own test", async () => {
  // Every import of the module, by specifier. A fourth one fails this case
  // until a case here draws what it brings in, or names the test that does.
  const imports = [...SOURCE.matchAll(/^import .* from "([^"]+)";$/gm)].map((m) => m[1]);
  assert.deepEqual(imports, ["./host-data.js", "./console-tail.js", "./analytics-data.js"]);
  // console-tail.js is not exercised here: these cases stub the tabs so they
  // name no console file, and the tail is the half its own test covers
  // (test/console_tail_test.mjs). The two files a page draws are named by what
  // only they can draw.
  const byPage = imports.filter((s) => s !== "./console-tail.js");
  const p = await page({ analytics: report() });
  assert.deepEqual(p.errors, []);
  // Each import, seen through what only it draws: host-data.js's layout() writes
  // the host note, analytics-data.js's analyticsView() the ten-day window.
  const exercised = [
    p.text("host-note").startsWith("4 CPUs, load 0.10 0.20 0.30") && "./host-data.js",
    p.text("analytics-note") === "2026-10-23 to 2026-10-24, UTC" && "./analytics-data.js",
  ].filter(Boolean);
  assert.equal(exercised.length, byPage.length);
  assert.deepEqual(exercised, byPage);
});

test("a page asks for each file it reads once, and for no other", async () => {
  const p = await page();
  assert.deepEqual(p.errors, []);
  assert.deepEqual(p.asked().sort(), [...ANSWERED].sort());
});

test("the console half is stubbed out, and stays out", async () => {
  const p = await page();
  const { tabs } = p.surface();
  assert.equal(String(tabs.consoleFile), "() => null");
  await p.tick(3);
  // tail() was never reached: no file was asked for beyond the five, and the
  // console was never written.
  assert.deepEqual(p.errors, []);
  assert.ok(p.asked().every((url) => ANSWERED.includes(url)));
  assert.equal(p.node("console"), undefined);
  assert.equal(p.node("console-note"), undefined);
});

test("the stub stays as small as it is", async () => {
  const p = await page();
  const { globals, node } = p.surface();
  assert.deepEqual([...globals].sort(), [...GLOBALS].sort());
  assert.deepEqual(node, NODE);
  for (const name of ["children", "focus", "checked", "scrollTop", "scrollHeight", "arrayBuffer", "import" + "Node", "TextDecoder"]) {
    assert.ok(!globals.includes(name) && !node.includes(name), name + " is in the stub");
  }
  // No DOM shim and no dependency: this file imports node's own modules and the
  // module under test, nothing else.
  const own = readFileSync(new URL(import.meta.url), "utf8");
  const specifiers = [...own.matchAll(/^import .* from "([^"]+)";$/gm)].map((m) => m[1]);
  assert.deepEqual(specifiers.filter((s) => !s.startsWith("node:")), ["../host/fun/dashboard-view.js"]);
});

test("no page or test reaches past the DOM the stub has", () => {
  // The names are put together here so that this file does not hold them.
  const banned = { host: ["inner" + "HTML"], test: [] };
  for (const name of ["attach" + "Shadow", "shadow" + "Root", "adopted" + "StyleSheets", "import" + "Node"]) {
    banned.host.push(name);
    banned.test.push(name);
  }
  const files = (dir) => readdirSync(dir).flatMap((f) => {
    const path = new URL(f, dir);
    return statSync(path).isDirectory() ? files(new URL(f + "/", dir)) : [path];
  });
  for (const [dir, names] of Object.entries(banned)) {
    for (const path of files(new URL(dir + "/", ROOT))) {
      const body = readFileSync(path, "latin1");
      for (const name of names) assert.ok(!body.includes(name), path.pathname + " holds " + name);
    }
  }
});

test("two pages from one copy of the module share no state", async () => {
  // One page on a host with no snapshot, one on a host with one; both from the
  // single import above. Each is turned on its own before either is read.
  const missed = await page({ top: 404 });
  const served = await page();
  // The miss sets the first page's hostMissedAt: it does not ask again at the
  // next refreshes. The second page's own hostMissedAt is still 0, so it asks
  // at each one, and its answers do not clear the first page's miss.
  await missed.tick(3);
  await served.tick(3);
  // Coming back to the first page clears its miss and asks again at once; the
  // 404 sets it again. The second page keeps asking.
  await missed.fire("visibilitychange");
  await served.tick(1);
  // A tab chosen on the first page is the first page's selected, not the second's.
  await missed.choose(7);
  await served.tick(1);

  assert.deepEqual(missed.errors, []);
  assert.deepEqual(served.errors, []);
  assert.equal(missed.topReads(), 2);
  assert.equal(missed.hidden(), true);
  assert.equal(served.topReads(), 6);
  assert.equal(served.hidden(), false);
  assert.equal(served.items("host-list").length, 3);
  assert.equal(missed.chosen().at(-1), 7);
  assert.ok(served.chosen().length > 1);
  assert.ok(served.chosen().every((issue) => issue === null));
});

test("a read-only factory says so and lists what its starts left, with the pull request text", async () => {
  const p = await page({ status: { readOnly: { on: true, starts: [
    { issue: 7, how: "cycle", at: new Date(T * 1000).toISOString(), workspace: "/h/work/widget-7",
      pr: { title: "Fix the thing", branch: "feature/issue-7", body: "The body." } },
    { issue: 9, how: "cycle", at: new Date(T * 1000).toISOString(), workspace: "/h/work/widget-9" },
  ] } } });
  assert.deepEqual(p.errors, []);
  assert.equal(p.hidden("readonly"), false);
  assert.equal(p.hidden("ro"), false);
  assert.equal(p.text("n-ro"), "(2)");
  const rows = p.items("ro-list");
  assert.match(rows[0], /^#7Fix the thing feature\/issue-7 in \/h\/work\/widget-7, /);
  assert.match(rows[0], /The body\.$/);
  assert.match(rows[1], /^#9cycle started, no pull request yet \/h\/work\/widget-9, /);
});

test("a factory that is not read-only shows no read-only pill or section", async () => {
  const p = await page();
  assert.deepEqual(p.errors, []);
  assert.equal(p.hidden("readonly"), true);
  assert.equal(p.hidden("ro"), true);
  assert.deepEqual(p.items("ro-list"), []);
});
