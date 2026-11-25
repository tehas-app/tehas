// The dock: a page in a frame at the side of the dashboard, which
// loads it. The host bridge's menu (host/fun/host-bridge.js), as it
// comes and goes, and the plugins menu of /live/plugins.json are items in
// the page's menu. Pressing one opens its page in the dock, or an item that
// asks for a page of its own in a new tab.
//
// bridge() and plugins() are the pure steps to the items, tested without a
// browser (test/dock_menu_test.mjs), and the page half in
// test/dock_test.mjs. What they read comes from the network and is
// untrusted: a label is set as text, and a URL is parsed again here, so only
// an http: or https: page on another origin is kept.
//
// The frame is made on the first press, never at load, so the dashboard asks
// a page's host for nothing until a person does. It may run scripts on its
// own origin and open new tabs, never navigate this page, and is passed
// nothing but its URL.
(() => {
  const MAX = 4;
  // As console/core/src/outside.rs, whose test holds them equal.
  const SANDBOX = "allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox";
  // A control character, or a line or paragraph separator, as host-bridge.js folds them.
  const CONTROL = /[\u0000-\u001f\u007f-\u009f\u2028\u2029]/g;

  // An http: or https: address on another origin, or null.
  function address(raw, origin) {
    if (typeof raw !== "string" || raw.length > 512) return null;
    let url;
    try {
      url = new URL(raw);
    } catch {
      return null;
    }
    if ((url.protocol !== "http:" && url.protocol !== "https:") || url.username || url.password) return null;
    return url.origin === origin ? null : url.href;
  }

  // The bridge's menu as items, at most four: a later item with an id takes
  // the place of the one before it. An item on the page's own origin is
  // dropped.
  function bridge(menu, origin) {
    const out = [];
    for (const m of Array.isArray(menu) ? menu : []) {
      let url;
      try {
        url = new URL(m.url);
      } catch {
        continue;
      }
      if (url.origin === origin) continue;
      const item = { id: m.id, label: m.label, url: url.href, open: m.open };
      const at = out.findIndex((i) => i.id === item.id);
      if (at >= 0) out[at] = item;
      else if (out.length < MAX) out.push(item);
    }
    return out;
  }

  const NAME = /^[a-z][a-z0-9]{0,11}$/;
  const VIEW = /^[a-z-]{1,16}$/;
  const PLUGINS = 8;
  const text = (s) => (typeof s === "string" ? s.replace(CONTROL, " ").trim() : "");
  const fits = (s) => s && [...s].length <= 24;

  // The plugins menu's items: each plugin's views under its label.
  function plugins(file, origin) {
    if (file === null || typeof file !== "object" || !Array.isArray(file.plugins)) return [];
    const items = [];
    const seen = new Set();
    for (const p of file.plugins.slice(0, PLUGINS)) {
      if (p === null || typeof p !== "object" || typeof p.name !== "string" || !NAME.test(p.name) || seen.has(p.name)) continue;
      const group = text(p.label);
      if (!fits(group) || !Array.isArray(p.views)) continue;
      seen.add(p.name);
      for (const v of p.views.slice(0, 4)) {
        if (v === null || typeof v !== "object" || typeof v.id !== "string" || !VIEW.test(v.id)) continue;
        const label = text(v.label);
        const url = address(v.url, origin);
        if (!fits(label) || !url) continue;
        const id = `m-${p.name}-${v.id}`;
        if (items.some((i) => i.id === id)) continue;
        items.push({ id, label, url, open: v.open === "page" ? "page" : "dock", group });
      }
    }
    return items;
  }

  globalThis.TEHAS_DOCK = { bridge, plugins };

  if (typeof document === "undefined") return;
  const nav = document.querySelector(".topbar nav");
  if (!nav) return;

  // The one el(), from host/fun/dom.js, the classic script the dashboard loads before
  // this file.
  const { el } = globalThis.TEHAS_DOM;

  let dock = null;
  let shown = null; // { item, link }
  const frames = new Map();

  function make() {
    dock = el("aside", "dock");
    dock.id = "dock";
    dock.hidden = true;
    dock.setAttribute("role", "dialog");
    dock.setAttribute("aria-labelledby", "dock-title");
    const head = el("header", "dock-head");
    const title = el("h2", "dock-title");
    title.id = "dock-title";
    const out = el("a", "dock-out", "Open in a new tab");
    out.target = "_blank";
    out.rel = "noopener noreferrer";
    const close = el("button", "dock-close", "Close");
    close.type = "button";
    close.addEventListener("click", hide);
    head.append(title, out, close);
    dock.append(head, el("div", "dock-body"));
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
    dock.querySelector(".dock-title").textContent = item.label;
    dock.querySelector(".dock-out").href = item.url;
    let frame = frames.get(item.id);
    if (!frame) {
      frame = document.createElement("iframe");
      frame.title = item.label;
      frame.referrerPolicy = "no-referrer";
      frame.setAttribute("allow", "");
      frame.setAttribute("sandbox", SANDBOX);
      frame.src = item.url;
      frames.set(item.id, frame);
      dock.querySelector(".dock-body").append(frame);
    }
    for (const [id, f] of frames) f.hidden = id !== item.id;
    dock.hidden = false;
    link.setAttribute("aria-expanded", "true");
    shown = { item, link };
    dock.querySelector(".dock-close").focus();
  }

  function hide() {
    if (!shown) return;
    dock.hidden = true;
    shown.link.setAttribute("aria-expanded", "false");
    shown.link.focus();
    shown = null;
  }

  // The menu's items drawn again: the bridge's, then the plugins'. An item
  // that went, or now opens as a page, keeps no frame, and a dock showing it
  // closes.
  function draw(items) {
    for (const old of [...nav.querySelectorAll(".dock-link"), ...nav.querySelectorAll(".dock-group")]) old.remove();
    const docked = (id) => items.find((i) => i.id === id && i.open !== "page");
    if (shown && docked(shown.item.id)?.url !== shown.item.url) {
      dock.hidden = true;
      shown = null;
    }
    for (const [id, f] of frames) {
      const now = docked(id);
      if (!now || f.src !== now.url) {
        f.remove();
        frames.delete(id);
      }
    }
    let group = null;
    for (const item of items) {
      if (item.group && item.group !== group) {
        group = item.group;
        nav.append(el("span", "dock-group", group));
      }
      const link = el("a", "dock-link", item.label);
      link.href = item.url;
      link.target = "_blank";
      link.rel = "noopener noreferrer";
      nav.append(link);
      if (item.open === "page") continue;
      link.setAttribute("aria-controls", "dock");
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
    }
  }

  let fromHost = [];
  let fromPlugins = [];
  // The bridge answers again each minute: the menu is drawn again only when
  // its items changed, so a link keeps its focus and a frame its page.
  let drawn = null;
  const redraw = () => {
    const items = bridge(fromHost, location.origin).concat(fromPlugins);
    const key = JSON.stringify(items);
    if (key === drawn) return;
    drawn = key;
    draw(items);
  };

  async function load() {
    try {
      const r = await fetch("/live/plugins.json", { cache: "no-store" });
      fromPlugins = r.ok ? plugins(await r.json(), location.origin) : [];
    } catch {
      fromPlugins = [];
    }
    redraw();
  }

  const host = globalThis.TEHAS_HOST_BRIDGE;
  if (host) {
    const current = host.current();
    if (current) fromHost = current.menu || [];
    host.onChange((a) => {
      fromHost = (a && a.menu) || [];
      redraw();
    });
  }
  load();
})();
