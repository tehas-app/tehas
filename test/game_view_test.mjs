// Tests the factory floor's decisions: game-data.js, the pure half of
// host/fun/game-view.js. Which stage a ticket stands at, how a pause reads, what
// an export's untrusted text is cut to, which words the wall is built from, how
// long ago a moment was, and which tickets a factory still finishes. Behaviour,
// not source text: each case hands the module a file and a moment.
//
//   node --test test/game_view_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  ago, collectWords, DEFAULT_KW, finishingIssues, LINE, normalize, pauseInfo, REASON_MAX,
  stageOf, STAGES, TITLE_MAX, WORD_LEN, WORD_MAX,
} from "../host/fun/game-data.js";

const AT = Date.parse("2026-10-03T12:00:00Z"); // a fixed present: the tests name it
const since = (secs) => new Date(AT - secs * 1000).toISOString();

test("the labels tehas writes say which stage a ticket stands at", () => {
  assert.equal(stageOf(["tehas:ready"], false), "ready");
  assert.equal(stageOf(["tehas:running"], false), "running");
  assert.equal(stageOf(["tehas:awaiting-merge"], false), "review");
  assert.equal(stageOf(["needs-human-attention"], false), "stopped");
  // A label tehas does not write is not a stage.
  assert.equal(stageOf(["bug", "help wanted"], false), "backlog");
  assert.equal(stageOf([], false), "backlog");
});

test("a closed ticket has left the line, and a worse label answers first", () => {
  assert.equal(stageOf(["tehas:running"], true), "shipped");
  assert.equal(stageOf(["tehas:ready", "needs-human-attention"], false), "stopped");
  assert.equal(stageOf(["tehas:ready", "tehas:awaiting-merge"], false), "review");
  assert.equal(stageOf(["tehas:ready", "tehas:running"], false), "running");
});

test("the floor has no planning of its own, so tehas:ready still reads ready beside tehas:plan", () => {
  // The one place the { planning: false } of game-data.js is settled: a
  // ticket the line draws as planning stands ready here.
  assert.equal(stageOf(["tehas:plan"], false), "backlog");
  assert.equal(stageOf(["tehas:planning"], false), "backlog");
  assert.equal(stageOf(["tehas:plan", "tehas:ready"], false), "ready");
  // A worse label still answers first, planning or not.
  assert.equal(stageOf(["tehas:plan", "tehas:running"], false), "running");
  assert.equal(stageOf(["tehas:plan", "needs-human-attention"], false), "stopped");
});

test("every stage the data names has a name the page draws", () => {
  for (const stage of [...LINE, "stopped", "backlog"]) {
    assert.equal(typeof STAGES[stage], "string");
  }
});

test("a paused factory reads as its reason, its flag and its label", () => {
  const p = pauseInfo({ pause: { flag: { at: "2026-10-03T11:00:00Z", reason: "no quota", by: "someone" }, label: true } });
  assert.deepEqual(p, { reason: "no quota", flag: true, label: true });
  // Who asked for the pause is not in what the page is given.
  assert.equal("by" in p, false);
  assert.deepEqual(pauseInfo({ pause: { flag: null, label: false } }), { reason: "", flag: false, label: false });
  assert.deepEqual(pauseInfo({ pause: { label: true } }), { reason: "", flag: false, label: true });
  // A reason that is not words, and a flag that is not a section, say nothing.
  assert.deepEqual(pauseInfo({ pause: { flag: { reason: 7 } } }), { reason: "", flag: true, label: false });
  assert.deepEqual(pauseInfo({ pause: { flag: "held", label: 1 } }), { reason: "", flag: false, label: false });
});

test("no pause, or not one, is no pause to show", () => {
  for (const raw of [{}, { pause: null }, { pause: "paused" }, { pause: 1 }, null, undefined]) {
    assert.equal(pauseInfo(raw), null);
  }
  // A section that holds nothing is a pause with nothing to say about it.
  assert.deepEqual(pauseInfo({ pause: [] }), { reason: "", flag: false, label: false });
});

test("a pause reason is cut to a length", () => {
  const p = pauseInfo({ pause: { flag: { reason: "x".repeat(REASON_MAX + 120) } } });
  assert.equal(p.reason.length, REASON_MAX);
});

const issue = (n, over = {}) => ({ number: n, title: `Ticket ${n}`, state: "open", labels: [], ...over });

test("an export becomes the tickets the page can draw", () => {
  const raw = {
    issues: [
      issue(12, {
        title: "Fix the gate", labels: ["tehas:ready", "Bug"], url: "https://github.com/o/r/issues/12",
        createdAt: since(90000), updatedAt: since(60), tokens: 1200, timeSpentSeconds: 300, runs: 2, kWh: 0.4,
      }),
      issue(13, { labels: [{ name: "tehas:running" }] }),
      issue(962, { state: "CLOSED", closedAt: since(10) }),
      issue(16, { url: "http://evil.example/x" }),
      issue(17, { createdAt: "last tuesday", updatedAt: "" }),
      issue(18, { tokens: 0, timeSpentSeconds: -5, kWh: "abc" }),
    ],
    paused: true, pause: { flag: { reason: "no quota" }, label: false }, noResources: true, assumedKw: 0.8,
  };
  const v = normalize(raw);
  assert.deepEqual(v.tickets.map((t) => t.number), [12, 13, 962, 16, 17, 18]);
  assert.deepEqual(v.tickets.map((t) => t.stage), ["ready", "running", "shipped", "backlog", "backlog", "backlog"]);
  // Labels are words, lower case, whatever the file writes them as.
  assert.deepEqual(v.tickets[0].labels, ["tehas:ready", "bug"]);
  assert.deepEqual(v.tickets[1].labels, ["tehas:running"]);
  // A link is offered only for an https address the file names.
  assert.equal(v.tickets[0].url, "https://github.com/o/r/issues/12");
  assert.equal(v.tickets[3].url, null);
  // A moment the data cannot read is no moment, not a guess.
  assert.deepEqual([v.tickets[4].createdAt, v.tickets[4].updatedAt], [null, null]);
  assert.deepEqual([v.tickets[0].createdAt, v.tickets[0].updatedAt, v.tickets[2].closedAt], [since(90000), since(60), since(10)]);
  // Nothing spent, or not a number, is left out of the work order.
  assert.deepEqual([v.tickets[5].tokens, v.tickets[5].seconds, v.tickets[5].kWh], [null, null, null]);
  assert.deepEqual([v.tickets[0].tokens, v.tickets[0].seconds, v.tickets[0].kWh, v.tickets[0].runs], [1200, 300, 0.4, 2]);
  assert.deepEqual([v.paused, v.noResources, v.sample], [true, true, false]);
  assert.deepEqual(v.pause, { reason: "no quota", flag: true, label: false });
  assert.equal(v.kw, 0.8);
});

test("untrusted ticket text is cut to a length", () => {
  const v = normalize({ issues: [issue(13, { title: "x".repeat(TITLE_MAX + 220) })] });
  assert.equal(v.tickets[0].title.length, TITLE_MAX);
  assert.equal(v.tickets[0].title, "x".repeat(TITLE_MAX));
});

test("a ticket the data cannot place is left out", () => {
  const v = normalize({
    issues: [
      { number: 12, title: "First" },
      { number: 12, title: "Same number twice" },
      { number: 0, title: "No number" },
      { number: -4, title: "Negative" },
      { number: 1e21, title: "Not a safe integer" },
      { number: 3.5, title: "Not a whole number" },
      { number: 14 },
      { number: 15, title: "A pull request", pull_request: { url: "https://github.com/o/r/pull/15" } },
      { number: "16", title: "A number written as words" },
      null,
    ],
  });
  // A number written as words is still a number the page can place.
  assert.deepEqual(v.tickets.map((t) => t.number), [12, 16]);
});

test("the shipped list is tickets too, and it closes what it names", () => {
  const v = normalize({ issues: [], shippedIssues: [issue(20, { labels: ["tehas:running"], state: "open" })] });
  assert.deepEqual(v.tickets.map((t) => [t.number, t.stage]), [[20, "shipped"]]);
  // A list that is not one names nothing.
  assert.deepEqual(normalize({ issues: [], shippedIssues: "gone" }).tickets, []);
});

test("no issues array is not an export", () => {
  for (const raw of [{}, null, undefined, { issues: "x" }, { issues: {} }]) {
    assert.throws(() => normalize(raw), /Expected an issues array/);
  }
  // A bare list is the issues themselves, and an empty one is an export with
  // nothing on the line.
  assert.deepEqual(normalize([issue(1)]).tickets.map((t) => t.number), [1]);
  assert.deepEqual(normalize([]).tickets, []);
});

test("the energy basis is the export's, or the assumption", () => {
  assert.equal(normalize({ issues: [] }).kw, DEFAULT_KW);
  assert.deepEqual([normalize({ issues: [], assumedKw: 2 }).kw, normalize({ issues: [], assumedKw: 0 }).kw], [2, DEFAULT_KW]);
});

test("the words come out of the titles and labels when the data counts none", () => {
  const v = normalize({ issues: [issue(1, { title: "Gate check for the review board", labels: ["tehas:ready"] })] });
  assert.deepEqual(v.words, [
    { word: "board", count: 1 }, { word: "check", count: 1 }, { word: "gate", count: 1 }, { word: "review", count: 1 },
  ]);
});

test("counts the data brings itself win, and are summed per word", () => {
  const v = normalize({
    issues: [issue(1, { title: "gate" })],
    buzzwords: [{ word: "gate", count: 4 }, { word: "Gate" }, "check"],
  });
  assert.deepEqual(v.words, [{ word: "gate", count: 5 }, { word: "check", count: 1 }]);
});

test("plain words and words too short to read are no words", () => {
  assert.deepEqual(collectWords({ buzzwords: ["the", "tehas", "ready", "awaiting-merge", "ab", ""] }, []), []);
  const w = collectWords({ buzzwords: ["Supercalifragilistic"] }, []);
  assert.deepEqual(w, [{ word: "supercalif", count: 1 }]);
  assert.equal(w[0].word.length, WORD_LEN);
});

test("a round is built from at most this many words", () => {
  const buzzwords = Array.from({ length: WORD_MAX + 12 }, (_, i) => `word${String(i).padStart(3, "0")}`);
  assert.equal(collectWords({ buzzwords }, []).length, WORD_MAX);
});

test("how long ago, at a moment the caller names", () => {
  assert.deepEqual([ago(since(1), AT), ago(since(89), AT)], ["just now", "just now"]);
  assert.deepEqual([ago(since(90), AT), ago(since(600), AT)], ["2 min ago", "10 min ago"]);
  assert.deepEqual([ago(since(3600), AT), ago(since(5400), AT)], ["60 min ago", "2 h ago"]);
  assert.deepEqual([ago(since(86400), AT), ago(since(129600), AT)], ["24 h ago", "2 d ago"]);
  // A moment ahead of the one it is read with is now, not a negative age.
  assert.equal(ago(new Date(AT + 60000).toISOString(), AT), "just now");
  assert.deepEqual([ago(null, AT), ago("", AT)], ["", ""]);
});

test("the tickets a factory still finishes are the ones it is building", () => {
  const tickets = [
    { number: 7, stage: "running" }, { number: 9, stage: "review" },
    { number: 8, stage: "running" }, { number: 10, stage: "shipped" }, { number: 11, stage: "stopped" },
  ];
  assert.deepEqual(finishingIssues(tickets), [7, 8]);
  assert.deepEqual(finishingIssues([]), []);
});
