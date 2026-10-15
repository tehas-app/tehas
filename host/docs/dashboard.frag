<h1>Tehas dashboard</h1>
<div class="dash-top">
  <span id="state" class="pill idle">loading</span>
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
<script>
(() => {
  const MAX = 10;
  const $ = (id) => document.getElementById(id);
  const el = (tag, cls, text) => {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text !== undefined) e.textContent = text;
    return e;
  };
  const ago = (iso) => {
    if (!iso) return "";
    const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
    if (s < 90) return "just now";
    if (s < 5400) return Math.round(s / 60) + " min ago";
    if (s < 129600) return Math.round(s / 3600) + " h ago";
    return Math.round(s / 86400) + " d ago";
  };
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
  // by tehasbot top when TEHAS_TOP is 1. A host that does not publish it has no
  // such file and the section stays hidden. Process names come from /proc and
  // are untrusted text: they reach the page through textContent only.
  const HOST_STALE = 30;
  function hostUptime(secs) {
    if (typeof secs !== "number" || !Number.isFinite(secs) || secs < 0) return "-";
    if (secs < 60) return Math.round(secs) + " s";
    if (secs < 3600) return Math.floor(secs / 60) + " min";
    if (secs < 86400) return Math.floor(secs / 3600) + " h";
    return Math.floor(secs / 86400) + " d";
  }
  const figure = (v) => (typeof v === "number" && Number.isFinite(v) ? Math.round(v * 10) / 10 + "%" : "-");
  async function refreshHost() {
    let t = null, servedAt = null;
    try {
      const r = await fetch("/live/top.json", { cache: "no-store" });
      if (r.ok) {
        t = await r.json();
        // Staleness is judged by the server's clock, not the visitor's. A host
        // whose clock runs behind would else read its own dead file as new.
        const served = Date.parse(r.headers.get("Date") || "");
        servedAt = Number.isNaN(served) ? null : served;
      }
    } catch { /* this host publishes no snapshot */ }
    const section = $("host");
    const h = t && typeof t === "object" && t.host && typeof t.host === "object" ? t.host : null;
    section.hidden = !h;
    if (!h) return;
    const at = Date.parse(t.generatedAt);
    const age = servedAt == null || Number.isNaN(at) ? null : Math.round((servedAt - at) / 1000);
    const note = [];
    note.push(typeof h.cpus === "number" ? h.cpus + " cpus" : "no cpu count");
    note.push(Array.isArray(h.load) && h.load.length === 3 ? "load " + h.load.join(" ") : "no load average");
    note.push(figure(h.cpuPercent) + " of the cpus in use");
    if (h.memory && typeof h.memory.totalMb === "number" && typeof h.memory.usedMb === "number") {
      note.push("memory " + Math.round(h.memory.usedMb) + " of " + Math.round(h.memory.totalMb) + " MB");
    }
    // The publisher rewrites the file every five seconds: an old time means the
    // unit is not running, and the numbers on screen are not the host's now.
    note.push(age === null ? "snapshot unreadable" : (age > HOST_STALE ? "snapshot is " + ago(t.generatedAt) : "snapshot written " + ago(t.generatedAt)));
    $("host-note").textContent = note.join(", ") + ".";
    const rows = Array.isArray(h.processes) ? h.processes : [];
    $("n-host").textContent = rows.length ? "(" + rows.length + ")" : "";
    const ul = $("host-list");
    ul.replaceChildren();
    if (!rows.length) ul.append(el("li", "empty", "No process was read from /proc."));
    for (const p of rows) {
      if (!p || typeof p.name !== "string" || !p.name) continue;
      const li = el("li");
      li.append(el("span", null, p.name), " ",
        el("span", "when", "pid " + (Number.isSafeInteger(p.pid) ? p.pid : "-") +
          "  cpu " + figure(p.cpuPercent) + "  mem " + figure(p.memPercent) + "  up " + hostUptime(p.upSeconds)));
      ul.append(li);
    }
    // The units of the factory, as tehas top lists them: name, whether systemd
    // has it installed, its state, and when a timer next fires.
    const units = (Array.isArray(t.units) ? t.units : [])
      .filter((u) => u && typeof u === "object" && typeof u.id === "string" && u.id).slice(0, 30);
    $("n-units").textContent = units.length ? "(" + units.length + ")" : "";
    const listed = $("unit-list");
    listed.replaceChildren();
    if (!units.length) { listed.append(el("li", "empty", "systemctl --user gave no units.")); return; }
    for (const u of units) {
      const li = el("li");
      li.append(el("span", null, u.id.slice(0, 64)), " ",
        el("span", "when", (u.unitFileState || "-") + "  " + (u.activeState || "-") +
          (typeof u.next === "string" && u.next ? "  next " + u.next : "")));
      listed.append(li);
    }
  }

  // Several cycles can run side by side: console-tabs.js says which there are
  // and which one is shown. running is the shown cycle while it runs.
  const TABS = globalThis.TEHAS_CONSOLE_TABS;
  let consoleFile = null, offset = 0, tailing = false, running = null, selected = null, lastStatus = null, lastNow = null;

  async function refresh() {
    refreshPreviews();
    refreshHost();
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
})();
</script>
