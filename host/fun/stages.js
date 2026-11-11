// Which stage a ticket stands at, from the labels tehas itself puts on it (see
// console/src/queue.rs). The factory floor (host/fun/game-data.js) and the
// business page (host/fun/business-view.js) both ask this one function, so the
// two cannot read one ticket as two stages.
//
// A label from anywhere else says nothing. A closed ticket has left the line
// whatever its labels say. `planning` says whether the labels of the plan stage
// count at all: the business page shows a Planning column, the factory floor has
// no such place on its line and lets tehas:ready win. A ticket carrying both
// tehas:plan and tehas:ready is therefore planning on the one page and ready on
// the other, which is what each page can show.
//
// The table is ordered: the first label that matches answers, so a ticket that
// is stopped is stopped whatever else it is labelled.
export const LABEL_STAGES = [
  ["needs-human-attention", "stopped"],
  ["tehas:awaiting-merge", "review"],
  ["tehas:running", "running"],
  ["tehas:plan", "plan"],
  ["tehas:planning", "plan"],
  ["tehas:ready", "ready"],
];

// Each page names the two stages this table cannot know, because they are not
// labels: what a closed ticket is, and what a ticket that carries none of these
// labels is. The factory floor answers "shipped" and "backlog"; the business
// page, whose line ends at Shipped, answers "ship" both times.
//
// labels: the ticket's labels, lower-cased; anything but a list names none of
// them, so a file that holds no labels is read as an unlabelled ticket and not
// as a crash. closed: whether the ticket is shut.
export function stageOf(labels, closed, { planning = false, shipped = "shipped", none = null } = {}) {
  if (closed) return shipped;
  const named = Array.isArray(labels) ? labels : [];
  for (const [label, stage] of LABEL_STAGES) {
    if (stage === "plan" && !planning) continue;
    if (named.includes(label)) return stage;
  }
  return none;
}
