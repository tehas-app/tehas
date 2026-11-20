// What the wire demo says, from what its feeds, streams and press gave. Pure.
export function echoText(echo, meta) {
  const seen = meta?.headers?.["x-kit-seen"] ?? "none";
  const date = meta?.date == null ? "no date" : new Date(meta.date).toISOString();
  return `session ${echo?.session ?? "none"}, seen ${seen}, ${date}`;
}

export function planText(plan, meta) {
  const n = plan?.releases?.length ?? 0;
  return n ? `${n} releases` : `No releases planned (${meta?.status ?? "?"})`;
}

export function eventsText(events, state) {
  return `${state}: ${events.map((e) => `${e.type}#${e.id}=${e.n}${e.after ? ` after ${e.after}` : ""}`).join(" ")}`;
}

export function linesText(lines, state) {
  return `${state}: ${lines.join(" ")}`;
}
