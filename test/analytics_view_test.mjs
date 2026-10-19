// Tests the "Last 10 days" view: analyticsView(), the pure step from
// live/analytics.json to what the plain dashboard draws, its formats, and the
// rule that says when the page reads the file again.
//
//   node --test test/analytics_view_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { analyticsView, dueToRead, duration, kwh, percent, READ_EVERY, tokens } from "../host/fun/analytics-data.js";

const days = (rows) => Array.from({ length: 10 }, (_, i) => ({
  day: `2026-10-${15 + i}`, shipped: 0, runs: 0, failed: 0, minutes: 0, ...(rows[i] || {}),
}));

// The report of the analytics program's known data set (console/analytics/src/tests.rs).
function report(over = {}) {
  return {
    report: 1, from: 1792022400, to: 1792843200,
    shipped: { n: 3 },
    planned: { n: 6, inLine: 5, planning: 1, trend: days({}).map(({ day }, i) => ({ day, ...({ 1: { n: 1 }, 9: { n: 6 } })[i] })) },
    runs: { n: 6, pr: 1, merged: 1, handoff: 1, parked: 0, failed: 2, refused: 1, stopped: 0 },
    planningTurns: { n: 1 },
    handoffRate: { n: 6, share: 0.167 },
    firstPassYield: { n: 2, share: 0.5 },
    leadTime: { n: 2, median: 43400, p90: 83600 },
    cycleTime: { n: 6, median: 750, p90: 1800 },
    machineTime: { n: 6, seconds: 4800, perShipped: 1600 },
    tokens: { n: 4, total: 6650, input: 6100, output: 500, cacheRead: 50, perShipped: 2217 },
    availability: { n: 5, seconds: 734400, share: 0.002 },
    energy: { n: 4, kwh: 0.075, perShipped: 0.025 },
    daily: days({ 0: { shipped: 1, runs: 3, minutes: 60 }, 1: { shipped: 1, runs: 2, failed: 2, minutes: 20 }, 2: { shipped: 1, runs: 1 } }),
    stopReasons: { n: 4, top: [{ reason: "explore-bodies-empty", n: 1 }, { reason: "other", n: 1 }, { reason: "refused", n: 1 }, { reason: "timeout", n: 1 }] },
    stations: { n: 5, steps: [
      { step: "explore", n: 5, passed: 3, share: 0.6 },
      { step: "handoff", n: 1, passed: 1, share: 1 },
      { step: "implement", n: 3, passed: 2, share: 0.667 },
      { step: "review", n: 1, passed: 1, share: 1 },
    ] },
    versions: [
      { tehas: "v1", rukas: "0.13.0", n: 4, prShare: 0.5, medianSeconds: 900, medianTokens: 1750 },
      { tehas: "v2", n: 1 },
      { tehas: "v2", rukas: "0.13.0", n: 1 },
    ],
    ...over,
  };
}

test("the row of figures, each with its count", () => {
  const v = analyticsView(report());
  assert.equal(v.present, true);
  assert.equal(v.window, "2026-10-15 to 2026-10-24, UTC");
  assert.deepEqual(v.figures.map((f) => [f.label, f.value, f.note]), [
    ["Shipped", "3", "issues merged, by day below"],
    ["Planned", "6", "5 in line, 1 in planning"],
    ["Runs", "6", "2 PR, 1 handoff, 0 parked, 2 failed, 1 refused"],
    ["First-pass yield", "50%", "in one run, of shipped, n=2"],
    ["Lead time", "12.1 h", "median; p90 23.2 h, n=2"],
    ["Cycle time", "13 min", "median per run; p90 30 min, n=6"],
    ["Machine time", "80 min", "27 min per shipped, n=6"],
    ["Tokens", "6.7k", "2.2k per shipped, n=4"],
    ["Availability", "0.2%", "could take work, of 8.5 d, n=5 samples"],
    ["Energy", "0.025 kWh", "per shipped; 0.075 kWh in all, n=4 samples"],
  ]);
  // Shipped per day under the figure, and the planned trend: shares of the largest.
  assert.deepEqual(v.figures[0].bars, [1, 1, 1, 0, 0, 0, 0, 0, 0, 0]);
  assert.deepEqual(v.figures[1].bars, [0, 1 / 6, 0, 0, 0, 0, 0, 0, 0, 1]);
});

test("the ten-day chart: shipped and failed bars on one scale, minutes on its own", () => {
  const v = analyticsView(report());
  assert.equal(v.chart.length, 10);
  assert.deepEqual(v.chart[1], {
    day: "2026-10-16", label: "16", shipped: 1, failed: 2, runs: 2, minutes: 20,
    shippedHeight: 0.5, failedHeight: 1, minutesHeight: 1 / 3,
    title: "2026-10-16: 1 shipped, 2 of 2 runs failed, 20 cycle-minutes",
  });
  assert.equal(v.chart[0].minutesHeight, 1);
  assert.deepEqual(v.chart.slice(3).map((d) => d.shippedHeight + d.failedHeight + d.minutesHeight), [0, 0, 0, 0, 0, 0, 0]);
});

test("stop reasons, station yield and the version table", () => {
  const v = analyticsView(report());
  assert.deepEqual(v.reasons.map((r) => [r.reason, r.n]), [["explore-bodies-empty", 1], ["other", 1], ["refused", 1], ["timeout", 1]]);
  assert.equal(v.reasonsNote, "of n=4 runs that ended without a pull request");
  assert.deepEqual(v.steps.map((s) => [s.step, s.share, s.text]), [
    ["explore", 0.6, "60%, 3 of 5"],
    ["handoff", 1, "100%, 1 of 1"],
    ["implement", 0.667, "66.7%, 2 of 3"],
    ["review", 1, "100%, 1 of 1"],
  ]);
  assert.deepEqual(v.versions, [
    { tehas: "v1", rukas: "0.13.0", runs: "4", pr: "50%", minutes: "15 min", tokens: "1.8k", few: false },
    // Fewer than three runs: the count only.
    { tehas: "v2", rukas: "unknown", runs: "1", pr: "", minutes: "", tokens: "", few: true },
    { tehas: "v2", rukas: "0.13.0", runs: "1", pr: "", minutes: "", tokens: "", few: true },
  ]);
});

test("a report without energy or samples leaves those figures out", () => {
  const r = report();
  delete r.energy;
  delete r.planned;
  r.availability = { n: 0 };
  r.firstPassYield = { n: 0 };
  r.shipped = { n: 0 };
  r.machineTime = { n: 0, seconds: 0 };
  const v = analyticsView(r);
  const labels = v.figures.map((f) => f.label);
  assert.equal(labels.includes("Energy"), false);
  assert.equal(labels.includes("Planned"), false);
  const byLabel = Object.fromEntries(v.figures.map((f) => [f.label, f]));
  assert.equal(byLabel["Availability"].value, "-");
  assert.equal(byLabel["Availability"].note, "could take work, n=0 samples");
  assert.equal(byLabel["First-pass yield"].value, "-");
  assert.equal(byLabel["Machine time"].note, "none shipped, n=0");
});

test("empty lists say so", () => {
  const v = analyticsView(report({ stopReasons: { n: 0, top: [] }, stations: { n: 0, steps: [] }, versions: [] }));
  assert.deepEqual([v.reasons, v.steps, v.versions], [[], [], []]);
  assert.equal(v.reasonsNote, "No run ended without a pull request.");
  assert.equal(v.stepsNote, "No run recorded its steps.");
  assert.equal(v.versionsNote, "No work run ended.");
});

test("no file, or a file that is not a report of this version, hides the section", () => {
  for (const data of [null, undefined, "text", [], {}, { report: 2, to: 1 }, { report: 1 }, { generatedAt: "x", queued: [] }]) {
    assert.deepEqual(analyticsView(data), { present: false });
  }
});

test("what the file says is text, cut, and odd values do not break the view", () => {
  const long = "x".repeat(500);
  const v = analyticsView(report({
    stopReasons: { n: 2, top: [{ reason: long, n: 1 }, { reason: { html: "<b>" }, n: 1 }, null] },
    versions: [{ tehas: long, rukas: 7, n: "lots" }],
    daily: [{ day: 5, shipped: -3, minutes: "x" }],
    runs: "none",
  }));
  assert.equal(v.reasons.length, 1);
  assert.equal(v.reasons[0].reason.length, 60);
  assert.deepEqual(v.versions[0], { tehas: "x".repeat(40), rukas: "unknown", runs: "0", pr: "", minutes: "", tokens: "", few: true });
  assert.deepEqual(v.chart.map((d) => [d.day, d.shipped, d.minutes]), [["", 0, 0]]);
  assert.equal(v.figures.find((f) => f.label === "Runs").note, "0 PR, 0 handoff, 0 parked, 0 failed");
});

test("formats", () => {
  assert.deepEqual([percent(0.5), percent(0.0021), percent(1), percent(null)], ["50%", "0.2%", "100%", "-"]);
  assert.deepEqual([duration(45), duration(750), duration(43400), duration(734400), duration(-1)],
    ["45 s", "13 min", "12.1 h", "8.5 d", "-"]);
  assert.deepEqual([tokens(950), tokens(6650), tokens(1234567), tokens(2.5e9), tokens(null)], ["950", "6.7k", "1.2M", "2.5G", "-"]);
  assert.deepEqual([kwh(0.075), kwh(12.34), kwh(undefined)], ["0.075 kWh", "12.3 kWh", "-"]);
});

test("the file is read at most once a minute, and not while the page is hidden", () => {
  const now = 1_000_000_000;
  assert.equal(dueToRead({}, now), true);
  assert.equal(dueToRead({ hidden: true }, now), false);
  assert.equal(dueToRead({ readAt: now - 1000 }, now), false);
  assert.equal(dueToRead({ readAt: now - READ_EVERY * 1000 }, now), true);
});
