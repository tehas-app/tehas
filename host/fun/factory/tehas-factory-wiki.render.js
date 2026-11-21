import { Errored, Loading, Show, createMemo, html } from "../kit/vendor/solid.js";
import { feed } from "../kit/feed.js";
import { now, view } from "../kit/parts.js";
import { wikiModel } from "./tehas-factory-wiki.model.js";

// The server answers /wiki only while live/wiki.json is there, so the page is
// drawn only with a file to read.
export const mount = view((props, ctx) => {
  const live = feed(props.src, { every: 30000 });
  const m = createMemo(() => wikiModel(live.data(), now()));
  return html`<${Errored} fallback=${(_e) => ctx.first}><${Loading} fallback=${() => ctx.first}><section id="wiki" hidden=${() => !m().present}>
    <p id="wiki-built">${() => m().built}</p>
    <p id="wiki-behind">${() => m().behind}</p>
    <p class="muted" id="wiki-main">${() => m().main}</p>
    <${Show} when=${() => m().href}><p><a id="wiki-open" href=${() => m().href}>Open the wiki: ${() => m().id}</a></p><//>
  </section><//><//>`;
});
