import { Errored, For, Loading, Show, createMemo, html } from "../kit/vendor/solid.js";
import { feed } from "../kit/feed.js";
import { view } from "../kit/parts.js";
import { daysModel } from "./tehas-factory-days.model.js";

// A length the stylesheet reads: the pages use no inline style, and a
// custom property set through the CSSOM is not one.
const share = (value) => (el) => el.style.setProperty("--tehas-factory-share", value);

// The file changes once a poll, every two minutes: it is read once a minute.
export const mount = view((props, ctx) => {
  const live = feed(props.src, { every: 60000 });
  const m = createMemo(() => daysModel(live.data()));
  return html`<${Errored} fallback=${(_e) => ctx.first}><${Loading} fallback=${() => ctx.first}><section id="analytics" hidden=${() => !m().present}>
    <h2>Last 10 days</h2>
    <p class="muted" id="analytics-note">${() => m().window}</p>
    <div class="an-figures" id="an-figures">
      <${For} each=${() => m().figures}>${(f) => html`<div class="an-figure">
        <div class="an-label">${f.label}</div><div class="an-value">${f.value}</div><div class="an-note">${f.note}</div>
        <${Show} when=${f.bars.length > 0}><div class="an-spark"><${For} each=${f.bars}>${(h) => html`<span ref=${share(h)}></span>`}<//></div><//>
      </div>`}<//>
    </div>
    <h3>Per day</h3>
    <div class="an-chart" id="an-chart">
      <${For} each=${() => m().chart}>${(d) => html`<div class="an-day" title=${d.title}>
        <div class="an-bars"><${For} each=${d.bars}>${(b) => html`<span class=${b.cls} ref=${share(b.height)}>${b.text}</span>`}<//></div>
        <div class="an-date">${d.label}</div>
      </div>`}<//>
      <${Show} when=${() => m().chart.length > 0}><svg class="an-line" viewBox=${() => m().viewBox} preserveAspectRatio="none" aria-hidden="true"><polyline points=${() => m().points} vector-effect="non-scaling-stroke"></polyline></svg><//>
    </div>
    <p class="muted an-legend"><span class="an-key shipped"></span> shipped <span class="an-key failed"></span> failed runs <span class="an-key minutes"></span> cycle-minutes</p>
    <div class="cols">
      <section><h3>Top stop reasons</h3><p class="muted" id="an-reasons-note">${() => m().reasonsNote}</p><ul class="issues" id="an-reasons">
        <${For} each=${() => m().reasons}>${(r) => html`<li><span class="num">${String(r.n)}</span><span>${r.reason}</span></li>`}<//>
      </ul></section>
      <section><h3>Station yield</h3><p class="muted" id="an-steps-note">${() => m().stepsNote}</p><ul class="issues" id="an-steps">
        <${For} each=${() => m().steps}>${(s) => html`<li class="an-step"><span>${s.step}</span><span class="an-yield"><span ref=${share(s.width)}></span></span><span class="when">${s.text}</span></li>`}<//>
      </ul></section>
    </div>
    <h3>Versions</h3>
    <p class="muted" id="an-versions-note">${() => m().versionsNote}</p>
    <div class="tablewrap"><table>
      <thead><tr><th>tehas</th><th>pi-rukas</th><th>runs</th><th>to a PR</th><th>median time</th><th>median tokens</th></tr></thead>
      <tbody id="an-versions"><${For} each=${() => m().versions}>${(row) => html`<tr><${For} each=${row}>${(cell) => html`<td>${cell}</td>`}<//></tr>`}<//></tbody>
    </table></div>
  </section><//><//>`;
});
