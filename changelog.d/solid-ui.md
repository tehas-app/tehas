### Changes

- The front end's kit, `host/fun/kit/` (`kit 1`), now carries Solid 2 as one vendored module, `kit/vendor/solid.js` (solid-js 2.0.3, @solidjs/web 2.0.3, @solidjs/signals 2.0.3, @solidjs/html 2.0.1, MIT), with the island base, the feed store, page stores shared by the islands of a page, and shared parts. No page loads it yet, so nothing a viewer sees changes. `host/docs/frontend.md` describes the split, the island contract and the Solid 2 rules.
- The byte ratchet counts the library once, on a `library solid` line of `host/fun/pages.max`, and in no page. It fails when the kit's `MANIFEST` does not match its files, on Solid 1.x names in `host/fun`, on a library import outside a `.render.js` file and the kit files that draw, and on `html` templates built from strings or with a `style=` attribute.
- The docs server sends `Cache-Control: no-cache` for every file under `/fun/` whose name holds no content hash: the theme, fonts, images and the kit's modules. Before, those had no cache header and a browser could reuse an old copy after an update.

### Adopter action

A program that builds pages on the kit copies `host/fun/kit/` whole at the tehas tag it pins, checks every file against `kit/MANIFEST` (`file <sha256> <bytes> <name>`), and does not edit it. Its render modules import Solid by a relative address to `kit/vendor/solid.js`, with no import map, and only the names `host/docs/frontend.md` lists under "Solid 2 in tehas". When it updates the pin, it copies the kit again and checks the manifest.
