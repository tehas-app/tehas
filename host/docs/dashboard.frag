<h1>Tehas dashboard</h1>
<div class="dash-top">
  <span id="state" class="pill idle">loading</span>
  <span id="allhands" class="pill pause" title="tehas:all-hands is on the control issue" hidden>all hands</span>
  <span id="fresh" class="muted"></span>
  <a id="all" class="muted" href="#">all issues on GitHub</a>
</div>

<section class="now" id="now" hidden>
  <div class="console-tabs" id="tabs" role="tablist" aria-label="Cycles" hidden></div>
  <h2 id="now-title"></h2>
  <div class="step" id="now-step"></div>
  <pre id="console" role="tabpanel" aria-label="Console of the selected cycle" tabindex="0"></pre>
  <div class="console-bar">
    <label><input type="checkbox" id="follow" checked> follow the output</label>
    <span class="muted" id="console-note"></span>
  </div>
</section>

<div class="cols">
  <section><h2>In line <span class="n" id="n-queued"></span></h2><ul class="issues" id="queued"></ul></section>
  <section><h2>Waiting for your review <span class="n" id="n-awaiting"></span></h2><ul class="issues" id="awaiting"></ul></section>
  <section><h2>Stopped, needs attention <span class="n" id="n-stopped"></span></h2><ul class="issues" id="stopped"></ul></section>
  <section><h2>Completed <span class="n" id="n-completed"></span></h2><ul class="issues" id="completed"></ul></section>
</div>

<section id="analytics" hidden>
  <h2>Last 10 days</h2>
  <p class="muted" id="analytics-note"></p>
  <div class="an-figures" id="an-figures"></div>
  <h3>Per day</h3>
  <div class="an-chart" id="an-chart"></div>
  <p class="muted an-legend"><span class="an-key shipped"></span> shipped <span class="an-key failed"></span> failed runs <span class="an-key minutes"></span> cycle-minutes</p>
  <div class="cols">
    <section><h3>Top stop reasons</h3><p class="muted" id="an-reasons-note"></p><ul class="issues" id="an-reasons"></ul></section>
    <section><h3>Station yield</h3><p class="muted" id="an-steps-note"></p><ul class="issues" id="an-steps"></ul></section>
  </div>
  <h3>Versions</h3>
  <p class="muted" id="an-versions-note"></p>
  <div class="tablewrap"><table>
    <thead><tr><th>tehas</th><th>pi-rukas</th><th>runs</th><th>to a PR</th><th>median time</th><th>median tokens</th></tr></thead>
    <tbody id="an-versions"></tbody>
  </table></div>
</section>

<section id="previews" hidden>
  <h2>Previews <span class="n" id="n-previews"></span></h2>
  <p class="muted" id="previews-note"></p>
  <ul class="issues" id="preview-list"></ul>
</section>

<section id="host" hidden>
  <h2>Host <span class="n" id="n-host"></span></h2>
  <p class="muted" id="host-note"></p>
  <ul class="issues" id="host-list"></ul>
  <h3>Units <span class="n" id="n-units"></span></h3>
  <ul class="issues" id="unit-list"></ul>
</section>

<script src="/fun/console-tabs.js"></script>
<script type="module">
// The figures of the host section, and the words that say how old a reading is:
// one module, shared with the panel on the factory floor (host/fun/host-view.js).
import { agoText, dueToRead, layout } from "/fun/host-data.js";
// The "Last 10 days" section: what live/analytics.json means, worked out by one
// module (host/fun/analytics-data.js); here it is drawn.
import { analyticsView, dueToRead as dueAnalytics } from "/fun/analytics-data.js";

(() => {
  const MAX = 10;
  const $ = (id) => document.getElementById(id);
  const el = (tag, cls, text) => {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text !== undefined) e.textContent = text;
    return e;
  };
  const ago = (iso) => agoText((Date.now() - new Date(iso).getTime()) / 1000);
  const dur = (secs) => {
    const m = Math.floor(secs / 60), h = Math.floor(m / 60);
    return h > 0 ? h + " h " + (m % 60) + " min" : m + " min";
  };

  // Issue titles are untrusted text: only ever set through textContent.
  function list(id, items, moreUrl, emptyText, whenKey) {
    const ul = $(id);
    ul.replaceChildren();
    $("n-" + id).textContent = items.length ? "(" + items.length + ")" : "";
    if (!items.length) {
      ul.append(el("li", "empty", emptyText));
      return;
    }
    for (const it of items.slice(0, MAX)) {
      const li = el("li");
      const a = el("a", null, it.title);
      a.href = it.url;
      li.append(el("span", "num", "#" + it.number), a, " ", el("span", "when", ago(it[whenKey] || it.updatedAt)));
      ul.append(li);
    }
    if (items.length > MAX) {
      const li = el("li", "empty");
      const a = el("a", "more", "all " + items.length + " on GitHub");
      a.href = moreUrl;
      li.append(a);
      ul.append(li);
    }
  }

  // Previews of pull requests: /live/previews.json, written by tehasbot
  // preview. A host without previews has no such file and the section stays
  // hidden. Titles are untrusted text and the address is checked before it
  // becomes a link.
  const PREVIEW_STATES = {
    building: "building",
    "no-recipe": "no preview/Dockerfile in the commit",
    "build-failed": "unable to create",
    "no-space": "unable to create",
    "over-limit": "over the limit of previews",
  };
  // What a preview that does not serve says about itself. The state, and for
  // the states that can fail in several ways the reason the run wrote into its
  // state file: a recipe whose volume was refused, a build that failed, a
  // container that stopped at once and a host low on disk are not the same
  // thing, and only the reason tells them apart. The reason is text this
  // command wrote. For a refused volume it says only that the recipe declares a
  // volume outside /tmp, /config and /data; the path of that volume, cut to 80
  // characters with its control characters gone, is in that pull request's
  // build log on the host. Text only ever reaches these nodes, through
  // textContent.
  function previewWhy(it) {
    const label = PREVIEW_STATES[it.state] || "not serving";
    const reason = typeof it.reason === "string" ? it.reason.trim() : "";
    return reason ? label + ": " + reason : label;
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
      note.push("List written " + ago(p.generatedAt) + ".");
    }
    $("previews-note").textContent = note.join(" ");
    const ul = $("preview-list");
    ul.replaceChildren();
    if (!p.previews.length) { ul.append(el("li", "empty", "No pull request is open.")); return; }
    for (const it of p.previews) {
      if (!it || !Number.isSafeInteger(it.number)) continue;
      const li = el("li");
      const title = typeof it.title === "string" && it.title ? it.title : "pull request " + it.number;
      li.append(el("span", "num", "#" + it.number));
      if (it.state === "serving" && typeof it.address === "string"
          && /^https:\/\/uat-[1-9][0-9]{0,8}\.[a-z0-9.-]+\/$/.test(it.address)) {
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

  // The host itself: /live/top.json, the snapshot libexec/top builds, written
  // by tehasbot top when TEHAS_TOP is 1. What the file means - the figures, the
  // words for how old a reading is, how many rows to draw - is worked out by the
  // same function the panel on the factory floor uses (host/fun/host-data.js);
  // here it is a plain list. A host that does not publish it has no such file
  // and the section stays hidden. Process names come from /proc and are
  // untrusted text: they reach the page through textContent only.
  let hostMissedAt = 0;
  async function refreshHost() {
    // Nothing on screen wants a fresh answer while the page is hidden, and a
    // file that was not there is not asked for again at every refresh.
    if (!dueToRead({ hidden: document.hidden, missedAt: hostMissedAt })) return;
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
  // read at most once a minute. Every word in it reaches the page through
  // textContent only.
  let analyticsReadAt = 0;
  const SVG = "http://www.w3.org/2000/svg";
  async function refreshAnalytics() {
    if (!dueAnalytics({ hidden: document.hidden, readAt: analyticsReadAt })) return;
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
      const svg = document.createElementNS(SVG, "svg");
      svg.setAttribute("class", "an-line");
      svg.setAttribute("viewBox", "0 0 " + v.chart.length * 10 + " 100");
      svg.setAttribute("preserveAspectRatio", "none");
      svg.setAttribute("aria-hidden", "true");
      const line = document.createElementNS(SVG, "polyline");
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
  const TABS = globalThis.TEHAS_CONSOLE_TABS;
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
    $("fresh").textContent = "queue read " + ago(s.generatedAt);
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
    const pill = $("state");
    // A paused factory still finishes the cycle it has. Saying "working" alone
    // would hide the pause; saying "paused" alone would hide the cycle.
    const finishing = v.tabs.filter((t) => t.running).map((t) => t.issue);
    if (s.paused) { pill.className = "pill pause"; pill.textContent = finishing.length ? "paused, finishing #" + finishing.join(", #") : "paused"; }
    else if (v.running) { pill.className = "pill run"; pill.textContent = v.running > 1 ? "working on " + v.running : "working"; }
    else if (s.noResources) { pill.className = "pill pause"; pill.textContent = "no resources"; }
    else { pill.className = "pill idle"; pill.textContent = "idle"; }
    // Reported only: all-hands changes nothing in this factory.
    $("allhands").hidden = s.allHands !== true;

    TABS.render($("tabs"), v, select);
    const tab = v.shown;
    running = tab && tab.running ? tab.cycle : null;
    $("now").hidden = !tab;
    if (!tab) return;
    const shown = tab.cycle;
    const issue = [...s.running, ...s.queued, ...s.stopped, ...s.awaiting, ...s.completed]
      .find((i) => i.number === shown.issue);
    $("now-title").textContent = (running ? "Working on #" : "Last cycle: #") + shown.issue +
      (issue ? "  " + issue.title : "");
    const elapsed = (running ? Date.now() / 1000 : shown.updatedAt) - shown.startedAt;
    $("now-step").textContent = running
      ? (shown.statusText || "step " + shown.step) + "   (" + dur(elapsed) + " so far)"
      : "ended as " + shown.status + " at step " + shown.step + " after " + dur(elapsed);
    const file = TABS.consoleFile(shown);
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
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) { hostMissedAt = 0; refreshHost(); analyticsReadAt = 0; refreshAnalytics(); }
  });
})();
</script>
