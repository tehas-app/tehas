import { Errored, For, Loading, Show, createMemo, html } from "../kit/vendor/solid.js";
import { feed } from "../kit/feed.js";
import { now, view } from "../kit/parts.js";
import { links, listen, useBridge } from "./bridge.state.js";
import { Notes, SP } from "./tehas-factory-issues.render.js";
import { readonlyModel } from "./tehas-factory-readonly.model.js";

export const mount = view((props, ctx) => {
  const live = feed(props.src, { every: 10000 });
  const bridge = useBridge();
  listen();
  const m = createMemo(() => readonlyModel(live.data(), now(), links(bridge.state.answer)));
  return html`<${Errored} fallback=${(_e) => ctx.first}><${Loading} fallback=${() => ctx.first}><section id="ro" hidden=${() => !m().on}>
    <h2>Read-only results${SP}<span class="n" id="n-ro">${() => m().n}</span></h2>
    <p class="muted">This factory writes nothing to GitHub. Each cycle's branch stays in its workspace on the host, with the pull request it would have opened.</p>
    <ul class="issues" id="ro-list">
      <${For} each=${() => m().rows} keyed=${false} fallback=${html`<${Show} when=${() => m().on}><li class="empty">Nothing has started yet.</li><//>`}>${(r) =>
        html`<li><span class="num">#${() => r().issue}</span><${Notes} list=${() => r().notes} />${SP}<span>${() => r().text}</span>${SP}<span class="muted">${() => r().where}</span><${Show} when=${() => r().body}><pre class="ro-body">${() => r().body}</pre><//></li>`}<//>
    </ul>
  </section><//><//>`;
});
