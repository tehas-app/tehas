// The open epics and their sub-issues, read from /live/epics.json (written by
// tehasbot status, see console/src/epics.rs). Both front pages draw it: the
// factory floor (index.html) and the business page (business.html), each in
// its own section with id "epics", styled by its own sheet.
//
// view() is the pure step from the file to what is drawn, tested without a
// browser (test/epics_view_test.mjs). The file is read from the network and its
// text is untrusted: titles are set as text, and every link is made here from
// the repository and a number, never taken from the file.
(() => {
  const REPO = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;
  const isNumber = (n) => Number.isSafeInteger(n) && n > 0;
  const count = (n) => (Number.isSafeInteger(n) && n >= 0 ? n : 0);
  const title = (t) => (typeof t === "string" ? t : "");

  // { repo, epics: [{ number, title, url, total, completed, more, subs }] },
  // or null when the file is not one. Each sub is { number, title, url, closed }.
  function view(file) {
    if (file === null || typeof file !== "object" || !Array.isArray(file.epics)) return null;
    const repo = typeof file.repo === "string" && REPO.test(file.repo) ? file.repo : null;
    const link = (n) => (repo ? `https://github.com/${repo}/issues/${n}` : null);
    const seen = new Set();
    const epics = [];
    for (const e of file.epics) {
      if (e === null || typeof e !== "object" || !isNumber(e.number) || seen.has(e.number)) continue;
      seen.add(e.number);
      const subs = (Array.isArray(e.subIssues) ? e.subIssues : [])
        .filter((s) => s !== null && typeof s === "object" && isNumber(s.number))
        .map((s) => ({ number: s.number, title: title(s.title), url: link(s.number), closed: s.state === "closed" }));
      const total = Math.max(count(e.total), subs.length);
      epics.push({
        number: e.number, title: title(e.title), url: link(e.number), total,
        completed: Math.min(count(e.completed), total), more: Math.max(total - subs.length, 0), subs,
      });
    }
    return { repo, epics };
  }

  function el(tag, cls, text) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text !== undefined) n.textContent = text;
    return n;
  }

  function anchor(cls, href, text) {
    const a = el(href ? "a" : "span", cls, text);
    if (href) {
      a.href = href;
      a.target = "_blank";
      a.rel = "noopener noreferrer";
    }
    return a;
  }

  // One <details> an epic: its summary is the stack's row, its body the list.
  // `opened` is the set of epic numbers open, kept across redraws.
  function render(list, v, opened) {
    list.replaceChildren(
      ...v.epics.map((e) => {
        const d = el("details", "epic");
        d.open = opened.has(e.number);
        d.addEventListener("toggle", () => (d.open ? opened.add(e.number) : opened.delete(e.number)));
        const s = el("summary", "epic-row");
        s.append(el("b", "epic-number", `#${e.number}`), el("span", "epic-title", e.title));
        const bar = el("span", "epic-bar");
        bar.setAttribute("aria-hidden", "true");
        for (const sub of e.subs) bar.append(el("i", sub.closed ? "is-closed" : ""));
        if (e.more) bar.append(el("i", "is-more"));
        s.append(bar, el("span", "epic-count", `${e.completed}/${e.total} closed`));
        const body = el("div", "epic-body");
        const items = el("ul", "epic-subs");
        for (const sub of e.subs) {
          const li = el("li", sub.closed ? "is-closed" : "is-open");
          li.append(el("span", "epic-state", sub.closed ? "Closed" : "Open"), anchor("epic-sub", sub.url, `#${sub.number} ${sub.title}`));
          items.append(li);
        }
        if (!e.subs.length) items.append(el("li", "epic-none", "No sub-issue of this repository."));
        body.append(items);
        const foot = el("p", "epic-foot");
        if (e.more) foot.append(el("span", "", `${e.more} more on GitHub. `));
        foot.append(anchor("epic-link", e.url, `Epic #${e.number} on GitHub`));
        body.append(foot);
        d.append(s, body);
        return d;
      }),
    );
  }

  globalThis.TEHAS_EPICS = { view };

  if (typeof document === "undefined") return;
  const section = document.getElementById("epics");
  const list = document.getElementById("epics-list");
  if (!section || !list) return;
  const note = document.getElementById("epics-count");
  const nav = document.querySelectorAll("[data-epics-nav]");
  const opened = new Set();

  async function refresh() {
    if (document.hidden) return;
    let v = null;
    try {
      const r = await fetch("/live/epics.json", { cache: "no-store" });
      if (r.ok) v = view(await r.json());
    } catch {
      v = null;
    }
    // No file is an older host or a first poll: the section stays out of sight.
    section.hidden = v === null;
    for (const a of nav) a.hidden = v === null;
    if (v === null) return;
    if (note) note.textContent = v.epics.length === 1 ? "1 open" : `${v.epics.length} open`;
    if (v.epics.length) render(list, v, opened);
    else list.replaceChildren(el("p", "epic-empty", "No open epics."));
  }

  refresh();
  setInterval(refresh, 60000);
  document.addEventListener("visibilitychange", refresh);
})();
