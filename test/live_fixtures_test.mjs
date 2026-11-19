// The pages against test/fixtures/live/: one committed sample of each file under
// /live/ that a page in host/fun/ reads. Every file a page fetches has a
// sample, each sample goes through the page code that reads it, and the plain
// dashboard's island models make their sections from the samples. A page reads
// these files add-only (host/docs/frontend.md): a field of a sample is one a
// page may read, so a writer keeps it.
//
//   node --test test/live_fixtures_test.mjs
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";
import "../host/fun/console-tabs.js";
import "../host/fun/epics-view.js";
import "../host/fun/panels.js";
import "../host/fun/versions.js";
import { analyticsView } from "../host/fun/analytics-data.js";
import { boardRows, consoleView, labBase, lineView, liveView } from "../host/fun/business-view.js";
import { layout as energyLayout } from "../host/fun/energy-view.js";
import { normalize } from "../host/fun/game-data.js";
import { layout as hostLayout } from "../host/fun/host-data.js";
import { consoleModel } from "../host/fun/factory/tehas-factory-console.model.js";
import { daysModel } from "../host/fun/factory/tehas-factory-days.model.js";
import { hostModel } from "../host/fun/factory/tehas-factory-host.model.js";
import { issuesModel } from "../host/fun/factory/tehas-factory-issues.model.js";
import { previewsModel } from "../host/fun/factory/tehas-factory-previews.model.js";
import { readonlyModel } from "../host/fun/factory/tehas-factory-readonly.model.js";
import { stateModel } from "../host/fun/factory/tehas-factory-state.model.js";

const FIXTURES = new URL("./fixtures/live/", import.meta.url);
const FUN = new URL("../host/fun/", import.meta.url);
const read = (name) => JSON.parse(readFileSync(new URL(name, FIXTURES), "utf8"));
// The time the samples were written: 2026-11-17T12:00:00Z.
const T = 1794916800;
const NOW = T * 1000 + 5000;

test("every file under /live/ a page fetches has a sample, and every sample is fetched", () => {
  const fetched = new Set();
  for (const f of readdirSync(FUN).filter((n) => n.endsWith(".js"))) {
    const src = readFileSync(new URL(f, FUN), "utf8");
    for (const m of src.matchAll(/fetch\(\s*"\/live\/([a-z0-9-]+\.json)"/g)) fetched.add(m[1]);
    for (const m of src.matchAll(/read\(\s*"\/live\/([a-z0-9-]+\.json)"/g)) fetched.add(m[1]);
  }
  // The islands' feeds are named by their elements' attributes.
  const frag = readFileSync(new URL("../host/docs/dashboard.frag", import.meta.url), "utf8");
  for (const m of frag.matchAll(/ (?:src|running|status)="\/live\/([a-z0-9-]+\.json)"/g)) fetched.add(m[1]);
  const samples = readdirSync(FIXTURES).filter((n) => n.endsWith(".json")).sort();
  assert.deepEqual(samples, [...fetched].sort());
});

test("the samples are written at one time and name no real repository", () => {
  for (const name of readdirSync(FIXTURES)) {
    const text = readFileSync(new URL(name, FIXTURES), "utf8");
    for (const m of text.matchAll(/github\.com\/([^/"]+\/[^/"?]+)/g)) assert.equal(m[1], "acme/widget", name);
    const f = JSON.parse(text);
    const at = f.generatedAt ?? f.time;
    if (at !== undefined) assert.equal(Date.parse(at), T * 1000, name);
  }
});

test("status.json and protolab.json draw the business page's line", () => {
  const v = lineView({ status: read("status.json"), lab: read("protolab.json"), nowMs: NOW });
  assert.equal(v.source, "live");
  assert.equal(v.repo, "https://github.com/acme/widget");
  assert.equal(v.lab, "https://lab.example.org");
  assert.equal(labBase(read("protolab.json"), null), "https://lab.example.org");
  assert.deepEqual(v.stages.map((s) => [s.key, s.count]),
    [["proto", 1], ["line", 2], ["plan", 1], ["build", 1], ["review", 1], ["stop", 1], ["ship", 28]]);
  assert.deepEqual(v.stages[0].items[0].url, "https://lab-45.example.org/");
  assert.deepEqual(boardRows(v).map((r) => r.number), [42, 43, 41, 39, 37]);
});

test("status.json draws the factory floor's tickets", () => {
  const s = read("status.json");
  // As the floor's fromSnapshot() hands the queue to normalize().
  const issues = [];
  for (const [key, label] of [["queued", "tehas:ready"], ["running", "tehas:running"], ["awaiting", "tehas:awaiting-merge"], ["stopped", "needs-human-attention"]]) {
    for (const i of s[key]) issues.push({ ...i, state: "OPEN", labels: [label] });
  }
  for (const i of s.completed) issues.push({ ...i, state: "CLOSED", labels: [] });
  const data = normalize({ issues, paused: s.paused, pause: s.pause, noResources: s.noResources, buzzwords: s.buzzwords });
  assert.equal(data.tickets.length, 7);
  assert.deepEqual(data.tickets.map((t) => t.stage), ["ready", "ready", "running", "review", "stopped", "shipped", "shipped"]);
  assert.equal(data.paused, false);
  assert.ok(data.words.length > 0);
});

test("running.json draws the console tabs and the business page's console", () => {
  const { view } = globalThis.TEHAS_CONSOLE_TABS;
  const v = view(read("running.json"), null);
  assert.equal(v.running, 1);
  assert.equal(v.shown.cycle.issue, 41);
  const titles = new Map(read("status.json").running.map((i) => [i.number, i.title]));
  const c = consoleView(v, titles, T);
  assert.equal(c.state, "live");
  assert.equal(c.cycle, "Working on #41  Export the report as CSV");
  assert.equal(c.file, "console-41.txt");
});

test("energy.json draws the energy section and the business page's meter", () => {
  const v = energyLayout(read("energy.json"), NOW);
  assert.equal(v.state, "live");
  assert.equal(v.watts, "1190");
  assert.deepEqual(v.rows.map((r) => r.name), ["node-a", "node-b"]);
  assert.equal(v.models.length, 1);
  const live = liveView(read("energy.json"), NOW);
  assert.equal(live.present, true);
  assert.equal(live.stale, false);
  assert.ok(live.tokenRates.length > 0);
});

test("analytics.json draws the ten-day report", () => {
  const v = analyticsView(read("analytics.json"));
  assert.equal(v.present, true);
  assert.equal(v.chart.length, 10);
  assert.ok(v.figures.length >= 8);
  assert.equal(v.versions.length, 2);
});

test("top.json draws the host section", () => {
  const v = hostLayout(read("top.json"), NOW);
  assert.equal(v.rows.length, 3);
  assert.equal(v.units.length, 2);
});

test("epics.json, panels.json and health.json draw their parts", () => {
  const epics = globalThis.TEHAS_EPICS.view(read("epics.json"));
  assert.equal(epics.epics.length, 1);
  assert.equal(epics.epics[0].subs.length, 3);
  assert.deepEqual(globalThis.TEHAS_PANELS.view(read("panels.json"), "https://example.org"),
    [{ id: "support", label: "Support", url: "https://support.example.net/" }]);
  const { line, messages } = globalThis.TEHAS_VERSIONS;
  assert.equal(line(read("health.json")), "tehas v1.0.0 · pi-rukas v0.14.84 · manager v0.9.0");
  assert.deepEqual(messages(read("health.json"), NOW).map((m) => m.word), ["Notice"]);
});

// The plain dashboard: each island's model makes its section from the samples,
// as the islands do with what their feeds read (test/browser/dashboard-check.mjs
// draws them in a browser).
test("the plain dashboard's islands make every section from the samples", () => {
  const status = read("status.json"), running = read("running.json");
  const state = stateModel(status, running, NOW);
  assert.deepEqual([state.pill, state.state, state.fresh], ["pill run", "working", "queue read just now"]);
  assert.deepEqual(["queued", "awaiting", "stopped", "completed"].map((list) => issuesModel(status, { list }, NOW).rows.length),
    [2, 1, 1, 2]);
  assert.equal(readonlyModel(status, NOW).on, false);
  const c = consoleModel(running, status, null, NOW);
  assert.deepEqual([c.tabs.length, c.title, c.file], [2, "Working on #41  Export the report as CSV", "console-41.txt"]);
  const previews = previewsModel(read("previews.json"), NOW);
  assert.deepEqual([previews.present, previews.rows.length], [true, 3]);
  const host = hostModel(read("top.json"), NOW);
  assert.deepEqual([host.present, host.rows.length, host.units.length], [true, 3, 2]);
  const days = daysModel(read("analytics.json"));
  assert.deepEqual([days.present, days.chart.length, days.versions.length > 0], [true, 10, true]);
});
