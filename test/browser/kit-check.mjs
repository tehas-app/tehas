// The kit in a real browser: serves the demo page in test/browser/ with the
// kit (host/fun) and the samples (test/fixtures/live) under a strict CSP,
// loads it in headless Chromium and checks what the islands drew.
//
//   node test/browser/kit-check.mjs
//
// Not part of the gate, which has no browser: it prints "skip" and exits 0
// when no Chromium is found. TEHAS_CHROMIUM names one; otherwise it looks for
// Playwright's headless shell under ~/.cache/ms-playwright.
//
// Exit: 0 passed or skipped, 1 a check failed.
import { createServer } from "node:http";
import { execFile } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { extname, join, normalize } from "node:path";

const ROOT = new URL("../..", import.meta.url).pathname;
const MOUNTS = { "/fun/": "host/fun/", "/live/": "test/fixtures/live/", "/demo/": "test/browser/" };
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".json": "application/json" };
const CSP = "default-src 'self'; script-src 'self'; style-src 'self'; base-uri 'none'";

function chromium() {
  if (process.env.TEHAS_CHROMIUM) return { bin: process.env.TEHAS_CHROMIUM, env: {} };
  const cache = join(homedir(), ".cache/ms-playwright");
  if (!existsSync(cache)) return null;
  const shells = readdirSync(cache).filter((d) => d.startsWith("chromium_headless_shell-")).sort();
  const bin = shells.length && join(cache, shells.at(-1), "chrome-linux/headless_shell");
  if (!bin || !existsSync(bin)) return null;
  // Playwright's own copies of the system libraries it needs, where it put them.
  const libs = join(cache, "extra-libs/root/usr/lib/x86_64-linux-gnu");
  return { bin, env: existsSync(libs) ? { LD_LIBRARY_PATH: libs } : {} };
}

function serve(req, res) {
  const path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  const mount = Object.keys(MOUNTS).find((m) => path.startsWith(m));
  const file = mount && normalize(join(ROOT, MOUNTS[mount], path.slice(mount.length) || "index.html"));
  if (!file || !file.startsWith(join(ROOT, MOUNTS[mount])) || !existsSync(file) || !statSync(file).isFile()) {
    res.writeHead(404, { "content-security-policy": CSP }).end("not found");
    return;
  }
  const send = () =>
    res
      .writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream", "content-security-policy": CSP })
      .end(readFileSync(file));
  // The feed is slow, so the page shows its loading state first.
  if (mount === "/live/") setTimeout(send, 300);
  else send();
}

const browser = chromium();
if (!browser) {
  console.log("skip  the kit in a browser (no Chromium; set TEHAS_CHROMIUM)");
  process.exit(0);
}

const server = createServer(serve).listen(0, "127.0.0.1");
await new Promise((r) => server.once("listening", r));
const url = `http://127.0.0.1:${server.address().port}/demo/index.html`;
const dom = await new Promise((resolve, reject) =>
  execFile(
    browser.bin,
    ["--headless", "--no-sandbox", "--disable-gpu", "--virtual-time-budget=15000", "--dump-dom", url],
    { env: { ...process.env, ...browser.env }, timeout: 60000, maxBuffer: 1 << 24 },
    (err, out, errOut) => (err ? reject(new Error(`${err.message}\n${errOut}`)) : resolve(out)),
  ),
);
server.close();

const raw = /<pre id="result">([^<]*)<\/pre>/.exec(dom)?.[1];
if (!raw) {
  console.log("FAIL  the kit in a browser: the page wrote no result");
  process.exit(1);
}
const found = JSON.parse(raw.replaceAll("&quot;", '"').replaceAll("&lt;", "<").replaceAll("&gt;", ">").replaceAll("&amp;", "&"));
const s = found.steps;
if (process.env.KIT_CHECK_DUMP) console.log(JSON.stringify(found, null, 1));
const checks = [
  ["no error in the check", !found.error, found.error],
  ["no CSP violation", found.violations.length === 0, found.violations],
  ["the server HTML stood first", found.firstViews.queue[0] === "Server HTML of the queue", found.firstViews.queue[0]],
  ["then the loading fallback", found.firstViews.queue[1] === "Loading the queue", found.firstViews.queue],
  ["then a For list from the feed", s.listDrawn && s.rows.length === 3, s.rows],
  ["rows bind the sample's text", s.rows?.[0] === "#41 Export the report as CSV", s.rows?.[0]],
  ["a time ago part", typeof s.ago === "string" && / ago$|yesterday/.test(s.ago), s.ago],
  ["the mounted mark", s.mounted === true, s.mounted],
  ["hydrate=visible mounts an island laid out as its children", s.contentsSeen && s.contentsDisplay === "contents", [s.contentsSeen, s.contentsDisplay]],
  ["the second island reads the same feed", s.count === "3 in line", s.count],
  ["one fetch of the address for two islands", found.fetches["/live/status.json"] === 1, found.fetches],
  ["a pick in one island shows in the other", s.pickedAfterClick === "#42 A search with no match says so", s.pickedAfterClick],
  ["the picking island marks the row", s.highlighted === "42", s.highlighted],
  ["the pick is a bubbling event with JSON detail", JSON.stringify(found.events[0]) === JSON.stringify({ type: "kit-demo-queue:pick", bubbles: true, detail: { number: 42 } }), found.events],
  ["a missing feed shows the error with a retry", s.missing && /missing\.json: 404/.test(s.missingText), s.missingText],
  ["the busy sign is hidden once the data is in", s.busyHidden === true, s.busyHidden],
  ["the retry polls again and fails again", s.retried && s.failedAgain, [s.retried, s.failedAgain]],
  ["the store outlives the island that wrote it", s.afterRemove === "#42 A search with no match says so", s.afterRemove],
  ["a new island gets the store as it was", s.againHighlighted === "42", s.againHighlighted],
  ["a pick after the removal reaches the island that stayed", s.pickedAfterRemove === "#41 Export the report as CSV", s.pickedAfterRemove],
];
let failed = 0;
for (const [name, ok, got] of checks) {
  if (ok) console.log(`ok    ${name}`);
  else {
    failed += 1;
    console.log(`FAIL  ${name}: ${JSON.stringify(got)}`);
  }
}
console.log(`${failed ? "FAIL" : "ok  "}  the kit in a browser: ${checks.length - failed} of ${checks.length}`);
process.exit(failed ? 1 : 0);
