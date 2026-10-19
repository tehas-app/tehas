// Runs the plain dashboard's own script against a page of stubs, to see the
// half that shows the host: the list it draws from live/top.json, and when it
// asks for that file; and the "Last 10 days" section it draws from
// live/analytics.json. The dashboard is a fragment served inside a page, so this
// case takes the one inline module out of host/docs/dashboard.frag and runs it
// as the page would. No browser and no server: the document, the fetches and the
// timers are stubs, and the file is the one host_view_test.mjs describes.
//
//   node --test test/dashboard_host_test.mjs
import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { test } from "node:test";
import { tmpdir } from "node:os";
import { join } from "node:path";

const FRAG = new URL("../host/docs/dashboard.frag", import.meta.url);
const DATA = new URL("../host/fun/host-data.js", import.meta.url);
const ANALYTICS = new URL("../host/fun/analytics-data.js", import.meta.url);

// The one inline module of the fragment, with its imports of /fun/host-data.js
// and /fun/analytics-data.js pointed at the files in this checkout.
function dashboardSource(key) {
  const lines = readFileSync(FRAG, "utf8").split("\n");
  const from = lines.findIndex((l) => l.trim() === '<script type="module">');
  const to = from < 0 ? -1 : lines.findIndex((l, i) => i > from && l.trim() === "</script>");
  if (from < 0 || to < 0) throw new Error("the fragment holds no module script");
  const body = lines.slice(from + 1, to).join("\n")
    .replace('from "/fun/host-data.js"', `from "${DATA.href}"`)
    .replace('from "/fun/analytics-data.js"', `from "${ANALYTICS.href}"`);
  mkdirSync(join(tmpdir(), "tehas-dashboard-test"), { recursive: true });
  const file = join(tmpdir(), "tehas-dashboard-test", `dashboard-${key}.mjs`);
  writeFileSync(file, body);
  return `file://${file}`;
}

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

async function page(key, opt = {}) {
  const { top = 200, unreachable = false } = opt;
  const byId = new Map(), listeners = {}, timers = [], errors = [];
  let hidden = false, topReads = 0, analyticsReads = 0;
  const node = (name) => {
    const el = { name, hidden: false, dataset: {}, style: {}, attrs: {}, textContent: "", className: "", href: "", kids: [] };
    el.setAttribute = (k, v) => { el.attrs[k] = String(v); };
    el.append = (...kids) => { el.kids.push(...kids); };
    el.replaceChildren = (...kids) => { el.kids = kids; };
    return el;
  };
  const note = (e) => errors.push(String(e.message ?? e));
  process.on("unhandledRejection", note);
  const names = ["document", "fetch", "setInterval", "setTimeout", "clearInterval", "TEHAS_CONSOLE_TABS"];
  const real = Object.fromEntries(names.map((k) => [k, globalThis[k]]));
  const stubs = {
    document: {
      get hidden() { return hidden; },
      getElementById: (id) => { let el = byId.get(id); if (!el) byId.set(id, (el = node(id))); return el; },
      addEventListener: (name, fn) => { (listeners[name] ??= []).push(fn); },
      createElement: (tag) => node(tag),
      createElementNS: (_ns, tag) => node(tag),
    },
    // The tabs are the plain dashboard's other module. Here they say nothing is
    // running, so what a case sees is the host half.
    TEHAS_CONSOLE_TABS: { view: () => ({ running: 0, shown: null, cycles: [], tabs: [] }), render: () => {}, keys: () => {}, consoleFile: () => null },
    fetch: async (url) => {
      if (url.endsWith("/live/top.json")) topReads++;
      if (url.endsWith("/live/analytics.json")) analyticsReads++;
      if (unreachable) throw new Error("no route to the host");
      if (url.endsWith("/live/top.json")) {
        return top === 200
          ? { ok: true, status: 200, json: async () => (opt.topFile || file)(), headers: { get: () => new Date(T * 1000).toUTCString() } }
          : { ok: false, status: top, json: async () => ({}), headers: { get: () => "" } };
      }
      if (url.endsWith("/live/analytics.json")) {
        return opt.analytics
          ? { ok: true, status: 200, json: async () => opt.analytics, headers: { get: () => "" } }
          : { ok: false, status: 404, json: async () => ({}), headers: { get: () => "" } };
      }
      // No previews and no running cycle on this host: the host section is what
      // these cases are about.
      if (url.endsWith("/live/previews.json") || url.endsWith("/live/running.json")) {
        return { ok: false, status: 404, json: async () => ({}), headers: { get: () => "" } };
      }
      return { ok: true, status: 200, json: async () => ({ ...status(), ...opt.status }), headers: { get: () => "" } };
    },
    // The page's own clock: the refresh it schedules is turned by a case, so no
    // timer outlives one.
    setInterval: (fn, ms) => { timers.push({ fn, ms }); return 0; },
    setTimeout: (fn) => { timers.push({ fn, ms: 0 }); return 0; },
    clearInterval: () => {},
  };
  const install = () => Object.assign(globalThis, stubs);
  const settle = async () => { for (let i = 0; i < 60; i++) await new Promise((r) => setImmediate(r)); };
  install();
  try {
    await import(dashboardSource(key));
    await settle();
    const text = (id) => byId.get(id)?.textContent;
    // The text of each row of one list: its cells joined as the page shows them.
    const items = (id) => (byId.get(id)?.kids ?? []).map((li) => li.kids
      .map((k) => (typeof k === "string" ? k : k.textContent)).join(""));
    return {
      errors, text, items, topReads: () => topReads, analyticsReads: () => analyticsReads,
      node: (id) => byId.get(id),
      periods: () => timers.map((t) => t.ms),
      hidden: (id = "host") => byId.get(id)?.hidden,
      hide: () => { hidden = true; },
      show: () => { hidden = false; },
      tick: async (n) => { install(); for (let i = 0; i < n; i++) { for (const t of timers) await t.fn(); await settle(); } },
      fire: async (name) => { install(); for (const fn of listeners[name] ?? []) await fn(); await settle(); },
    };
  } finally {
    process.off("unhandledRejection", note);
    for (const [k, v] of Object.entries(real)) globalThis[k] = v;
  }
}

test("the plain dashboard lists the host it is given", async () => {
  const p = await page("list");
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

test("a host that publishes no snapshot has no host section", async () => {
  const p = await page("absent", { top: 404 });
  assert.deepEqual(p.errors, []);
  assert.equal(p.hidden(), true);
  assert.deepEqual(p.items("host-list"), []);
});

test("the dashboard asks for the host file only while the page is shown", async () => {
  const p = await page("hidden");
  assert.equal(p.topReads(), 1);
  p.hide();
  await p.tick(3); // three marks of the refresh it schedules
  assert.equal(p.topReads(), 1); // nothing on screen wants a fresh answer
  p.show();
  await p.fire("visibilitychange");
  assert.equal(p.topReads(), 2); // coming back asks at once
});

test("a file that is not there is not asked for at every refresh", async () => {
  const p = await page("miss", { top: 404 });
  assert.equal(p.topReads(), 1);
  await p.tick(5);
  assert.equal(p.topReads(), 1); // once a minute, not at every period
  await p.fire("visibilitychange");
  assert.equal(p.topReads(), 2);
});

test("a host that cannot be reached keeps what was on screen", async () => {
  const p = await page("down", { unreachable: true });
  assert.deepEqual(p.errors, []); // the script caught it rather than rejecting
  assert.equal(p.hidden(), true); // nothing was ever drawn
});

test("all-hands is shown beside the state only while the snapshot says so", async () => {
  const on = await page("allhands-on", { top: 404, status: { allHands: true } });
  assert.deepEqual(on.errors, []);
  assert.equal(on.hidden("allhands"), false);
  for (const allHands of [false, null, undefined]) {
    const off = await page("allhands-off-" + allHands, { top: 404, status: { allHands } });
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
  const p = await page("analytics", { top: 404, analytics: report() });
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
  const p = await page("analytics-absent", { top: 404 });
  assert.deepEqual(p.errors, []);
  assert.equal(p.hidden("analytics"), true);
  assert.equal(p.analyticsReads(), 1);
  await p.tick(5);
  assert.equal(p.analyticsReads(), 1);
  await p.fire("visibilitychange");
  assert.equal(p.analyticsReads(), 2);
});
