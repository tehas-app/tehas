// The browser check of the kit (test/browser/kit-check.mjs): runs before the
// islands, watches them mount and writes what it found into #result as JSON.
const found = { violations: [], fetches: {}, events: [], firstViews: {}, steps: {} };

document.addEventListener("securitypolicyviolation", (e) => {
  found.violations.push(`${e.violatedDirective} ${e.blockedURI}`);
});

const fetchOf = globalThis.fetch;
// found.answers counts the answers each address has had.
found.answers = {};
globalThis.fetch = (src, init) => {
  found.fetches[src] = (found.fetches[src] ?? 0) + 1;
  return fetchOf(src, init).finally(() => {
    found.answers[src] = (found.answers[src] ?? 0) + 1;
  });
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

  // A feed whose rows change only a time keeps each row's nodes.
  await wait(() => document.querySelectorAll("#ticking li").length > 0);
  const row = document.querySelector("#ticking li");
  s.ticked = await wait(() => (found.fetches["/tick/status.json"] ?? 0) >= 3);
  s.rowKept = document.querySelector("#ticking li") === row && row.isConnected;
  document.getElementById("ticking").remove();

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
  // A new queue island gets the store as it was, and a pick in it reaches the
  // island that stayed.
  const again = document.createElement("kit-demo-queue");
  again.id = "again";
  again.setAttribute("data-tehas-island", "");
  again.setAttribute("src", "/live/status.json");
  document.querySelector("main").append(again);
  await wait(() => document.querySelectorAll("#again li").length > 0);
  s.againHighlighted = document.querySelector("#again li.is-picked button")?.dataset.number ?? null;
  document.querySelector('#again button[data-number="41"]').click();
  await wait(() => text("#picked .picked") !== s.afterRemove);
  s.pickedAfterRemove = text("#picked .picked");

  // The wire island: feeds with headers and an accepted 404, two streams
  // into a page store, and a press with an optimistic value.
  await wait(() => /^session/.test(text("#wire .echo") ?? ""));
  s.echo = text("#wire .echo");
  s.plan = text("#wire .plan");
  s.eventsDone = await wait(() => /^closed/.test(text("#wire .events") ?? ""));
  s.events = text("#wire .events");
  s.linesDone = await wait(() => /^closed/.test(text("#wire .lines") ?? ""));
  s.lines = text("#wire .lines");
  await wait(() => text("#wire .votes") === "0");
  // Each value the vote shows, with the number of press answers in by then.
  const votes = document.querySelector("#wire .votes");
  const answered = () => found.answers["/api/vote"] ?? 0;
  s.voteViews = [`${votes.textContent}@${answered()}`];
  new MutationObserver(() => {
    const v = `${votes.textContent}@${answered()}`;
    if (s.voteViews.at(-1) !== v) s.voteViews.push(v);
  }).observe(votes, { childList: true, subtree: true, characterData: true });
  document.querySelector("#wire .vote").click();
  await wait(() => answered() === 1 && (found.answers["/api/votes"] ?? 0) >= 2);
  await new Promise((r) => setTimeout(r, 100));
  s.voteAfter = votes.textContent;
  document.querySelector("#wire .vote").click();
  await wait(() => text("#wire .failed") !== "");
  await new Promise((r) => setTimeout(r, 50));
  s.failAfter = votes.textContent;
  s.failText = text("#wire .failed");
}

run()
  .catch((e) => {
    found.error = String(e?.stack ?? e);
  })
  .finally(() => {
    document.getElementById("result").textContent = JSON.stringify(found);
  });
