import { Errored, Loading, createMemo, html } from "../kit/vendor/solid.js";
import { feed } from "../kit/feed.js";
import { view } from "../kit/parts.js";
import { wikiLink } from "./tehas-factory-wikilink.model.js";

// No live/wiki.json, or no build: the link stays hidden.
export const mount = view((props, ctx) => {
  const live = feed(props.src, { every: 60000 });
  const l = createMemo(() => wikiLink(live.data()));
  return html`<${Errored} fallback=${(_e) => ctx.first}><${Loading} fallback=${() => ctx.first}><a id="wiki" class="muted" hidden=${() => !l()} href=${() => (l() ? l().href : null)} title=${() => (l() ? l().title : null)}>wiki</a><//><//>`;
});
