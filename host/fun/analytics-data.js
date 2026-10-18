// The ten-day report as data: live/analytics.json in, what the plain
// dashboard's "Last 10 days" section draws out (host/docs/dashboard.frag).
// The file is written by tehasbot status from `analytics report --json`; its
// keys are in host/docs/tools.md.
//
// Pure: it opens no file and reads no clock. A test hands it a file
// (test/analytics_view_test.mjs). Every figure says its count as "n=N": with
// ten days of data the numbers are small.

export const READ_EVERY = 60; // seconds: the file changes once a poll, every two minutes
export const VERSION_FLOOR = 3; // the report gives a pair's figures from three runs

const num = (v) => (typeof v === "number" && Number.isFinite(v) ? v : null);
const obj = (v) => (v && typeof v === "object" && !Array.isArray(v) ? v : {});
const list = (v) => (Array.isArray(v) ? v : []);
// Words in the file are vocabulary words or versions, but a page never trusts
// a file: text, cut to a length, and only ever set through textContent.
const text = (v, max = 60) => (typeof v === "string" ? v.slice(0, max) : "");
const count = (v) => `n=${num(v) == null ? 0 : Math.round(num(v))}`;

export function percent(share) {
  const s = num(share);
  return s == null ? "-" : `${Math.round(s * 1000) / 10}%`;
}

// A duration in the coarsest unit that reads well.
export function duration(secs) {
  const s = num(secs);
  if (s == null || s < 0) return "-";
  if (s < 90) return `${Math.round(s)} s`;
  if (s < 5400) return `${Math.round(s / 60)} min`;
  if (s < 172800) return `${Math.round(s / 360) / 10} h`;
  return `${Math.round(s / 8640) / 10} d`;
}

// A token count: 6650 is 6.7k, 1234567 is 1.2M.
export function tokens(v) {
  const n = num(v);
  if (n == null || n < 0) return "-";
  if (n < 1000) return String(Math.round(n));
  if (n < 1e6) return `${Math.round(n / 100) / 10}k`;
  if (n < 1e9) return `${Math.round(n / 1e5) / 10}M`;
  return `${Math.round(n / 1e8) / 10}G`;
}

export function kwh(v) {
  const n = num(v);
  if (n == null) return "-";
  return `${n < 10 ? Math.round(n * 1000) / 1000 : Math.round(n * 10) / 10} kWh`;
}

// Heights for a row of bars: each value's share of the largest, 0 where all
// are 0, so an empty ten days draws flat rather than full.
function heights(values) {
  const max = Math.max(0, ...values);
  return values.map((v) => (max > 0 ? v / max : 0));
}

// Should the page read the file now? Not while it is hidden, and not more
// than once in READ_EVERY seconds, whether the last answer was a file or not.
// readAt: when it was last asked for, 0 for never.
export function dueToRead({ hidden = false, readAt = 0 } = {}, nowMs = Date.now()) {
  if (hidden) return false;
  return !readAt || nowMs - readAt >= READ_EVERY * 1000;
}

// data: the parsed file, or null where there is none.
export function analyticsView(data) {
  const r = obj(data);
  if (r.report !== 1 || num(r.to) == null) return { present: false };

  const runs = obj(r.runs);
  const days = list(r.daily).slice(0, 10).map((d) => ({
    day: text(obj(d).day, 10),
    shipped: Math.max(0, num(obj(d).shipped) ?? 0),
    failed: Math.max(0, num(obj(d).failed) ?? 0),
    runs: Math.max(0, num(obj(d).runs) ?? 0),
    minutes: Math.max(0, num(obj(d).minutes) ?? 0),
  }));
  const bar = heights(days.flatMap((d) => [d.shipped, d.failed]));
  const line = heights(days.map((d) => d.minutes));
  const chart = days.map((d, i) => ({
    ...d,
    label: d.day.slice(8), // the day of the month: ten fit on a phone
    shippedHeight: bar[2 * i],
    failedHeight: bar[2 * i + 1],
    minutesHeight: line[i],
    title: `${d.day}: ${d.shipped} shipped, ${d.failed} of ${d.runs} runs failed, ${d.minutes} cycle-minutes`,
  }));

  const figures = [];
  const add = (key, label, value, note, bars) => figures.push({ key, label, value, note, ...(bars ? { bars } : {}) });
  const shipped = obj(r.shipped);
  add("shipped", "Shipped", String(num(shipped.n) ?? 0), "issues merged, by day below",
    heights(days.map((d) => d.shipped)));
  if (r.planned && typeof r.planned === "object") {
    const p = obj(r.planned);
    const parts = [`${num(p.inLine) ?? 0} in line`];
    if (num(p.planning) != null) parts.push(`${p.planning} in planning`);
    add("planned", "Planned", String(num(p.n) ?? 0), parts.join(", "),
      heights(list(p.trend).slice(0, 10).map((t) => Math.max(0, num(obj(t).n) ?? 0))));
  }
  // A merged run reached its pull request too. Refused and stopped are named
  // only when there are some.
  const of = (k) => num(runs[k]) ?? 0;
  const split = [`${of("pr") + of("merged")} PR`, `${of("handoff")} handoff`, `${of("parked")} parked`,
    `${of("failed")} failed`];
  for (const k of ["refused", "stopped"]) if (of(k)) split.push(`${of(k)} ${k}`);
  add("runs", "Runs", String(num(runs.n) ?? 0), split.join(", "));
  const fpy = obj(r.firstPassYield);
  add("fpy", "First-pass yield", percent(fpy.share), `in one run, of shipped, ${count(fpy.n)}`);
  const lead = obj(r.leadTime);
  add("lead", "Lead time", duration(lead.median), `median; p90 ${duration(lead.p90)}, ${count(lead.n)}`);
  const cycle = obj(r.cycleTime);
  add("cycle", "Cycle time", duration(cycle.median), `median per run; p90 ${duration(cycle.p90)}, ${count(cycle.n)}`);
  const machine = obj(r.machineTime);
  add("machine", "Machine time", duration(machine.seconds),
    `${num(machine.perShipped) == null ? "none shipped" : duration(machine.perShipped) + " per shipped"}, ${count(machine.n)}`);
  const tok = obj(r.tokens);
  add("tokens", "Tokens", tokens(tok.total),
    `${num(tok.perShipped) == null ? "none shipped" : tokens(tok.perShipped) + " per shipped"}, ${count(tok.n)}`);
  const avail = obj(r.availability);
  add("availability", "Availability", percent(avail.share),
    `could take work${num(avail.seconds) == null ? "" : ", of " + duration(avail.seconds)}, ${count(avail.n)} samples`);
  if (r.energy && typeof r.energy === "object") {
    const e = obj(r.energy);
    add("energy", "Energy", num(e.perShipped) == null ? "-" : kwh(e.perShipped),
      `per shipped; ${kwh(e.kwh)} in all, ${count(e.n)} samples`);
  }

  const stops = obj(r.stopReasons);
  const reasons = list(stops.top).slice(0, 5).map((t) => ({ reason: text(obj(t).reason), n: num(obj(t).n) ?? 0 }))
    .filter((t) => t.reason);
  const stations = obj(r.stations);
  const steps = list(stations.steps).slice(0, 12).map((s) => ({
    step: text(obj(s).step),
    share: Math.max(0, Math.min(1, num(obj(s).share) ?? 0)),
    text: `${percent(obj(s).share)}, ${num(obj(s).passed) ?? 0} of ${num(obj(s).n) ?? 0}`,
  })).filter((s) => s.step);
  const versions = list(r.versions).slice(0, 10).map((v) => {
    const o = obj(v);
    const n = num(o.n) ?? 0;
    const full = n >= VERSION_FLOOR && num(o.prShare) != null;
    return {
      tehas: text(o.tehas, 40) || "unknown",
      rukas: text(o.rukas, 40) || "unknown",
      runs: String(n),
      pr: full ? percent(o.prShare) : "",
      minutes: full ? duration(o.medianSeconds) : "",
      tokens: full ? tokens(o.medianTokens) : "",
      few: !full,
    };
  });

  return {
    present: true,
    window: days.length ? `${days[0].day} to ${days[days.length - 1].day}, UTC` : "",
    figures,
    chart,
    reasons,
    reasonsNote: reasons.length ? `of ${count(stops.n)} runs that ended without a pull request`
      : "No run ended without a pull request.",
    steps,
    stepsNote: steps.length ? `share of the runs that reached a step and got through it, ${count(stations.n)} runs`
      : "No run recorded its steps.",
    versions,
    versionsNote: versions.length ? `A pair with fewer than ${VERSION_FLOOR} runs shows its count only.`
      : "No work run ended.",
  };
}
