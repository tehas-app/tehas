// The second case of test/browser/dashboard-check.mjs: the first read of
// status.json fails and running.json never comes. The page must keep its
// headings, keep polling and draw what it can once status.json is there.
const found = { violations: [], errors: [], steps: {} };
document.addEventListener("securitypolicyviolation", (e) => {
  found.violations.push(`${e.violatedDirective} ${e.blockedURI}`);
});
addEventListener("error", (e) => found.errors.push(String(e.message)));
addEventListener("unhandledrejection", (e) => found.errors.push(String(e.reason?.message ?? e.reason)));

const fetches = {};
const fetchOf = globalThis.fetch;
globalThis.fetch = (src, init) => {
  fetches[src] = (fetches[src] ?? 0) + 1;
  return fetchOf(src, init);
};

const s = found.steps;
const $ = (sel) => document.querySelector(sel);
const text = (sel) => $(sel)?.textContent.trim().replace(/\s+/g, " ") ?? null;
const wait = async (ok, ms) => {
  const end = performance.now() + ms;
  while (!ok()) {
    if (performance.now() > end) return false;
    await new Promise((r) => setTimeout(r, 25));
  }
  return true;
};
const headings = () => [...document.querySelectorAll("tehas-factory-issues h2")].length;

async function run() {
  await wait(() => fetches["/live/status.json"] >= 1 && fetches["/live/analytics.json"] >= 1, 6000);
  // After the failed first read, before the next poll.
  await new Promise((r) => setTimeout(r, 1000));
  s.headingsAfterFailure = headings();
  s.freshAfterFailure = text("#fresh");
  await wait(() => document.querySelectorAll("#queued li").length === 2 && text("#state") !== "loading", 14000);
  s.headings = headings();
  s.pill = text("#state");
  s.fresh = text("#fresh");
  s.queued = document.querySelectorAll("#queued li").length;
  s.analyticsHidden = $("#analytics").hidden;
  s.fetches = fetches;
}

const out = document.createElement("pre");
out.id = "probe-result";
out.hidden = true;
document.body.append(out);
run()
  .catch((e) => {
    found.error = String(e?.stack ?? e);
  })
  .finally(() => {
    out.textContent = JSON.stringify(found);
  });
