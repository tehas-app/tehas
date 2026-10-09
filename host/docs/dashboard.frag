<h1>Tehas dashboard</h1>
<div class="dash-top">
  <span id="state" class="pill idle">loading</span>
  <span id="fresh" class="muted"></span>
  <a id="all" class="muted" href="#">all issues on GitHub</a>
</div>

<section class="now" id="now" hidden>
  <h2 id="now-title"></h2>
  <div class="step" id="now-step"></div>
  <pre id="console" aria-label="Console of the running cycle" tabindex="0"></pre>
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

  let consoleFile = null, offset = 0, running = null;

  async function refresh() {
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
    running = now && now.running ? now : null;
    const pill = $("state");
    if (running) { pill.className = "pill run"; pill.textContent = "working"; }
    else if (s.paused) { pill.className = "pill pause"; pill.textContent = "paused"; }
    else { pill.className = "pill idle"; pill.textContent = "idle"; }
    $("fresh").textContent = "queue read " + ago(s.generatedAt);
    $("all").href = s.links.all;

    list("queued", s.queued, s.links.queued, "Nothing is waiting. Label an issue tehas:ready.", "updatedAt");
    list("awaiting", s.awaiting, s.links.awaiting, "No pull request is waiting.", "updatedAt");
    list("stopped", s.stopped, s.links.stopped, "Nothing is stopped.", "updatedAt");
    list("completed", s.completed, s.links.completed, "Nothing completed yet.", "closedAt");

    // The running (or last) cycle and its console.
    const shown = now || null;
    $("now").hidden = !shown;
    if (shown) {
      const issue = [...s.running, ...s.queued, ...s.stopped, ...s.awaiting, ...s.completed]
        .find((i) => i.number === shown.issue);
      $("now-title").textContent = (running ? "Working on #" : "Last cycle: #") + shown.issue +
        (issue ? "  " + issue.title : "");
      const elapsed = (running ? Date.now() / 1000 : shown.updatedAt) - shown.startedAt;
      $("now-step").textContent = running
        ? (shown.statusText || "step " + shown.step) + "   (" + dur(elapsed) + " so far)"
        : "ended as " + shown.status + " at step " + shown.step + " after " + dur(elapsed);
      if (shown.console !== consoleFile) {
        consoleFile = shown.console;
        offset = 0;
        $("console").textContent = "";
      }
    }
  }

  async function tail() {
    if (!consoleFile) return;
    try {
      const r = await fetch("/live/" + consoleFile, { headers: { Range: "bytes=" + offset + "-" }, cache: "no-store" });
      if (r.status === 206 || (r.status === 200 && offset === 0)) {
        const buf = new Uint8Array(await r.arrayBuffer());
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
    }
  }

  refresh();
  setInterval(refresh, 10000);
  setInterval(tail, 2000);
  setTimeout(tail, 600);
})();
</script>
