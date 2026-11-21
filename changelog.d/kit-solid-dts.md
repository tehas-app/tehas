### Changes

- The kit ships `vendor/solid.d.ts`, types for the names its Solid exports, and lists it in `kit/MANIFEST`. A program that copies the kit can check its pages with `tsc --checkJs` against it. An install does not copy a `.d.ts` file into the home, so no page changes (#2629).
