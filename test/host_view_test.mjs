// Tests the host view: layout(), the pure step from live/top.json to what is
// drawn, the rule that says when a page asks for the file again, and the half of
// host-view.js that fetches and draws, against a page of stubs. Both pages that
// show the host - the panel on the factory floor and the plain dashboard - draw
// from the same module (host/fun/host-data.js), so these cases are what the two
// have in common.
//
//   node --test test/host_view_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { agoText, dueToRead, layout, MISS_RETRY, PERIOD, ROWS, UNITS, uptime } from "../host/fun/host-data.js";

const T = 1791526545;
const NOW = T * 1000 + 5000; // five seconds after the file was written

function file(over = {}) {
  return {
    generatedAt: new Date(T * 1000).toISOString(),
    factory: { status: "pass" },
    queue: { counts: { inLine: 1 } },
    cycle: { cycles: [] },
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
      { id: "tehas-dispatch.timer", loadState: "loaded", activeState: "active", subState: "waiting",
        unitFileState: "enabled", next: "10:00:00" },
      { id: "tehas-cycle@12.service", loadState: "loaded", activeState: "active", subState: "running",
        unitFileState: "", next: "" },
    ],
    energy: null,
    ...over,
  };
}

test("the readings of a fresh file", () => {
  const v = layout(file(), NOW);
  assert.deepEqual(v.state, "live");
  assert.deepEqual(v.present, true);
  assert.deepEqual(v.cpus, "4 CPUs");
  assert.deepEqual(v.load, ["0.10", "0.20", "0.30"]);
  assert.deepEqual([v.cpu, v.cpuFilled], ["12.5%", 1]);
  assert.deepEqual(v.memory, { used: "1600", total: "6400", percent: 25, filled: 3 });
  assert.deepEqual(v.rows, [
    { pid: 300, name: "pi-rukas", cpu: "40%", mem: "7.3%", up: "16 min" },
    { pid: 200, name: "tehas-cycle", cpu: "2%", mem: "4.9%", up: "1 min" },
    // No difference between the two readings of /proc: a dash, never a zero.
    { pid: 100, name: "systemd", cpu: "--", mem: "1.8%", up: "1 d" },
  ]);
  assert.deepEqual(v.label, "Host: 4 cpus, using 12.5% of its cpus.");
});

test("no host section is a stale view with no rows", () => {
  const none = { ...file() };
  delete none.host;
  const cases = [file({ host: null }), file({ host: undefined }), none, file({ host: "not a section" })];
  for (const data of cases) {
    const v = layout(data, NOW);
    assert.deepEqual(v.state, "stale");
    assert.deepEqual(v.present, false);
    assert.deepEqual([v.cpus, v.cpu, v.memory, v.rows], ["-", "--", null, []]);
    assert.deepEqual(v.cpuFilled, 0);
  }
});

test("a file the publisher has not rewritten is stale, up to the second it is due", () => {
  const at = (ms) => layout(file(), T * 1000 + ms).state;
  assert.deepEqual([at(0), at(PERIOD * 1000), at(30000)], ["live", "live", "live"]);
  assert.deepEqual([at(30001), at(45000)], ["stale", "stale"]);
  // A file with no time at all is not fresh however new its contents look.
  assert.deepEqual(layout(file({ generatedAt: undefined }), NOW).state, "stale");
});

test("a reading that is missing shows a dash and an empty gauge, not a zero", () => {
  const v = layout(file({ host: { cpus: null, load: null, cpuPercent: null, memory: null, processes: [] } }), NOW);
  assert.deepEqual([v.cpus, v.cpu, v.cpuFilled, v.memory], ["-", "--", 0, null]);
  assert.deepEqual(v.load, ["-", "-", "-"]);
  assert.deepEqual(v.state, "live"); // the section is there and recent, only blank
  // Memory with nothing in it divides by nothing.
  const zero = layout(file({ host: { cpus: 1, memory: { totalMb: 0, usedMb: 0 } } }), NOW);
  assert.deepEqual(zero.memory, null);
});

test("the rows are the ones the snapshot holds, cut to fifteen", () => {
  const many = Array.from({ length: 40 }, (_, i) => ({ pid: i + 1, name: `p${i}`, cpuPercent: 1, memPercent: 1, upSeconds: 10 }));
  assert.deepEqual(layout(file({ host: { ...file().host, processes: many } }), NOW).rows.length, ROWS);
  const ragged = [
    { pid: 1, name: "kept", cpuPercent: 1, memPercent: 1, upSeconds: 5 },
    null,
    "not a row",
    { pid: "x", name: "", cpuPercent: 1 },
    { pid: 2, name: "x".repeat(200), cpuPercent: null, memPercent: null, upSeconds: null },
    { pid: "not a number", name: "nopid", cpuPercent: 1, memPercent: 1, upSeconds: 5 },
  ];
  const v = layout(file({ host: { ...file().host, processes: ragged } }), NOW);
  assert.deepEqual(v.rows.length, 3);
  assert.deepEqual([v.rows[0].name, v.rows[0].pid], ["kept", 1]);
  // A name is text, cut to a length; a pid that is not a number stays no
  // reading, which the draw turns into a dash.
  assert.deepEqual([v.rows[1].name.length, v.rows[1].pid, v.rows[1].cpu, v.rows[1].up], [64, 2, "--", "-"]);
  assert.deepEqual(v.rows[2].pid, null);
});

test("the units are the ones the snapshot lists, cut to a bound", () => {
  const v = layout(file(), NOW);
  assert.deepEqual(v.units, [
    { name: "tehas-dispatch.timer", installed: "enabled", state: "active", next: "10:00:00" },
    // No unit file installed, no timer to fire: a dash and an empty next.
    { name: "tehas-cycle@12.service", installed: "-", state: "active", next: "" },
  ]);
  const ragged = [null, "not a unit", { id: "", activeState: "active" }, { activeState: "active" },
    { id: "tehas-real.timer", activeState: "inactive" }];
  const bad = layout(file({ units: ragged }), NOW);
  assert.deepEqual(bad.units.length, 1);
  assert.deepEqual(bad.units[0], { name: "tehas-real.timer", installed: "-", state: "inactive", next: "" });
  // No units section at all is no rows, not a guess about the host.
  assert.deepEqual(layout(file({ units: null }), NOW).units, []);
  const many = Array.from({ length: 60 }, (_, i) => ({ id: `tehas-u${i}.timer`, activeState: "active" }));
  assert.deepEqual(layout(file({ units: many }), NOW).units.length, UNITS);
});

test("a process name with markup in it stays text", () => {
  const hostile = [
    { pid: 7, name: "<img src=x onerror=alert(1)>", cpuPercent: 1, memPercent: 1, upSeconds: 5 },
    { pid: 8, name: "<script>alert(1)</script>".repeat(10), cpuPercent: 1, memPercent: 1, upSeconds: 5 },
    { pid: 9, name: '"><b>not markup</b>', cpuPercent: 1, memPercent: 1, upSeconds: 5 },
  ];
  const v = layout(file({ host: { ...file().host, processes: hostile } }), NOW);
  assert.deepEqual(v.rows.map((r) => r.name), [
    "<img src=x onerror=alert(1)>",
    "<script>alert(1)</script>".repeat(10).slice(0, 64),
    '"><b>not markup</b>',
  ]);
});

test("three load figures or none", () => {
  const at = (load) => layout(file({ host: { ...file().host, load } }), NOW).load;
  assert.deepEqual(at([1, 2, 3]), ["1.00", "2.00", "3.00"]);
  assert.deepEqual(at([0.5, 0.5]), ["-", "-", "-"]);
  assert.deepEqual(at([1, 2, "three"]), ["-", "-", "-"]);
});

test("how long a process has been running", () => {
  assert.deepEqual([uptime(0), uptime(59.4), uptime(60), uptime(3599), uptime(3600), uptime(86399), uptime(86400), uptime(90000)],
    ["0 s", "59 s", "1 min", "59 min", "1 h", "23 h", "1 d", "1 d"]);
  assert.deepEqual([uptime(null), uptime(-5), uptime("60")], ["-", "-", "-"]);
});

// The plain dashboard says when the snapshot was written in these words, and the
// terminal view says how long ago a section is in the same buckets.
test("how long ago, in the words both pages use", () => {
  assert.deepEqual(layout(file(), NOW).ageSeconds, 5);
  assert.deepEqual(agoText(layout(file(), NOW).ageSeconds), "just now");
  assert.deepEqual([agoText(0), agoText(89), agoText(90), agoText(3600), agoText(5400), agoText(90000), agoText(200000)],
    ["just now", "just now", "2 min ago", "60 min ago", "2 h ago", "25 h ago", "2 d ago"]);
  // A snapshot stamped just ahead of the clock it is read with has no negative
  // age. A figure that is not a number says nothing.
  assert.deepEqual([agoText(-3), agoText(null), agoText("60")], ["just now", "", ""]);
  // The age is measured against the moment the page got, which is the server's.
  assert.deepEqual(layout(file(), T * 1000 + 45000).ageSeconds, 45);
  assert.deepEqual(layout(file({ generatedAt: undefined }), NOW).ageSeconds, null);
});

// When a page asks for the file again. Both pages ask with this one rule.
test("a read is not asked for while the page is hidden or just after a miss", () => {
  assert.deepEqual(dueToRead({ hidden: false, missedAt: 0 }, NOW), true);
  // Nothing on screen wants a fresh answer while the page is behind another one.
  assert.deepEqual(dueToRead({ hidden: true, missedAt: 0 }, NOW), false);
  assert.deepEqual(dueToRead({ hidden: true, missedAt: 0 }, NOW + 3600 * 1000), false);
  // A file that was not there: quiet until MISS_RETRY has passed, then once.
  assert.deepEqual(dueToRead({ hidden: false, missedAt: NOW }, NOW), false);
  assert.deepEqual(dueToRead({ hidden: false, missedAt: NOW }, NOW + MISS_RETRY * 1000 - 1), false);
  assert.deepEqual(dueToRead({ hidden: false, missedAt: NOW }, NOW + MISS_RETRY * 1000), true);
  // A read that got a file asks again at the next period.
  assert.deepEqual(dueToRead({ hidden: false, missedAt: 0 }, NOW + PERIOD * 1000), true);
  // What a page that says nothing about itself gets: shown, and no miss.
  assert.deepEqual(dueToRead(), true);
});

// The half that fetches and draws. It runs only where there is a document, so a
// case gives the module one: a page of stubs holding every element the script
// names except those listed, a file to read, and no timers that outlive the
// case. Each page gets a fresh copy of the module (the ?key on the import).
const VIEW = ["host", "menu-host", "floor-host"];

async function page(missing, key, opt = {}) {
  const { status = 200, ok = true, unreachable = false } = opt;
  const found = new Map(), errors = [], labels = {}, inserted = new Map();
  const timers = [], listeners = {};
  let reads = 0, scheduled = 0;
  // The rows one table was given, so that a case can read a row back: its cells
  // and what was appended to it.
  const rowsOf = (id) => (inserted.has(id) ? inserted.get(id) : inserted.set(id, []).get(id));
  const node = (id) => ({
    id, hidden: false, dataset: {}, textContent: "", className: "",
    setAttribute: (name, value) => { if (name === "aria-label") labels[id] = value; },
    // Children are counted so a gauge can be read: how many blocks it holds and
    // how many of them are filled.
    replaceChildren: (...kids) => { kids.length && (found.set(`${id}-kids`, kids.length), found.set(`${id}-used`, kids.filter((k) => k.className === "is-used").length)); },
    append(...kids) { (this.kids ??= []).push(...kids); },
    insertRow: () => { const r = node(`${id}-row`); rowsOf(id).push(r); return r; },
    insertCell() { const c = node(`${id}-cell`); (this.cells ??= []).push(c); return c; },
  });
  const note = (e) => errors.push(String(e.message ?? e));
  process.on("unhandledRejection", note);
  const real = Object.fromEntries(["document", "window", "performance", "fetch", "setInterval", "clearInterval"].map((k) => [k, globalThis[k]]));
  const stubs = {
    document: {
      hidden: false,
      getElementById: (id) => {
        if (missing.includes(id)) return null;
        let el = found.get(id);
        if (!el) found.set(id, (el = node(id)));
        return el;
      },
      addEventListener: (name, fn) => { (listeners[name] ??= []).push(fn); },
      createElement: (tag) => node(tag),
    },
    window: { matchMedia: () => ({ matches: true }) },
    performance: { now: () => 0 },
    setInterval: (fn, ms) => { scheduled++; timers.push({ fn, ms }); return 0; },
    clearInterval: () => {},
    fetch: async () => {
      reads++;
      if (unreachable) throw new Error("no route to the host");
      return { status, ok, json: async () => (opt.file || file)(), headers: { get: () => new Date(T * 1000).toUTCString() } };
    },
  };
  // The page is there while the module runs and while a case turns the schedule
  // of its own, and not in between: no other case sees these globals.
  const install = () => Object.assign(globalThis, stubs);
  const uninstall = () => Object.assign(globalThis, real);
  const settle = async () => {
    // The module does not await its own refresh: turn the loop until its chain
    // (read, json, draw) has run. A case that ran out of turns would find no
    // read and no row, which the reads assertion of every case rules out.
    for (let i = 0; i < 40; i++) await new Promise((r) => setImmediate(r));
  };
  install();
  try {
    await import(`../host/fun/host-view.js?${key}`);
    await settle();
    uninstall();
    const text = (id) => found.get(id)?.textContent;
    return {
      errors, labels, scheduled, text,
      // A live count, not the number of reads when this was handed back: a case
      // that turns the schedule sees what those turns read.
      get reads() { return reads; },
      // One table at a time: how many rows it was given, the text of the cells
      // of each row, and the text of the header cell each row builds.
      rows: (id) => rowsOf(id).length,
      cellText: (id) => rowsOf(id).map((r) => (r.cells ?? []).map((c) => c.textContent)),
      headers: (id) => rowsOf(id).map((r) => (r.kids ?? []).find((k) => k.id === "th")?.textContent),
      kids: (id) => found.get(`${id}-kids`) ?? 0,
      used: (id) => found.get(`${id}-used`) ?? 0,
      // The three elements the view shows or hides. An element the page does not
      // have reads "absent", which no element the page has can answer.
      hidden: () => VIEW.map((id) => (found.has(id) ? found.get(id).hidden : "absent")),
      // The read the view schedules for itself and the events it listens for, so
      // that a case can ask what happens after the first answer.
      tick: async (n) => { install(); for (let i = 0; i < n; i++) { for (const t of timers) await t.fn(); await settle(); } uninstall(); },
      fire: async (name) => { install(); for (const fn of listeners[name] ?? []) await fn(); await settle(); uninstall(); },
    };
  } finally {
    process.off("unhandledRejection", note);
    for (const [k, v] of Object.entries(real)) globalThis[k] = v;
  }
}

test("a page with its section, its link and its head chip is drawn", async () => {
  const p = await page([], "whole");
  assert.deepEqual(p.errors, []);
  assert.equal(p.reads, 1); // the file was read: what follows is a draw, not a wait
  assert.deepEqual(p.hidden(), [false, false, false]); // all three shown: the file is there
  assert.deepEqual(p.text("host-cpus"), "4 CPUs");
  assert.deepEqual(p.text("host-load"), "0.10 0.20 0.30");
  assert.deepEqual([p.text("host-cpu"), p.used("host-cpu-gauge")], ["12.5%", 1]);
  assert.deepEqual([p.text("host-mem"), p.text("host-mem-percent"), p.used("host-mem-gauge")], ["1600 of 6400 MB", "25%", 3]);
  assert.deepEqual(p.text("host-state"), "LIVE");
  assert.equal(p.rows("host-rows"), 3); // one row per process of the file
  assert.equal(p.rows("host-unit-rows"), 2); // and one per unit
  // The head chip says what the panel says, in the short form.
  assert.deepEqual([p.text("floor-host-cpu"), p.labels["floor-host"]], ["12.5%", "Host: 4 cpus, using 12.5% of its cpus. Opens Host."]);
});

test("the units are drawn below the processes", async () => {
  const p = await page([], "units");
  assert.deepEqual(p.errors, []);
  assert.deepEqual([p.rows("host-rows"), p.rows("host-unit-rows")], [3, 2]);
  assert.deepEqual(p.headers("host-unit-rows"), ["tehas-dispatch.timer", "tehas-cycle@12.service"]);
  // installed, state, next firing.
  assert.deepEqual(p.cellText("host-unit-rows"), [["enabled", "active", "10:00:00"], ["-", "active", ""]]);
});

test("a page without the units table still draws the processes", async () => {
  const p = await page(["host-unit-rows"], "no-units");
  assert.deepEqual(p.errors, []);
  assert.equal(p.rows("host-rows"), 3);
  assert.deepEqual(p.hidden(), [false, false, false]);
});

test("a page that does not have the parts is left alone", async () => {
  for (const missing of [["host"], ["menu-host"], ["floor-host"], ["host", "menu-host", "floor-host"]]) {
    const p = await page(missing, `missing-${missing.join("-")}`);
    assert.deepEqual(p.errors, []);
    assert.equal(p.reads, 1); // the read happens: the guard is in the draw
    assert.deepEqual(p.hidden(), VIEW.map((id) => (missing.includes(id) ? "absent" : false)));
    // Nothing was written to any element the page does have.
    assert.deepEqual(p.rows("host-rows"), 0);
    assert.deepEqual(p.rows("host-unit-rows"), 0);
    assert.deepEqual(p.text("host-cpu"), undefined);
  }
});

test("a page missing one figure of the draw still draws the rest", async () => {
  // The panel is written figure by figure. A variant of the page without one of
  // them loses that figure and nothing else: no assignment to what is not there.
  for (const gone of ["host-cpus", "host-load", "host-mem-percent", "host-state", "floor-host-cpu", "host-rows"]) {
    const p = await page([gone], `partial-${gone}`);
    assert.deepEqual(p.errors, []);
    assert.equal(p.reads, 1);
    assert.deepEqual(p.hidden(), [false, false, false]); // the panel is still shown
    assert.equal(p.rows("host-rows"), gone === "host-rows" ? 0 : 3);
    assert.equal(p.rows("host-unit-rows"), 2);
    assert.deepEqual(p.text(gone), undefined);
    assert.deepEqual(p.text("host-cpu"), gone === "host-cpu" ? undefined : "12.5%");
  }
});

test("no published file takes the section and its links off the page", async () => {
  const p = await page([], "gone", { status: 404, ok: false });
  assert.deepEqual(p.errors, []);
  assert.equal(p.reads, 1);
  assert.deepEqual(p.hidden(), [true, true, true]);
  assert.equal(p.rows("host-rows"), 0);
  assert.deepEqual(p.text("host-cpu"), undefined);
});

test("a reading with nothing in it draws dashes, not zeros and not blanks", async () => {
  const blank = () => file({ host: { cpus: null, load: null, cpuPercent: null, memory: null,
    processes: [{ pid: null, name: "nopid", cpuPercent: null, memPercent: null, upSeconds: null }] } });
  const p = await page([], "blank", { file: blank });
  assert.deepEqual(p.errors, []);
  assert.equal(p.rows("host-rows"), 1);
  // pid, cpu, memory, up: the name is the header cell of the row.
  assert.deepEqual(p.cellText("host-rows"), [["-", "--", "--", "-"]]);
  assert.deepEqual(p.headers("host-rows"), ["nopid"]);
  assert.deepEqual([p.text("host-cpus"), p.text("host-load"), p.text("host-cpu"), p.text("host-mem")],
    ["-", "- - -", "--", "-"]);
  assert.deepEqual([p.kids("host-cpu-gauge"), p.used("host-cpu-gauge")], [10, 0]);
});

test("a host that cannot be reached keeps what was on screen and retries", async () => {
  const p = await page([], "unreachable", { unreachable: true });
  assert.deepEqual(p.errors, []); // the module caught it rather than rejecting
  assert.equal(p.reads, 1);
  assert.deepEqual(p.hidden(), [true, true, true]); // nothing was ever drawn
  assert.deepEqual(p.scheduled, 1); // the read is scheduled again, not abandoned
  await p.tick(2);
  assert.equal(p.reads, 3); // a host that may come back is asked again every period
});

test("a file that is not there at all is not asked for every five seconds", async () => {
  const p = await page([], "absent", { status: 404, ok: false });
  assert.equal(p.reads, 1);
  await p.tick(3); // three more marks of the interval the view schedules
  assert.equal(p.reads, 1); // 404 every time: asking again is work for nothing
  await p.fire("visibilitychange"); // coming back to the page asks once more
  assert.equal(p.reads, 2);
});

test("a process name with markup in it is drawn as the text of its row", async () => {
  const name = "<img src=x onerror=alert(1)>";
  const hostile = () => file({ host: { ...file().host, processes: [{ pid: 7, name, cpuPercent: 1, memPercent: 1, upSeconds: 5 }] } });
  const p = await page([], "markup", { file: hostile });
  assert.deepEqual(p.errors, []);
  assert.equal(p.rows("host-rows"), 1);
  // Whole, and as the text of the row's header cell: never markup the page
  // builds.
  assert.deepEqual(p.headers("host-rows"), [name]);
  assert.deepEqual(p.cellText("host-rows"), [["7", "1%", "1%", "5 s"]]);
});
