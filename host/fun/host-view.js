// The Host section of the front page: the machine the factory runs on, read
// from /live/top.json (the snapshot lib/top.sh builds, published by
// libexec/top-publish when TEHAS_TOP is 1). See host/docs/tools.md.
//
// What that file means is worked out in host/fun/host-data.js, which the plain
// dashboard draws from too. This file only puts the figures it gives into the
// panel, the chip on the head panel and the two tables the page holds, and asks
// for the file on the publisher's own period.

import { MISS_RETRY, PERIOD, dueToRead, layout } from "./host-data.js";

if (typeof document !== "undefined") {
  const $ = (id) => document.getElementById(id);
  // The three elements this view shows or hides together, looked up once. A
  // page that does not have one of them has nothing to show: render() leaves
  // them alone rather than assigning to what is not there.
  const section = $("host"), menuLink = $("menu-host"), floorHost = $("floor-host");
  const hasParts = Boolean(section && menuLink && floorHost);

  const cell = (tr, value, cls) => {
    const td = tr.insertCell();
    td.textContent = value;
    if (cls) td.className = cls;
  };
  // One row: the name as the row header, then its figures as cells.
  function drawRow(body, name, values) {
    const tr = body.insertRow();
    const th = document.createElement("th");
    th.scope = "row";
    th.textContent = name;
    tr.append(th);
    for (const [value, cls] of values) cell(tr, value, cls);
  }

  function gauge(id, filled) {
    const el = $(id);
    if (!el) return;
    const bars = [];
    for (let i = 0; i < 10; i++) {
      const b = document.createElement("i");
      if (i < filled) b.className = "is-used";
      bars.push(b);
    }
    el.replaceChildren(...bars);
  }

  // A page variant that holds no one of these figures is drawn without it: what
  // is looked up here is checked before anything is written to it.
  const setText = (el, value) => { if (el) el.textContent = value; };

  function render() {
    if (!hasParts) return;
    section.hidden = menuLink.hidden = floorHost.hidden = !data;
    if (!data) return;
    const view = layout(data, serverAt + (performance.now() - localAt));
    // Nothing changed since the last draw: leave the rows and the status tag
    // alone, so a selection stays and a screen reader is not told again.
    const json = JSON.stringify(view);
    if (json === drawn) return;
    drawn = json;

    setText($("host-cpus"), view.cpus);
    setText($("host-load"), view.load.join(" "));
    setText($("host-cpu"), view.cpu);
    gauge("host-cpu-gauge", view.cpuFilled);
    setText($("host-mem"), view.memory ? `${view.memory.used} of ${view.memory.total} MB` : "-");
    setText($("host-mem-percent"), view.memory ? `${view.memory.percent}%` : "--");
    gauge("host-mem-gauge", view.memory ? view.memory.filled : 0);
    const state = $("host-state");
    if (state) {
      state.textContent = view.state === "live" ? "LIVE" : "STALE";
      state.dataset.state = view.state;
    }
    setText($("floor-host-cpu"), view.cpu);
    // The label ends in its full stop already: the chip adds its own sentence.
    floorHost.setAttribute("aria-label", `${view.label} Opens Host.`);

    const rows = $("host-rows");
    if (rows) {
      rows.replaceChildren();
      for (const r of view.rows) {
        drawRow(rows, r.name, [[r.pid == null ? "-" : String(r.pid)], [r.cpu, "num"], [r.mem, "num"], [r.up, "num"]]);
      }
    }

    // The units sit below the processes, as they do in the terminal.
    const unitRows = $("host-unit-rows");
    if (unitRows) {
      unitRows.replaceChildren();
      for (const u of view.units) drawRow(unitRows, u.name, [[u.installed], [u.state], [u.next, "num col-next"]]);
    }
  }

  let data = null, serverAt = 0, localAt = 0, drawn = "", missedAt = 0;

  async function refresh() {
    // Nothing on screen wants a fresh answer while the page is hidden, and a
    // host that publishes no snapshot is not asked again every five seconds.
    if (!dueToRead({ hidden: document.hidden, missedAt }, Date.now())) return;
    try {
      const r = await fetch("/live/top.json", { cache: "no-store" });
      // No file at all: this host does not publish the snapshot (TEHAS_TOP is
      // 0), so the section and its link come off the page.
      if (r.status === 404) { data = null; missedAt = Date.now(); }
      else if (r.ok) {
        const next = await r.json();
        if (next && typeof next === "object") {
          // Staleness is judged by the server's clock, not the visitor's.
          const date = Date.parse(r.headers.get("Date") || "");
          data = next;
          serverAt = Number.isFinite(date) ? date : Date.now();
          localAt = performance.now();
          missedAt = 0;
        }
      }
    } catch { /* unreachable: what is on screen stays and grows stale */ }
    render();
  }

  refresh();
  setInterval(refresh, PERIOD * 1000);
  // Coming back to the page asks again at once, whatever the last answer was.
  document.addEventListener("visibilitychange", () => { if (!document.hidden) { missedAt = 0; refresh(); } });
}
