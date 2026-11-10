// The console of a cycle, read a piece at a time: one Range request for what a
// page has not seen yet, and the rule that keeps a long cycle's text from
// eating the page. The plain dashboard (dashboard-view.js), the business page
// (business-view.js) and the factory floor (game-view.js) each tail a cycle's
// console of their own; only this step was the same in the three of them
// (test/console_tail_test.mjs).
//
// What a page keeps for itself is the rest: when to ask (the business page and
// the floor ask nothing while the tab is hidden or an ended cycle has been read
// to its end), what to do with the answer, and the note beside the console.
// Console text is untrusted: it reaches the page through appendCapped, which
// appends text, and never as markup.
export const CAP = 400000; // where the console's text is cut
export const KEEP = 300000; // what is kept: the newest of it
const TIMEOUT = 10000; // how long one answer may take to come at all
// The name of a console, as console-tabs.js reads it out of a cycle. The same
// test again, because this is where the name becomes a path: a caller that
// forgot to check would otherwise ask for anything under the same origin.
const NAME = /^[\w-][\w.-]*$/;

// { kind } names what the answer was, so a page reads one field rather than
// guessing at a type. chunk: text and the bytes of it; replaced: a whole file
// rather than a piece of one, so what the page holds is of no use; ended: a
// range past the end of the file; none: an answer the page ignores; failed: no
// answer, or one that came too late, which a page says beside its console.
// A page that asked for one file and was shown another while the request was
// out drops the answer itself: tailStep names nothing of the page's.
//
// file: a name under /live/, checked here. offset: how many bytes the page
// already holds.
export async function tailStep(file, offset) {
  if (typeof file !== "string" || !NAME.test(file)) return { kind: "none" };
  const stop = new AbortController();
  const timer = setTimeout(() => stop.abort(), TIMEOUT);
  try {
    const r = await fetch("/live/" + file, {
      headers: { Range: "bytes=" + offset + "-" },
      cache: "no-store",
      signal: stop.signal,
    });
    // A piece of the file, or the whole of a file that was not there at all.
    if (r.status === 206 || (r.status === 200 && offset === 0)) {
      const bytes = new Uint8Array(await r.arrayBuffer());
      return { kind: "chunk", text: new TextDecoder().decode(bytes), size: bytes.length };
    }
    if (r.status === 200) return { kind: "replaced" };
    if (r.status === 416) return { kind: "ended" };
    return { kind: "none" };
  } catch {
    return { kind: "failed" };
  } finally {
    clearTimeout(timer);
  }
}

// The answer's text after what the console holds, cut to the newest KEEP
// characters once it passes CAP. pre: the element that shows the console.
export function appendCapped(pre, text) {
  pre.append(text);
  if (pre.textContent.length > CAP) pre.textContent = pre.textContent.slice(-KEEP);
}
