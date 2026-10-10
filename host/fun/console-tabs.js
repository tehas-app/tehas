// The tabs of the console: which cycles can be shown and which one is.
// See DESIGN.md, "Production Console".
//
// The factory runs up to TEHAS_TICKET_CONCURRENCY cycles side by side, and
// each has a console of its own. live/running.json lists the running cycles
// and the one that finished last. view() turns that file into the tabs. It
// has no side effects and draws nothing, so it is tested without a browser
// (test/console_tabs_test.mjs). The factory floor (game-view.js) and the
// plain dashboard (dashboard.frag) both draw what it returns, with render().
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
    const failure = last && !FINE.includes(String(last.status))
      ? { issue: last.issue, status: String(last.status).slice(0, 40) } : null;
    return { tabs, shown, running: running.length, failure };
  }

  // The name is put into a path under /live/.
  const consoleFile = (cycle) =>
    typeof cycle.console === "string" && /^[\w-][\w.-]*$/.test(cycle.console) ? cycle.console : null;

  // Draws the row. The buttons are made again only when the tabs change, so
  // a refresh every ten seconds does not take the keyboard's place away.
  function render(row, v, onSelect) {
    const key = v.tabs.map((t) => t.issue + (t.running ? "r" : "e")).join(" ");
    row.hidden = v.tabs.length < 2;
    if (row.dataset.tabs !== key) {
      const focused = row.contains(document.activeElement);
      row.dataset.tabs = key;
      row.replaceChildren(...v.tabs.map((t) => {
        const b = document.createElement("button");
        b.type = "button";
        b.setAttribute("role", "tab");
        b.dataset.issue = String(t.issue);
        b.textContent = "#" + t.issue + (t.running ? "" : " ended");
        b.addEventListener("click", () => onSelect(t.issue));
        return b;
      }));
      if (focused && v.shown) {
        const b = [...row.children].find((c) => Number(c.dataset.issue) === v.shown.issue);
        if (b) b.focus();
      }
    }
    for (const b of row.children) {
      const on = v.shown !== null && Number(b.dataset.issue) === v.shown.issue;
      b.setAttribute("aria-selected", String(on));
      b.tabIndex = on ? 0 : -1;
    }
  }

  // The arrow keys move between tabs and select, as in any tab list.
  function keys(row, onSelect) {
    row.addEventListener("keydown", (event) => {
      const tabs = [...row.children], at = tabs.indexOf(document.activeElement);
      if (at < 0) return;
      const to = { ArrowRight: at + 1, ArrowLeft: at - 1, Home: 0, End: tabs.length - 1 }[event.key];
      if (to === undefined) return;
      event.preventDefault();
      const next = tabs[(to + tabs.length) % tabs.length];
      next.focus();
      onSelect(Number(next.dataset.issue));
    });
  }

  globalThis.TEHAS_CONSOLE_TABS = { view, consoleFile, render, keys };
})();
