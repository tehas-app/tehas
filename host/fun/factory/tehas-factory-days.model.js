// "Last 10 days", from live/analytics.json: what the file means is worked out
// by analytics-data.js; here the shares become the lengths and the points the
// section draws. Pure.
import { analyticsView } from "../analytics-data.js";

const pct = (share) => Math.round(share * 100) + "%";

export function daysModel(file) {
  const v = analyticsView(file);
  if (!v.present) return { present: false };
  return {
    ...v,
    figures: v.figures.map((f) => ({ ...f, bars: (f.bars ?? []).map(pct) })),
    // Two bars a day on one scale, and the cycle-minutes as a line on its own.
    chart: v.chart.map((d) => ({
      title: d.title,
      label: d.label,
      bars: [["shipped", pct(d.shippedHeight), d.shipped], ["failed", pct(d.failedHeight), d.failed]]
        .map(([cls, height, n]) => ({ cls, height, text: n ? String(n) : "" })),
    })),
    viewBox: "0 0 " + v.chart.length * 10 + " 100",
    points: v.chart.map((d, i) => (i * 10 + 5) + "," + (100 - d.minutesHeight * 100).toFixed(1)).join(" "),
    steps: v.steps.map((s) => ({ ...s, width: pct(s.share) })),
    versions: v.versions.map((r) => [r.tehas, r.rukas, r.runs, r.pr, r.minutes, r.tokens]),
  };
}
