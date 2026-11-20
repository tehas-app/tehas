### Changes

- The kit's seven modules carry JSDoc types and check clean under `tsc --checkJs --strict`, held by a workspace test with a pinned tsc 7.0.2. A program that copies the kit gets its types from the modules and `vendor/solid.d.ts`, with no copies of its own. The dashboard's script grows by 5091 bytes, nearly all of them comments (#2629).
- A second `pageStore` of the same name whose actions read the state while they are made now gets the store, where it threw a TypeError (#2629).
