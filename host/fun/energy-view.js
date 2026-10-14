// The Energy use section of the game view: what the model mesh draws and
// which models it serves, read from /live/energy.json (written by
// libexec/energy). See DESIGN.md, "Energy Use".
//
// layout() turns the file into what is drawn and has no side effects, so it is
// tested without a browser (test/energy_view_test.mjs). The rest of the module
// fetches and draws, and runs only where there is a document; the test runs it
// there too, against a page of stubs.

export const SLOTS = 120;
export const PERIOD = 15;
export const COLUMN = 4;
export const WIDTH = SLOTS * COLUMN;
export const HEIGHT = 136;
export const STALE_AFTER = 45;
// Band colours, all already on the page (the Thirty-Two Rule). From the ninth
// band on the list starts again as a checker with CRT Black.
export const BANDS = ["#ea3", "#4a9", "#c53", "#7cb", "#ccd", "#b97", "#694", "#fc6"];

const list = (v) => (Array.isArray(v) ? v : []);
const amount = (v) => (typeof v === "number" && Number.isFinite(v) && v >= 0 ? v : null);
const name = (v) => (typeof v === "string" ? v.slice(0, 64) : "");
// What serves a model: the mesh, its version and the engine, for each set the
// sampler found. The engine goes by the name the mesh gives it for people, or
// by the mesh's id where it gives none, and then its own version. No table of
// display names here: it would go stale when the mesh gains an engine.
function engines(v, mesh) {
  const each = list(v).filter((e) => e && typeof e === "object").map((e) => {
    const version = name(e.version), engine = name(e.engineName) || name(e.engine);
    if (!version && !engine) return "";
    const served = engine && `(${[engine, name(e.engineVersion)].filter(Boolean).join(" ")})`;
    return [mesh, version, served].filter(Boolean).join(" ");
  });
  return each.filter(Boolean).join(", ") || "-";
}
const kwh = (v) => (amount(v) == null ? null : amount(v).toFixed(1));

// The live meter reads tokens as a rate over the last minute.
export const RATE_WINDOW = 60;
const counts = (s) => (s.k && typeof s.k === "object" ? s.k : null);
// Tokens counted between two samples. A counter that went down has started
// again from zero; a machine not seen before adds nothing yet.
function tokensBetween(a, b) {
  let sum = 0;
  for (const host of Object.keys(b.k)) {
    const now = amount(b.k[host]), was = Object.hasOwn(a.k, host) ? amount(a.k[host]) : null;
    if (now == null || was == null) continue;
    sum += now >= was ? now - was : now;
  }
  return sum;
}
// Tokens a minute at sample i, over the samples of the minute before it.
function tokenRate(samples, i) {
  let sum = 0, from = i;
  for (let j = i; j > 0 && samples[j].t > samples[i].t - RATE_WINDOW; j--) {
    if (!counts(samples[j]) || !counts(samples[j - 1])) break;
    sum += tokensBetween(samples[j - 1], samples[j]);
    from = j - 1;
  }
  return from === i ? null : sum / (samples[i].t - samples[from].t) * 60;
}
const blocks = (share) => Math.max(0, Math.min(10, Math.ceil(share * 10)));
// From ten thousand on, in thousands.
export const rateText = (rate) => (rate >= 10000 ? `${Math.round(rate / 1000)}K` : String(Math.round(rate)));

// A meter reading that changed counts to its new value in this many frames.
export const COUNT_FRAMES = 8;
export const COUNT_FRAME_MS = 60;
// The frames of a count from one value to another. The last is the new value.
export function countFrames(from, to, frames = COUNT_FRAMES) {
  return Array.from({ length: frames }, (_, i) => Math.round(from + (to - from) * (i + 1) / frames));
}

// data: the parsed energy file. nowMs: the present, by the server's clock.
export function layout(data, nowMs) {
  const file = data && typeof data === "object" ? data : {};
  const hosts = list(file.hosts).filter((h) => h && typeof h === "object" && name(h.name));
  const samples = list(file.samples).filter((s) => s && typeof s === "object" && amount(s.t) != null && s.w && typeof s.w === "object");
  const totals = file.totals && typeof file.totals === "object" ? file.totals : {};
  const mesh = name(file.source);
  const generated = Date.parse(file.generatedAt);
  const stale = !(nowMs - generated <= STALE_AFTER * 1000);
  const endSlot = Math.floor((Number.isFinite(generated) ? generated / 1000 : 0) / PERIOD);
  const column = (t) => SLOTS - 1 - (endSlot - Math.floor(t / PERIOD));
  const shown = samples.filter((s) => column(s.t) >= 0 && column(s.t) < SLOTS).sort((a, b) => a.t - b.t);

  // Bands follow the rows. A machine that has left the mesh but is still in
  // the window keeps its band, above the others.
  // The full name is the key into each sample; only the displayed name is cut.
  const bands = hosts.map((h) => h.name);
  const gone = new Set();
  for (const s of shown) for (const key of Object.keys(s.w)) if (!bands.includes(key) && amount(s.w[key]) != null) gone.add(key);
  bands.push(...[...gone].sort());

  const stacks = Array(SLOTS).fill(null);
  let peak = 0;
  for (const s of shown) {
    let sum = 0;
    const parts = [];
    bands.forEach((host, band) => {
      const w = Object.hasOwn(s.w, host) ? amount(s.w[host]) : null;
      if (w == null) return;
      parts.push({ band, from: sum, to: sum + w });
      sum += w;
    });
    stacks[column(s.t)] = parts;
    peak = Math.max(peak, sum);
  }
  const scaleTop = Math.max(500, Math.ceil(peak / 500) * 500);
  const px = (w) => Math.round(w / scaleTop * HEIGHT);
  const columns = stacks.map((parts) => parts && parts.map((p) => ({ band: p.band, from: px(p.from), to: px(p.to) })));

  const reporting = amount(totals.reporting) ?? 0;
  const energyHosts = amount(totals.energyHosts) ?? 0;
  const watts = stale || amount(totals.watts) == null ? "--" : String(Math.round(totals.watts));

  // The meter's token line exists only where the mesh counts tokens. Its gauge
  // is full at the busiest minute of the window.
  const rates = shown.map((_, i) => tokenRate(shown, i));
  const last = shown.length - 1;
  const rate = !stale && last >= 0 && column(shown[last].t) === SLOTS - 1 ? rates[last] : null;
  const busiest = Math.max(0, ...rates.filter((r) => r != null));
  // wattsNow and tokensNow are the readings as numbers, for the count; null
  // where there is no reading.
  const meter = {
    watts,
    wattsNow: watts === "--" ? null : Number(watts),
    wattsFilled: watts === "--" ? 0 : blocks(totals.watts / scaleTop),
    tokens: !shown.some(counts) ? null : rate == null ? "--" : rateText(rate),
    tokensNow: rate == null ? null : Math.round(rate),
    tokensFilled: rate == null || busiest === 0 ? 0 : blocks(rate / busiest),
  };
  return {
    meter,
    state: stale ? "stale" : "live",
    // Where the numbers come from. The name is a link only to an https address.
    source: mesh ? { name: mesh, url: typeof file.sourceUrl === "string" && file.sourceUrl.startsWith("https://") ? file.sourceUrl : null } : null,
    scope: `${reporting}/${hosts.length} HOSTS` + (energyHosts < reporting ? `, KWH FROM ${energyHosts}` : ""),
    watts,
    kwh24: kwh(totals.kWh24h) ?? "--",
    kwh30: kwh(totals.kWh30d) ?? "--",
    scaleTop,
    from: (endSlot - SLOTS + 1) * PERIOD,
    to: endSlot * PERIOD,
    columns,
    label: `Power of the machines serving the factory over 30 minutes, one bar per 15 seconds, ${reporting} of ${hosts.length} hosts reporting. ` +
      (watts === "--" ? "No current reading." : `Now ${watts} watts.`),
    rows: hosts.map((h, band) => {
      const total = amount(h.ramTotalMb), used = amount(h.ramUsedMb);
      const share = total && used != null ? Math.min(1, used / total) : null;
      return {
        name: name(h.name),
        band: amount(h.watts) == null ? null : band,
        source: name(h.source).toUpperCase() || "-",
        now: stale || amount(h.watts) == null ? "-" : `${Math.round(h.watts)} W`,
        day: kwh(h.kWh24h) == null ? "-" : `${kwh(h.kWh24h)} KWH`,
        month: kwh(h.kWh30d) == null ? "-" : `${kwh(h.kWh30d)} KWH`,
        ram: share == null ? null : { filled: Math.ceil(share * 10), percent: Math.round(share * 100) },
      };
    }),
    models: list(file.models).filter((m) => m && typeof m === "object" && name(m.name)).map((m) => ({
      name: name(m.name),
      hosts: list(m.hosts).map(name).filter(Boolean).join(", ") || "-",
      engines: engines(m.engines, mesh),
      busy: stale || amount(m.busy) == null || amount(m.slots) == null ? "-" : `${m.busy}/${m.slots}`,
      queued: stale || amount(m.queued) == null ? "-" : String(m.queued),
    })),
  };
}

// One block of a band: a flat fill, or a one-pixel checker from the ninth on.
export function fillBand(ctx, band, x, y, w, h) {
  ctx.fillStyle = BANDS[band % BANDS.length];
  ctx.fillRect(x, y, w, h);
  if (band < BANDS.length) return;
  ctx.fillStyle = "#111";
  for (let dy = 0; dy < h; dy++) for (let dx = (x + y + dy) % 2; dx < w; dx += 2) ctx.fillRect(x + dx, y + dy, 1, 1);
}
if (typeof document !== "undefined") {
  const $ = (id) => document.getElementById(id);
  const clock = (t) => new Date(t * 1000).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
  let data = null, serverAt = 0, localAt = 0, drawn = "";

  // The elements the view shows or hides, looked up once. A page that does not
  // have one of them (an old index.html beside a new script, a partial install,
  // a later rename) has nothing to draw: render() leaves it alone rather than
  // assigning to what is not there at every refresh.
  const section = $("energy"), menuLink = $("menu-energy"), floorMeter = $("floor-meter");
  const whole = Boolean(section && menuLink && floorMeter);

  // Names come from the mesh and are untrusted: text only, never markup.
  function cell(row, text, cls) {
    const td = row.insertCell();
    td.textContent = text;
    if (cls) td.className = cls;
    return td;
  }
  function named(row, text) {
    const th = document.createElement("th");
    th.scope = "row";
    th.textContent = text;
    row.append(th);
  }

  function draw(view) {
    const ctx = $("energy-canvas").getContext("2d");
    ctx.fillStyle = "#111";
    ctx.fillRect(0, 0, WIDTH, HEIGHT);
    view.columns.forEach((parts, i) => {
      for (const p of parts || []) fillBand(ctx, p.band, i * COLUMN, HEIGHT - p.to, COLUMN, p.to - p.from);
    });
    // The top and the half of the scale: a dotted rule, one pixel on, three off.
    ctx.fillStyle = "#aab";
    for (const y of [0, HEIGHT / 2]) for (let x = 0; x < WIDTH; x += 4) ctx.fillRect(x, y, 1, 1);
  }

  function gauge(id, filled) {
    const blocks = [];
    for (let i = 0; i < 10; i++) {
      const block = document.createElement("i");
      if (i < filled) block.className = "is-used";
      blocks.push(block);
    }
    $(id).replaceChildren(...blocks);
  }

  // One line of the meter: its figure and its gauge. A reading that changed
  // counts up or down to the new one in stepped frames, the gauge with it; the
  // first reading counts from zero. No reading, or reduced motion: no count.
  const calm = window.matchMedia("(prefers-reduced-motion: reduce)");
  const dials = {};
  function dial(id, text, now, filled, show) {
    const d = dials[id] ??= { now: null, filled: 0, timer: 0 };
    const set = (t, f) => { $(id).textContent = t; gauge(`${id}-gauge`, f); };
    clearInterval(d.timer);
    if (now == null || calm.matches || (now === d.now && filled === d.filled)) {
      d.now = now;
      d.filled = filled;
      return set(text, filled);
    }
    const values = countFrames(d.now ?? 0, now), fills = countFrames(d.filled, filled);
    let i = 0;
    const step = () => {
      d.now = values[i];
      d.filled = fills[i];
      const last = ++i === values.length;
      set(last ? text : show(d.now), d.filled);
      if (last) clearInterval(d.timer);
    };
    step();
    d.timer = setInterval(step, COUNT_FRAME_MS);
  }

  // The meter on the head panel: the draw now and the tokens of the last minute.
  function drawMeter(meter) {
    dial("meter-watts", meter.watts, meter.wattsNow, meter.wattsFilled, String);
    $("meter-tokens-row").hidden = $("meter-tokens-gauge").hidden = meter.tokens == null;
    dial("meter-tokens", meter.tokens ?? "--", meter.tokensNow, meter.tokensFilled, rateText);
    floorMeter.setAttribute("aria-label",
      `Live meter: ${meter.watts === "--" ? "no power reading" : `${meter.watts} watts`}` +
      (meter.tokens == null ? "" : meter.tokens === "--" ? ", no token reading" : `, ${meter.tokens} tokens a minute`) +
      ". Opens Energy use.");
  }

  function render() {
    if (!whole) return;
    section.hidden = menuLink.hidden = floorMeter.hidden = !data;
    if (!data) return;
    const view = layout(data, serverAt + (performance.now() - localAt));
    // Nothing changed since the last draw: leave the tables and the status tag
    // alone, so a selection stays and a screen reader is not told again.
    const json = JSON.stringify(view);
    if (json === drawn) return;
    drawn = json;
    drawMeter(view.meter);
    $("energy-scope").textContent = view.scope;
    const tag = view.state === "live" ? "LIVE" : "STALE";
    if ($("energy-state").textContent !== tag) $("energy-state").textContent = tag;
    $("energy-state").dataset.state = view.state;
    $("energy-watts").textContent = view.watts;
    $("energy-kwh24").textContent = view.kwh24;
    $("energy-kwh30").textContent = view.kwh30;
    $("energy-top").textContent = `${view.scaleTop} W`;
    $("energy-half").textContent = String(view.scaleTop / 2);
    $("energy-from").textContent = clock(view.from);
    $("energy-to").textContent = clock(view.to);
    $("energy-canvas").setAttribute("aria-label", view.label);
    draw(view);

    const rows = $("energy-rows");
    rows.replaceChildren();
    for (const r of view.rows) {
      const tr = rows.insertRow();
      const swatch = document.createElement("canvas");
      swatch.width = swatch.height = 4;
      swatch.className = "energy-swatch";
      const sctx = swatch.getContext("2d");
      if (r.band == null) {
        sctx.fillStyle = "#eec";
        sctx.fillRect(0, 0, 4, 4);
      } else fillBand(sctx, r.band, 0, 0, 4, 4);
      tr.insertCell().append(swatch);
      named(tr, r.name);
      cell(tr, r.source, "col-source");
      cell(tr, r.now, "num");
      cell(tr, r.day, "num");
      cell(tr, r.month, "num col-month");
      const ram = cell(tr, "", "col-ram");
      if (r.ram) {
        const gauge = document.createElement("span");
        gauge.className = "energy-gauge";
        gauge.setAttribute("aria-hidden", "true");
        for (let i = 0; i < 10; i++) {
          const block = document.createElement("i");
          if (i < r.ram.filled) block.className = "is-used";
          gauge.append(block);
        }
        ram.append(gauge, ` ${r.ram.percent}%`);
      } else ram.textContent = "-";
    }

    $("energy-models").hidden = view.models.length === 0;
    const models = $("energy-model-rows");
    models.replaceChildren();
    for (const m of view.models) {
      const tr = models.insertRow();
      tr.insertCell();
      named(tr, m.name);
      cell(tr, m.hosts, "col-hosts");
      cell(tr, m.engines, "col-hosts");
      cell(tr, m.busy, "num");
      cell(tr, m.queued, "num");
    }

    // The name and the address come from the file: text and an attribute,
    // never markup.
    const source = $("energy-source");
    source.hidden = !view.source;
    source.replaceChildren();
    if (view.source) {
      let named = view.source.name;
      if (view.source.url) {
        named = document.createElement("a");
        named.href = view.source.url;
        named.target = "_blank";
        named.rel = "noopener noreferrer";
        named.textContent = view.source.name;
      }
      source.append("DATA SOURCE: ", named);
    }
  }

  async function refresh() {
    if (document.hidden) return;
    try {
      const r = await fetch("/live/energy.json", { cache: "no-store" });
      if (r.status === 404) data = null;
      else if (r.ok) {
        const next = await r.json();
        if (next && typeof next === "object" && Array.isArray(next.hosts)) {
          // Staleness is judged by the server's clock, not the visitor's.
          const date = Date.parse(r.headers.get("Date") || "");
          data = next;
          serverAt = Number.isFinite(date) ? date : Date.now();
          localAt = performance.now();
        }
      }
    } catch { /* unreachable: what is on screen stays and grows stale */ }
    render();
  }

  refresh();
  setInterval(refresh, PERIOD * 1000);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) refresh(); });
}
