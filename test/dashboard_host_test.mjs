// The models of the plain dashboard's islands (host/fun/factory/*.model.js):
// what each makes of the file it is given, with no document and no Solid. The
// state row, the issue lists, the read-only results, the previews, the host
// and the last ten days. What the islands draw from them is checked in a
// browser (test/browser/dashboard-check.mjs).
//
//   node --test test/dashboard_host_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { consoleModel, duration } from "../host/fun/factory/tehas-factory-console.model.js";
import { daysModel } from "../host/fun/factory/tehas-factory-days.model.js";
import { hostModel, serverNow } from "../host/fun/factory/tehas-factory-host.model.js";
import { MAX, chips, emptyText, issuesModel } from "../host/fun/factory/tehas-factory-issues.model.js";
import { previewWhy, previewsModel } from "../host/fun/factory/tehas-factory-previews.model.js";
import { readonlyModel } from "../host/fun/factory/tehas-factory-readonly.model.js";
import { stateModel } from "../host/fun/factory/tehas-factory-state.model.js";

const T = 1791526545;
const NOW = T * 1000;
const at = (secs = 0) => new Date((T - secs) * 1000).toISOString();

function top(over = {}) {
  return {
    generatedAt: at(),
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

function status(over = {}) {
  return {
    generatedAt: at(120),
    repo: "acme/widget",
    links: { all: "https://github.com/acme/widget/issues", queued: "q", awaiting: "a", stopped: "s", completed: "c" },
    running: [], queued: [], awaiting: [], stopped: [], completed: [], paused: false, noResources: false,
    ...over,
  };
}

const running = (issues) => ({ cycles: issues.map((issue) => ({ issue, running: true, startedAt: T - 600, step: "develop", console: `console-${issue}.txt` })), last: null });

test("the host is a list of its processes and units, with a note of its readings", () => {
  const m = hostModel(top(), NOW);
  assert.equal(m.present, true);
  assert.deepEqual(m.rows.map((r) => r.name + " " + r.text), [
    "pi-rukas pid 300  cpu 40%  mem 7.3%  up 16 min",
    "tehas-cycle pid 200  cpu 2%  mem 4.9%  up 1 min",
    "systemd pid 100  cpu --  mem 1.8%  up 1 d",
  ]);
  assert.deepEqual(m.units.map((u) => u.name + " " + u.text), [
    "tehas-dispatch.timer enabled  active  next 10:00:00",
    "tehas-cycle@12.service -  active",
  ]);
  assert.deepEqual([m.n, m.nUnits], ["(3)", "(2)"]);
  assert.equal(m.note, "4 CPUs, load 0.10 0.20 0.30, 12.5% of the cpus in use, memory 1600 of 6400 MB, snapshot written just now.");
});

test("the snapshot's age is judged by the server's clock, not the viewer's", () => {
  // The viewer's clock runs two hours ahead of the host's; the file is new.
  const viewer = NOW + 7200 * 1000;
  const meta = { status: 200, headers: {}, date: NOW, at: viewer };
  assert.equal(serverNow(meta, viewer), NOW);
  assert.match(hostModel(top(), serverNow(meta, viewer)).note, /, snapshot written just now\.$/);
  assert.match(hostModel(top(), viewer).note, /, snapshot is 2 h ago\.$/, "by the viewer's clock it would be stale");
  // The clock moves on from the answer by the time the viewer's has run.
  assert.equal(serverNow(meta, viewer + 40000), NOW + 40000);
  // A clock read older than the answer does not take the present back.
  assert.equal(serverNow(meta, viewer - 30000), NOW);
  // No Date header, no answer yet: the viewer's clock.
  assert.equal(serverNow({ status: 200, headers: {}, date: null, at: viewer }, viewer), viewer);
  assert.equal(serverNow(undefined, viewer), viewer);
});

test("an old snapshot is said to be old, and a host with no file has no section", () => {
  assert.match(hostModel(top({ generatedAt: at(7200) }), NOW).note, /, snapshot is 2 h ago\.$/);
  assert.match(hostModel(top({ generatedAt: "no" }), NOW).note, /, snapshot unreadable\.$/);
  assert.equal(hostModel(null, NOW).present, false);
  assert.equal(hostModel({ units: [] }, NOW).present, false);
  const empty = hostModel(top({ host: { cpus: 1 }, units: [] }), NOW);
  assert.deepEqual([empty.rows, empty.units, empty.n, empty.nUnits], [[], [], "", ""]);
});

test("the state says working, paused, finishing or idle", () => {
  assert.deepEqual([stateModel(status(), null, NOW).pill, stateModel(status(), null, NOW).state], ["pill idle", "idle"]);
  assert.equal(stateModel(status(), running([41]), NOW).state, "working");
  assert.equal(stateModel(status(), running([41, 42]), NOW).state, "working on 2");
  assert.equal(stateModel(status({ paused: true }), running([41, 42]), NOW).state, "paused, finishing #41, #42");
  assert.equal(stateModel(status({ paused: true }), null, NOW).pill, "pill pause");
  assert.equal(stateModel(status({ noResources: true }), null, NOW).state, "no resources");
  const m = stateModel(status(), null, NOW);
  assert.equal(m.fresh, "queue read 2 min ago");
  assert.equal(m.all, "https://github.com/acme/widget/issues");
});

test("all-hands and read-only are shown beside the state only while the snapshot says so", () => {
  assert.equal(stateModel(status({ allHands: true }), null, NOW).allHands, true);
  for (const allHands of [false, null, undefined, "yes"]) assert.equal(stateModel(status({ allHands }), null, NOW).allHands, false);
  assert.equal(stateModel(status({ readOnly: { on: true } }), null, NOW).readOnly, true);
  assert.equal(stateModel(status(), null, NOW).readOnly, false);
});

test("a list shows the first ten rows, its count, the time and a link to the rest", () => {
  const issue = (n) => ({ number: n, title: "t" + n, url: `https://github.com/acme/widget/issues/${n}`, updatedAt: at(3600), closedAt: at(120) });
  const many = Array.from({ length: 12 }, (_, i) => issue(i + 1));
  const m = issuesModel(status({ queued: many }), { list: "queued" }, NOW);
  assert.equal(m.title, "In line");
  assert.equal(m.n, "(12)");
  assert.equal(m.rows.length, MAX);
  assert.deepEqual(m.rows[0], { number: 1, title: "t1", href: "https://github.com/acme/widget/issues/1", notes: [], when: "60 min ago" });
  assert.deepEqual(m.more, { text: "all 12 on GitHub", href: "q" });
  // Completed rows tell the time they were closed.
  assert.equal(issuesModel(status({ completed: [issue(3)] }), { list: "completed" }, NOW).rows[0].when, "2 min ago");
  const empty = issuesModel(status(), { list: "stopped" }, NOW);
  assert.deepEqual([empty.n, empty.rows, empty.more], ["", [], null]);
  // What an empty list says comes from the list's name alone, as it is drawn
  // before the file is read.
  assert.equal(emptyText("stopped"), "Nothing is stopped.");
  // A list the element does not know of is the queue, not a key of the file.
  assert.equal(issuesModel(status(), { list: "__proto__" }, NOW).name, "queued");
  assert.equal(emptyText("__proto__"), "Nothing is waiting. Label an issue tehas:ready.");
});

test("a link of the queue file that is not a web address is not drawn as one", () => {
  const issue = (url) => ({ number: 3, title: "t", url, updatedAt: at() });
  const s = status({
    links: { all: "javascript:void 0", queued: "data:text/html,x" },
    queued: [issue("javascript:void 0"), ...Array.from({ length: 10 }, () => issue("https://github.com/acme/widget/issues/3"))],
  });
  const m = issuesModel(s, { list: "queued" }, NOW);
  assert.deepEqual(m.rows.map((r) => r.href), ["", ...Array(9).fill("https://github.com/acme/widget/issues/3")]);
  assert.equal(m.more.href, "");
  assert.equal(stateModel(s, null, NOW).all, "");
});

test("the host bridge's link and notes reach a row", () => {
  const host = {
    ticket: (repo, url) => (repo === "acme/widget" && url.endsWith("/42") ? "/host/tickets/42" : null),
    notes: (n) => (n === 42 ? [{ text: "2 votes", link: null }, { text: "chat", link: "https://chat.example.org/42" }, { text: "here", link: "/host/x" }] : []),
  };
  const s = status({ queued: [{ number: 42, title: "a", url: "https://github.com/acme/widget/issues/42" }, { number: 43, title: "b", url: "https://github.com/acme/widget/issues/43" }] });
  const [a, b] = issuesModel(s, { list: "queued" }, NOW, host).rows;
  assert.equal(a.href, "/host/tickets/42");
  assert.deepEqual(a.notes, [
    { text: "2 votes", href: null, away: false },
    { text: "chat", href: "https://chat.example.org/42", away: true },
    { text: "here", href: "/host/x", away: false },
  ]);
  assert.deepEqual([b.href, b.notes], ["https://github.com/acme/widget/issues/43", []]);
  assert.deepEqual(chips(undefined), []);
});

test("a read-only factory lists what its starts left, with the pull request text", () => {
  const m = readonlyModel(status({ readOnly: { on: true, starts: [
    { issue: 7, how: "cycle", at: at(120), workspace: "/h/work/widget-7", pr: { title: "Fix the thing", branch: "feature/issue-7", body: "The body." } },
    { issue: 9, how: "cycle", at: at(120), workspace: "/h/work/widget-9" },
  ] } }), NOW);
  assert.equal(m.on, true);
  assert.equal(m.n, "(2)");
  assert.deepEqual(m.rows.map((r) => [r.issue, r.text, r.where, r.body]), [
    [7, "Fix the thing", "feature/issue-7 in /h/work/widget-7, 2 min ago", "The body."],
    [9, "cycle started, no pull request yet", "/h/work/widget-9, 2 min ago", ""],
  ]);
  const off = readonlyModel(status(), NOW);
  assert.deepEqual([off.on, off.rows, off.n], [false, [], ""]);
  assert.deepEqual(readonlyModel(status({ readOnly: { on: true } }), NOW).rows, []);
});

test("the previews say when the list was written and why one is not serving", () => {
  const m = previewsModel({
    generatedAt: at(300),
    disk: { low: true, freeBytes: 1 },
    previews: [
      { name: "main", address: "https://uat-main.example.org/", state: "serving" },
      { number: 46, title: "Export", address: "https://uat-46.example.org/", state: "serving" },
      { number: 47, title: "", state: "build-failed", reason: " the build failed " },
      { number: 48, title: "Odd", address: "https://uat-048.example.org/", state: "serving" },
      { number: "49", title: "Not a number" },
      null,
    ],
  }, NOW);
  assert.equal(m.present, true);
  assert.equal(m.n, "(6)");
  assert.equal(m.note, "The host is low on disk: previews are not being built. List written 5 min ago.");
  assert.deepEqual(m.rows.map((r) => [r.num, r.title, r.href, r.why]), [
    ["main", "default branch", "https://uat-main.example.org/", ""],
    ["#46", "Export", "https://uat-46.example.org/", ""],
    ["#47", "pull request 47", null, "unable to create: the build failed"],
    ["#48", "Odd", null, "not serving"],
  ]);
  assert.equal(previewWhy({ state: "no-recipe" }), "no preview/Dockerfile in the commit");
  assert.equal(previewsModel({ disk: { low: true, freeBytes: null }, previews: [] }, NOW).note,
    "The host's free disk space could not be read: previews are not being built.");
  assert.equal(previewsModel(null, NOW).present, false);
  assert.equal(previewsModel({ previews: "no" }, NOW).present, false);
});

// A report as the analytics program prints it, cut short.
function report() {
  return {
    report: 1, from: 1792022400, to: 1792843200,
    shipped: { n: 2 },
    runs: { n: 4, pr: 2, merged: 0, handoff: 1, parked: 0, failed: 1, refused: 0, stopped: 0 },
    firstPassYield: { n: 2, share: 0.5 },
    leadTime: { n: 2, median: 3600, p90: 7200 },
    cycleTime: { n: 4, median: 600, p90: 900 },
    machineTime: { n: 4, seconds: 2400, perShipped: 1200 },
    tokens: { n: 4, total: 4000, perShipped: 2000 },
    availability: { n: 10, seconds: 3600, share: 0.9 },
    daily: [
      { day: "2026-10-23", shipped: 1, runs: 2, failed: 1, minutes: 20 },
      { day: "2026-10-24", shipped: 1, runs: 2, failed: 0, minutes: 10 },
    ],
    stopReasons: { n: 2, top: [{ reason: "timeout", n: 2 }] },
    stations: { n: 4, steps: [{ step: "explore", n: 4, passed: 3, share: 0.75 }] },
    versions: [
      { tehas: "v0.7.22", rukas: "0.13.0", n: 3, prShare: 0.667, medianSeconds: 600, medianTokens: 1000 },
      { tehas: "v0.7.21", rukas: "0.12.80", n: 1 },
    ],
  };
}

test("the last ten days are the report's figures, bars, line, reasons, steps and versions", () => {
  const m = daysModel(report());
  assert.equal(m.present, true);
  assert.equal(m.window, "2026-10-23 to 2026-10-24, UTC");
  assert.deepEqual(m.figures.map((f) => f.label + " " + f.value), ["Shipped 2", "Runs 4", "First-pass yield 50%",
    "Lead time 60 min", "Cycle time 10 min", "Machine time 40 min", "Tokens 4k", "Availability 90%"]);
  assert.deepEqual(m.figures[0].bars, ["100%", "100%"]);
  assert.deepEqual(m.figures[1].bars, []);
  assert.deepEqual(m.chart[0].bars, [{ cls: "shipped", height: "100%", text: "1" }, { cls: "failed", height: "100%", text: "1" }]);
  assert.deepEqual(m.chart[1].bars[1], { cls: "failed", height: "0%", text: "" });
  assert.equal(m.chart[0].label, "23");
  assert.equal(m.viewBox, "0 0 20 100");
  assert.equal(m.points, "5,0.0 15,50.0");
  assert.deepEqual(m.reasons, [{ reason: "timeout", n: 2 }]);
  assert.deepEqual(m.steps.map((s) => [s.step, s.width, s.text]), [["explore", "75%", "75%, 3 of 4"]]);
  assert.deepEqual(m.versions, [["v0.7.22", "0.13.0", "3", "66.7%", "10 min", "1k"], ["v0.7.21", "0.12.80", "1", "", "", ""]]);
  assert.equal(daysModel(null).present, false);
  assert.equal(daysModel({ report: 2 }).present, false);
});

test("the console shows the selected cycle, running or the last, and names its file", () => {
  const now = { cycles: [{ issue: 41, running: true, step: "develop", statusText: "", startedAt: T - 3720, console: "console-41.txt" }],
    last: { issue: 36, running: false, status: "pr", step: "handoff", startedAt: T - 9000, updatedAt: T - 4200, console: "../x" } };
  const s = status({ running: [{ number: 41, title: "Export" }], completed: [{ number: 36, title: "Login" }] });
  const m = consoleModel(now, s, null, NOW);
  assert.deepEqual(m.tabs, [{ issue: 41, label: "#41", on: true }, { issue: 36, label: "#36 ended", on: false }]);
  assert.equal(m.title, "Working on #41  Export");
  assert.equal(m.step, "step develop   (1 h 2 min so far)");
  assert.deepEqual([m.file, m.live], ["console-41.txt", true]);
  const last = consoleModel(now, s, 36, NOW);
  assert.equal(last.title, "Last cycle: #36  Login");
  assert.equal(last.step, "ended as pr at step handoff after 1 h 20 min");
  // A name that is not a plain file name under /live/ names no file.
  assert.deepEqual([last.file, last.live], [null, false]);
  assert.equal(consoleModel(null, s, null, NOW).shown, false);
  assert.equal(consoleModel(now, null, null, NOW).title, "Working on #41");
  assert.equal(duration(59), "0 min");
});
