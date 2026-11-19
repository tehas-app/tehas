import { Errored, For, Loading, createEffect, createMemo, createRoot, getOwner, html, onCleanup, runWithOwner } from "../kit/vendor/solid.js";
import { feed } from "../kit/feed.js";
import { now, view } from "../kit/parts.js";
import { SP } from "./parts.render.js";
import { hostModel } from "./tehas-factory-host.model.js";

// The file is asked for every ten seconds while it is there, and once a
// minute while it is not: the minute is this island's own period, and a
// second reader asks for ten seconds while the last answer was good. A host
// that publishes nothing answers 404 every time.
const SOON = 10000, LATER = 60000;

export const mount = view((props, ctx) => {
  const live = feed(props.src, { every: LATER });
  const owner = getOwner();
  let fast = null;
  const stop = () => {
    fast?.();
    fast = null;
  };
  const good = createMemo(() => live.error() == null);
  createEffect(() => good(), {
    effect: (ok) => {
      if (!ok) stop();
      else if (!fast) fast = runWithOwner(owner, () => createRoot((dispose) => (feed(props.src, { every: SOON }), dispose)));
    },
    error: stop,
  });
  onCleanup(stop);
  const m = createMemo(() => hostModel(live.data(), now()));
  const row = (r) => html`<li><span>${() => r().name}</span>${SP}<span class="when">${() => r().text}</span></li>`;
  return html`<${Errored} fallback=${(_e) => ctx.first}><${Loading} fallback=${() => ctx.first}><section id="host" hidden=${() => !m().present}>
    <h2>Host${SP}<span class="n" id="n-host">${() => m().n}</span></h2>
    <p class="muted" id="host-note">${() => m().note}</p>
    <ul class="issues" id="host-list">
      <${For} each=${() => m().rows} keyed=${false} fallback=${html`<li class="empty">No process was read from /proc.</li>`}>${row}<//>
    </ul>
    <h3>Units${SP}<span class="n" id="n-units">${() => m().nUnits}</span></h3>
    <ul class="issues" id="unit-list">
      <${For} each=${() => m().units} keyed=${false} fallback=${html`<li class="empty">systemctl --user gave no units.</li>`}>${row}<//>
    </ul>
  </section><//><//>`;
});
