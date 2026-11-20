import { Errored, For, Loading, Show, createMemo, html } from "../kit/vendor/solid.js";
import { feed } from "../kit/feed.js";
import { now, view } from "../kit/parts.js";
import { SP } from "./parts.render.js";
import { previewsModel } from "./tehas-factory-previews.model.js";

// A host without previews has no file: the section stays as the server wrote it, hidden.
export const mount = view((props, ctx) => {
  const live = feed(props.src, { every: 10000 });
  const m = createMemo(() => previewsModel(live.data(), now()));
  return html`<${Errored} fallback=${(_e) => ctx.first}><${Loading} fallback=${() => ctx.first}><section id="previews" hidden=${() => !m().present}>
    <h2>Previews${SP}<span class="n" id="n-previews">${() => m().n}</span></h2>
    <p class="muted" id="previews-note">${() => m().note}</p>
    <ul class="issues" id="preview-list">
      <${For} each=${() => m().rows} keyed=${(r) => r.key} fallback=${html`<li class="empty">No pull request is open.</li>`}>${(r) =>
        html`<li><span class="num">${() => r().num}</span><${Show} when=${() => r().href} fallback=${html`<span>${() => r().title}</span>${SP}<span class="when">${() => r().why}</span>`}><a href=${() => r().href} target="_blank" rel="noopener">${() => r().title}</a><//></li>`}<//>
    </ul>
  </section><//><//>`;
});
