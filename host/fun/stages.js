// Which stage a ticket stands at, from the labels tehas writes (see
// console/src/queue.rs). The business page's line (business-view.js) and the
// factory floor's board (game-data.js) both read a ticket's labels, and a
// ticket stands at one stage in both: the order here is theirs.
//
// A label from anywhere else is not a stage, and a closed ticket has left the
// line whatever its labels say.
//
// The two pages name their stages their own way - the line says "line", the
// floor says "ready" - so the table gives each step the labels that answer to
// it, and each page reads its own name from its own list, written by step
// rather than by position. The floor has no planning of its own, so it asks
// for the table without that step: a ticket carrying both tehas:plan and
// tehas:ready is still ready there. A closed ticket has left the line, which
// the floor draws as shipped and the line as its last stage
// (test/stages_test.mjs).
export const STAGE_LABELS = {
  stopped: ["needs-human-attention"],
  review: ["tehas:awaiting-merge"],
  building: ["tehas:running"],
  planning: ["tehas:plan", "tehas:planning"],
  line: ["tehas:ready"],
};

// The steps, worst first: the order the labels answer in. A page names each
// one for its page (business-view.js's LINE_KEY, game-data.js's FLOOR_STAGES);
// a step no page names is drawn as nothing, so the case that says so fails.
export const STAGE_ORDER = Object.keys(STAGE_LABELS);

// labels: the ticket's labels, matched without their case. closed: whether it
// is closed. options: { planning } - whether the page has a planning step,
// true for the line, false for the floor, where planning labels mean nothing
// of their own. Anything that is not a list of labels is no label at all, and
// no options is the table with planning.
// The stage the labels name, or null for a ticket no stage lists. The list
// above is the same order, read by a page that names its own stage for each.
export function stageOf(labels, closed, options) {
  if (closed) return "shipped";
  const { planning = true } = options || {};
  const list = Array.isArray(labels) ? labels : [];
  const has = (label) => list.some((l) => (typeof l === "string" ? l : String(l ?? "")).toLowerCase() === label);
  for (const step of STAGE_ORDER) {
    if (step === "planning" && !planning) continue;
    if (STAGE_LABELS[step].some(has)) return step;
  }
  return null;
}
