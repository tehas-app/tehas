// Small pieces drawn in Solid that islands share (host/docs/frontend.md, "The
// kit"). Props are read where they are used, so a getter stays reactive.
//
//   export const mount = view((props, ctx) => html`…`);
//   html`<${TimeAgo} at=${() => item.updatedAt} />`
//   html`<${Empty} text="Nothing in line" />`
//   html`<${Busy} when=${() => isPending(() => live.data())} />`
import {
  Errored,
  Loading,
  createSignal,
  createStore,
  html,
  onCleanup,
  render,
  untrack,
} from "./vendor/solid.js";
import { shared } from "./state.js";

/**
 * An island's render module from a component: `mount(host, ctx)` draws
 * `Component(props, ctx)` into `host` in a Solid root of its own. `props` is
 * a store of the element's attributes that `update` writes; `unmount` is the
 * root's disposer.
 */
export function view(Component) {
  return (host, ctx) => {
    const [props, setProps] = createStore({ ...ctx.props });
    const unmount = render(() => untrack(() => Component(props, ctx)), host);
    const update = (next) =>
      setProps((d) => {
        for (const k of Object.keys(d)) if (!(k in next)) delete d[k];
        Object.assign(d, next);
      });
    return { update, unmount };
  };
}

const UNITS = [
  ["year", 365 * 24 * 3600],
  ["month", 30 * 24 * 3600],
  ["week", 7 * 24 * 3600],
  ["day", 24 * 3600],
  ["hour", 3600],
  ["minute", 60],
  ["second", 1],
];

/** "5 minutes ago" for a time in ms against `nowMs`, in `locale`. */
export function ago(atMs, nowMs, locale) {
  if (!Number.isFinite(atMs) || !Number.isFinite(nowMs)) return "";
  const s = Math.round((atMs - nowMs) / 1000);
  const words = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  const [unit, size] = UNITS.find(([, n]) => Math.abs(s) >= n) ?? UNITS[6];
  return words.format(Math.trunc(s / size), unit);
}

/** A time as ms: a number stays, a string is parsed. */
export const toMs = (at) => (typeof at === "number" ? at : Date.parse(at));

/** The page's clock in ms, a signal that moves every 30 s. */
export function now() {
  return shared("kit:clock", () => {
    const [read, set] = createSignal(Date.now());
    const id = setInterval(() => set(Date.now()), 30000);
    onCleanup(() => clearInterval(id));
    return read;
  })();
}

const lang = () => globalThis.document?.documentElement?.lang || undefined;

export function TimeAgo(props) {
  const ms = () => toMs(props.at);
  return html`<time class="tehas-ago" datetime=${() =>
    Number.isFinite(ms()) ? new Date(ms()).toISOString() : ""}>${() => ago(ms(), now(), lang())}</time>`;
}

export function Empty(props) {
  return html`<p class="tehas-empty">${() => props.text}</p>`;
}

export function Busy(props) {
  return html`<span class="tehas-busy" role="status" hidden=${() => !props.when}>${() =>
    props.label ?? "Updating"}</span>`;
}

/** The message of an error, or of the error accessor <Errored> passes. */
export function message(error) {
  const e = typeof error === "function" ? error() : error;
  return e instanceof Error ? e.message : String(e ?? "");
}

export function Failed(props) {
  return html`<div class="tehas-failed" role="alert">
    <p>${() => message(props.error)}</p>
    <button type="button" class="tehas-retry" onClick=${(e) => props.retry?.(e)}>${() =>
      props.label ?? "Try again"}</button>
  </div>`;
}

/**
 * <Loading> inside <Errored>: `fallback` until the data is in, then the
 * children; a failure shows <Failed>, whose button calls `retry` and resets.
 */
export function Guard(props) {
  return html`<${Errored} fallback=${(err, reset) =>
    html`<${Failed} error=${() => err()} retry=${(e) => {
      props.retry?.(e);
      reset();
    }} />`}>
    <${Loading} fallback=${() => props.fallback ?? html`<${Busy} when=${true} label="Loading" />`}>${() =>
      props.children}<//>
  <//>`;
}
