// The business layout's front page (TEHAS_LAYOUT=business, host/fun/business.html):
// the live strip, the production line, the last ten days and the issue board,
// from the files the dispatcher writes under /live/. See host/docs/host.md,
// "Layouts".
//
// lineView(), liveView() and boardRows() turn files into what is drawn and
// have no side effects, so they are tested without a browser
// (test/business_view_test.mjs). The rest fetches and draws, and runs only on
// a page that has the line.

import { analyticsView, dueToRead as dueAnalytics } from "./analytics-data.js";
import { layout as energyLayout, PERIOD as ENERGY_PERIOD } from "./energy-view.js";

export const STALE_QUEUE = 10 * 60; // seconds, as the factory floor counts it
export const FLIP_EVERY = 7; // seconds between waves of the line's cards

// The line, left to right. `from` is the list in status.json; `search` the
// GitHub query for the stage's "all" link.
export const STAGES = [
  { key: "proto", label: "Prototypes", first: "Latest", later: "Prototype", none: "No prototypes." },
  { key: "line", label: "In line", from: "queued", first: "Next up", later: "In line", search: "is:open label:tehas:ready", none: "Nothing in line." },
  { key: "build", label: "Building", from: "running", first: "In work", later: "In work", search: "is:open label:tehas:running", none: "Nothing building." },
  { key: "review", label: "Review", from: "awaiting", first: "Needs review", later: "Needs review", search: "is:open label:tehas:awaiting-merge", none: "Nothing to review." },
  { key: "stop", label: "Stopped", from: "stopped", first: "Needs a human", later: "Needs a human", search: "is:open label:needs-human-attention", none: "Nothing stopped." },
  { key: "ship", label: "Shipped", from: "completed", first: "Latest", later: "Shipped", search: "is:closed", none: "Nothing shipped yet." },
];
const MORE = { proto: "in Protolab", line: "in line", build: "building", review: "in review", stop: "stopped", ship: "shipped" };

const list = (v) => (Array.isArray(v) ? v : []);
const obj = (v) => (v && typeof v === "object" && !Array.isArray(v) ? v : {});
const github = (v) => (typeof v === "string" && v.startsWith("https://github.com/") ? v : null);
// Ticket text is untrusted: cut, and only ever set through textContent.
function ticket(item) {
  const number = Number(item && item.number);
  if (!Number.isSafeInteger(number) || number <= 0 || typeof item.title !== "string") return null;
  return { number, title: item.title.slice(0, 180), url: github(item.url) };
}

// The Protolab's own list, as the factory floor reads it: a ticket's page is
// at the address the file gives, or under the Protolab's base address.
export function labBase(lab, pageUrl) {
  const s = obj(lab);
  if (typeof s.url === "string" && /^https?:\/\/[A-Za-z0-9.-]+(:[0-9]+)?$/.test(s.url)) return s.url;
  const u = pageUrl ? new URL(pageUrl) : null;
  return u && u.protocol === "http:" ? `http://${u.hostname}:8081` : null;
}
function labItems(lab, base) {
  return list(obj(lab).tickets).map((t) => {
    const item = ticket(t);
    if (!item) return null;
    const own = typeof t.address === "string" && /^https:\/\/lab-[1-9][0-9]{0,8}\.[a-z0-9.-]+\/$/.test(t.address) ? t.address : null;
    return { ...item, url: own || (base ? `${base}/${item.number}/` : item.url) };
  }).filter(Boolean);
}

// The sample, as the factory floor stages it: by label, a closed one shipped.
function fromSample(raw) {
  const out = Object.fromEntries(STAGES.map((s) => [s.key, []]));
  for (const i of list(obj(raw).issues)) {
    const item = ticket(i);
    if (!item) continue;
    const labels = list(i.labels).map((l) => String(typeof l === "string" ? l : obj(l).name || "").toLowerCase());
    if (labels.includes("tehas:prototype")) out.proto.push(item);
    const key = String(i.state || "").toLowerCase() === "closed" ? "ship"
      : labels.includes("needs-human-attention") ? "stop"
        : labels.includes("tehas:awaiting-merge") ? "review"
          : labels.includes("tehas:running") ? "build"
            : labels.includes("tehas:ready") ? "line" : null;
    if (key) out[key].push(item);
  }
  for (const i of list(obj(raw).shippedIssues)) { const item = ticket(i); if (item) out.ship.push(item); }
  return out;
}

// status: /live/status.json or null. lab: /live/protolab.json or null.
// sample: the bundled sample, used only where there is no status file.
export function lineView({ status = null, lab = null, sample = null, nowMs = Date.now(), pageUrl = null } = {}) {
  const s = status && typeof status === "object" ? status : null;
  const generated = s ? Date.parse(s.generatedAt) : NaN;
  const source = s ? (nowMs - generated <= STALE_QUEUE * 1000 ? "live" : "stale") : sample ? "sample" : "none";
  const repo = s && typeof s.repo === "string" && /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(s.repo) ? s.repo : null;
  const base = lab ? labBase(lab, pageUrl) : null;
  const items = s
    ? Object.fromEntries(STAGES.map((st) => [st.key, st.from ? list(s[st.from]).map(ticket).filter(Boolean) : []]))
    : sample ? fromSample(sample) : Object.fromEntries(STAGES.map((st) => [st.key, []]));
  if (s) items.proto = base ? labItems(lab, base) : [];
  // A ticket stands at one place on the line, as on the factory floor: a
  // stopped one is stopped wherever else the queue lists it.
  const seen = new Set();
  for (const key of ["stop", "review", "build", "line"]) {
    items[key] = items[key].filter((t) => !seen.has(t.number) && seen.add(t.number));
  }
  const stages = STAGES.map((st) => {
    const these = items[st.key];
    const href = st.key === "proto" ? base
      : repo ? `https://github.com/${repo}/issues?q=${encodeURIComponent(st.search)}` : null;
    const text = st.key === "stop" ? "See what stopped" : `All ${these.length} ${MORE[st.key]}`;
    return { key: st.key, label: st.label, count: these.length, items: these, first: st.first, later: st.later, none: st.none,
      more: these.length && href ? { text, href } : null };
  });
  return {
    source,
    paused: Boolean(s && s.paused),
    repo: repo ? `https://github.com/${repo}` : null,
    lab: base,
    stages,
  };
}

// The open tickets on the line, for the issue board: in the line's order.
export function boardRows(view) {
  return view.stages.filter((s) => ["line", "build", "review", "stop"].includes(s.key))
    .flatMap((s) => s.items.map((t) => ({ ...t, stage: s.key, stageLabel: s.label })));
}

// energy: /live/energy.json or null. The power of the last 30 minutes from the
// energy view's own columns, and the tokens a minute between samples.
export function liveView(energy, nowMs = Date.now()) {
  if (!energy || typeof energy !== "object" || !Array.isArray(energy.hosts)) return { present: false };
  const v = energyLayout(energy, nowMs);
  const power = v.columns.map((parts) => (parts && parts.length ? parts[parts.length - 1].to : null));
  const samples = list(energy.samples)
    .filter((s) => s && Number.isFinite(s.t) && s.k && typeof s.k === "object" && s.t * 1000 > nowMs - 30 * 60 * 1000)
    .sort((a, b) => a.t - b.t);
  const sum = (k) => Object.values(k).reduce((a, n) => a + (Number.isFinite(n) && n >= 0 ? n : 0), 0);
  const tokens = [];
  for (let i = 1; i < samples.length; i++) {
    const dt = samples[i].t - samples[i - 1].t, dk = sum(samples[i].k) - sum(samples[i - 1].k);
    if (dt > 0 && dk >= 0) tokens.push(Math.round(dk / dt * 60));
  }
  return {
    present: true,
    stale: v.state === "stale",
    watts: v.watts,
    tokens: v.meter.tokens, // null where the mesh counts no tokens
    kwh24: v.kwh24,
    kwh30: v.kwh30,
    power,
    tokenRates: v.meter.tokens == null ? [] : tokens,
  };
}

// ---- The page.
if (typeof document !== "undefined" && document.getElementById("line")) {
  const $ = (id) => document.getElementById(id);
  const NS = "http://www.w3.org/2000/svg";
  const el = (tag, cls, text) => {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  };
  const svgEl = (name, attrs, parent) => {
    const e = document.createElementNS(NS, name);
    for (const k in attrs) e.setAttribute(k, attrs[k]);
    parent.append(e);
    return e;
  };
  const still = matchMedia("(prefers-reduced-motion: reduce)");
  const data = { status: null, lab: null, analytics: null, analyticsAt: 0, energy: null, serverAt: 0, localAt: 0 };
  const now = () => (data.serverAt ? data.serverAt + (performance.now() - data.localAt) : Date.now());

  async function read(path) {
    try {
      const r = await fetch(path, { cache: "no-store" });
      if (!r.ok) return null;
      const date = Date.parse(r.headers.get("Date") || "");
      if (Number.isFinite(date)) { data.serverAt = date; data.localAt = performance.now(); }
      return await r.json();
    } catch { return null; }
  }

  // ---- Bars: one rect per value, scaled to the tallest, drawn at the
  // element's real width so its text stays the page's size.
  function bars(svg, values, { height = 56, labels = null, cls = "bar" } = {}) {
    const W = Math.round(svg.getBoundingClientRect().width) || 600;
    const H = height + (labels ? 16 : 0);
    svg.setAttribute("viewBox", `0 0 ${W} ${H}`);
    svg.replaceChildren();
    const max = Math.max(1, ...values.map((v) => v || 0));
    const step = W / Math.max(1, values.length), w = Math.max(1, step - 2);
    svgEl("line", { class: "base", x1: 0, x2: W, y1: height + 0.5, y2: height + 0.5 }, svg);
    values.forEach((v, i) => {
      if (!v) return;
      const h = Math.max(1, v / max * (height - 4));
      svgEl("rect", { class: cls, x: i * step + 1, y: height - h, width: w, height: h }, svg);
    });
    if (labels) labels.forEach(([text, at, anchor]) =>
      (svgEl("text", { x: at * W, y: H - 3, "text-anchor": anchor }, svg).textContent = text));
  }

  // ---- The live strip.
  function renderLive() {
    const v = liveView(data.energy, now());
    $("live").hidden = $("meter").hidden = $("nav-power").hidden = !v.present;
    if (!v.present) return;
    $("live").classList.toggle("is-stale", v.stale);
    $("live-tag").textContent = v.stale ? "Stale" : "Live";
    $("meter-w").textContent = v.watts + " W";
    $("meter").setAttribute("aria-label", v.watts === "--" ? "No power reading. Opens the live figures." : `Drawing ${v.watts} watts. Opens the live figures.`);
    $("live-w").textContent = v.watts;
    $("live-24").textContent = v.kwh24;
    $("live-30").textContent = v.kwh30;
    $("live-tokens-box").hidden = $("live-tokens-chart").hidden = v.tokens == null;
    $("live").classList.toggle("no-tokens", v.tokens == null);
    if (v.tokens != null) $("live-tokens").textContent = v.tokens;
    const ends = [["30 min ago", 0, "start"], ["now", 1, "end"]];
    bars($("live-power"), v.power, { height: 44, labels: ends });
    if (v.tokens != null) bars($("live-token-bars"), v.tokenRates, { height: 44, labels: ends, cls: "bar is-alt" });
  }

  // ---- The line. A card flips to the next ticket of its stage; the wave
  // runs left to right, one card at a time, and holds while the pointer or
  // the focus is on the line.
  const cards = new Map(); // stage key -> { items, at }
  let held = false, flipping = false;
  function fillCard(box, st, card) {
    const t = card.items[card.at];
    box.querySelector(".card-label").textContent = card.at === 0 ? st.first : st.later;
    box.querySelector(".card-pos").textContent = card.items.length > 1 ? `${card.at + 1}/${card.items.length}` : "";
    const a = box.querySelector("a");
    a.querySelector("code").textContent = "#" + t.number;
    a.querySelector(".card-title").textContent = t.title;
    a.title = t.title;
    if (t.url) { a.href = t.url; a.target = "_blank"; a.rel = "noopener noreferrer"; } else a.removeAttribute("href");
  }
  function renderLine() {
    const v = lineView({ status: data.status, lab: data.lab, sample: data.status ? null : window.TEHAS_SAMPLE_ISSUES || null, nowMs: now(), pageUrl: location.href });
    const tag = $("source");
    tag.hidden = v.source === "live";
    tag.textContent = v.source === "sample" ? "Sample data" : v.source === "stale" ? "Queue stale" : "No queue";
    const state = $("state");
    const said = v.source === "sample" || v.source === "none" ? "No factory" : v.paused ? "Paused" : "Running";
    $("state-text").textContent = said;
    state.dataset.state = v.paused ? "paused" : v.source === "live" || v.source === "stale" ? "running" : "none";
    $("aside-state").textContent = { Running: "Operations running", Paused: "Operations paused", "No factory": "No factory here" }[said];
    $("aside-note").textContent = { live: "Live queue", stale: "Queue not written for ten minutes", sample: "Sample tickets", none: "No queue" }[v.source];
    for (const [id, href] of [["nav-repo", v.repo], ["nav-lab", v.lab]]) { $(id).hidden = !href; if (href) $(id).href = href; }

    const line = $("line");
    const signature = JSON.stringify(v.stages.map((s) => [s.key, s.items, s.more]));
    if (line.dataset.signature !== signature) {
      line.dataset.signature = signature;
      line.replaceChildren(...v.stages.map((st) => {
        const col = el("article", `stage s-${st.key}`);
        const head = el("header");
        head.append(el("h3", null, st.label), el("b", null, String(st.count).padStart(2, "0")));
        col.append(head);
        const prev = cards.get(st.key);
        const keep = prev && st.items.some((t) => t.number === prev.items[prev.at]?.number);
        const card = { items: st.items, at: keep ? Math.max(0, st.items.findIndex((t) => t.number === prev.items[prev.at].number)) : 0 };
        cards.set(st.key, card);
        if (st.items.length) {
          const box = el("div", "card");
          const top = el("small");
          top.append(el("span", "card-label"), el("span", "card-pos"));
          const a = el("a");
          a.append(el("code"), el("span", "card-title"));
          box.append(top, a);
          fillCard(box, st, card);
          col.append(box);
        } else col.append(el("p", "card-none", st.none));
        if (st.more) {
          const more = el("a", "more", st.more.text);
          more.href = st.more.href;
          more.target = "_blank";
          more.rel = "noopener noreferrer";
          col.append(more);
        }
        col.dataset.key = st.key;
        return col;
      }));
    }
    $("line-note").textContent = v.source === "sample" ? "Sample tickets: this host has no live queue."
      : v.source === "stale" ? "The queue has not been written for over ten minutes." : "";
    renderBoard(v);
  }
  async function flip(col) {
    const st = STAGES.find((s) => s.key === col.dataset.key), card = cards.get(st.key), box = col.querySelector(".card");
    card.at = (card.at + 1) % card.items.length;
    if (still.matches || !box.animate) return fillCard(box, st, card);
    await box.animate([{ transform: "rotateX(0deg)" }, { transform: "rotateX(-90deg)" }], { duration: 200, easing: "ease-in" }).finished;
    fillCard(box, st, card);
    await box.animate([{ transform: "rotateX(90deg)" }, { transform: "rotateX(0deg)" }], { duration: 240, easing: "ease-out" }).finished;
  }
  async function wave() {
    if (flipping || held || document.hidden) return;
    flipping = true;
    for (const col of $("line").children) {
      if ((cards.get(col.dataset.key)?.items.length || 0) < 2) continue;
      if (held) break;
      await flip(col);
      await new Promise((r) => setTimeout(r, 120));
    }
    flipping = false;
  }
  for (const [ev, h] of [["pointerenter", true], ["pointerleave", false], ["focusin", true], ["focusout", false]])
    $("line").addEventListener(ev, () => { held = h; });

  // ---- The last ten days.
  function renderPeriod() {
    const v = analyticsView(data.analytics);
    $("period").hidden = $("nav-period").hidden = !v.present;
    if (!v.present) return;
    $("period-window").textContent = v.window;
    const keys = ["shipped", "fpy", "cycle", "machine", "tokens", "availability"];
    $("tiles").replaceChildren(...v.figures.filter((f) => keys.includes(f.key)).map((f) => {
      const t = el("div", "tile");
      t.append(el("small", null, f.label), el("b", null, f.value), el("span", "tile-note", f.note));
      return t;
    }));
    // Stacked: shipped from the base, failed above it.
    const svg = $("days");
    svg.replaceChildren();
    const box = svg.getBoundingClientRect();
    const W = Math.round(box.width) || 600, H = Math.round(box.height) || 96, base = H - 18;
    svg.setAttribute("viewBox", `0 0 ${W} ${H}`);
    const max = Math.max(1, ...v.chart.map((d) => d.shipped + d.failed));
    const step = W / Math.max(1, v.chart.length), w = Math.max(2, step - 8);
    svgEl("line", { class: "base", x1: 0, x2: W, y1: base + 0.5, y2: base + 0.5 }, svg);
    v.chart.forEach((d, i) => {
      const x = i * step + 4, ys = d.shipped / max * (base - 6), yf = d.failed / max * (base - 6);
      const g = svgEl("g", {}, svg);
      svgEl("title", {}, g).textContent = d.title;
      if (d.shipped) svgEl("rect", { class: "bar", x, y: base - ys, width: w, height: ys }, g);
      if (d.failed) svgEl("rect", { class: "bar is-failed", x, y: base - ys - yf, width: w, height: yf }, g);
      svgEl("text", { x: x + w / 2, y: H - 3, "text-anchor": "middle" }, g).textContent = d.label;
    });
    const weakest = Math.min(...v.steps.map((s) => s.share));
    $("steps").replaceChildren(...v.steps.map((s) => {
      const li = el("li", s.share === weakest && s.share < 0.6 ? "is-weak" : null);
      const bar = el("span", "step-bar");
      const fill = el("i");
      fill.style.width = Math.round(s.share * 100) + "%";
      bar.append(fill);
      li.append(el("span", "step-name", s.step), bar, el("b", null, s.text.split(",")[0]));
      li.title = s.text;
      return li;
    }));
    $("steps-note").textContent = v.stepsNote;
    $("reasons").replaceChildren(...v.reasons.map((r) => {
      const li = el("li");
      li.append(el("span", null, r.reason), el("b", null, String(r.n)));
      return li;
    }));
    $("reasons-note").textContent = v.reasonsNote;
  }

  // ---- The issue board.
  let filter = "all";
  function renderBoard(v) {
    const all = boardRows(v);
    const rows = all.filter((r) => filter === "all" || r.stage === filter);
    $("board-count").textContent = `${all.length} on the line`;
    for (const b of document.querySelectorAll("#filters button")) {
      b.querySelector("span").textContent = all.filter((r) => b.dataset.filter === "all" || r.stage === b.dataset.filter).length;
    }
    $("board-rows").replaceChildren(...rows.map((r) => {
      const tr = el("tr");
      const num = el("td", "n");
      const title = el("td");
      if (r.url) {
        const a = el("a", null, r.title);
        a.href = r.url; a.target = "_blank"; a.rel = "noopener noreferrer";
        title.append(a);
      } else title.textContent = r.title;
      num.textContent = "#" + r.number;
      const stage = el("td");
      stage.append(el("span", `st s-${r.stage}`, r.stageLabel));
      tr.append(num, title, stage);
      return tr;
    }));
    $("board-empty").hidden = rows.length > 0;
  }
  for (const b of document.querySelectorAll("#filters button")) {
    b.addEventListener("click", () => {
      filter = b.dataset.filter;
      for (const o of document.querySelectorAll("#filters button")) o.setAttribute("aria-pressed", String(o === b));
      renderLine();
    });
  }

  // ---- The menu marks the section the address names.
  const navLinks = [...document.querySelectorAll(".topbar nav a[href^='#']")];
  const markNav = () => {
    const at = navLinks.some((a) => a.getAttribute("href") === location.hash) ? location.hash : "#top";
    for (const a of navLinks) {
      if (a.getAttribute("href") === at) a.setAttribute("aria-current", "location");
      else a.removeAttribute("aria-current");
    }
  };
  addEventListener("hashchange", markNav);
  markNav();

  // ---- Polling, as the factory floor does: the queue every ten seconds, the
  // power every period, the Protolab and the report once a minute.
  const lift = () => document.querySelector(".topbar").classList.toggle("is-scrolled", scrollY > 4);
  addEventListener("scroll", lift, { passive: true });
  lift();
  async function queue() { if (!document.hidden) { data.status = await read("/live/status.json"); renderLine(); } }
  async function power() { if (!document.hidden) { data.energy = await read("/live/energy.json"); renderLive(); } }
  async function lab() { if (!document.hidden) { data.lab = await read("/live/protolab.json"); renderLine(); } }
  async function report() {
    if (!dueAnalytics({ hidden: document.hidden, readAt: data.analyticsAt })) return;
    data.analyticsAt = Date.now();
    data.analytics = await read("/live/analytics.json");
    renderPeriod();
  }
  queue(); power(); lab(); report();
  setInterval(queue, 10000);
  setInterval(power, ENERGY_PERIOD * 1000);
  setInterval(lab, 60000);
  setInterval(report, 15000);
  setInterval(wave, FLIP_EVERY * 1000);
  addEventListener("resize", () => { renderLive(); renderPeriod(); });
  document.addEventListener("visibilitychange", () => { if (!document.hidden) { queue(); power(); } });
}
