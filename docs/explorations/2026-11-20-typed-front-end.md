# Typed front-end code

Outcome of #2588, 2026-11-20. Measured on origin/main at f789b252 in scratch
worktrees. Nothing in the repository was changed.

## The question

Should the front end under `host/fun/` be typed, and how, without giving up
"no build step" (`host/docs/frontend.md`)? Three options were tried:

1. JSDoc types checked by `tsc --checkJs --noEmit`, with a hand-written
   `kit.d.ts`.
2. `.ts` source with erasable syntax only, stripped to `.js` by a Rust tool
   on oxc.
3. Checking with tsrs, the Rust port of TypeScript 7, instead of tsc.

The kit's own JS is 1,108 lines in 7 modules. All front-end JS outside the
kit and vendor is 5,990 lines.

## Research facts

- The TC39 type annotations proposal is still Stage 1. It has not been
  presented in plenary since September 2023. Browsers will not run type
  syntax within any horizon worth planning for.
- Node strips erasable types by default since 23.6 and 22.18, but the
  Ubuntu apt Node 22.22.1 here is built without it. None of it reaches the
  browser.
- tehas-obeya already checks its islands with tsc 7.0.2 strict against 165
  lines of hand-written kit `.d.ts` copies. Those copies have drifted:
  `stream.d.ts` says `broken?: "end"` where the kit says `"fail" | "end"`.
- tehas-industry's solid-ui branch runs the native tsc 7.0.2 binary from a
  Rust xtask, with no Node. It is non-strict and ignores errors under
  `vendor/tehas/`.
- The Solid plan (`docs/plans/2026-11-19-solid.md` section 10) promised a
  `solid.d.ts` and kit JSDoc. Neither was built.

## Option 1: JSDoc and tsc --checkJs

Tried: strict check of the kit and the dashboard with tsc 7.0.2, then
annotating `feed.js` and `island.js` and writing a 105-line `kit.d.ts`.

- Raw kit: 2,255 errors, 2,075 in minified `vendor/solid.js`. With a
  `solid.d.ts`: 181. After annotating feed and island: 110, all in the five
  modules not annotated.
- Dashboard: 974 strict errors in its own files, mostly implicit-any
  parameters and possibly-null DOM lookups. Typing the kit did not change
  that number.
- Real bugs found: none. Every non-implicit-any error was read.
- A probe with 5 deliberate consumer mistakes: 2 caught before, 5 after.
- Runtime unchanged: whitespace-minified output is byte-identical, and 140
  of 140 node tests pass.
- The native Go binary, with no Node on PATH: 0.39 s for the kit, under
  0.5 s for the dashboard.

Costs:

- JSDoc is served bytes. Two modules added 1,800 bytes (502 gzipped). That
  fails the `pages.max` dashboard line (85,632 > 83,844) and takes the core
  kit modules from 24,493 to 26,283 bytes, over the 24,576 review figure.
- MANIFEST changes for the two files and the new `.d.ts` files.
- The asset scanner reads comments and counts `./kit.js` in a JSDoc import
  as an address. Writing `./kit.d.ts` passes both the scanner and tsc.
- Obeya can drop its copies only once all 7 modules check strict-clean.
  checkJs reports errors in every imported `.js`.
- The gate is offline, so `npx` is out. A pinned binary found on PATH,
  skipped in the fast gate and required in `--full`, follows industry's
  model. `scripts/verify.sh` is a person's file.

## Option 2: erasable .ts, stripped by oxc

Tried: `request.js` and `state.js` converted to `.ts`, a Rust strip tool on
oxc 0.158.0 (already in the vendor-solid lock) in two modes: blanking types
to spaces, and oxc's transformer plus codegen.

- Neither mode gave output byte-identical to today's `.js`. Both gave a
  semantically identical program, and 119 of 119 node tests passed.
- Blanking keeps lines and columns and returned 50 of 50 plain `host/fun`
  files unchanged. Codegen reflows code, drops a header comment, and would
  re-hash the kit on every oxc change.
- About 2 ms per file. The blank-only binary adds about 0.84 MB and 65
  crates.
- oxc compiled an enum and a parameter property silently. Only tsc's
  `--erasableSyntaxOnly` refused them.
- The prototype blanker produced a SyntaxError on a `declare` field and on a
  multi-line arrow return type.
- The type check found one TS2345 in `state.ts`: `actionNames`
  (`state.js:47`) calls `spec.actions` with a stand-in `{}` state. A second
  `pageStore` of the same name whose actions read state while being made
  throws a TypeError instead of "defined twice". No current caller does
  that, so it is a latent edge, not a live bug.

Costs: stripping at install or pack time breaks "no build step" and the
copy-at-a-tag contract with Obeya and industry. Committing both `.ts` and
`.js`, as `tools/vendor-solid` does, keeps them, but needs a gate check that
`.js` equals the stripped `.ts`, with its 65 crates. The check itself still
needs tsc.

## Option 3: tsrs

- Output was byte-identical to tsc 7.1.0-beta in all 8 configurations.
- Whole `host/fun` strict: tsrs 80 ms, tsc 7.1 366 ms, tsc 7.0.2 382 ms,
  tsc 5.9.3 1,809 ms. Peak memory 34 MB against 67 MB for tsc 7.0.2.
- It ships as a crate (`tsrs_cli`, needs cmake and Rust 1.100; the
  workspace declares 1.89) and as native npm binaries. The crate binary runs
  alone with no Node.
- Every release is a prerelease against a TypeScript beta.

tsrs unlocks nothing that tsc 7's own Go binary does not, and the gate
still needs Node for `node --test`.

## Recommendation

Take option 1, scoped to the kit. Annotate all 7 kit modules with JSDoc,
ship `kit.d.ts` and `vendor/solid.d.ts` in the MANIFEST, and check the kit
strict with a pinned tsc 7 binary from a Rust test. Do not check the
dashboard yet. The value is an accurate kit API for Obeya, industry and
agents, not bug finding: no real bug was found. Skip option 2: it adds a
second file per module and a strip check for nicer syntax. Revisit tsrs
when it ships against a stable TypeScript.

A person has to decide:

1. Whether to accept the served-byte growth, which needs a `pages.max` raise
   and a new kit review figure. The other way is sibling `.d.ts` files with
   no JSDoc, but then the kit's JS is not checked against them.
2. Whether to add the step to `scripts/verify.sh`.

Smallest next step: a pull request that adds `vendor/solid.d.ts` and
`kit.d.ts` to the kit and its MANIFEST, with no JSDoc in the `.js`, so
Obeya can test them against its copies. It changes no served page.
