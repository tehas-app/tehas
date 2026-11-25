// The tabs of the console: which cycles can be shown and which one is.
// See DESIGN.md, "Production Console".
//
// The factory runs up to TEHAS_MAX_CYCLES cycles side by side, and
// each has a console of its own. live/running.json lists the running cycles
// and the one that finished last. view() turns that file into the tabs. It
// has no side effects and draws nothing, so it is tested without a browser
// (test/console_tabs_test.mjs). The plain dashboard draws it in Solid
// (factory/).
//
// The file is read from the network and its text is untrusted: a tab's label
// is an issue number, set as text.
(() => {
  const FINE = ["merged", "handoff"];
  const isCycle = (c) =>
    c !== null && typeof c === "object" && !Array.isArray(c) && Number.isSafeInteger(c.issue) && c.issue > 0;

  // The running cycles and the last one, from the file as it is now or from
  // the one object an earlier version wrote.
  function cycles(now) {
    if (now === null || typeof now !== "object" || Array.isArray(now)) return { running: [], last: null };
    if (!("cycles" in now) && !("last" in now)) {
      if (!isCycle(now)) return { running: [], last: null };
      return now.running === true ? { running: [now], last: null } : { running: [], last: now };
    }
    const seen = new Set();
    const running = (Array.isArray(now.cycles) ? now.cycles : []).filter((c) => {
      if (!isCycle(c) || seen.has(c.issue)) return false;
      seen.add(c.issue);
      return true;
    });
    return { running, last: isCycle(now.last) ? now.last : null };
  }

  function view(now, selected) {
    const { running, last } = cycles(now);
    const tabs = running.map((cycle) => ({ issue: cycle.issue, running: true, cycle }));
    if (last && !running.some((c) => c.issue === last.issue)) tabs.push({ issue: last.issue, running: false, cycle: last });
    const shown = tabs.find((t) => t.issue === selected) || tabs[0] || null;
    // A cycle that started after the last one ended clears the failure.
    const retried = last && running.some((c) => (c.startedAt || 0) >= (last.updatedAt || 0));
    const failure = last && !retried && !FINE.includes(String(last.status))
      ? { issue: last.issue, status: String(last.status).slice(0, 40) } : null;
    return { tabs, shown, running: running.length, failure };
  }

  // The name is put into a path under /live/.
  const consoleFile = (cycle) =>
    typeof cycle.console === "string" && /^[\w-][\w.-]*$/.test(cycle.console) ? cycle.console : null;

  globalThis.TEHAS_CONSOLE_TABS = { view, consoleFile };
})();
