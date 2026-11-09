// The pages of the project the dashboard links, read once from
// /live/panels.json (written by tehas panel, see console/src/panel.rs), and
// the host bridge's menu (host/fun/host-bridge.js), as it comes and goes. Both
// front pages load it: each is an item in the page's menu, and pressing it
// opens the page in a dock at the side.
//
// view() is the pure step from the file to the items, and merge() adds the
// bridge's, both tested without a browser (test/panels_view_test.mjs). The file is read from the network and its text
// is untrusted: a label is set as text, and a URL is parsed again here, so
// only an http: or https: page on another origin is kept.
//
// The frame is made on the first press, never at load, so the dashboard asks a
// panel's host for nothing until a person does. It may run scripts on its own
// origin and open new tabs, never navigate this page, and is passed nothing
// but its URL.
(() => {
  const ID = /^[a-z0-9-]{1,32}$/;
  const MAX = 4;
  const SANDBOX = "allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox";
  // As tehas panel folds them: a control character, a line or paragraph separator.
  const CONTROL = /[\u0000-\u001f\u007f-\u009f\u2028\u2029]/g;

  // [{ id, label, url }], at most four, or [] when the file is not a list.
  // `origin` is the page's own, which no panel may be on.
  function view(file, origin) {
    if (file === null || typeof file !== "object" || !Array.isArray(file.panels)) return [];
    const seen = new Set();
    const items = [];
    for (const p of file.panels) {
      if (items.length === MAX) break;
      if (p === null || typeof p !== "object" || typeof p.id !== "string" || !ID.test(p.id) || seen.has(p.id)) continue;
      const label = typeof p.label === "string" ? p.label.replace(CONTROL, " ").trim() : "";
      if (!label || [...label].length > 24) continue;
      if (typeof p.url !== "string" || p.url.length > 512) continue;
      let url;
      try {
        url = new URL(p.url);
      } catch {
        continue;
      }
      if ((url.protocol !== "http:" && url.protocol !== "https:") || url.username || url.password) continue;
      if (url.origin === origin) continue;
      seen.add(p.id);
      items.push({ id: p.id, label, url: url.href });
    }
    return items;
  }

  // The file's items and the bridge's menu as one list of at most four: a
  // bridge item takes the place of the file's item with its id, the others
  // follow the file's. A bridge item on the page's own origin is dropped.
  function merge(items, menu, origin) {
    const out = items.slice();
    for (const m of Array.isArray(menu) ? menu : []) {
      let url;
      try {
        url = new URL(m.url);
      } catch {
        continue;
      }
      if (url.origin === origin) continue;
      const item = { id: m.id, label: m.label, url: url.href };
      const at = out.findIndex((i) => i.id === item.id);
      if (at >= 0) out[at] = item;
      else if (out.length < MAX) out.push(item);
    }
    return out.slice(0, MAX);
  }

  globalThis.TEHAS_PANELS = { view, merge };

  if (typeof document === "undefined") return;
  const nav = document.querySelector(".topbar nav");
  if (!nav) return;

  function el(tag, cls, text) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text !== undefined) n.textContent = text;
    return n;
  }

  let dock = null;
  let shown = null; // { item, link }
  const frames = new Map();

  function make() {
    dock = el("aside", "panel-dock");
    dock.id = "panel-dock";
    dock.hidden = true;
    dock.setAttribute("role", "dialog");
    dock.setAttribute("aria-labelledby", "panel-dock-title");
    const head = el("header", "panel-dock-head");
    const title = el("h2", "panel-dock-title");
    title.id = "panel-dock-title";
    const out = el("a", "panel-dock-out", "Open in a new tab");
    out.target = "_blank";
    out.rel = "noopener noreferrer";
    const close = el("button", "panel-dock-close", "Close");
    close.type = "button";
    close.addEventListener("click", hide);
    head.append(title, out, close);
    dock.append(head, el("div", "panel-dock-body"));
    // A key pressed inside the frame stays there: Escape works from this page.
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && shown) hide();
    });
    document.body.append(dock);
  }

  function show(item, link) {
    if (shown && shown.item.id === item.id) return hide();
    if (!dock) make();
    if (shown) shown.link.setAttribute("aria-expanded", "false");
    dock.querySelector(".panel-dock-title").textContent = item.label;
    dock.querySelector(".panel-dock-out").href = item.url;
    let frame = frames.get(item.id);
    if (!frame) {
      frame = document.createElement("iframe");
      frame.title = item.label;
      frame.referrerPolicy = "no-referrer";
      frame.setAttribute("allow", "");
      frame.setAttribute("sandbox", SANDBOX);
      frame.src = item.url;
      frames.set(item.id, frame);
      dock.querySelector(".panel-dock-body").append(frame);
    }
    for (const [id, f] of frames) f.hidden = id !== item.id;
    dock.hidden = false;
    link.setAttribute("aria-expanded", "true");
    shown = { item, link };
    dock.querySelector(".panel-dock-close").focus();
  }

  function hide() {
    if (!shown) return;
    dock.hidden = true;
    shown.link.setAttribute("aria-expanded", "false");
    shown.link.focus();
    shown = null;
  }

  // The menu's items drawn again: the file's, then the bridge's. An item
  // that went keeps no frame, and a dock showing it closes.
  function draw(items) {
    for (const old of nav.querySelectorAll(".panel-link")) old.remove();
    const ids = new Set(items.map((i) => i.id));
    if (shown && (!ids.has(shown.item.id) || items.find((i) => i.id === shown.item.id).url !== shown.item.url)) {
      dock.hidden = true;
      shown = null;
    }
    for (const [id, f] of frames) {
      const now = items.find((i) => i.id === id);
      if (!now || f.src !== now.url) {
        f.remove();
        frames.delete(id);
      }
    }
    for (const item of items) {
      const link = el("a", "panel-link", item.label);
      link.href = item.url;
      link.target = "_blank";
      link.rel = "noopener noreferrer";
      link.setAttribute("aria-controls", "panel-dock");
      link.setAttribute("aria-expanded", "false");
      link.addEventListener("click", (e) => {
        // A press with a modifier, or another button, opens the tab it asks for.
        if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
        e.preventDefault();
        show(item, link);
      });
      if (shown && shown.item.id === item.id) {
        link.setAttribute("aria-expanded", "true");
        shown.link = link;
      }
      nav.append(link);
    }
  }

  let fromFile = [];
  let fromHost = [];
  // The bridge answers again each minute: the menu is drawn again only when
  // its items changed, so a link keeps its focus and a frame its page.
  let drawn = null;
  const redraw = () => {
    const items = merge(fromFile, fromHost, location.origin);
    const key = JSON.stringify(items);
    if (key === drawn) return;
    drawn = key;
    draw(items);
  };

  async function load() {
    try {
      const r = await fetch("/live/panels.json", { cache: "no-store" });
      if (r.ok) fromFile = view(await r.json(), location.origin);
    } catch {
      fromFile = [];
    }
    redraw();
  }

  const bridge = globalThis.TEHAS_HOST_BRIDGE;
  if (bridge) {
    const current = bridge.current();
    if (current) fromHost = current.menu || [];
    bridge.onChange((a) => {
      fromHost = (a && a.menu) || [];
      redraw();
    });
  }
  load();
})();
