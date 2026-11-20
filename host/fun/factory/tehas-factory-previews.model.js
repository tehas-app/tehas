// The previews of pull requests, from live/previews.json (tehasbot preview).
// A host without previews has no such file: present is false. The address is
// checked before it becomes a link. Pure: the clock is nowMs.
import { ago } from "../host-data.js";

const STATES = {
  building: "building",
  "no-recipe": "no preview/Dockerfile in the commit",
  "build-failed": "unable to create",
  "no-space": "unable to create",
  "over-limit": "over the limit of previews",
};
const ADDRESS = /^https:\/\/uat-([1-9][0-9]{0,8}|main)[.-][a-z0-9.-]+\/$/;

// What a preview that does not serve says about itself: the state, and the
// reason the run wrote into its state file, which tells a refused volume from
// a failed build. The reason is text tehasbot preview wrote.
export function previewWhy(it) {
  const label = STATES[it.state] || "not serving";
  const reason = typeof it.reason === "string" ? it.reason.trim() : "";
  return reason ? label + ": " + reason : label;
}

export function previewsModel(file, nowMs) {
  const p = file && typeof file === "object" ? file : null;
  if (!p || !Array.isArray(p.previews)) return { present: false, n: "", note: "", rows: [] };
  // A run that cannot build still writes the list, so an old time means the
  // runs have stopped.
  const note = [];
  if (p.disk && p.disk.low) {
    note.push(p.disk.freeBytes === null
      ? "The host's free disk space could not be read: previews are not being built."
      : "The host is low on disk: previews are not being built.");
  }
  if (typeof p.generatedAt === "string" && !Number.isNaN(Date.parse(p.generatedAt))) {
    note.push("List written " + ago(p.generatedAt, nowMs) + ".");
  }
  const rows = [];
  for (const it of p.previews) {
    // The default branch's preview is listed first and has the name "main"
    // where a pull request has a number.
    const isMain = !!it && it.name === "main";
    if (!it || (!isMain && !Number.isSafeInteger(it.number))) continue;
    const serving = it.state === "serving" && typeof it.address === "string" && ADDRESS.test(it.address);
    rows.push({
      key: isMain ? "main" : it.number,
      num: isMain ? "main" : "#" + it.number,
      title: isMain ? "default branch"
        : typeof it.title === "string" && it.title ? it.title : "pull request " + it.number,
      href: serving ? it.address : null,
      why: serving ? "" : previewWhy(it),
    });
  }
  return { present: true, n: p.previews.length ? "(" + p.previews.length + ")" : "", note: note.join(" "), rows };
}
