import { action, createOptimistic, createSignal, html } from "../fun/kit/vendor/solid.js";
import { feed } from "../fun/kit/feed.js";
import { Guard, message, view } from "../fun/kit/parts.js";
import { press } from "../fun/kit/press.js";
import { stream } from "../fun/kit/stream.js";
import { useWire } from "./wire.state.js";
import { echoText, eventsText, linesText, planText } from "./kit-demo-wire.model.js";

export const mount = view((props) => {
  const wire = useWire();
  const headers = () => ({ "X-Kit-Session": props.session });
  const echo = feed("/api/echo", { headers, expose: ["X-Kit-Seen"] });
  const plan = feed("/api/plan", { accept: { 404: { releases: [] } } });
  const events = stream("/api/events", { headers, on: wire.actions.event, retry: 200 });
  const lines = stream("/api/lines", { format: "ndjson", headers, on: wire.actions.line, retry: 200 });
  const votes = feed("/api/votes");
  const [shown, setShown] = createOptimistic(() => votes.data().count);
  const [failed, setFailed] = createSignal("");
  const vote = action(function* () {
    setShown((n) => n + 1);
    yield press("/api/vote", { headers, body: { by: 1 } });
    votes.refresh();
  });
  const press1 = (_e) => {
    setFailed("");
    vote().catch((e) => setFailed(message(e)));
  };
  return html`<div>
    <${Guard} retry=${(_e) => echo.refresh()}>
      <p class="echo">${() => echoText(echo.data(), echo.meta())}</p>
      <p class="plan">${() => planText(plan.data(), plan.meta())}</p>
    <//>
    <p class="events">${() => eventsText(wire.state.events, events.state())}</p>
    <p class="lines">${() => linesText(wire.state.lines, lines.state())}</p>
    <${Guard} retry=${(_e) => votes.refresh()}>
      <p><span class="votes">${() => shown()}</span> <button type="button" class="vote" onClick=${press1}>Vote</button></p>
    <//>
    <p class="failed">${() => failed()}</p>
  </div>`;
});
