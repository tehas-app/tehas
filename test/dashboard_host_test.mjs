// The models of the plain dashboard's islands (host/fun/factory/*.model.js):
// what each makes of the file it is given, with no document and no Solid. The
// state row, the issue lists, the read-only results and the previews. What
// the islands draw from them is checked in a browser (test/browser/dashboard-check.mjs).
//
//   node --test test/dashboard_host_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { consoleModel, duration } from "../host/fun/factory/tehas-factory-console.model.js";
import { MAX, chips, emptyText, issuesModel } from "../host/fun/factory/tehas-factory-issues.model.js";
import { previewWhy, previewsModel } from "../host/fun/factory/tehas-factory-previews.model.js";
import { readonlyModel } from "../host/fun/factory/tehas-factory-readonly.model.js";
import { stateModel } from "../host/fun/factory/tehas-factory-state.model.js";

const T = 1791526545;
const NOW = T * 1000;
const at = (secs = 0) => new Date((T - secs) * 1000).toISOString();

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

test("read-only is shown beside the state only while the snapshot says so", () => {
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
    { issue: 7, how: "cycle", at: at(120), workspace: "/h/work/widget-7", change: { title: "Fix the thing", branch: "feature/issue-7", body: "The body." } },
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
