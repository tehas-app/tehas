import { action, createOptimistic, createSignal, html } from "../fun/kit/vendor/solid.js";
import { feed } from "../fun/kit/feed.js";
import { Guard, message, view } from "../fun/kit/parts.js";
import { press } from "../fun/kit/press.js";
import { stream } from "../fun/kit/stream.js";
import { useWire } from "./wire.state.js";
import { echoText, endText, eventsText, heardText, linesText, planText } from "./kit-demo-wire.model.js";

export const mount = view((props) => {
  const wire = useWire();
  const headers = () => ({ "X-Kit-Session": props.session });
  const echo = feed("/api/echo", { headers, expose: ["X-Kit-Seen"] });
  const plan = feed("/api/plan", { accept: { 404: { releases: [] } } });
  const events = stream("/api/events", { headers, on: wire.actions.event, retry: 200 });
  const lines = stream("/api/lines", { format: "ndjson", headers, on: wire.actions.line, retry: 200 });
  const votes = feed("/api/votes");
  // Ends for good on a refusal, and on an answer an EventSource would not read.
  const refused = stream("/api/refused", { end: [401], retry: 200 });
  const eventSource = (status, type) => status !== 200 || type !== "text/event-stream";
  const wrong = stream("/api/wrongtype", { end: eventSource, retry: 200 });
  // Retried with its body on the error; asked again at once after a clean end.
  const flaky = stream("/api/flaky", { retry: 200, grow: "fixed", ended: 0, parse: "text", on: wire.actions.flaky });
  const count = feed("/api/count");
  const count2 = feed("/api/count");
  const [heard, setHeard] = createSignal("");
  const ask = (_e) => count.refresh().then((r) => setHeard(heardText(r)));
  const give = (_e) => count2.put({ count: 99 });
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
    <p class="refused">${() => endText(refused.state(), refused.status(), refused.error())}</p>
    <p class="wrong">${() => endText(wrong.state(), wrong.status(), wrong.error())}</p>
    <p class="flaky">${() => `${endText(flaky.state(), flaky.status(), flaky.error())} | ${wire.state.flaky.join(" ")}`}</p>
    <${Guard}>
      <p><span class="count">${() => count.data().count}</span>/<span class="count2">${() => count2.data().count}</span>
        <button type="button" class="ask" onClick=${ask}>Ask</button>
        <button type="button" class="give" onClick=${give}>Give</button>
        <span class="heard">${() => heard()}</span></p>
    <//>
  </div>`;
});
