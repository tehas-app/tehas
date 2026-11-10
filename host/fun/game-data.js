// The factory floor as data: an export or the queue snapshot in, the tickets and
// the words the page draws out. host/fun/game-view.js draws what these return
// and keeps the drawing, the polling and the event handlers.
//
// Pure: they open no file, ask no command and read no clock. A test hands them a
// file and a moment (test/game_view_test.mjs).
//
// Ticket text is untrusted: it comes from a file the viewer loads or from the
// factory's queue snapshot. Each piece is cut to a bound here, and the page sets
// it only through textContent, never as markup.
import { stageOf as stage } from "./stages.js";

// Stages follow the labels tehas itself puts on issues (see console/src/queue.rs):
// the order and the labels are host/fun/stages.js, the names are the floor's.
export const STAGES = {
  ready: "IN LINE",
  running: "BUILDING",
  review: "REVIEW",
  stopped: "STOPPED",
  backlog: "NOT ON THE LINE",
  shipped: "SHIPPED",
};
export const LINE = ["ready", "running", "review", "shipped"];
// Energy is estimated, not measured: time spent multiplied by an assumed draw
// for the machines doing the work. An export can replace the assumption with
// "assumedKw", or give a measured "kWh" per issue.
export const DEFAULT_KW = 0.5;
export const TITLE_MAX = 180; // a ticket's title
export const REASON_MAX = 200; // the reason a pause was asked for
export const WORD_MAX = 40; // the words a round of Buzzword Buster is built from
export const WORD_LEN = 10; // one word, cleaned

// The floor's own names for the steps of the one table (host/fun/stages.js),
// each named by its step: a step the table gains and the floor does not name
// is drawn as nothing, not as the stage beside it. The floor has no planning
// of its own, so the table is read without that step and the step needs no
// name here: tehas:ready still reads ready beside tehas:plan. A ticket no
// label of tehas answers for is not on the line, which is the floor's own
// stage, not a step of the table.
const FLOOR_STAGES = { stopped: "stopped", review: "review", building: "running", line: "ready" };

export function stageOf(labels, closed) {
  const step = stage(labels, closed, { planning: false });
  // A closed ticket has left the line, and a ticket no label of tehas answers
  // for is not on it: both are stages of the floor's own, no step of the
  // table. The rest keep their name from the list above.
  if (step === null) return "backlog";
  if (step === "shipped") return "shipped";
  return FLOOR_STAGES[step] || "backlog";
}

// The pause, as tehasbot status writes it: {flag: {at, reason} or null, label}.
// Only the reason is shown, and only on the page: `by` is not in the snapshot.
export function pauseInfo(raw) {
  const p = raw && raw.pause;
  if (!p || typeof p !== "object") return null;
  const flag = p.flag && typeof p.flag === "object" ? p.flag : null;
  const reason = flag && typeof flag.reason === "string" ? flag.reason.slice(0, REASON_MAX) : "";
  return { reason, flag: Boolean(flag), label: p.label === true };
}

// One list of tickets from a file: the issues it names, plus the ones it names
// as shipped, each cut to what the page can draw. A ticket with no number, no
// title, or one that repeats a number already taken, is left out.
export function normalize(raw) {
  const list = Array.isArray(raw) ? raw : raw && Array.isArray(raw.issues) ? raw.issues : null;
  if (!list) throw new Error("Expected an issues array");
  const shippedList = raw && Array.isArray(raw.shippedIssues) ? raw.shippedIssues : [];
  const seen = new Set();
  const tickets = [];
  const add = (item, forceClosed) => {
    const number = Number(item && item.number);
    if (!item || item.pull_request || !Number.isSafeInteger(number) || number <= 0) return;
    if (typeof item.title !== "string" || seen.has(number)) return;
    seen.add(number);
    const labels = (Array.isArray(item.labels) ? item.labels : [])
      .map((l) => String(typeof l === "string" ? l : (l && l.name) || "").toLowerCase())
      .filter(Boolean);
    const closed = forceClosed || String(item.state || "").toLowerCase() === "closed";
    const url = typeof item.url === "string" && item.url.startsWith("https://") ? item.url : null;
    const when = (v) => (typeof v === "string" && !Number.isNaN(Date.parse(v)) ? v : null);
    const amount = (v) => (Number.isFinite(Number(v)) && Number(v) > 0 ? Number(v) : null);
    tickets.push({
      number, title: item.title.slice(0, TITLE_MAX), labels, url, stage: stageOf(labels, closed),
      createdAt: when(item.createdAt), updatedAt: when(item.updatedAt), closedAt: when(item.closedAt),
      tokens: amount(item.tokens), seconds: amount(item.timeSpentSeconds), kWh: amount(item.kWh), runs: amount(item.runs),
    });
  };
  for (const item of list) add(item, false);
  for (const item of shippedList) add(item, true);
  return {
    tickets,
    paused: Boolean(raw && raw.paused),
    pause: pauseInfo(raw),
    noResources: Boolean(raw && raw.noResources),
    kw: raw && Number(raw.assumedKw) > 0 ? Number(raw.assumedKw) : DEFAULT_KW,
    sample: Boolean(raw && raw.sampleData),
    words: collectWords(raw, tickets),
  };
}

// The words of the tickets, for the wall of Buzzword Buster. A file that names
// its own counts wins: the queue snapshot counts every issue of the repository,
// not only the tickets on the line. Otherwise they are counted out of the
// titles and labels here.
export function collectWords(raw, tickets) {
  const clean = (w) => String(w).toLowerCase().replace(/[^a-z0-9-]/g, "").slice(0, WORD_LEN);
  const explicit = raw && Array.isArray(raw.buzzwords) ? raw.buzzwords : [];
  const counts = new Map();
  const stop = new Set(("the and for with from this that should would could when while into onto your their there about " +
    "after before also have has had are was were will can not but all any our you its use make add show fix new more " +
    "each than then them through off under over between across tehas ready running awaiting-merge").split(" "));
  if (explicit.length) {
    for (const item of explicit) {
      const said = String(typeof item === "string" ? item : (item && item.word) || "").toLowerCase();
      const word = clean(said);
      if (word.length < 3 || stop.has(said)) continue;
      counts.set(word, (counts.get(word) || 0) + Math.max(1, Number(item && item.count) || 1));
    }
  } else {
    for (const t of tickets) {
      for (const word of (t.title + " " + t.labels.join(" ")).toLowerCase().match(/[a-z][a-z0-9-]{2,}/g) || []) {
        if (!stop.has(word)) counts.set(clean(word), (counts.get(clean(word)) || 0) + 1);
      }
    }
  }
  return [...counts].map(([word, count]) => ({ word, count }))
    .sort((a, b) => b.count - a.count || a.word.localeCompare(b.word)).slice(0, WORD_MAX);
}

// The words for how long ago an ISO moment was, beside agoText: the game view's
// stamps come from an export or the queue and need no work of their own. A
// stamp that is empty or not a moment says nothing, which is a blank beside a
// ticket rather than "NaN d ago".
export { ago } from "./host-data.js";

// The tickets a factory still finishes: the ones it has on the line at the
// building stage.
export function finishingIssues(tickets) {
  return tickets.filter((t) => t.stage === "running").map((t) => t.number);
}
