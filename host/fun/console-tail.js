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

// file: a name under /live/. offset: how many bytes the page already holds.
// { text, size } for what the file holds from offset on, "replaced" when the
// answer is a whole file rather than a piece of one - a new cycle wrote over
// the old console, so what the page holds is of no use - "ended" for a range
// past the end of the file, and null for any other answer or no answer at all.
// A page that asked for one file and was shown another while the request was
// out drops the answer itself: tailStep names nothing of the page's.
export async function tailStep(file, offset) {
  try {
    const r = await fetch("/live/" + file, {
      headers: { Range: "bytes=" + offset + "-" },
      cache: "no-store",
    });
    // A piece of the file, or the whole of a file that was not there at all.
    if (r.status === 206 || (r.status === 200 && offset === 0)) {
      const bytes = new Uint8Array(await r.arrayBuffer());
      return { text: new TextDecoder().decode(bytes), size: bytes.length };
    }
    if (r.status === 200) return "replaced";
    if (r.status === 416) return "ended";
    return null;
  } catch {
    return null;
  }
}

// The answer's text after what the console holds, cut to the newest KEEP
// characters once it passes CAP. pre: the element that shows the console.
export function appendCapped(pre, text) {
  pre.append(text);
  if (pre.textContent.length > CAP) pre.textContent = pre.textContent.slice(-KEEP);
}
