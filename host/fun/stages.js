// Which stage a ticket stands at, from the labels tehas itself puts on it (see
// console/src/queue.rs). The factory floor (host/fun/game-data.js) asks it.
//
// A label from anywhere else says nothing. A closed ticket has left the line
// whatever its labels say. The plan labels name no stage: the floor has no
// Planning column, so tehas:ready wins a ticket that carries both.
//
// The table is ordered: the first label that matches answers, so a ticket that
// is stopped is stopped whatever else it is labelled.
export const LABEL_STAGES = [
  ["needs-human-attention", "stopped"],
  ["tehas:awaiting-merge", "review"],
  ["tehas:running", "running"],
  ["tehas:ready", "ready"],
];

// Each page names the two stages this table cannot know, because they are not
// labels: what a closed ticket is, and what a ticket that carries none of these
// labels is. The factory floor answers "shipped" and "idea".
//
// labels: the ticket's labels, lower-cased; anything but a list names none of
// them, so a file that holds no labels is read as an unlabelled ticket and not
// as a crash. closed: whether the ticket is shut.
export function stageOf(labels, closed, { shipped = "shipped", none = null } = {}) {
  if (closed) return shipped;
  const named = Array.isArray(labels) ? labels : [];
  for (const [label, stage] of LABEL_STAGES) {
    if (named.includes(label)) return stage;
  }
  return none;
}
