// The plain dashboard: host/docs/dashboard.frag holds the markup, this file
// draws it. It reads /live/status.json and /live/running.json for the queue and
// the cycles, a cycle's console, and where a host publishes them
// /live/previews.json, /live/top.json and /live/analytics.json.
//
// What a file means is worked out by pure functions with no side effects:
// view() and the ones below it here, host/fun/host-data.js for the host,
// host/fun/analytics-data.js for the last ten days and host/fun/console-tabs.js
// for the tabs. test/dashboard_view_test.mjs runs them with no document at all.
// start() draws one page and keeps that page's state in its own closure, so two
// pages share none; test/dashboard_host_test.mjs runs it against a page of stubs.
//
// Issue titles, process names and preview reasons are untrusted text: they
// only ever reach the page through textContent.

// The figures of the host section, and the words that say how old a reading is:
// one module, shared with the panel on the factory floor (host/fun/host-view.js).
import { agoText, dueToRead, layout } from "./host-data.js";
// The "Last 10 days" section: what live/analytics.json means, worked out by one
// module (host/fun/analytics-data.js); here it is drawn.
import { analyticsView, dueToRead as dueAnalytics } from "./analytics-data.js";

export const MAX = 10;
const SVG = "http://www.w3.org/2000/svg";

export const ago = (iso, nowMs) => agoText((nowMs - new Date(iso).getTime()) / 1000);
export const dur = (secs) => {
  const m = Math.floor(secs / 60), h = Math.floor(m / 60);
  return h > 0 ? h + " h " + (m % 60) + " min" : m + " min";
};

// One list of issues: at most MAX rows, and a link to the rest on GitHub.
export function issueRows(items, whenKey, nowMs) {
  const rows = items.slice(0, MAX).map((it) => ({
    number: it.number, title: it.title, url: it.url, when: ago(it[whenKey] || it.updatedAt, nowMs),
  }));
  return { count: items.length, rows, more: items.length > MAX ? items.length : 0 };
}

// Previews of pull requests: /live/previews.json, written by tehasbot
// preview. A host without previews has no such file and the section stays
// hidden. Titles are untrusted text and the address is checked before it
// becomes a link.
const PREVIEW_STATES = Object.freeze({
  building: "building",
  "no-recipe": "no preview/Dockerfile in the commit",
  "build-failed": "unable to create",
  "no-space": "unable to create",
  "over-limit": "over the limit of previews",
});
// What a preview that does not serve says about itself. The state, and for
// the states that can fail in several ways the reason the run wrote into its
// state file: a recipe whose volume was refused, a build that failed, a
// container that stopped at once and a host low on disk are not the same
// thing, and only the reason tells them apart. The reason is text this
// command wrote. For a refused volume it says only that the recipe declares a
// volume outside /tmp, /config and /data; the path of that volume, cut to 80
// characters with its control characters gone, is in that pull request's
// build log on the host.
export function previewWhy(it) {
  const label = Object.hasOwn(PREVIEW_STATES, it.state) ? PREVIEW_STATES[it.state] : "not serving";
  const reason = typeof it.reason === "string" ? it.reason.trim() : "";
  return reason ? label + ": " + reason : label;
}

// The state pill and the shown cycle's heading, from the status file and what
// console-tabs.js made of the running file. nowSec is the time in seconds.
export function view(s, v, nowSec) {
  // A paused factory still finishes the cycle it has. Saying "working" alone
  // would hide the pause; saying "paused" alone would hide the cycle.
  const finishing = v.tabs.filter((t) => t.running).map((t) => t.issue);
  let pill;
  if (s.paused) pill = { className: "pill pause", text: finishing.length ? "paused, finishing #" + finishing.join(", #") : "paused" };
  else if (v.running) pill = { className: "pill run", text: v.running > 1 ? "working on " + v.running : "working" };
  else if (s.noResources) pill = { className: "pill pause", text: "no resources" };
  else pill = { className: "pill idle", text: "idle" };
  // Reported only: all-hands changes nothing in this factory.
  const out = { pill, allHands: s.allHands === true, running: null, shown: null, title: "", step: "" };
  const tab = v.shown;
  if (!tab) return out;
  const shown = tab.cycle, running = tab.running ? shown : null;
  const issue = [...s.running, ...s.queued, ...s.stopped, ...s.awaiting, ...s.completed]
    .find((i) => i.number === shown.issue);
  const elapsed = (running ? nowSec : shown.updatedAt) - shown.startedAt;
  return {
    ...out, running, shown,
    title: (running ? "Working on #" : "Last cycle: #") + shown.issue + (issue ? "  " + issue.title : ""),
    step: running
      ? (shown.statusText || "step " + shown.step) + "   (" + dur(elapsed) + " so far)"
      : "ended as " + shown.status + " at step " + shown.step + " after " + dur(elapsed),
  };
}

// Puts the children make() gives into node, only when key is not the one it
// was drawn with: a refresh that changes nothing leaves the nodes alone.
export function render(node, key, make) {
  if (node.dataset.key === key) return false;
  node.dataset.key = key;
  node.replaceChildren(...make());
  return true;
}

// Draws one dashboard on doc, with the tabs module of that page.
export function start(doc, TABS) {
  const $ = (id) => doc.getElementById(id);
  const el = (tag, cls, text) => {
    const e = doc.createElement(tag);
    if (cls) e.className = cls;
    if (text !== undefined) e.textContent = text;
    return e;
  };

  function list(id, items, moreUrl, emptyText, whenKey) {
    const v = issueRows(items, whenKey, Date.now());
    $("n-" + id).textContent = v.count ? "(" + v.count + ")" : "";
    render($(id), JSON.stringify([v, moreUrl, emptyText]), () => {
      if (!v.count) return [el("li", "empty", emptyText)];
      const out = v.rows.map((it) => {
        const li = el("li");
        const a = el("a", null, it.title);
        a.href = it.url;
        li.append(el("span", "num", "#" + it.number), a, " ", el("span", "when", it.when));
        return li;
      });
      if (v.more) {
        const li = el("li", "empty");
        const a = el("a", "more", "all " + v.more + " on GitHub");
        a.href = moreUrl;
        li.append(a);
        out.push(li);
      }
      return out;
    });
  }

  async function refreshPreviews() {
    let p = null;
    try {
      const r = await fetch("/live/previews.json", { cache: "no-store" });
      if (r.ok) p = await r.json();
    } catch { /* no previews on this host */ }
    const section = $("previews");
    if (!p || !Array.isArray(p.previews)) { section.hidden = true; return; }
    section.hidden = false;
    $("n-previews").textContent = p.previews.length ? "(" + p.previews.length + ")" : "";
    // When the list was written: a run that cannot build still writes it,
    // so an old time means the runs have stopped.
    const note = [];
    if (p.disk && p.disk.low) {
      note.push(p.disk.freeBytes === null
        ? "The host's free disk space could not be read: previews are not being built."
        : "The host is low on disk: previews are not being built.");
    }
    if (typeof p.generatedAt === "string" && !Number.isNaN(Date.parse(p.generatedAt))) {
      note.push("List written " + ago(p.generatedAt, Date.now()) + ".");
    }
    $("previews-note").textContent = note.join(" ");
    const ul = $("preview-list");
    ul.replaceChildren();
    if (!p.previews.length) { ul.append(el("li", "empty", "No pull request is open.")); return; }
    for (const it of p.previews) {
      // The default branch's preview is listed first and has the name "main"
      // where a pull request has a number.
      const isMain = !!it && it.name === "main";
      if (!it || (!isMain && !Number.isSafeInteger(it.number))) continue;
      const li = el("li");
      const title = isMain ? "default branch"
        : typeof it.title === "string" && it.title ? it.title : "pull request " + it.number;
      li.append(el("span", "num", isMain ? "main" : "#" + it.number));
      if (it.state === "serving" && typeof it.address === "string"
          && /^https:\/\/uat-([1-9][0-9]{0,8}|main)\.[a-z0-9.-]+\/$/.test(it.address)) {
        const a = el("a", null, title);
        a.href = it.address;
        a.target = "_blank";
        a.rel = "noopener";
        li.append(a);
      } else {
        li.append(el("span", null, title), " ", el("span", "when", previewWhy(it)));
      }
      ul.append(li);
    }
  }

  // The host itself: /live/top.json, the snapshot of tehas top, written
  // by tehasbot top when TEHAS_TOP is 1. What the file means - the figures, the
  // words for how old a reading is, how many rows to draw - is worked out by the
  // same function the panel on the factory floor uses (host/fun/host-data.js);
  // here it is a plain list. A host that does not publish it has no such file
  // and the section stays hidden. Process names come from /proc.
  let hostMissedAt = 0;
  async function refreshHost() {
    // Nothing on screen wants a fresh answer while the page is hidden, and a
    // file that was not there is not asked for again at every refresh.
    if (!dueToRead({ hidden: doc.hidden, missedAt: hostMissedAt })) return;
    let t = null, servedAt = null;
    try {
      const r = await fetch("/live/top.json", { cache: "no-store" });
      // No file at all: this host publishes no snapshot, so the next question
      // is asked once a minute rather than at every refresh.
      if (r.status === 404) hostMissedAt = Date.now();
      else if (r.ok) {
        t = await r.json();
        // Staleness is judged by the server's clock, not the visitor's. A host
        // whose clock runs behind would else read its own dead file as new.
        const served = Date.parse(r.headers.get("Date") || "");
        servedAt = Number.isNaN(served) ? null : served;
        hostMissedAt = 0;
      }
    } catch { /* no answer: what is on screen stays */ }
    const view = layout(t, servedAt == null ? Date.now() : servedAt);
    const section = $("host");
    section.hidden = !view.present;
    if (!view.present) return;
    const note = [];
    note.push(view.cpus === "-" ? "no cpu count" : view.cpus);
    note.push(view.load[0] === "-" ? "no load average" : "load " + view.load.join(" "));
    note.push(view.cpu === "--" ? "no reading of cpu use" : view.cpu + " of the cpus in use");
    if (view.memory) note.push("memory " + view.memory.used + " of " + view.memory.total + " MB");
    // The publisher rewrites the file every five seconds: an old time means the
    // unit is not running, and the numbers on screen are not the host's now.
    note.push(view.ageSeconds == null ? "snapshot unreadable"
      : (view.state === "stale" ? "snapshot is " + agoText(view.ageSeconds)
        : "snapshot written " + agoText(view.ageSeconds)));
    $("host-note").textContent = note.join(", ") + ".";
    $("n-host").textContent = view.rows.length ? "(" + view.rows.length + ")" : "";
    const ul = $("host-list");
    ul.replaceChildren();
    if (!view.rows.length) ul.append(el("li", "empty", "No process was read from /proc."));
    for (const p of view.rows) {
      const li = el("li");
      li.append(el("span", null, p.name), " ",
        el("span", "when", "pid " + (p.pid == null ? "-" : p.pid) +
          "  cpu " + p.cpu + "  mem " + p.mem + "  up " + p.up));
      ul.append(li);
    }
    // The units of the factory, as tehas top lists them: name, whether systemd
    // has it installed, its state, and when a timer next fires.
    $("n-units").textContent = view.units.length ? "(" + view.units.length + ")" : "";
    const listed = $("unit-list");
    listed.replaceChildren();
    if (!view.units.length) { listed.append(el("li", "empty", "systemctl --user gave no units.")); return; }
    for (const u of view.units) {
      const li = el("li");
      li.append(el("span", null, u.name), " ",
        el("span", "when", u.installed + "  " + u.state + (u.next ? "  next " + u.next : "")));
      listed.append(li);
    }
  }

  // The factory's ten-day report: /live/analytics.json, written by tehasbot
  // status at every poll unless TEHAS_ANALYTICS is off. A host without it has
  // no file and the section stays hidden. It changes once a poll, so it is
  // read at most once a minute.
  let analyticsReadAt = 0;
  async function refreshAnalytics() {
    if (!dueAnalytics({ hidden: doc.hidden, readAt: analyticsReadAt })) return;
    analyticsReadAt = Date.now();
    let a = null;
    try {
      const r = await fetch("/live/analytics.json", { cache: "no-store" });
      if (r.ok) a = await r.json();
    } catch { return; /* no answer: what is on screen stays */ }
    const v = analyticsView(a);
    const section = $("analytics");
    section.hidden = !v.present;
    if (!v.present) return;
    $("analytics-note").textContent = v.window;

    const figures = $("an-figures");
    figures.replaceChildren();
    for (const f of v.figures) {
      const box = el("div", "an-figure");
      box.append(el("div", "an-label", f.label), el("div", "an-value", f.value), el("div", "an-note", f.note));
      if (f.bars) {
        const spark = el("div", "an-spark");
        for (const h of f.bars) {
          const b = el("span");
          b.style.height = Math.round(h * 100) + "%";
          spark.append(b);
        }
        box.append(spark);
      }
      figures.append(box);
    }

    // Two bars a day on one scale, and the cycle-minutes as a line on its own.
    const chart = $("an-chart");
    chart.replaceChildren();
    for (const d of v.chart) {
      const col = el("div", "an-day");
      col.title = d.title;
      const bars = el("div", "an-bars");
      for (const [cls, h, n] of [["shipped", d.shippedHeight, d.shipped], ["failed", d.failedHeight, d.failed]]) {
        const b = el("span", cls, n ? String(n) : "");
        b.style.height = Math.round(h * 100) + "%";
        bars.append(b);
      }
      col.append(bars, el("div", "an-date", d.label));
      chart.append(col);
    }
    if (v.chart.length) {
      const svg = doc.createElementNS(SVG, "svg");
      svg.setAttribute("class", "an-line");
      svg.setAttribute("viewBox", "0 0 " + v.chart.length * 10 + " 100");
      svg.setAttribute("preserveAspectRatio", "none");
      svg.setAttribute("aria-hidden", "true");
      const line = doc.createElementNS(SVG, "polyline");
      line.setAttribute("points", v.chart.map((d, i) => (i * 10 + 5) + "," + (100 - d.minutesHeight * 100).toFixed(1)).join(" "));
      line.setAttribute("vector-effect", "non-scaling-stroke");
      svg.append(line);
      chart.append(svg);
    }

    $("an-reasons-note").textContent = v.reasonsNote;
    const reasons = $("an-reasons");
    reasons.replaceChildren();
    for (const r of v.reasons) {
      const li = el("li");
      li.append(el("span", "num", String(r.n)), el("span", null, r.reason));
      reasons.append(li);
    }

    $("an-steps-note").textContent = v.stepsNote;
    const steps = $("an-steps");
    steps.replaceChildren();
    for (const st of v.steps) {
      const li = el("li", "an-step");
      const bar = el("span", "an-yield");
      const fill = el("span");
      fill.style.width = Math.round(st.share * 100) + "%";
      bar.append(fill);
      li.append(el("span", null, st.step), " ", bar, " ", el("span", "when", st.text));
      steps.append(li);
    }

    $("an-versions-note").textContent = v.versionsNote;
    const rows = $("an-versions");
    rows.replaceChildren();
    for (const ver of v.versions) {
      const tr = el("tr");
      for (const cell of [ver.tehas, ver.rukas, ver.runs, ver.pr, ver.minutes, ver.tokens]) tr.append(el("td", null, cell));
      rows.append(tr);
    }
  }

  // Several cycles can run side by side: console-tabs.js says which there are
  // and which one is shown. running is the shown cycle while it runs.
  let consoleFile = null, offset = 0, tailing = false, running = null, selected = null, lastStatus = null, lastNow = null;

  async function refresh() {
    refreshPreviews();
    refreshHost();
    refreshAnalytics();
    let s, now;
    try {
      s = await (await fetch("/live/status.json", { cache: "no-store" })).json();
    } catch {
      $("fresh").textContent = "no status yet: the dispatcher has not written one";
      return;
    }
    try {
      now = await (await fetch("/live/running.json", { cache: "no-store" })).json();
    } catch {
      now = null;
    }
    lastStatus = s;
    lastNow = now;
    $("fresh").textContent = "queue read " + ago(s.generatedAt, Date.now());
    $("all").href = s.links.all;

    list("queued", s.queued, s.links.queued, "Nothing is waiting. Label an issue tehas:ready.", "updatedAt");
    list("awaiting", s.awaiting, s.links.awaiting, "No pull request is waiting.", "updatedAt");
    list("stopped", s.stopped, s.links.stopped, "Nothing is stopped.", "updatedAt");
    list("completed", s.completed, s.links.completed, "Nothing completed yet.", "closedAt");
    show();
  }

  function select(issue) {
    selected = issue;
    show();
  }

  // The selected cycle, running or the last one, and its console.
  function show() {
    const s = lastStatus;
    if (!s) return;
    const v = TABS.view(lastNow, selected);
    const page = view(s, v, Date.now() / 1000);
    const pill = $("state");
    pill.className = page.pill.className;
    pill.textContent = page.pill.text;
    $("allhands").hidden = !page.allHands;

    TABS.render($("tabs"), v, select);
    running = page.running;
    $("now").hidden = !page.shown;
    if (!page.shown) return;
    $("now-title").textContent = page.title;
    $("now-step").textContent = page.step;
    const file = TABS.consoleFile(page.shown);
    if (file !== consoleFile) {
      consoleFile = file;
      offset = 0;
      $("console").textContent = "";
      tail();
    }
  }

  async function tail() {
    // Two requests for the same offset would both append their answer.
    if (!consoleFile || tailing) return;
    tailing = true;
    const file = consoleFile;
    try {
      const r = await fetch("/live/" + file, { headers: { Range: "bytes=" + offset + "-" }, cache: "no-store" });
      // A tab was chosen while this request was out: its answer is for the old one.
      if (file !== consoleFile) return;
      if (r.status === 206 || (r.status === 200 && offset === 0)) {
        const buf = new Uint8Array(await r.arrayBuffer());
        if (file !== consoleFile) return;
        if (buf.length) {
          offset += buf.length;
          const pre = $("console");
          pre.append(new TextDecoder().decode(buf));
          // Keep the page light on a long cycle.
          if (pre.textContent.length > 400000) pre.textContent = pre.textContent.slice(-300000);
          if ($("follow").checked) pre.scrollTop = pre.scrollHeight;
        }
      } else if (r.status === 200) {
        // The file was replaced by a new cycle.
        offset = 0;
        $("console").textContent = "";
      }
      $("console-note").textContent = running ? "live" : "ended";
    } catch {
      $("console-note").textContent = "console unavailable";
    } finally {
      tailing = false;
    }
  }

  refresh();
  TABS.keys($("tabs"), select);
  setInterval(refresh, 10000);
  setInterval(tail, 2000);
  setTimeout(tail, 600);
  // Coming back to the page asks for the host file again at once, whatever the
  // last answer was.
  doc.addEventListener("visibilitychange", () => {
    if (!doc.hidden) { hostMissedAt = 0; refreshHost(); analyticsReadAt = 0; refreshAnalytics(); }
  });
}

// Only the dashboard's page draws; a test imports the functions above.
if (typeof document !== "undefined" && document.getElementById("now")) start(document, globalThis.TEHAS_CONSOLE_TABS);
