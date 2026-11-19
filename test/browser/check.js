// The browser check of the kit (test/browser/kit-check.mjs): runs before the
// islands, watches them mount and writes what it found into #result as JSON.
const found = { violations: [], fetches: {}, events: [], firstViews: {}, steps: {} };

document.addEventListener("securitypolicyviolation", (e) => {
  found.violations.push(`${e.violatedDirective} ${e.blockedURI}`);
});

const fetchOf = globalThis.fetch;
globalThis.fetch = (src, init) => {
  found.fetches[src] = (found.fetches[src] ?? 0) + 1;
  return fetchOf(src, init);
};

document.addEventListener("kit-demo-queue:pick", (e) => {
  found.events.push({ type: e.type, bubbles: e.bubbles, detail: e.detail });
});

// Every text an island shows, in order, from the server HTML on.
for (const id of ["queue", "picked", "missing"]) {
  const el = document.getElementById(id);
  const views = (found.firstViews[id] = [el.textContent.trim().replace(/\s+/g, " ")]);
  new MutationObserver(() => {
    const text = el.textContent.trim().replace(/\s+/g, " ");
    if (text && views.at(-1) !== text) views.push(text);
  }).observe(el, { childList: true, subtree: true, characterData: true });
}

const text = (sel) => document.querySelector(sel)?.textContent.trim().replace(/\s+/g, " ") ?? null;
const wait = async (ok, ms = 5000) => {
  const end = performance.now() + ms;
  while (!ok()) {
    if (performance.now() > end) return false;
    await new Promise((r) => setTimeout(r, 20));
  }
  return true;
};

async function run() {
  const s = found.steps;
  s.listDrawn = await wait(() => document.querySelectorAll("#queue li").length > 0);
  s.pickedDrawn = await wait(() => text("#picked .count") !== null);
  s.rows = [...document.querySelectorAll("#queue li button")].map((b) => b.textContent.trim().replace(/\s+/g, " "));
  s.ago = text("#queue li time");
  s.count = text("#picked .count");
  s.mounted = document.getElementById("queue").hasAttribute("data-tehas-mounted");
  s.contentsSeen = await wait(() => document.getElementById("seen").hasAttribute("data-tehas-mounted"));
  s.contentsDisplay = getComputedStyle(document.getElementById("seen")).display;

  document.querySelector('#queue button[data-number="42"]').click();
  await wait(() => text("#picked .picked") !== "Nothing picked");
  s.pickedAfterClick = text("#picked .picked");
  s.highlighted = document.querySelector("#queue li.is-picked button")?.dataset.number ?? null;

  s.missing = await wait(() => text("#missing [role=alert] p") !== null);
  s.missingText = text("#missing [role=alert] p");
  s.busyHidden = document.querySelector("#picked .tehas-busy")?.hidden ?? null;
  document.querySelector("#missing .tehas-retry").click();
  s.retried = await wait(() => found.fetches["/live/missing.json"] === 2);
  s.failedAgain = await wait(() => text("#missing [role=alert] p") !== null);

  // The queue goes; the picked island keeps the store and the feed.
  document.getElementById("queue").remove();
  document.querySelector("#missing").remove();
  await new Promise((r) => setTimeout(r, 50));
  s.afterRemove = text("#picked .picked");
  s.queueEmptied = document.getElementById("picked").isConnected;
}

run()
  .catch((e) => {
    found.error = String(e?.stack ?? e);
  })
  .finally(() => {
    document.getElementById("result").textContent = JSON.stringify(found);
  });
