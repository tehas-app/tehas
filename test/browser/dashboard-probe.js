// The browser check of the plain dashboard (test/browser/dashboard-check.mjs):
// runs beside the islands, waits for them to draw, clicks a tab and writes
// what it found into #probe-result as JSON.
const found = { violations: [], steps: {} };
document.addEventListener("securitypolicyviolation", (e) => {
  found.violations.push(`${e.violatedDirective} ${e.blockedURI}`);
});

const fetches = {};
const fetchOf = globalThis.fetch;
globalThis.fetch = (src, init) => {
  fetches[src] = (fetches[src] ?? 0) + 1;
  return fetchOf(src, init);
};

const s = found.steps;
const $ = (sel) => document.querySelector(sel);
const text = (sel) => $(sel)?.textContent.trim().replace(/\s+/g, " ") ?? null;
s.firstPill = text("#state");
const wait = async (ok, ms = 6000) => {
  const end = performance.now() + ms;
  while (!ok()) {
    if (performance.now() > end) return false;
    await new Promise((r) => setTimeout(r, 25));
  }
  return true;
};

async function run() {
  const islands = () => [...document.querySelectorAll("[data-tehas-island]")];
  await wait(() => islands().every((el) => el.hasAttribute("data-tehas-mounted")));
  s.unmounted = islands().filter((el) => !el.hasAttribute("data-tehas-mounted")).map((el) => el.localName);
  await wait(() => text("#state") !== "loading" && $("#an-figures") && $(".host-note"));
  s.pill = text("#state");
  s.pillClass = $("#state").className;
  s.fresh = text("#fresh");
  s.all = $("#all").getAttribute("href");
  s.counts = ["queued", "awaiting", "stopped", "completed"].map((id) => document.querySelectorAll(`#${id} li`).length);
  s.ns = ["queued", "awaiting", "stopped", "completed"].map((id) => text(`#n-${id}`));
  s.firstRow = $("#queued li")?.textContent ?? null;
  s.note = text("#queued .host-note");
  s.roHidden = $("#ro").hidden;
  s.roPill = $("#readonly").hidden;
  s.tabs = [...document.querySelectorAll("#tabs button")].map((b) => b.textContent.trim());
  s.title = $("#now-title")?.textContent ?? null;
  await wait(() => /line 40/.test(text("#console") ?? "") && text("#console-note") === "live");
  s.console = $("#console").textContent;
  s.consoleNote = text("#console-note");
  document.querySelectorAll("#tabs button")[1].click();
  await wait(() => /^Last cycle/.test($("#now-title").textContent) && /^#36/.test($("#console").textContent));
  s.clicked = $("#now-title").textContent;
  s.clickedStep = text("#now-step");
  s.clickedConsole = $("#console").textContent;
  const second = document.querySelectorAll("#tabs button")[1];
  second.focus();
  second.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  await wait(() => /^Working on/.test($("#now-title").textContent));
  s.arrowed = text("#tabs button[aria-selected=true]");
  s.arrowFocus = document.activeElement?.textContent.trim() ?? null;
  s.previews = document.querySelectorAll("#preview-list li").length;
  s.previewsHidden = $("#previews").hidden;
  s.notServing = text("#preview-list li:last-child .when");
  s.hostRows = document.querySelectorAll("#host-list li").length;
  s.units = document.querySelectorAll("#unit-list li").length;
  s.hostHidden = $("#host").hidden;
  s.hostNote = text("#host-note");
  s.analyticsHidden = $("#analytics").hidden;
  s.figures = document.querySelectorAll("#an-figures .an-figure").length;
  s.days = document.querySelectorAll("#an-chart .an-day").length;
  s.line = !!$("#an-chart svg.an-line polyline") && $("#an-chart svg").namespaceURI === "http://www.w3.org/2000/svg";
  const bar = $("#an-chart .an-bars span");
  s.barHeight = !!bar && /%$/.test(bar.style.getPropertyValue("--tehas-factory-share")) && getComputedStyle(bar).height !== "auto";
  s.versions = document.querySelectorAll("#an-versions tr").length;
  // Every ten seconds while the file is there.
  await wait(() => fetches["/live/top.json"] >= 2, 11000);
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
