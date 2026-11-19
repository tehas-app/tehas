// The console of the plain dashboard: the tabs of the running cycles and the
// last one (console-tabs.js), and the shown cycle's title, step and console
// file, from live/running.json and live/status.json. `selected` is the issue
// a viewer chose, or null. Pure: the clock is nowMs.
import "../console-tabs.js";

const list = (v) => (Array.isArray(v) ? v : []);

export function duration(secs) {
  const m = Math.floor(secs / 60), h = Math.floor(m / 60);
  return h > 0 ? h + " h " + (m % 60) + " min" : m + " min";
}

export function consoleModel(running, status, selected, nowMs) {
  const TABS = globalThis.TEHAS_CONSOLE_TABS;
  const v = TABS.view(running, selected);
  const tab = v.shown;
  if (!tab) return { shown: false, tabs: [], title: "", step: "", file: null, live: false };
  const c = tab.cycle;
  const live = tab.running;
  const s = status && typeof status === "object" ? status : {};
  const issue = ["running", "queued", "stopped", "awaiting", "completed"].flatMap((k) => list(s[k]))
    .find((i) => i && i.number === c.issue);
  const elapsed = (live ? nowMs / 1000 : c.updatedAt) - c.startedAt;
  return {
    shown: true,
    tabs: v.tabs.map((t) => ({ issue: t.issue, label: "#" + t.issue + (t.running ? "" : " ended"), on: t.issue === tab.issue })),
    title: (live ? "Working on #" : "Last cycle: #") + c.issue + (issue ? "  " + issue.title : ""),
    step: live
      ? (c.statusText || "step " + c.step) + "   (" + duration(elapsed) + " so far)"
      : "ended as " + c.status + " at step " + c.step + " after " + duration(elapsed),
    file: TABS.consoleFile(c),
    live,
  };
}
