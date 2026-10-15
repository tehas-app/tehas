// The tehas game view. Read-only: it shows tickets from the bundled sample or
// from a JSON export the viewer loads, and never talks to the factory.
(() => {
  const $ = (id) => document.getElementById(id);
  const el = (tag, cls, text) => {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text !== undefined) e.textContent = text;
    return e;
  };

  // Stages follow the labels tehas itself puts on issues (see libexec/status).
  const STAGES = {
    ready: "IN LINE",
    running: "BUILDING",
    review: "REVIEW",
    stopped: "STOPPED",
    backlog: "NOT ON THE LINE",
    shipped: "SHIPPED",
  };
  const LINE = ["ready", "running", "review", "shipped"];
  // Energy is estimated, not measured: time spent multiplied by an assumed draw
  // for the machines doing the work. An export can replace the assumption with
  // "assumedKw", or give a measured "kWh" per issue.
  const DEFAULT_KW = 0.5;
  const STORAGE_KEY = "tehas-game-view-export-v1";

  function stageOf(labels, closed) {
    if (closed) return "shipped";
    if (labels.includes("needs-human-attention")) return "stopped";
    if (labels.includes("tehas:awaiting-merge")) return "review";
    if (labels.includes("tehas:running")) return "running";
    if (labels.includes("tehas:ready")) return "ready";
    return "backlog";
  }

  // Ticket text is untrusted: it is only ever set through textContent.
  function normalize(raw) {
    const list = Array.isArray(raw) ? raw : raw && Array.isArray(raw.issues) ? raw.issues : null;
    if (!list) throw new Error("Expected an issues array");
    const shippedList = raw && Array.isArray(raw.shippedIssues) ? raw.shippedIssues : [];
    const seen = new Set();
    const tickets = [];
    const add = (item, forceClosed) => {
      const number = Number(item && item.number);
      if (!item || item.pull_request || !Number.isSafeInteger(number) || number <= 0) return;
      if (typeof item.title !== "string" || seen.has(number)) return;
      seen.add(number);
      const labels = (Array.isArray(item.labels) ? item.labels : [])
        .map((l) => String(typeof l === "string" ? l : (l && l.name) || "").toLowerCase())
        .filter(Boolean);
      const closed = forceClosed || String(item.state || "").toLowerCase() === "closed";
      const url = typeof item.url === "string" && item.url.startsWith("https://") ? item.url : null;
      const when = (v) => (typeof v === "string" && !Number.isNaN(Date.parse(v)) ? v : null);
      const amount = (v) => (Number.isFinite(Number(v)) && Number(v) > 0 ? Number(v) : null);
      tickets.push({
        number, title: item.title.slice(0, 180), labels, url, stage: stageOf(labels, closed),
        createdAt: when(item.createdAt), updatedAt: when(item.updatedAt), closedAt: when(item.closedAt),
        tokens: amount(item.tokens), seconds: amount(item.timeSpentSeconds), kWh: amount(item.kWh),
      });
    };
    for (const item of list) add(item, false);
    for (const item of shippedList) add(item, true);
    return {
      tickets,
      paused: Boolean(raw && raw.paused),
      noResources: Boolean(raw && raw.noResources),
      kw: raw && Number(raw.assumedKw) > 0 ? Number(raw.assumedKw) : DEFAULT_KW,
      sample: Boolean(raw && raw.sampleData),
      words: collectWords(raw, tickets),
    };
  }

  function collectWords(raw, tickets) {
    const clean = (w) => String(w).toLowerCase().replace(/[^a-z0-9-]/g, "").slice(0, 10);
    const explicit = raw && Array.isArray(raw.buzzwords) ? raw.buzzwords : [];
    const counts = new Map();
    const stop = new Set(("the and for with from this that should would could when while into onto your their there about " +
      "after before also have has had are was were will can not but all any our you its use make add show fix new more " +
      "each than then them through off under over between across tehas ready running awaiting-merge").split(" "));
    if (explicit.length) {
      // A list that came with the data: the queue snapshot counts every issue
      // of the repository, not only the tickets on the line.
      for (const item of explicit) {
        const said = String(typeof item === "string" ? item : (item && item.word) || "").toLowerCase();
        const word = clean(said);
        if (word.length < 3 || stop.has(said)) continue;
        counts.set(word, (counts.get(word) || 0) + Math.max(1, Number(item && item.count) || 1));
      }
    } else {
      for (const t of tickets) {
        for (const word of (t.title + " " + t.labels.join(" ")).toLowerCase().match(/[a-z][a-z0-9-]{2,}/g) || []) {
          if (!stop.has(word)) counts.set(clean(word), (counts.get(clean(word)) || 0) + 1);
        }
      }
    }
    return [...counts].map(([word, count]) => ({ word, count }))
      .sort((a, b) => b.count - a.count || a.word.localeCompare(b.word)).slice(0, 40);
  }

  // "live" is true while the tickets come from the factory's own queue snapshot.
  // That snapshot lists only tickets on the line, so counts it cannot know stay blank.
  const state = { tickets: [], paused: false, noResources: false, sample: false, live: false, loaded: false, selected: null, filter: "open", query: "", kw: DEFAULT_KW };

  const ago = (iso) => {
    if (!iso) return "";
    const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
    if (s < 90) return "just now";
    if (s < 5400) return Math.round(s / 60) + " min ago";
    if (s < 129600) return Math.round(s / 3600) + " h ago";
    return Math.round(s / 86400) + " d ago";
  };
  const calm = window.matchMedia("(prefers-reduced-motion: reduce)");
  // Set by the production console when the last cycle ended badly: { issue, status }.
  let failure = null;

  // The tag beside the title is the factory's state at a glance. A cycle that
  // ended as anything but merged or handoff (aborted, died, timed out,
  // interrupted) counts as a failure until the next cycle starts.
  function renderGate() {
    const gate = $("gate");
    const [stateName, text] = failure ? ["failed", "FACTORY FAILED"]
      : !state.loaded ? ["none", "NO DATA"]
      : state.paused ? ["paused", "FACTORY PAUSED"]
      : state.noResources ? ["starved", "NO RESOURCES"]
      : ["running", "FACTORY RUNNING!"];
    gate.dataset.state = stateName;
    gate.textContent = text;
  }

  // Restart a one-shot CSS animation by removing and re-adding its class.
  function replay(node, cls) {
    node.classList.remove(cls);
    void node.offsetWidth;
    node.classList.add(cls);
  }

  // A count that changes latches in with a two-frame flash.
  function setCount(id, value) {
    const node = $(id);
    const text = state.loaded && value != null ? String(value).padStart(2, "0") : "--";
    if (node.textContent === text) return;
    node.textContent = text;
    if (state.loaded) replay(node, "is-changed");
  }
  let words = [];

  const inStage = (stage) => {
    const list = state.tickets.filter((t) => t.stage === stage);
    // The queue is worked lowest number first; everything else shows newest first.
    return list.sort((a, b) => (stage === "ready" ? a.number - b.number : b.number - a.number));
  };

  function ticketButton(ticket) {
    const button = el("button", "ticket");
    button.type = "button";
    button.dataset.number = ticket.number;
    button.append(el("b", null, "#" + ticket.number), el("span", null, ticket.title));
    button.addEventListener("click", () => select(ticket.number, button));
    return button;
  }

  function renderFloor() {
    const open = state.tickets.filter((t) => t.stage !== "shipped");
    const n = (stage) => state.tickets.filter((t) => t.stage === stage).length;
    const summary = $("floor-summary");
    const text = !state.loaded
      ? "No ticket data loaded. Use the sample or load an export at the foot of the page."
      : (failure ? `Last cycle failed: #${failure.issue} ended as ${failure.status}. ` : "") +
        (state.paused ? "Factory paused. " : "") +
        `${open.length} open: ${n("ready")} in line, ${n("running")} building, ${n("review")} in review, ` +
        `${n("stopped")} stopped` + (state.live ? "." : `, ${n("backlog")} not on the line.`) +
        (state.live && queue.readAt ? ` Queue read ${ago(queue.readAt)}.` : "");
    // The summary is a line of the head panel, and the panel is part of the
    // first screen: when its text changes, so does the fold. It asks for one
    // refit per turn (refitFold), which measures once the render it is in ends.
    const moved = summary.textContent !== text;
    summary.textContent = text;

    renderGate();

    for (const stage of LINE) {
      const list = inStage(stage);
      const station = document.querySelector(`.station[data-stage="${stage}"]`);
      setCount("count-" + stage, list.length);
      const slot = $("slot-" + stage);
      slot.replaceChildren(list.length ? ticketButton(list[0]) : el("p", "empty-slot", state.loaded ? "Nothing here." : "No data."));
      const more = station.querySelector(".see-all");
      more.hidden = list.length < 2;
      more.textContent = `ALL ${list.length} ${STAGES[stage]}`;
    }

    setCount("door-open", open.length);
    setCount("door-stopped", n("stopped"));
    setCount("door-backlog", state.live ? null : n("backlog"));
    $("door-backlog-note").textContent = state.live ? "NOT IN THE LIVE QUEUE" : "OPEN, NOT QUEUED";
    $("door-stopped-link").classList.toggle("is-alert", n("stopped") > 0);
    $("door-stopped-note").textContent = n("stopped") ? "NEEDS A HUMAN" : "NOTHING STOPPED";
    if (moved) refitFold();
  }

  // The lab is not a stage. A prototype is a ticket with this label, wherever it stands on the line.
  const LAB_LABEL = "tehas:prototype";
  const inLab = () => state.tickets.filter((t) => t.stage !== "shipped" && t.labels.includes(LAB_LABEL))
    .sort((a, b) => b.number - a.number);

  function rowFor(ticket) {
    const row = el("button", "row");
    row.type = "button";
    row.dataset.number = ticket.number;
    const when = ago(ticket.stage === "shipped" ? ticket.closedAt || ticket.updatedAt : ticket.updatedAt);
    row.append(el("b", null, "#" + ticket.number), el("span", "row-title", ticket.title),
      el("span", "row-stage", STAGES[ticket.stage] + (when ? ", " + when.toUpperCase() : "")));
    row.addEventListener("click", () => select(ticket.number, row));
    const li = el("li");
    li.append(row);
    return li;
  }

  // The lab is the quick way to a prototype, so its rows also link straight to the ticket.
  function labRowFor(ticket) {
    const li = rowFor(ticket);
    if (ticket.url) {
      const a = el("a", "row-open", "OPEN");
      a.href = ticket.url;
      a.target = "_blank";
      a.rel = "noopener noreferrer";
      a.setAttribute("aria-label", `Open ticket ${ticket.number}`);
      li.classList.add("has-link");
      li.append(a);
    }
    return li;
  }

  // The Protolab's own list, /live/protolab.json, written by tehasbot protolab: every
  // ticket at the prototype stage and how much is attached to it. The live queue
  // does not carry prototypes, so with the live queue the lab is filled from this.
  // The lab is another server. A ticket is at the address the file gives it
  // when the host has a domain for the edge; otherwise under the address the
  // file names, or else at port 8081 of this host when the page came over
  // plain HTTP.
  const lab = { tickets: null, base: null, signature: "" };

  function hostedRowFor(item) {
    const li = el("li", "has-link");
    const row = el("a", "row");
    row.href = item.address || `${lab.base}/${item.number}/`;
    row.target = "_blank";
    row.rel = "noopener";
    row.setAttribute("aria-label", `Ticket ${item.number} in the Protolab: ${item.title}`);
    row.append(el("b", null, "#" + item.number), el("span", "row-title", item.title),
      el("span", "row-stage", item.files ? `${item.files} ${item.files === 1 ? "FILE" : "FILES"}` : "NO FILES"));
    li.append(row);
    if (item.url) {
      const a = el("a", "row-open", "GITHUB");
      a.href = item.url;
      a.target = "_blank";
      a.rel = "noopener noreferrer";
      a.setAttribute("aria-label", `Open ticket ${item.number} on GitHub`);
      li.append(a);
    }
    return li;
  }

  async function refreshLab() {
    if (document.hidden) return;
    let s = null;
    try {
      const r = await fetch("/live/protolab.json", { cache: "no-store" });
      if (r.ok) s = await r.json();
    } catch { /* unreachable, or no Protolab on this host */ }
    const ok = s && typeof s === "object" && Array.isArray(s.tickets);
    const tickets = !ok ? null : s.tickets.filter((t) => t && Number.isSafeInteger(t.number) && t.number > 0 && typeof t.title === "string")
      .map((t) => ({ number: t.number, title: t.title.slice(0, 180), files: Number.isSafeInteger(t.files) ? t.files : 0,
        address: typeof t.address === "string" && /^https:\/\/lab-[1-9][0-9]{0,8}\.[a-z0-9.-]+\/$/.test(t.address) ? t.address : null,
        url: typeof t.url === "string" && t.url.startsWith("https://github.com/") ? t.url : null }));
    const base = !ok ? null : /^https?:\/\/[A-Za-z0-9.-]+(:[0-9]+)?$/.test(s.url) ? s.url
      : location.protocol === "http:" ? `http://${location.hostname}:8081` : null;
    const signature = JSON.stringify([base, tickets]);
    if (signature === lab.signature) return;
    Object.assign(lab, { tickets: base ? tickets : null, base, signature });
    renderLab();
  }

  function renderLab() {
    if (state.live && lab.tickets) {
      setCount("door-lab", lab.tickets.length);
      $("lab-count").textContent = `${lab.tickets.length} OPEN`;
      $("lab-rows").replaceChildren(...(lab.tickets.length ? lab.tickets.map(hostedRowFor)
        : [el("li", "rows-note", `Nothing in the lab. An allowed account labels an open ticket ${LAB_LABEL} to put it here.`)]));
      return;
    }
    const list = inLab();
    setCount("door-lab", state.live ? null : list.length);
    $("lab-count").textContent = state.loaded && !state.live ? `${list.length} OPEN` : "";
    if (list.length) { $("lab-rows").replaceChildren(...list.map(labRowFor)); return; }
    $("lab-rows").replaceChildren(el("li", "rows-note", !state.loaded ? "No ticket data loaded."
      : state.live ? "The live queue lists only tickets on the line, and this host runs no Protolab. Load an export to see the prototypes."
      : `Nothing in the lab. An allowed account labels an open ticket ${LAB_LABEL} to put it here.`));
  }

  // Work order facts. A dash means the data does not say, or nothing was spent yet.
  const MONTHS = "JAN FEB MAR APR MAY JUN JUL AUG SEP OCT NOV DEC".split(" ");
  const day = (iso) => {
    if (!iso) return "-";
    const d = new Date(iso);
    return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`;
  };
  const tokenCount = (n) => (n == null ? "-" : n >= 1e6 ? (n / 1e6).toFixed(2) + " M" : n >= 1e3 ? Math.round(n / 1e3) + " K" : String(n));
  const spent = (secs) => {
    if (secs == null) return "-";
    const m = Math.max(1, Math.round(secs / 60)), h = Math.floor(m / 60);
    return h ? `${h} H ${m % 60} MIN` : `${m} MIN`;
  };

  // A measured figure is shown as it is; an estimate is marked with a tilde.
  const energy = (t) => {
    if (t.kWh != null) return t.kWh.toFixed(2) + " KWH";
    if (t.seconds == null) return "-";
    return "~" + (t.seconds / 3600 * state.kw).toFixed(2) + " KWH";
  };

  function renderOrder() {
    endPrint();
    const ticket = state.tickets.find((t) => t.number === state.selected);
    $("order-created").textContent = ticket ? day(ticket.createdAt) : "-";
    $("order-changed").textContent = ticket ? day(ticket.updatedAt) : "-";
    $("order-tokens").textContent = ticket ? tokenCount(ticket.tokens) : "-";
    $("order-time").textContent = ticket ? spent(ticket.seconds) : "-";
    $("order-energy").textContent = ticket ? energy(ticket) : "-";
    $("order-number").textContent = ticket ? "#" + ticket.number : "----";
    $("order-title").textContent = ticket ? ticket.title : state.loaded ? "No ticket selected." : "No ticket data loaded.";
    $("order-stage").textContent = ticket ? STAGES[ticket.stage] : "-";
    $("order-labels").textContent = ticket && ticket.labels.length ? ticket.labels.join(", ") : "-";
    const link = $("order-link");
    link.replaceChildren();
    if (ticket && ticket.url) {
      const a = el("a", null, "OPEN THE ISSUE");
      a.href = ticket.url;
      a.target = "_blank";
      a.rel = "noopener noreferrer";
      link.append(a);
    } else if (ticket) {
      link.textContent = state.sample ? "Sample ticket. No link." : "This export has no link for the ticket.";
    }
    markCurrent();
  }

  function markCurrent() {
    for (const node of document.querySelectorAll(".ticket, .row")) {
      const current = Number(node.dataset.number) === state.selected;
      node.classList.toggle("is-current", current);
      if (node.classList.contains("row")) node.setAttribute("aria-current", current ? "true" : "false");
    }
  }

  function matches(ticket) {
    if (state.filter === "open" ? ticket.stage === "shipped" : ticket.stage !== state.filter) return false;
    const q = state.query.toLowerCase();
    if (!q) return true;
    return ("#" + ticket.number).includes(q) || ticket.title.toLowerCase().includes(q) ||
      STAGES[ticket.stage].toLowerCase().includes(q) || ticket.labels.some((l) => l.includes(q));
  }

  function renderBoard() {
    const rows = $("board-rows");
    const pool = state.tickets.filter((t) => (state.filter === "open" ? t.stage !== "shipped" : t.stage === state.filter));
    const shown = state.tickets.filter(matches).sort((a, b) => b.number - a.number);
    const narrowed = state.filter !== "open" || state.query !== "";
    $("board-clear").hidden = !narrowed;
    $("board-filter").value = state.filter;
    for (const station of document.querySelectorAll(".station")) {
      station.classList.toggle("is-filtered", station.dataset.stage === state.filter);
    }
    $("board-count").textContent = !state.loaded ? "" : state.query
      ? `${shown.length} OF ${pool.length} ${state.filter === "open" ? "OPEN" : STAGES[state.filter]}`
      : `${shown.length} ${state.filter === "open" ? "OPEN" : STAGES[state.filter]}`;

    if (!shown.length) {
      const text = !state.loaded
        ? "No ticket data loaded. Use the sample or load an export at the foot of the page."
        : state.query
          ? `Nothing matches "${state.query}". Clear the search to see every ticket.`
          : state.live && state.filter === "backlog" ? "The live queue lists only tickets on the line. Load an export to see the rest."
          : `No tickets are ${state.filter === "open" ? "open" : STAGES[state.filter].toLowerCase()}.`;
      rows.replaceChildren(el("li", "rows-note", text));
      return;
    }
    rows.replaceChildren(...shown.map(rowFor));
  }

  // Selecting never re-renders the list, so focus and scroll position stay put.
  // The work order arrives over a 1200 bps line: ten bits a character, so 120
  // characters a second, printed in reading order, line by line. A newer order
  // or a re-render simply replaces the text; reduced motion shows it at once.
  const BAUD = 1200;
  let printJob = 0, printing = [];
  // End a print in progress by putting every piece of text back whole. The
  // labels on the screen are fixed text, so a cut-off print must not leave them short.
  function endPrint() {
    printJob++;
    for (const [node, full] of printing) node.nodeValue = full;
    printing = [];
    document.querySelector(".monitor").removeAttribute("aria-busy");
  }
  function printOrder() {
    endPrint();
    const job = printJob;
    const monitor = document.querySelector(".monitor");
    if (calm.matches) return;
    const walker = document.createTreeWalker(document.querySelector(".screen"), NodeFilter.SHOW_TEXT);
    const pieces = [];
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      // Whitespace between elements is layout, not text on the line.
      if (/\S/.test(node.nodeValue)) { pieces.push([node, node.nodeValue]); node.nodeValue = ""; }
    }
    printing = pieces;
    // Screen readers get the finished order, not every character.
    monitor.setAttribute("aria-busy", "true");
    const began = performance.now();
    let piece = 0, sent = 0;
    const tick = (now) => {
      if (job !== printJob) return;
      const due = Math.floor((now - began) / 1000 * (BAUD / 10));
      while (sent < due && piece < pieces.length) {
        const [node, full] = pieces[piece];
        const have = node.nodeValue.length;
        const take = Math.min(full.length - have, due - sent);
        node.nodeValue = full.slice(0, have + take);
        sent += take;
        if (have + take === full.length) piece++;
      }
      if (piece < pieces.length) requestAnimationFrame(tick);
      else { printing = []; monitor.removeAttribute("aria-busy"); }
    };
    requestAnimationFrame(tick);
  }

  // Selecting a ticket shows where it goes: its number lifts off the place that
  // was pressed and flies, in sprite steps, to the work order monitor, which
  // then prints the ticket at modem speed. The flight is skipped when
  // motion is reduced or the monitor's number is not on screen.
  // Whatever the flight does, the place that was pressed answers: it latches in
  // brass for a beat. Below 1440px the work order is a panel in the column and
  // can be off screen when a row is tapped, so the latch is what the tap gets.
  let flight = 0;
  function select(number, origin) {
    const changed = state.selected !== number;
    state.selected = number;
    markCurrent();
    if (origin) replay(origin, "is-changed");
    const screen = document.querySelector(".screen");
    const load = () => { renderOrder(); if (changed) printOrder(); };
    const from = origin && origin.querySelector("b");
    const to = $("order-number").getBoundingClientRect();
    const onScreen = to.width > 0 && to.top >= 0 && to.bottom <= window.innerHeight;
    if (!changed || !from || calm.matches || !onScreen || !from.animate) { flight++; monitorWaits(false); load(); return; }

    const a = from.getBoundingClientRect();
    const token = ++flight;
    const flyer = el("span", "flyer", "#" + number);
    flyer.style.left = a.left + "px";
    flyer.style.top = a.top + "px";
    document.body.append(flyer);
    monitorWaits(true);
    const move = flyer.animate(
      [{ transform: "translate(0, 0) scale(1)" }, { transform: `translate(${to.left - a.left}px, ${to.top - a.top}px) scale(2)` }],
      { duration: 320, easing: "steps(8, end)" },
    );
    const land = () => {
      flyer.remove();
      if (token !== flight) return;
      monitorWaits(false);
      load();
    };
    move.onfinish = land;
    move.oncancel = land;
  }
  function monitorWaits(on) { document.querySelector(".monitor").classList.toggle("is-receiving", on); }

  function setFilter(filter, query = state.query) {
    // A new stage prints its rows in; typing in the search does not.
    if (filter !== state.filter) {
      replay($("board-rows"), "is-fresh");
      clearTimeout(setFilter.timer);
      setFilter.timer = setTimeout(() => $("board-rows").classList.remove("is-fresh"), 400);
    }
    state.filter = filter;
    state.query = query;
    $("board-search").value = query;
    renderBoard();
    renderOrder();
  }

  // On the widths where the monitor stands beside the stations the first screen
  // (the head panel, the stations, the doors) is centred in the window with
  // equal room above and below, and the board starts under the fold. Offsets are
  // used, not client rects, so the load animation does not skew them. The head
  // panel's box is one of the inputs: its border, its padding and its lines
  // stand above the doors. renderFloor asks for a refit when that panel changes;
  // so do a resize and the fonts settling. The width is the one where game-view.css
  // gives the stage its monitor column: centring the first screen is what fills
  // the height that column stands over.
  const wide = window.matchMedia("(min-width: 1440px)");
  function fitFold() {
    const fold = globalThis.TEHAS_FOLD_FIT;
    if (!fold) return; // fold-fit.js did not load: the CSS fallbacks stand
    const style = document.documentElement.style;
    style.setProperty("--fold-pad", fold.TOP + "px");
    style.setProperty("--fold-gap", fold.GAP + "px");
    style.removeProperty("--monitor-height");
    if (!wide.matches) return;
    const doors = document.querySelector(".doors");
    const line = document.querySelector(".line");
    if (!doors || !line) return;
    style.setProperty("--monitor-height", fold.monitorHeight(line.offsetTop, doors.offsetTop + doors.offsetHeight) + "px");
    // Read again: the monitor's height feeds the rows the doors stand in. The
    // height is a floor, so a work order taller than the stand moves the doors
    // as well, and is measured again at the next refit.
    const room = fold.fit(window.innerHeight, doors.offsetTop + doors.offsetHeight);
    style.setProperty("--fold-pad", room.foldPad + "px");
    style.setProperty("--fold-gap", room.foldGap + "px");
  }
  // fitFold reads offsets, which forces layout, and a render that asks for it has
  // not finished: measured mid-render the numbers come from content that is about
  // to be replaced. Several renders may ask in one turn, so they get one read,
  // taken at the end of the turn when every change in it has been made.
  let foldAsked = false;
  function refitFold() {
    if (foldAsked) return;
    foldAsked = true;
    queueMicrotask(() => { foldAsked = false; fitFold(); });
  }
  window.addEventListener("resize", refitFold);
  // Centring the first screen is a nicety: a failure here stops nothing else.
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(refitFold, () => {});

  function show(data, sourceText, statusText) {
    Object.assign(state, { tickets: data.tickets, paused: data.paused, noResources: data.noResources, sample: data.sample, live: Boolean(data.live), loaded: true, filter: "open", query: "", kw: data.kw });
    $("board-all").hidden = !(state.live && queue.all);
    if (state.live && queue.all) $("board-all").href = queue.all;
    $("energy-basis").textContent = `time spent at an assumed ${state.kw} kW, unless the data carries a measured figure`;
    words = data.words;
    const first = inStage("running")[0] || inStage("review")[0] || inStage("ready")[0] || state.tickets[0];
    state.selected = first ? first.number : null;
    const source = $("source");
    source.textContent = sourceText;
    source.title = statusText;
    source.dataset.kind = data.live ? "live" : data.sample ? "sample" : "export";
    $("data-status").textContent = statusText;
    $("board-search").value = "";
    renderFloor();
    renderBoard();
    renderLab();
    renderOrder();
    printOrder();
    resetGame();
  }

  // The live queue: the snapshot the dispatcher writes to /live/status.json, the
  // same file the plain dashboard reads. In "auto" mode the page shows it whenever
  // it can be reached and re-reads it every ten seconds; choosing the sample or an
  // export switches to "manual" until the viewer asks for the live queue again.
  const MODE_KEY = "tehas-game-view-mode";
  const queue = { mode: "auto", readAt: null, all: null, signature: "" };
  const IMPLIED = { queued: "tehas:ready", running: "tehas:running", awaiting: "tehas:awaiting-merge", stopped: "needs-human-attention" };

  function fromSnapshot(s) {
    const byNumber = new Map();
    const take = (key, closed) => {
      for (const item of Array.isArray(s[key]) ? s[key] : []) {
        const number = Number(item && item.number);
        if (!Number.isSafeInteger(number) || number <= 0 || typeof item.title !== "string") continue;
        const t = byNumber.get(number) || { number, title: item.title, state: "OPEN", labels: [], url: item.url, updatedAt: item.updatedAt, closedAt: item.closedAt };
        if (IMPLIED[key]) t.labels.push(IMPLIED[key]);
        if (closed) t.state = "CLOSED";
        byNumber.set(number, t);
      }
    };
    for (const key of ["queued", "running", "awaiting", "stopped"]) take(key, false);
    take("completed", true);
    const data = normalize({ issues: [...byNumber.values()], paused: Boolean(s.paused), noResources: Boolean(s.noResources), buzzwords: s.buzzwords });
    data.live = true;
    return data;
  }

  function setMode(mode) {
    queue.mode = mode;
    try { localStorage.setItem(MODE_KEY, mode); } catch { /* storage may be unavailable */ }
  }

  function freshness() {
    if (!state.live) return;
    const stale = !queue.readAt || Date.now() - new Date(queue.readAt).getTime() > 10 * 60 * 1000;
    const source = $("source");
    source.textContent = stale ? "QUEUE STALE" : "LIVE QUEUE";
    source.dataset.kind = stale ? "stale" : "live";
    const text = `Showing the factory's live queue, read ${ago(queue.readAt) || "at an unknown time"}: ${state.tickets.length} tickets on the line. ` +
      "Prototypes and tickets not on the line are not in the queue snapshot.";
    source.title = text;
    $("data-status").textContent = text;
  }

  // Returns true when the live queue was read. Re-renders only when it changed,
  // and then keeps the viewer's filter, search, selection, focus and game.
  async function refreshQueue() {
    if (queue.mode !== "auto" || document.hidden) return state.live;
    let s = null;
    try {
      const r = await fetch("/live/status.json", { cache: "no-store" });
      if (r.ok) s = await r.json();
    } catch { /* unreachable */ }
    if (queue.mode !== "auto") return state.live;
    if (!s || typeof s !== "object") { freshness(); return false; }
    queue.readAt = typeof s.generatedAt === "string" ? s.generatedAt : null;
    const all = s.links && typeof s.links.all === "string" && s.links.all.startsWith("https://") ? s.links.all : null;
    queue.all = all;
    const data = fromSnapshot(s);
    // The words can change while the tickets on the line stay as they were.
    if (state.live) words = data.words;
    const signature = JSON.stringify([data.paused, data.noResources, data.tickets.map((t) => [t.number, t.title, t.stage, t.updatedAt, t.closedAt])]);
    if (!state.live) {
      queue.signature = signature;
      show(data, "LIVE QUEUE", "");
    } else if (signature !== queue.signature) {
      queue.signature = signature;
      const focused = document.activeElement && document.activeElement.dataset ? document.activeElement : null;
      const keep = focused && focused.dataset.number ? { list: focused.closest("ul, .slot"), number: focused.dataset.number } : null;
      state.tickets = data.tickets;
      state.paused = data.paused;
      state.noResources = data.noResources;
      words = data.words;
      if (!state.tickets.some((t) => t.number === state.selected)) {
        const first = inStage("running")[0] || inStage("review")[0] || inStage("ready")[0] || state.tickets[0];
        state.selected = first ? first.number : null;
      }
      renderFloor();
      renderBoard();
      renderLab();
      renderOrder();
      if (game.mode === "ready") resetGame();
      if (keep && keep.list) {
        const again = keep.list.querySelector(`[data-number="${keep.number}"]`);
        if (again) again.focus({ preventScroll: true });
      }
    } else {
      renderFloor();
    }
    freshness();
    return true;
  }

  function useSample() {
    if (!window.TEHAS_SAMPLE_ISSUES) {
      $("data-status").textContent = "The sample file did not load. Reload the page, or load an export.";
      return;
    }
    const data = normalize(window.TEHAS_SAMPLE_ISSUES);
    show(data, "SAMPLE DATA",
      `Showing the bundled sample: ${data.tickets.length} fictional tickets. Load an export to see real ones.`);
  }

  function useExport(raw, name, savedAt) {
    const data = normalize(raw);
    data.sample = false;
    const when = new Date(savedAt).toLocaleString();
    show(data, "LOCAL EXPORT",
      `Showing ${data.tickets.length} tickets from ${name}, loaded ${when}. Kept in this browser only.`);
  }

  $("data-sample").addEventListener("click", () => {
    setMode("manual");
    try { localStorage.removeItem(STORAGE_KEY); } catch { /* storage may be unavailable */ }
    useSample();
  });
  $("data-live").addEventListener("click", async () => {
    setMode("auto");
    try { localStorage.removeItem(STORAGE_KEY); } catch { /* storage may be unavailable */ }
    state.live = false;
    if (!(await refreshQueue())) {
      $("data-status").textContent = "The live queue could not be read from this page. Showing what was here before; it will switch over when the queue is reachable.";
    }
  });
  $("data-file").addEventListener("change", async (event) => {
    const file = event.target.files && event.target.files[0];
    if (!file) return;
    try {
      const raw = JSON.parse(await file.text());
      const savedAt = new Date().toISOString();
      setMode("manual");
      useExport(raw, file.name, savedAt);
      try { localStorage.setItem(STORAGE_KEY, JSON.stringify({ raw, name: file.name, savedAt })); } catch { /* too large or blocked */ }
    } catch {
      $("data-status").textContent =
        `Could not read ${file.name}. It needs to be JSON with a list of issues, each with a number, title, state and labels. ` +
        "createdAt, updatedAt, tokens, timeSpentSeconds and kWh are shown when present.";
    }
    event.target.value = "";
  });

  $("board-search").addEventListener("input", (event) => setFilter(state.filter, event.target.value.trim()));
  $("board-filter").addEventListener("change", (event) => setFilter(event.target.value));
  $("board-clear").addEventListener("click", () => { setFilter("open", ""); $("board-search").focus(); });
  for (const door of document.querySelectorAll("a[data-filter]")) {
    door.addEventListener("click", () => setFilter(door.dataset.filter, ""));
  }
  for (const button of document.querySelectorAll(".see-all")) {
    button.addEventListener("click", () => {
      setFilter(button.dataset.filter, "");
      $("board").scrollIntoView({ block: "start" });
      $("board").focus({ preventScroll: true });
    });
  }

  // Buzzword Buster: a break, not a score. Nothing here reads or writes tickets.
  const canvas = $("brick-canvas");
  const ctx = canvas.getContext("2d");
  const W = canvas.width, H = canvas.height;
  const game = { mode: "ready", score: 0, lives: 3, time: 45, paddleX: W / 2, paddleW: 72, x: W / 2, y: H - 40, vx: 145, vy: -175, bricks: [] };
  let last = performance.now(), leftHeld = false, rightHeld = false;
  let debris = [], paddleFlash = 0;

  function gameUi(message) {
    $("brick-score").textContent = String(game.score).padStart(3, "0");
    $("brick-time").textContent = String(Math.ceil(game.time)).padStart(2, "0");
    $("brick-lives").textContent = game.lives;
    const play = $("brick-play");
    play.disabled = !words.length;
    play.textContent = game.mode === "playing" ? "PAUSE" : game.mode === "paused" ? "RESUME" : game.mode === "over" ? "PLAY AGAIN" : "PLAY";
    if (message) $("brick-message").textContent = message;
  }

  function resetGame() {
    Object.assign(game, { mode: "ready", score: 0, lives: 3, time: 45, paddleX: W / 2, x: W / 2, y: H - 40, vx: 145, vy: -175 });
    // A new wall every round: other words, other tetrominoes, other places, and
    // never more pieces than the wall has room for (brick-wall.js).
    game.bricks = window.TEHAS_BRICK_WALL.build(words).map((piece) => ({ ...piece, alive: true }));
    $("brick-word").textContent = "NO WORD HIT YET";
    gameUi(words.length ? "45 seconds, 3 balls." : "Load ticket data to play.");
  }

  function endGame(message) { game.mode = "over"; gameUi(`${message} Score ${game.score}.`); }

  function toggleGame() {
    if (!words.length) return;
    if (game.mode === "playing") { game.mode = "paused"; gameUi("Paused."); return; }
    if (game.mode === "over") resetGame();
    game.mode = "playing";
    gameUi("Playing.");
  }

  function stepGame(dt) {
    game.time = Math.max(0, game.time - dt);
    if (game.time === 0) return endGame("Time is up.");
    const half = game.paddleW / 2;
    game.paddleX = Math.max(half + 8, Math.min(W - half - 8, game.paddleX + (Number(rightHeld) - Number(leftHeld)) * 270 * dt));
    game.x += game.vx * dt;
    game.y += game.vy * dt;
    if (game.x < 8) { game.x = 8; game.vx = Math.abs(game.vx); }
    if (game.x > W - 8) { game.x = W - 8; game.vx = -Math.abs(game.vx); }
    if (game.y < 8) { game.y = 8; game.vy = Math.abs(game.vy); }
    if (game.vy > 0 && game.y >= H - 32 && game.y <= H - 16 && Math.abs(game.x - game.paddleX) < half + 4) {
      game.y = H - 32;
      paddleFlash = 0.08;
      game.vy = -Math.abs(game.vy);
      game.vx = 165 * (game.x - game.paddleX) / half + Math.sign(game.vx) * 55;
    }
    for (const b of game.bricks) {
      // A piece is hit where one of its cells is, and that one piece goes.
      const hit = b.alive && b.rects.find((r) => game.x + 4 > r.x && game.x - 4 < r.x + r.w && game.y + 4 > r.y && game.y - 4 < r.y + r.h);
      if (!hit) continue;
      b.alive = false;
      if (!calm.matches) {
        for (let i = 0; i < 4; i++) {
          debris.push({ x: hit.x + hit.w / 2, y: hit.y + hit.h / 2, vx: (i - 1.5) * 40, vy: -70 - (i % 3) * 30, life: 0.4, color: b.color });
        }
      }
      // The ball turns back the way it came in: off a side when it is less
      // deep in the cell sideways than up or down, as it is in a notch.
      const deepX = Math.min(game.x + 4 - hit.x, hit.x + hit.w - (game.x - 4));
      const deepY = Math.min(game.y + 4 - hit.y, hit.y + hit.h - (game.y - 4));
      if (deepX < deepY) game.vx *= -1; else game.vy *= -1;
      // The word scores when its last piece goes, not at the first one.
      if (game.bricks.every((o) => o.wordKey !== b.wordKey || !o.alive)) {
        game.score += 10;
        $("brick-word").textContent = `${b.word.word.toUpperCase()}: ${b.word.count} MENTIONS${state.sample ? " IN THE SAMPLE" : ""}`;
      }
      break;
    }
    if (game.y > H + 8) {
      game.lives--;
      if (game.lives <= 0) return endGame("Out of balls.");
      Object.assign(game, { x: game.paddleX, y: H - 40, vx: (Math.random() < 0.5 ? -1 : 1) * 145, vy: -175 });
    }
    if (game.bricks.every((b) => !b.alive)) return endGame("Wall cleared.");
    gameUi();
  }

  function drawGame() {
    ctx.fillStyle = "#222";
    ctx.fillRect(0, 0, W, H);
    ctx.fillStyle = "#445";
    ctx.fillRect(0, H - 16, W, 16);
    ctx.font = "16px Topaz, monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    for (const b of game.bricks) {
      if (!b.alive) continue;
      // Every shadow first, so no part of a piece lies in its own shade.
      ctx.fillStyle = "#111";
      for (const r of b.rects) ctx.fillRect(r.x + 4, r.y + 4, r.w, r.h);
      ctx.fillStyle = b.color;
      for (const r of b.rects) ctx.fillRect(r.x, r.y, r.w, r.h);
      // One letter in each cell it has one. The gap between the cells is drawn
      // inside a word too, so its pieces can still be told apart.
      ctx.fillStyle = "#144";
      for (const [i, letter] of b.letters.entries()) {
        if (!letter) continue;
        const cell = b.rects[i];
        ctx.fillText(letter.toUpperCase(), cell.x + cell.w / 2, cell.y + cell.h / 2 + 1);
      }
    }
    const left = Math.round(game.paddleX - game.paddleW / 2);
    ctx.fillStyle = "#111";
    ctx.fillRect(left + 4, H - 28, game.paddleW, 8);
    ctx.fillStyle = paddleFlash > 0 ? "#eec" : "#fc6";
    ctx.fillRect(left, H - 32, game.paddleW, 8);
    for (const d of debris) {
      ctx.fillStyle = d.color;
      ctx.fillRect(Math.round(d.x / 4) * 4, Math.round(d.y / 4) * 4, 4, 4);
    }
    ctx.fillStyle = "#eec";
    ctx.fillRect(Math.round(game.x) - 4, Math.round(game.y) - 4, 8, 8);
    if (game.mode !== "playing") {
      const label = game.mode === "over" ? "ROUND OVER" : game.mode === "paused" ? "PAUSED" : words.length ? "BUZZWORD BUSTER" : "NO TICKET DATA";
      ctx.fillStyle = "#222";
      ctx.fillRect(W / 2 - 136, H / 2 + 28, 272, 40);
      ctx.fillStyle = "#fc6";
      ctx.fillText(label, W / 2, H / 2 + 49);
    }
  }

  function frame(now) {
    const dt = Math.min(0.04, (now - last) / 1000 || 0);
    last = now;
    if (game.mode === "playing") stepGame(dt);
    paddleFlash = Math.max(0, paddleFlash - dt);
    for (const d of debris) { d.life -= dt; d.x += d.vx * dt; d.y += d.vy * dt; d.vy += 420 * dt; }
    debris = debris.filter((d) => d.life > 0);
    drawGame();
    requestAnimationFrame(frame);
  }

  const paddleTo = (event) => {
    const rect = canvas.getBoundingClientRect();
    const half = game.paddleW / 2;
    game.paddleX = Math.max(half + 8, Math.min(W - half - 8, (event.clientX - rect.left) * W / rect.width));
  };
  $("brick-play").addEventListener("click", toggleGame);
  canvas.addEventListener("pointermove", paddleTo);
  canvas.addEventListener("pointerdown", (event) => { paddleTo(event); if (game.mode !== "playing") toggleGame(); });
  document.addEventListener("keydown", (event) => {
    if (event.target.closest("input, select, textarea")) return;
    if ((event.key === "p" || event.key === "P") && game.mode !== "ready" && game.mode !== "over") { toggleGame(); return; }
    if (game.mode !== "playing") return;
    if (event.key === "ArrowLeft") { leftHeld = true; event.preventDefault(); }
    if (event.key === "ArrowRight") { rightHeld = true; event.preventDefault(); }
  });
  document.addEventListener("keyup", (event) => {
    if (event.key === "ArrowLeft") leftHeld = false;
    if (event.key === "ArrowRight") rightHeld = false;
  });
  document.addEventListener("visibilitychange", () => {
    if (document.hidden && game.mode === "playing") { game.mode = "paused"; gameUi("Paused while the page was away."); }
  });

  // The production console. Unlike the tickets above, this is live: it reads the
  // files the dispatcher writes under /live/, the same ones the plain dashboard
  // reads, and tails the selected cycle's console with Range requests so it only
  // fetches what is new. Console text is untrusted and only ever appended as text.
  // Several cycles can run side by side: console-tabs.js says which there are.
  const TABS = globalThis.TEHAS_CONSOLE_TABS;
  const live = { file: null, offset: 0, tailing: false, running: false, titles: new Map(), now: null, selected: null };
  const duration = (secs) => {
    const m = Math.max(0, Math.floor(secs / 60)), h = Math.floor(m / 60);
    return h > 0 ? `${h} h ${m % 60} min` : `${m} min`;
  };

  // The tag describes the cycle that is shown. The door, the monitor's lamp
  // and the console tag's blink say whether any cycle is running at all.
  function consoleState(state, text, doorText, working) {
    $("console-state").dataset.state = state;
    $("console-state").textContent = text;
    $("door-console").textContent = doorText;
    $("door-console-link").classList.toggle("is-alert", working);
    document.body.dataset.factory = working ? "live" : state;
  }

  async function refreshConsole() {
    if (document.hidden) return;
    let now = null;
    try {
      const r = await fetch("/live/running.json", { cache: "no-store" });
      if (r.ok) now = await r.json();
    } catch { /* unreachable: handled in showConsole */ }
    live.now = now;
    if (TABS.view(now, null).tabs.length) {
      try {
        const r = await fetch("/live/status.json", { cache: "no-store" });
        if (r.ok) {
          const s = await r.json();
          for (const key of ["running", "queued", "stopped", "awaiting", "completed"]) {
            for (const t of Array.isArray(s[key]) ? s[key] : []) live.titles.set(Number(t.number), String(t.title || ""));
          }
        }
      } catch { /* titles are a nicety */ }
    }
    showConsole();
  }

  function selectConsole(issue) {
    live.selected = issue;
    showConsole();
  }

  function showConsole() {
    const v = TABS.view(live.now, live.selected);
    const before = failure && failure.issue + failure.status;
    failure = v.failure;
    if ((failure && failure.issue + failure.status) !== before) renderFloor();
    TABS.render($("console-tabs"), v, selectConsole);
    const tab = v.shown;
    if (!tab) {
      live.file = null;
      live.running = false;
      consoleState("off", "NO CONSOLE", "NOTHING TO SHOW", false);
      $("console-cycle").textContent = "No cycle to show.";
      $("console-step").textContent = "Either no cycle has run yet, or this page cannot reach the factory's live files.";
      $("console-out").textContent = "";
      $("console-out").dataset.empty = "The console of a running cycle appears here.";
      $("console-note").textContent = "";
      return;
    }
    const now = tab.cycle;
    live.running = tab.running;
    if (live.running) live.idle = false;
    const title = live.titles.get(now.issue);
    $("console-cycle").textContent = (live.running ? "Working on #" : "Last cycle: #") + now.issue + (title ? "  " + title : "");
    const elapsed = (live.running ? Date.now() / 1000 : Number(now.updatedAt)) - Number(now.startedAt);
    $("console-step").textContent = live.running
      ? `${now.statusText || "step " + now.step} (${duration(elapsed)} so far)`
      : `Ended as ${now.status} at step ${now.step} after ${duration(elapsed)}.`;
    const door = v.running === 0 ? "IDLE" : v.running === 1 ? `WORKING ON #${v.tabs[0].issue}` : `WORKING ON ${v.running} ISSUES`;
    consoleState(live.running ? "live" : "ended", live.running ? "LIVE" : "ENDED", door, v.running > 0);

    const file = TABS.consoleFile(now);
    if (file !== live.file) {
      live.file = file;
      live.offset = 0;
      live.idle = false;
      $("console-out").textContent = "";
      $("console-out").dataset.empty = file ? "Waiting for output." : "This cycle has no console file.";
      tailConsole();
    }
  }

  async function tailConsole() {
    // An ended cycle's console is read once; asking again only earns a 416.
    // Two requests for the same offset would both append their answer.
    if (!live.file || document.hidden || live.idle || live.tailing) return;
    live.tailing = true;
    const out = $("console-out"), file = live.file;
    try {
      const r = await fetch("/live/" + file, { headers: { Range: `bytes=${live.offset}-` }, cache: "no-store" });
      // A tab was chosen while this request was out: its answer is for the old one.
      if (file !== live.file) return;
      if (r.status === 206 || (r.status === 200 && live.offset === 0)) {
        const bytes = new Uint8Array(await r.arrayBuffer());
        if (file !== live.file) return;
        if (bytes.length) {
          live.offset += bytes.length;
          out.append(new TextDecoder().decode(bytes));
          // Keep the page light on a long cycle.
          if (out.textContent.length > 400000) out.textContent = out.textContent.slice(-300000);
          if ($("console-follow").checked) out.scrollTop = out.scrollHeight;
        }
      } else if (r.status === 416) {
        if (!live.running) live.idle = true;
      } else if (r.status === 200) {
        // The file was replaced by a new cycle.
        live.offset = 0;
        out.textContent = "";
      }
      $("console-note").textContent = live.running ? "LIVE" : "ENDED";
    } catch {
      $("console-note").textContent = "CONSOLE UNAVAILABLE";
    } finally {
      live.tailing = false;
    }
  }

  $("console-follow").addEventListener("change", (event) => {
    if (event.target.checked) $("console-out").scrollTop = $("console-out").scrollHeight;
  });
  TABS.keys($("console-tabs"), selectConsole);
  refreshConsole();
  setInterval(refreshConsole, 10000);
  setInterval(tailConsole, 2000);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) refreshConsole(); });

  // Where the tickets come from: an export saved in this browser if the viewer
  // chose one, otherwise the factory's live queue, otherwise the bundled sample.
  let saved = null;
  try { saved = JSON.parse(localStorage.getItem(STORAGE_KEY) || "null"); } catch { saved = null; }
  let storedMode = null;
  try { storedMode = localStorage.getItem(MODE_KEY); } catch { /* storage may be unavailable */ }
  queue.mode = storedMode === "manual" || (saved && saved.raw && storedMode !== "auto") ? "manual" : "auto";
  const fallback = () => {
    try {
      if (saved && saved.raw) { useExport(saved.raw, saved.name || "export", saved.savedAt || Date.now()); return; }
    } catch {
      $("data-status").textContent = "The export saved in this browser could not be read. Showing the sample.";
    }
    useSample();
  };
  renderFloor(); renderBoard(); renderLab(); renderOrder(); resetGame();
  if (queue.mode === "manual") fallback();
  else refreshQueue().then((ok) => { if (!ok && !state.loaded) fallback(); });
  setInterval(refreshQueue, 10000);
  refreshLab();
  setInterval(refreshLab, 60000);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) { refreshQueue(); refreshLab(); } });
  requestAnimationFrame(frame);
})();
