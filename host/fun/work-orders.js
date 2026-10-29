// What each ticket's runs spent, read from /live/orders.json (written by
// `analytics orders`, see console/analytics/src/orders.rs) for the work order
// of the factory floor. figures() is the pure step from the file to the
// figures, tested without a browser (test/work_orders_view_test.mjs).
(() => {
  const amount = (v) => (Number.isSafeInteger(v) && v > 0 ? v : null);

  // A Map from ticket number to { tokens, seconds }, either null when the file
  // does not say. null when the file is not one.
  function figures(file) {
    if (file === null || typeof file !== "object" || file.orders !== 1) return null;
    const issues = file.issues;
    if (issues === null || typeof issues !== "object" || Array.isArray(issues)) return null;
    const out = new Map();
    for (const [key, f] of Object.entries(issues)) {
      if (!/^[1-9][0-9]{0,8}$/.test(key) || f === null || typeof f !== "object") continue;
      out.set(Number(key), { tokens: amount(f.tokens), seconds: amount(f.seconds) });
    }
    return out;
  }

  async function read() {
    try {
      const r = await fetch("/live/orders.json", { cache: "no-store" });
      return r.ok ? figures(await r.json()) : null;
    } catch {
      return null;
    }
  }

  globalThis.TEHAS_ORDERS = { figures, read };
})();
