// The Host section of the front page: the machine the factory runs on, read
// from /live/top.json (the snapshot libexec/top builds, published by
// libexec/top-publish when TEHAS_TOP is 1). See host/docs/tools.md.
//
// layout() turns the file into what is drawn and has no side effects, so it is
// tested without a browser (test/host_view_test.mjs). The rest of the module
// fetches and draws, and runs only where there is a document.

export const PERIOD = 5; // the publisher rewrites the file every five seconds
export const STALE_AFTER = 30; // six missed publishes: the numbers are not the host's now
export const ROWS = 15; // the snapshot holds the busiest fifteen processes only
export const UNITS = 30; // one factory has far fewer units; this bounds what a file draws
const MISS_RETRY = 60; // seconds before asking again for a file that is not there

const list = (v) => (Array.isArray(v) ? v : []);
const num = (v) => (typeof v === "number" && Number.isFinite(v) ? v : null);
// Process names come from the host and are untrusted: text, cut to a length.
const text = (v) => (typeof v === "string" ? v.slice(0, 64) : "");
// A share of ten blocks, for the gauges.
const blocks = (share) => Math.max(0, Math.min(10, Math.round(share * 10)));
// One figure with as little rounding as the reading needs, or "--" where the
// snapshot has no reading. A percentage is null when the two readings of /proc
// measured nothing, which is not the same as zero.
const pct = (v) => (num(v) == null ? "--" : `${Math.round(num(v) * 10) / 10}%`);
const mb = (v) => (num(v) == null ? "--" : String(Math.round(num(v))));
const load = (v) => {
  const each = list(v).map(num);
  return each.length === 3 && each.every((n) => n != null) ? each.map((n) => n.toFixed(2)) : null;
};
// How long a process has been running, in the coarsest unit that reads well.
export function uptime(secs) {
  const s = num(secs);
  if (s == null || s < 0) return "-";
  if (s < 60) return `${Math.round(s)} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min`;
  if (s < 86400) return `${Math.floor(s / 3600)} h`;
  return `${Math.floor(s / 86400)} d`;
}

// data: the parsed top file. nowMs: the present, by the server's clock.
export function layout(data, nowMs) {
  const file = data && typeof data === "object" ? data : {};
  const host = file.host && typeof file.host === "object" ? file.host : null;
  const generated = Date.parse(file.generatedAt);
  const stale = !host || !(nowMs - generated <= STALE_AFTER * 1000);

  const mem = host && host.memory && typeof host.memory === "object" ? host.memory : null;
  const total = num(mem?.totalMb), used = num(mem?.usedMb);
  const share = total > 0 && used != null ? Math.min(1, used / total) : null;
  const cpus = num(host?.cpus);

  return {
    state: stale ? "stale" : "live",
    present: Boolean(host),
    cpus: cpus == null ? "-" : `${Math.round(cpus)} CPUs`,
    load: (host && load(host.load)) || ["-", "-", "-"],
    cpu: pct(host?.cpuPercent),
    cpuFilled: num(host?.cpuPercent) == null ? 0 : blocks(num(host.cpuPercent) / 100),
    memory: share == null ? null : { used: mb(used), total: mb(total), percent: Math.round(share * 100), filled: blocks(share) },
    // The snapshot is already the busiest first; this only drops what a longer
    // file would hold and what is not a row at all.
    rows: list(host?.processes).filter((p) => p && typeof p === "object" && text(p.name)).slice(0, ROWS)
      .map((p) => ({ pid: num(p.pid), name: text(p.name), cpu: pct(p.cpuPercent), mem: pct(p.memPercent), up: uptime(p.upSeconds) })),
    // The units of the factory, as tehas top lists them: name, whether systemd
    // has it installed, its state, and when a timer next fires.
    units: list(file.units).filter((u) => u && typeof u === "object" && text(u.id)).slice(0, UNITS)
      .map((u) => ({ name: text(u.id), installed: text(u.unitFileState) || "-", state: text(u.activeState) || "-", next: text(u.next) })),
    label: `Host: ${cpus == null ? "no cpu count" : `${Math.round(cpus)} cpus`}, ` +
      (stale ? "no recent reading of /proc." : `using ${pct(host.cpuPercent)} of its cpus.`),
  };
}

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
    if (document.hidden) return;
    // A host that publishes no snapshot answers 404 every time. Asking for it
    // again every five seconds is work for nothing: once a minute is enough to
    // notice that someone turned the publisher on.
    if (missedAt && Date.now() - missedAt < MISS_RETRY * 1000) return;
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
