import { Errored, For, Loading, createMemo, createSignal, html } from "../kit/vendor/solid.js";
import { feed } from "../kit/feed.js";
import { now, view } from "../kit/parts.js";
import { consoleModel } from "./tehas-factory-console.model.js";
import "./tehas-factory-tail.js";

// The arrow keys move between the tabs and select, as in any tab list.
const MOVES = { ArrowRight: 1, ArrowLeft: -1 };

export const mount = view((props, ctx) => {
  const running = feed(props.src, { every: 10000 });
  const status = feed(props.status, { every: 10000 });
  const [selected, select] = createSignal(null);
  const m = createMemo(() => consoleModel(running.data(), status.data(), selected(), now()));
  let row;
  const keys = (event) => {
    const tabs = [...row.children], at = tabs.indexOf(document.activeElement);
    if (at < 0) return;
    const to = event.key === "Home" ? 0 : event.key === "End" ? tabs.length - 1 : MOVES[event.key] === undefined ? null : at + MOVES[event.key];
    if (to === null) return;
    event.preventDefault();
    const next = tabs[(to + tabs.length) % tabs.length];
    next.focus();
    select(Number(next.dataset.issue));
  };
  return html`<${Errored} fallback=${(_e) => ctx.first}><${Loading} fallback=${() => ctx.first}><section class="now" id="now" hidden=${() => !m().shown}>
    <div class="console-tabs" id="tabs" role="tablist" aria-label="Cycles" ref=${(el) => (row = el)} hidden=${() => m().tabs.length < 2} onKeyDown=${keys}>
      <${For} each=${() => m().tabs} keyed=${(t) => t.issue}>${(t) =>
        html`<button type="button" role="tab" data-issue=${() => t().issue} aria-selected=${() => String(t().on)} tabindex=${() => (t().on ? 0 : -1)} onClick=${() => select(t().issue)}>${() => t().label}</button>`}<//>
    </div>
    <h2 id="now-title">${() => m().title}</h2>
    <div class="step" id="now-step">${() => m().step}</div>
    <tehas-factory-tail file=${() => m().file} live=${() => m().live}>
      <pre id="console" role="tabpanel" aria-label="Console of the selected cycle" tabindex="0"></pre>
      <div class="console-bar">
        <label><input type="checkbox" id="follow" checked /> follow the output</label>
        <span class="muted" id="console-note"></span>
      </div>
    </tehas-factory-tail>
  </section><//><//>`;
});
