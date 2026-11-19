import { Errored, Loading, createMemo, html } from "../kit/vendor/solid.js";
import { feed } from "../kit/feed.js";
import { now, view } from "../kit/parts.js";
import { stateModel } from "./tehas-factory-state.model.js";

export const mount = view((props, ctx) => {
  const status = feed(props.src, { every: 10000 });
  const running = feed(props.running, { every: 10000 });
  const m = createMemo(() => stateModel(status.data(), running.data(), now()));
  const none = (_e) => html`<div class="dash-top">
    <span id="state" class="pill idle">loading</span>
    <span id="fresh" class="muted">no status yet: the dispatcher has not written one</span>
    <a id="all" class="muted" href="#">all issues on GitHub</a>
  </div>`;
  return html`<${Errored} fallback=${none}><${Loading} fallback=${() => ctx.first}><div class="dash-top">
    <span id="state" class=${() => m().pill}>${() => m().state}</span>
    <span id="allhands" class="pill pause" title="tehas:all-hands is on the control issue" hidden=${() => !m().allHands}>all hands</span>
    <span id="readonly" class="pill pause" title="TEHAS_READ_ONLY=1: this factory writes nothing to GitHub" hidden=${() => !m().readOnly}>read-only</span>
    <span id="fresh" class="muted">${() => m().fresh}</span>
    <a id="all" class="muted" href=${() => m().all}>all issues on GitHub</a>
  </div><//><//>`;
});
