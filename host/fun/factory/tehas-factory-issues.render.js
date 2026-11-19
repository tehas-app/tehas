import { Errored, For, Loading, Show, createMemo, html } from "../kit/vendor/solid.js";
import { feed } from "../kit/feed.js";
import { now, view } from "../kit/parts.js";
import { links, listen, useBridge } from "./bridge.state.js";
import { issuesModel } from "./tehas-factory-issues.model.js";

// A space between two elements, which a template drops.
export const SP = " ";

// A host's notes on a ticket, as chips after a space; nothing when there are none.
export function Notes(props) {
  return html`<${Show} when=${() => props.list.length > 0}>${SP}<span class="host-notes"><${For} each=${() => props.list} keyed=${false}>${(c) =>
    html`<${Show} when=${() => c().href} fallback=${html`<span class="host-note">${() => c().text}</span>`}><a class="host-note" href=${() => c().href} target=${() => (c().away ? "_blank" : null)} rel=${() => (c().away ? "noopener noreferrer" : null)}>${() => c().text}</a><//>`}<//></span><//>`;
}

export const mount = view((props, ctx) => {
  const live = feed(props.src, { every: 10000 });
  const bridge = useBridge();
  listen();
  const m = createMemo(() => issuesModel(live.data(), props, now(), links(bridge.state.answer)));
  return html`<${Errored} fallback=${(_e) => ctx.first}><${Loading} fallback=${() => ctx.first}><section>
    <h2>${() => m().title}${SP}<span class="n" id=${() => "n-" + m().name}>${() => m().n}</span></h2>
    <ul class="issues" id=${() => m().name}>
      <${For} each=${() => m().rows} keyed=${(r) => r.number} fallback=${html`<li class="empty">${() => m().empty}</li>`}>${(r) =>
        html`<li><span class="num">#${() => r().number}</span><a href=${() => r().href}>${() => r().title}</a><${Notes} list=${() => r().notes} />${SP}<span class="when">${() => r().when}</span></li>`}<//>
      <${Show} when=${() => m().more}><li class="empty"><a class="more" href=${() => m().more.href}>${() => m().more.text}</a></li><//>
    </ul>
  </section><//><//>`;
});
