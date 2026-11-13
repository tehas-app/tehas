// The host bridge: what a host that fronts the factory's domain says to the
// dashboard, read from /host/bridge.json on the page's own origin. tehas puts
// nothing under /host/ (host/edge/docs.Caddyfile answers 404 there), so a
// factory without a host gets no answer and every page stays as it is. The
// contract is in host/docs/api.md, "The host bridge".
//
// read() is the pure step from the text of the answer to what the pages use,
// and keeper() holds the last good answer; both are tested without a browser
// (test/host_bridge_test.mjs). The answer is data from outside: an element
// that breaks a rule is dropped, a link is used only when it passes its rule,
// and every word is set with textContent.
//
// The front pages and /dashboard load this file before the views, and it
// unregisters any service worker on the origin: tehas registers none, and one
// left there would answer for pages it does not own.
(() => {
  const PATH = "/host/bridge.json";
  const MAX_BYTES = 256 * 1024;
  const MAX_AGE = 5 * 60 * 1000;
  const EVERY = 60 * 1000;
  const TIMEOUT = 5 * 1000;
  const RFC3339 = /^\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(\.\d+)?([Zz]|[+-]\d{2}:\d{2})$/;
  const ID = /^[a-z0-9-]{1,32}$/;
  const LEVELS = ["info", "warn", "alert"];
  const WORDS = { info: "Notice", warn: "Warning", alert: "Alert" };
  // A control character, or a line or paragraph separator, as panels.js folds them.
  const CONTROL = /[\u0000-\u001f\u007f-\u009f\u2028\u2029]/g;
  const SPACE_OR_CONTROL = /[\s\u0000-\u001f\u007f-\u009f]/;
  const REPO = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;

  const object = (o) => (o !== null && typeof o === "object" && !Array.isArray(o) ? o : null);
  const length = (s) => [...s].length;

  // One line of 1 to <max> characters, control characters as spaces, or null.
  function line(s, max) {
    if (typeof s !== "string") return null;
    const t = s.replace(CONTROL, " ").trim();
    const n = length(t);
    return n >= 1 && n <= max ? t : null;
  }

  // A path under /host/, or an https:// address, at most 512 characters.
  function link(s) {
    if (typeof s !== "string" || s.length > 512 || SPACE_OR_CONTROL.test(s)) return null;
    if (s.startsWith("/host/")) return s;
    if (!s.startsWith("https://")) return null;
    try {
      const u = new URL(s);
      return u.protocol === "https:" && u.hostname && !u.username && !u.password ? s : null;
    } catch {
      return null;
    }
  }

  // A page for the dock: an http: or https: address with no user or
  // password, at most 512 characters. panels.js drops one on the page's own
  // origin, as it does a panel's.
  function page(s) {
    if (typeof s !== "string" || s.length > 512 || SPACE_OR_CONTROL.test(s)) return null;
    try {
      const u = new URL(s);
      return (u.protocol === "http:" || u.protocol === "https:") && u.hostname && !u.username && !u.password ? u.href : null;
    } catch {
      return null;
    }
  }

  function ticketLink(s) {
    if (typeof s !== "string" || s.length > 200 || !s.startsWith("/host/") || SPACE_OR_CONTROL.test(s)) return null;
    return s.split("{n}").length === 2 ? s : null;
  }

  // The answer as the pages use it, or null when it is ignored whole:
  // { ticketLink, notices: [{ id, level, text, link }], notes: Map(issue -> [{ text, link }]),
  //   menu: [{ id, label, url }] }. A `tokens` field is ignored.
  // `now` is the time to judge the answer's age by, in ms.
  function read(text, now) {
    if (typeof text !== "string" || new TextEncoder().encode(text).length > MAX_BYTES) return null;
    let v;
    try {
      v = JSON.parse(text);
    } catch {
      return null;
    }
    v = object(v);
    if (!v || v.bridge !== 1) return null;
    if (typeof v.time !== "string" || !RFC3339.test(v.time)) return null;
    const t = Date.parse(v.time);
    // A time far ahead would keep an answer young for ever.
    if (Number.isNaN(t) || Math.abs(now - t) > MAX_AGE) return null;

    const notices = [];
    const ids = new Set();
    for (const n of Array.isArray(v.notices) ? v.notices : []) {
      if (notices.length === 5) break;
      const e = object(n);
      if (!e || typeof e.id !== "string" || !ID.test(e.id) || ids.has(e.id)) continue;
      if (!LEVELS.includes(e.level)) continue;
      const words = line(e.text, 200);
      if (words === null) continue;
      let to = null;
      if (e.link !== undefined && (to = link(e.link)) === null) continue;
      ids.add(e.id);
      notices.push({ id: e.id, level: e.level, text: words, link: to });
    }

    const notes = new Map();
    let kept = 0;
    for (const n of Array.isArray(v.ticketNotes) ? v.ticketNotes : []) {
      if (kept === 200) break;
      const e = object(n);
      if (!e || !Number.isInteger(e.issue) || e.issue < 1 || e.issue > 999999999) continue;
      const words = line(e.text, 60);
      if (words === null) continue;
      let to = null;
      if (e.link !== undefined && (to = link(e.link)) === null) continue;
      const these = notes.get(e.issue) || [];
      if (these.length === 3) continue;
      these.push({ text: words, link: to });
      notes.set(e.issue, these);
      kept++;
    }

    const menu = [];
    const menuIds = new Set();
    for (const m of Array.isArray(v.menu) ? v.menu : []) {
      if (menu.length === 4) break;
      const e = object(m);
      if (!e || typeof e.id !== "string" || !ID.test(e.id) || menuIds.has(e.id)) continue;
      const label = line(e.label, 24);
      const url = page(e.url);
      if (label === null || url === null) continue;
      menuIds.add(e.id);
      menu.push({ id: e.id, label, url });
    }

    return { ticketLink: ticketLink(v.ticketLink), notices, notes, menu };
  }

  // Where a ticket links on the dashboard: the host's page for a ticket of the
  // factory's own repository, or null to keep the link the view has. Only an
  // issue address of that repository matches, so a pull request keeps GitHub.
  function ticketHref(answer, repo, url) {
    if (!answer || !answer.ticketLink || typeof repo !== "string" || !REPO.test(repo) || typeof url !== "string") return null;
    const m = /^https:\/\/github\.com\/([A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+)\/issues\/([1-9][0-9]{0,8})$/.exec(url);
    if (!m || m[1].toLowerCase() !== repo.toLowerCase()) return null;
    return answer.ticketLink.replace("{n}", m[2]);
  }

  function notesOf(answer, issue) {
    return (answer && answer.notes.get(issue)) || [];
  }

  // A host page opens in the same tab; another site in a new one.
  function aim(a, href) {
    a.href = href;
    if (href.startsWith("/host/")) return;
    a.setAttribute("target", "_blank");
    a.setAttribute("rel", "noopener noreferrer");
  }

  // The notes of one ticket as chips in one span, a link where the note has
  // one; null when there is none, so the view adds nothing.
  function chips(doc, list) {
    if (!list.length) return null;
    const box = doc.createElement("span");
    box.className = "host-notes";
    box.append(...list.map((n) => {
      const c = doc.createElement(n.link ? "a" : "span");
      c.className = "host-note";
      c.textContent = n.text;
      if (n.link) aim(c, n.link);
      return c;
    }));
    return box;
  }

  // The notices, beside the alerts banner and drawn as its lines are.
  function drawNotices(el, list, doc) {
    const node = (name, cls, words) => {
      const n = doc.createElement(name);
      if (cls) n.className = cls;
      if (words) n.textContent = words;
      return n;
    };
    el.replaceChildren(
      ...list.map((n) => {
        const p = node("p", "industry-alert host-notice");
        p.setAttribute("data-level", n.level);
        p.append(node("b", "", WORDS[n.level]), node("span", "", n.text));
        if (n.link) {
          const a = node("a", "", "Open");
          aim(a, n.link);
          p.append(a);
        }
        return p;
      }),
    );
    el.hidden = list.length === 0;
  }

  // Asks for the answer and keeps the last good one. A 404 is no host and
  // drops it at once; a failed ask, a slow one or an answer that is ignored
  // keeps it for up to five minutes after it came. `changed` is called with
  // the answer, or null, whenever that changes.
  function keeper({ fetch, now, changed }) {
    let good = null;
    let at = 0;
    const set = (a) => {
      if (a === good) return;
      good = a;
      changed(a);
    };
    async function ask() {
      const stop = new AbortController();
      const timer = setTimeout(() => stop.abort(), TIMEOUT);
      let answer = null;
      let none = false;
      try {
        const r = await fetch(PATH, { cache: "no-store", signal: stop.signal });
        if (r.status === 404) none = true;
        else if (r.ok && !(Number(r.headers.get("Content-Length")) > MAX_BYTES)) {
          const served = Date.parse(r.headers.get("Date") || "");
          answer = read(await r.text(), Number.isNaN(served) ? now() : served);
        }
      } catch {
        answer = null;
      } finally {
        clearTimeout(timer);
      }
      if (answer) {
        at = now();
        set(answer);
      } else if (none || (good && now() - at > MAX_AGE)) {
        set(null);
      }
    }
    return { ask, current: () => good };
  }

  // What the views call. With no answer every one of these leaves the page as
  // it is: no link changes and no chip is drawn.
  const listeners = [];
  let answer = null;
  const api = {
    read, ticketHref, notesOf, chips, drawNotices, keeper,
    current: () => answer,
    ticket: (repo, url) => ticketHref(answer, repo, url),
    notes: (doc, issue) => chips(doc, notesOf(answer, issue)),
    onChange: (fn) => listeners.push(fn),
  };
  globalThis.TEHAS_HOST_BRIDGE = api;

  if (typeof document === "undefined") return;

  try {
    const sw = navigator.serviceWorker;
    if (sw && sw.getRegistrations) {
      sw.getRegistrations().then((all) => all.forEach((r) => r.unregister()), () => {});
    }
  } catch { /* no service workers here */ }

  const notices = document.getElementById("host-notices");
  const k = keeper({
    fetch: (...a) => fetch(...a),
    now: Date.now,
    changed: (a) => {
      answer = a;
      if (notices) drawNotices(notices, a ? a.notices : [], document);
      for (const fn of listeners) {
        try { fn(a); } catch { /* one view's failure stops no other */ }
      }
    },
  });
  const tick = () => { if (!document.hidden) k.ask(); };
  tick();
  setInterval(tick, EVERY);
  document.addEventListener("visibilitychange", tick);
})();
