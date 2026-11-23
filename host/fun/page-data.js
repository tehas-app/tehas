// Small rules the pages share: how long ago a moment was, and an address a
// link may take. Pure: it opens no file and reads no clock.

const num = (v) => (typeof v === "number" && Number.isFinite(v) ? v : null);

// How long ago, in the coarsest unit that reads well. A moment just ahead of
// the clock it is read with is "just now", not a negative age.
function agoText(secs) {
  const s = num(secs);
  if (s == null) return "";
  const at = Math.max(0, s);
  if (at < 90) return "just now";
  if (at < 5400) return `${Math.round(at / 60)} min ago`;
  if (at < 129600) return `${Math.round(at / 3600)} h ago`;
  return `${Math.round(at / 86400)} d ago`;
}

// How long ago an ISO stamp was. An empty or unparseable stamp is "", not a
// reading: a page draws no age where the file names no moment. nowMs is the
// present, given by the caller.
export function ago(iso, nowMs) {
  const at = typeof iso === "string" ? Date.parse(iso) : NaN;
  if (Number.isNaN(at)) return "";
  return agoText((nowMs - at) / 1000);
}

// An address a published file names, as a link may take it: an http: or https:
// address, or one relative to the page. Anything else is "", no link.
export function webHref(u) {
  if (typeof u !== "string") return "";
  try {
    const p = new URL(u, "https://page.invalid/").protocol;
    return p === "https:" || p === "http:" ? u : "";
  } catch {
    return "";
  }
}
