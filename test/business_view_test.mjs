// Tests for host/fun/business-view.js: the business page's line, board and
// live strip as data, from files like the ones the dispatcher writes. No
// browser: the module draws only where a page has the line.
import { test } from "node:test";
import assert from "node:assert/strict";
import { STAGES, STALE_QUEUE, lineView, boardRows, liveView, labBase } from "../host/fun/business-view.js";

const NOW = Date.parse("2026-10-20T12:00:00Z");
const at = (secondsAgo) => new Date(NOW - secondsAgo * 1000).toISOString();
const issue = (number, title = `Ticket ${number}`) => ({ number, title, url: `https://github.com/acme/widget/issues/${number}` });
const status = (extra = {}) => ({
  generatedAt: at(5), repo: "acme/widget", paused: false,
  queued: [issue(13), issue(14)], running: [issue(23)], awaiting: [issue(18)], stopped: [issue(2)], completed: [issue(83)],
  ...extra,
});

test("the line runs prototypes, in line, building, review, stopped, shipped", () => {
  assert.deepEqual(STAGES.map((s) => s.key), ["proto", "line", "build", "review", "stop", "ship"]);
  const v = lineView({ status: status(), nowMs: NOW });
  assert.deepEqual(v.stages.map((s) => [s.key, s.count]), [["proto", 0], ["line", 2], ["build", 1], ["review", 1], ["stop", 1], ["ship", 1]]);
  assert.equal(v.source, "live");
  assert.equal(v.repo, "https://github.com/acme/widget");
  assert.deepEqual(v.stages[1].items[0], issue(13));
});

test("each stage links to its tickets on GitHub, the stopped one by what it says", () => {
  const v = lineView({ status: status(), nowMs: NOW });
  const line = v.stages.find((s) => s.key === "line");
  assert.deepEqual(line.more, { text: "All 2 in line",
    href: "https://github.com/acme/widget/issues?q=" + encodeURIComponent("is:open label:tehas:ready") });
  assert.equal(v.stages.find((s) => s.key === "stop").more.text, "See what stopped");
  assert.equal(v.stages.find((s) => s.key === "proto").more, null); // nothing there, and no Protolab
});

test("a queue older than ten minutes is stale, and a pause is shown", () => {
  const v = lineView({ status: status({ generatedAt: at(STALE_QUEUE + 1), paused: true }), nowMs: NOW });
  assert.equal(v.source, "stale");
  assert.equal(v.paused, true);
  assert.equal(lineView({ status: status({ generatedAt: "not a date" }), nowMs: NOW }).source, "stale");
});

test("ticket text is cut and anything but a GitHub link is dropped", () => {
  const v = lineView({ status: status({
    queued: [{ number: 7, title: "x".repeat(300), url: "javascript:alert(1)" }, { number: "nine", title: "No number" }, { number: 8 }],
  }), nowMs: NOW });
  const line = v.stages.find((s) => s.key === "line");
  assert.equal(line.count, 1);
  assert.equal(line.items[0].title.length, 180);
  assert.equal(line.items[0].url, null);
});

test("a repository name that is not owner/name gives no links", () => {
  const v = lineView({ status: status({ repo: "acme/widget/../../evil" }), nowMs: NOW });
  assert.equal(v.repo, null);
  assert.ok(v.stages.every((s) => s.more === null));
});

test("prototypes come from the Protolab's list, at its own address or under the lab", () => {
  const lab = { url: "https://lab.example.org", tickets: [
    { number: 21, title: "Visual diff", address: "https://lab-21.example.org/" },
    { number: 22, title: "Replay" },
    { number: 0, title: "Not a ticket" },
  ] };
  const v = lineView({ status: status(), lab, nowMs: NOW });
  const proto = v.stages.find((s) => s.key === "proto");
  assert.deepEqual(proto.items.map((t) => [t.number, t.url]), [[21, "https://lab-21.example.org/"], [22, "https://lab.example.org/22/"]]);
  assert.deepEqual(proto.more, { text: "All 2 in Protolab", href: "https://lab.example.org" });
  assert.equal(v.lab, "https://lab.example.org");
});

test("a Protolab with no address of its own is at port 8081 of a plain HTTP page", () => {
  assert.equal(labBase({ tickets: [] }, "http://factory.example:8080/"), "http://factory.example:8081");
  assert.equal(labBase({ tickets: [] }, "https://factory.example.org/"), null);
  assert.equal(labBase({ url: "javascript:x", tickets: [] }, "https://factory.example.org/"), null);
});

test("the sample is staged by its labels, and only where there is no queue", () => {
  const sample = { issues: [
    { number: 1, title: "Ready", labels: [{ name: "tehas:ready" }] },
    { number: 2, title: "Running prototype", labels: ["tehas:running", "tehas:prototype"] },
    { number: 3, title: "Stuck", labels: [{ name: "needs-human-attention" }, { name: "tehas:ready" }] },
    { number: 4, title: "Done", state: "CLOSED", labels: [] },
    { number: 5, title: "Backlog", labels: [{ name: "bug" }] },
  ] };
  const v = lineView({ sample, nowMs: NOW });
  assert.equal(v.source, "sample");
  assert.deepEqual(v.stages.map((s) => s.items.map((t) => t.number)), [[2], [1], [2], [], [3], [4]]);
  assert.equal(lineView({ status: status(), sample, nowMs: NOW }).stages[1].count, 2); // the queue wins
  assert.equal(lineView({ nowMs: NOW }).source, "none");
});

test("a ticket the queue lists twice stands at one place: stopped first, then review, building, in line", () => {
  const v = lineView({ status: status({ queued: [issue(2), issue(13), issue(18)], running: [issue(18)] }), nowMs: NOW });
  assert.deepEqual(v.stages.map((s) => s.items.map((t) => t.number)), [[], [13], [], [18], [2], [83]]);
});

// The shipped figure is the label's total, so it keeps rising past the fifty
// items the list holds. Its "all" link carries the same number and opens the
// stage's own search, which is the plain `is:closed` list and so counts more
// than the labelled figure does.
test("shipped counts what the label says, not the fifty the list holds", () => {
  const fifty = Array.from({ length: 50 }, (_, i) => issue(i + 100));
  const v = lineView({ status: status({ completed: fifty, completedCount: 138 }), nowMs: NOW });
  const ship = v.stages.find((s) => s.key === "ship");
  assert.equal(ship.count, 138);
  assert.equal(ship.items.length, 50);
  assert.deepEqual(ship.more, { text: "All 138 shipped",
    href: "https://github.com/acme/widget/issues?q=" + encodeURIComponent("is:closed") });
  // The other stages keep counting their own list.
  assert.equal(v.stages.find((s) => s.key === "line").count, 2);
});

test("without the count the shipped figure is the length of the list, as on an older host", () => {
  const shipOf = (extra) => lineView({ status: status(extra), nowMs: NOW }).stages.find((s) => s.key === "ship");
  for (const missing of [{}, { completedCount: null }, { completedCount: "138" },
    { completedCount: 1.5 }, { completedCount: -3 }, { completedCount: NaN },
    { completedCount: Infinity }, { completedCount: -Infinity }]) {
    const ship = shipOf({ completed: [issue(83)], ...missing });
    assert.equal(ship.count, 1, JSON.stringify(missing));
    assert.deepEqual(ship.more, { text: "All 1 shipped",
      href: "https://github.com/acme/widget/issues?q=" + encodeURIComponent("is:closed") });
  }
  // Nothing listed and the count says none: an empty stage keeps no link.
  assert.equal(shipOf({ completed: [], completedCount: 0 }).more, null);
  // Nothing listed and no count: as it was before the field.
  assert.equal(shipOf({ completed: [] }).more, null);
});

// The read set is one named list, not whatever string a row of STAGES names:
// a count field the producer of status.json does not publish is a bug in one
// of the two files, not a lookup the page should make. A stage whose figure is
// its list's length names no total, so it reads no field of the file.
test("the only count field the page reads is the shipped one", () => {
  assert.deepEqual(STAGES.filter((s) => s.total).map((s) => [s.key, s.total]), [["ship", "completedCount"]]);
  const ship = lineView({
    status: status({ completed: [], completedCount: 1e400 }), nowMs: NOW,
  }).stages.find((s) => s.key === "ship");
  assert.equal(ship.count, 0);
  assert.equal(ship.more, null);
});

test("the board lists the open tickets on the line, in the line's order", () => {
  const rows = boardRows(lineView({ status: status(), nowMs: NOW }));
  assert.deepEqual(rows.map((r) => [r.number, r.stage]), [[13, "line"], [14, "line"], [23, "build"], [18, "review"], [2, "stop"]]);
});

const energy = (extra = {}) => ({
  generatedAt: at(4), hosts: [{ name: "gpu-1", watts: 180 }], totals: { watts: 180, kWh24h: 2.05, kWh30d: 29.82, reporting: 1 },
  samples: [0, 1, 2].map((i) => ({ t: (NOW - (30 - i * 15) * 1000) / 1000, w: { "gpu-1": 150 + i * 10 }, k: { "gpu-1": 1000 + i * 3000 } })),
  ...extra,
});

test("the live strip has the power now, the day and month, and the token rate", () => {
  const v = liveView(energy(), NOW);
  assert.equal(v.present, true);
  assert.equal(v.stale, false);
  assert.equal(v.watts, "180");
  assert.equal(v.kwh30, "29.8");
  assert.equal(v.power.length, 120);
  assert.deepEqual(v.tokenRates, [12000, 12000]); // 3000 tokens in 15 s
});

test("without a file there is no strip; a stale one shows no reading", () => {
  assert.deepEqual(liveView(null, NOW), { present: false });
  assert.deepEqual(liveView({ totals: {} }, NOW), { present: false });
  const v = liveView(energy({ generatedAt: at(120) }), NOW);
  assert.equal(v.stale, true);
  assert.equal(v.watts, "--");
});

test("a mesh that counts no tokens gives no token figure", () => {
  const plain = energy();
  for (const s of plain.samples) delete s.k;
  const v = liveView(plain, NOW);
  assert.equal(v.tokens, null);
  assert.deepEqual(v.tokenRates, []);
});
