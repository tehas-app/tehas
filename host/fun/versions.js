// The versions in the footers of the two front pages, and the industry's
// alerts at their top, read from the factory's health (/live/health.json,
// written by tehasbot health): tehas from checks.platform, the pi-rukas the
// sandbox image runs from checks.rukas, and the tehas-industry that runs the
// factory from checks.industry, when there is one, with the alerts it raised
// about this factory. So the footer, the banner, /health and the MCP server
// tell one story, however the image was built. The banner also lists the
// messages of tehas host-message, from checks.hostMessages.
//
// line(), alerts() and messages() are the pure steps from the file to the
// text, and draw() sets the banner through the document it is given; all are
// tested without a browser (test/versions_test.mjs). The file comes from the
// network: a value of another shape is left out, and every word is set as text.
(() => {
  const VERSION = /^v[0-9]{1,6}(\.[0-9]{1,6}){1,3}(-[0-9A-Za-z.]{1,32})?$/;
  const COMMIT = /^[0-9a-f]{7,40}$/;
  const WEB = /^https?:\/\/[^\s]+$/;
  const KINDS = { paused: "Paused", frozen: "Frozen", "held-by-one": "Held by one ticket" };
  const LEVELS = { alert: "Alert", warn: "Warning", info: "Notice" };
  const version = (v) => (typeof v === "string" && VERSION.test(v) ? v : null);
  const commit = (c) => (typeof c === "string" && COMMIT.test(c) ? c.slice(0, 7) : null);
  const object = (o) => (o !== null && typeof o === "object" ? o : {});
  const text = (s) => (typeof s === "string" ? s.slice(0, 200) : "");

  // "tehas v0.24.6 · pi-rukas v0.14.54 · industry v0.9.0", or null when the
  // file names none of them.
  function line(health) {
    const checks = object(object(health).checks);
    const platform = object(checks.platform);
    const rukas = object(checks.rukas);
    const parts = [
      ["tehas", version(platform.release) ?? commit(platform.commit)],
      ["pi-rukas", version(rukas.imageVersion) ?? commit(rukas.image)],
      ["industry", version(object(checks.industry).version)],
    ];
    const said = parts.filter(([, v]) => v).map(([name, v]) => `${name} ${v}`);
    return said.length ? said.join(" · ") : null;
  }

  // "4 h ago": how long ago <since> was at <now> (ms), or "" for no time.
  function ago(since, now) {
    const t = typeof since === "string" ? Date.parse(since) : NaN;
    if (Number.isNaN(t)) return "";
    const s = Math.max(0, Math.floor((now - t) / 1000));
    if (s < 120) return "just now";
    if (s < 7200) return `${Math.floor(s / 60)} min ago`;
    if (s < 172800) return `${Math.floor(s / 3600)} h ago`;
    return `${Math.floor(s / 86400)} days ago`;
  }

  // The alerts that hold now, each as { word, text, ago, link }: none for a
  // factory with no industry, a file the industry has not written, or one
  // that is stale or unreadable. A kind this page does not know is named by
  // its own word.
  function alerts(health, now = Date.now()) {
    const industry = object(object(object(health).checks).industry);
    if (industry.alertsStale !== false || !Array.isArray(industry.alerts)) return [];
    return industry.alerts
      .map(object)
      .filter((a) => typeof a.kind === "string" && a.kind)
      .map((a) => ({
        word: Object.hasOwn(KINDS, a.kind) ? KINDS[a.kind] : text(a.kind),
        text: text(a.text),
        ago: ago(a.since, now),
        link: typeof a.link === "string" && WEB.test(a.link) ? a.link : null,
      }));
  }

  // The host messages, each as { word, text, ago, link, set: true }: none for
  // a file with no list or one that cannot be read. A message of a level this
  // page does not know is left out.
  function messages(health, now = Date.now()) {
    const list = object(object(object(health).checks).hostMessages).list;
    if (!Array.isArray(list)) return [];
    return list
      .map(object)
      .filter((m) => typeof m.level === "string" && Object.hasOwn(LEVELS, m.level))
      .map((m) => ({
        word: LEVELS[m.level],
        text: text(m.text),
        ago: ago(m.at, now),
        link: typeof m.link === "string" && WEB.test(m.link) ? m.link : null,
        set: true,
      }));
  }

  // The banner: one line for each alert and message, hidden when there is none.
  function draw(el, list, doc) {
    const node = (name, cls, words) => {
      const n = doc.createElement(name);
      if (cls) n.className = cls;
      if (words) n.textContent = words;
      return n;
    };
    el.replaceChildren(
      ...list.map((a) => {
        const p = node("p", "industry-alert");
        p.append(node("b", "", a.word), node("span", "", a.text));
        if (a.ago) p.append(node("small", "", `${a.set ? "set" : "started"} ${a.ago}`));
        if (a.link) {
          const l = node("a", "", "Open");
          l.href = a.link;
          l.setAttribute("target", "_blank");
          l.setAttribute("rel", "noopener noreferrer");
          p.append(l);
        }
        return p;
      }),
    );
    el.hidden = list.length === 0;
  }

  globalThis.TEHAS_VERSIONS = { line, alerts, messages, draw };

  if (typeof document === "undefined") return;
  const el = document.getElementById("versions");
  const banner = document.getElementById("industry-alerts");
  if (!el && !banner) return;

  async function refresh() {
    if (document.hidden) return;
    let health = null;
    try {
      const r = await fetch("/live/health.json", { cache: "no-store" });
      if (r.ok) health = await r.json();
    } catch {
      health = null;
    }
    // No health yet keeps what the page showed last, or nothing.
    if (health === null) return;
    if (banner) draw(banner, [...alerts(health), ...messages(health)], document);
    const said = line(health);
    if (!el || said === null) return;
    el.textContent = said;
    el.hidden = false;
  }

  refresh();
  setInterval(refresh, 60000);
  document.addEventListener("visibilitychange", refresh);
})();
