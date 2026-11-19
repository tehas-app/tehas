// The host section of the plain dashboard, from live/top.json (tehasbot top).
// What the file means is worked out by host-data.js, which the floor's panel
// uses too; here it becomes the lines of a plain list. Pure: the clock is nowMs.
import { agoText, layout } from "../host-data.js";

export function hostModel(top, nowMs) {
  const view = layout(top, nowMs);
  if (!view.present) return { present: false, note: "", n: "", rows: [], nUnits: "", units: [] };
  const note = [
    view.cpus === "-" ? "no cpu count" : view.cpus,
    view.load[0] === "-" ? "no load average" : "load " + view.load.join(" "),
    view.cpu === "--" ? "no reading of cpu use" : view.cpu + " of the cpus in use",
  ];
  if (view.memory) note.push("memory " + view.memory.used + " of " + view.memory.total + " MB");
  // The publisher rewrites the file every five seconds: an old time means the
  // unit is not running, and the numbers on screen are not the host's now.
  note.push(view.ageSeconds == null ? "snapshot unreadable"
    : (view.state === "stale" ? "snapshot is " : "snapshot written ") + agoText(view.ageSeconds));
  return {
    present: true,
    note: note.join(", ") + ".",
    n: view.rows.length ? "(" + view.rows.length + ")" : "",
    rows: view.rows.map((p) => ({
      name: p.name,
      text: "pid " + (p.pid == null ? "-" : p.pid) + "  cpu " + p.cpu + "  mem " + p.mem + "  up " + p.up,
    })),
    nUnits: view.units.length ? "(" + view.units.length + ")" : "",
    units: view.units.map((u) => ({ name: u.name, text: u.installed + "  " + u.state + (u.next ? "  next " + u.next : "") })),
  };
}
