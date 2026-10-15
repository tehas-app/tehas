// The host snapshot as data: live/top.json in, the figures a page draws out.
// The factory floor's panel (host/fun/host-view.js) and the plain dashboard
// (host/docs/dashboard.frag) both draw from this one function, so the two
// cannot disagree about what a reading means or when a file has gone stale.
//
// Pure: it opens no file, asks no command and reads no clock. A test hands it a
// file and a moment (test/host_view_test.mjs). The published file itself is
// written by libexec/top-publish; see host/docs/tools.md.

export const PERIOD = 5; // the publisher rewrites the file every five seconds
export const STALE_AFTER = 30; // six missed publishes: the numbers are not the host's now
export const ROWS = 15; // the snapshot holds the busiest fifteen processes only
export const UNITS = 30; // one factory has far fewer units; this bounds what a page draws
export const MISS_RETRY = 60; // seconds before asking again for a file that is not there

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

// How long ago, in the same coarse units. A snapshot stamped just ahead of the
// clock it is read with is "just now", not a negative age.
export function agoText(secs) {
  const s = num(secs);
  if (s == null) return "";
  const at = Math.max(0, s);
  if (at < 90) return "just now";
  if (at < 5400) return `${Math.round(at / 60)} min ago`;
  if (at < 129600) return `${Math.round(at / 3600)} h ago`;
  return `${Math.round(at / 86400)} d ago`;
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
    // Seconds between the moment the publisher wrote the file and nowMs, or
    // null where the file names no moment at all.
    ageSeconds: Number.isFinite(generated) ? (nowMs - generated) / 1000 : null,
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

// Is a read worth doing now? Not while the page is hidden: nothing on screen
// wants a fresh answer. And not before MISS_RETRY after an answer that there is
// no file at all: a host that publishes nothing answers 404 every time, and
// asking again at every period is work for nothing until someone turns the
// publisher on.
//
// hidden: whether the page is behind another tab or window. missedAt: the moment
// of the last answer that there was no file, or 0 when the last read got one.
export function dueToRead({ hidden = false, missedAt = 0 } = {}, nowMs = Date.now()) {
  if (hidden) return false;
  return !missedAt || nowMs - missedAt >= MISS_RETRY * 1000;
}
