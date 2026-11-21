// The product wiki's state, from live/wiki.json (tehasbot wiki). The page links
// a build only by an address of its own form, /wiki/<date>-<8 hex>/. Pure: the
// clock is nowMs.
import { ago } from "../host-data.js";
import { BUILD } from "./tehas-factory-wikilink.model.js";

export { wikiLink } from "./tehas-factory-wikilink.model.js";
const text = (v) => (typeof v === "string" ? v : "");
const short = (c) => (/^[0-9a-f]{8,40}$/.test(text(c)) ? c.slice(0, 8) : "");

export function wikiModel(file, nowMs) {
  const w = file && typeof file === "object" ? file : null;
  if (!w || w.api !== 1) return { present: false, href: null, id: "", built: "", behind: "", main: "" };
  const c = w.current && typeof w.current === "object" ? w.current : null;
  const href = c && BUILD.test(text(c.url)) ? c.url : null;
  const id = href ? text(c.id) : "";
  let built = "No build yet.";
  if (c && href) {
    const when = ago(text(c.builtAt), nowMs);
    const describe = text(c.describe);
    built = "Built " + (when || "at an unknown time") + " at commit " + short(c.commit) + (describe ? " (" + describe + ")" : "") + ".";
  }
  const behind = w.behind && typeof w.behind === "object" ? text(w.behind.line) : "";
  const m = w.main && typeof w.main === "object" ? w.main : null;
  const main = m && short(m.commit) ? "Main is " + short(m.commit) + (text(m.commitDate) ? ", " + text(m.commitDate).slice(0, 10) : "") + "." : "";
  return { present: true, href, id, built, behind: behind ? behind.charAt(0).toUpperCase() + behind.slice(1) + "." : "", main };
}
