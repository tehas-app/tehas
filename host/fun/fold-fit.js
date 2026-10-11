// The fold of the game view: how much room the first screen gets above it, and
// where the board starts below it. See DESIGN.md, "Overview".
//
// Nothing here measures or draws, so it is tested without a browser
// (test/fold_fit_test.mjs). fitFold() in game-view.js measures the page and
// writes what these return as --fold-pad, --fold-gap and --monitor-height.
//
// The measurements are offsets the CSS owns: the head panel's border, its
// padding and the lines under it stand between the top of the page and the
// doors, so a change to that panel moves doorsBottom and with it the fold.
(() => {
  const TOP = 32; // --fold-pad when the window has nothing to spare
  const MAX_EXTRA = 200; // the first screen is never pushed further than this
  const SPARE_STEP = 16, PAD_STEP = 8; // the spare read on the grid, half given back
  const GAP = 72; // --fold-gap when the window has nothing to spare
  const CLEAR = 24; // room left under the fold before the board

  // The monitor, stand included, is as tall as the stations and doors beside it.
  // Never negative: an offset taller than the row leaves the monitor with no
  // height rather than a length the CSS would throw away.
  function monitorHeight(lineTop, doorsBottom) {
    return Math.max(0, doorsBottom - lineTop - TOP);
  }

  // innerHeight: the window. doorsBottom: the bottom of the doors at the fold
  // offset they are measured at, so it carries everything above them.
  // Half the spare goes above the first screen and stays below it, which is what
  // centres it: the room above is TOP + extra, the room below is the spare less
  // that same extra. The gap keeps the rest, so the board starts CLEAR under the
  // fold whenever the minimum gap does not already cover it.
  function fit(innerHeight, doorsBottom) {
    const spare = innerHeight - doorsBottom - TOP;
    const extra = Math.max(0, Math.min(MAX_EXTRA, Math.floor(spare / SPARE_STEP) * PAD_STEP));
    return {
      foldPad: TOP + extra,
      foldGap: Math.max(GAP, innerHeight - (doorsBottom + extra) + CLEAR),
    };
  }

  globalThis.TEHAS_FOLD_FIT = { fit, monitorHeight, TOP, GAP };
})();
