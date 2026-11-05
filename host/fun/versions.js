// The versions in the footers of the two front pages, read from the factory's
// health (/live/health.json, written by tehasbot health): tehas from
// checks.platform, the pi-rukas the sandbox image runs from checks.rukas, and
// the tehas-industry that runs the factory from checks.industry, when there is
// one. So the footer, /health and the MCP server tell one story, however the
// image was built.
//
// line() is the pure step from the file to the text, tested without a browser
// (test/versions_test.mjs). The file comes from the network: a value of
// another shape is left out, and the line is set as text.
(() => {
  const VERSION = /^v[0-9]{1,6}(\.[0-9]{1,6}){1,3}(-[0-9A-Za-z.]{1,32})?$/;
  const COMMIT = /^[0-9a-f]{7,40}$/;
  const version = (v) => (typeof v === "string" && VERSION.test(v) ? v : null);
  const commit = (c) => (typeof c === "string" && COMMIT.test(c) ? c.slice(0, 7) : null);
  const object = (o) => (o !== null && typeof o === "object" ? o : {});

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

  globalThis.TEHAS_VERSIONS = { line };

  if (typeof document === "undefined") return;
  const el = document.getElementById("versions");
  if (!el) return;

  async function refresh() {
    if (document.hidden) return;
    let text = null;
    try {
      const r = await fetch("/live/health.json", { cache: "no-store" });
      if (r.ok) text = line(await r.json());
    } catch {
      text = null;
    }
    // No health yet keeps what the footer showed last, or nothing.
    if (text === null) return;
    el.textContent = text;
    el.hidden = false;
  }

  refresh();
  setInterval(refresh, 60000);
  document.addEventListener("visibilitychange", refresh);
})();
