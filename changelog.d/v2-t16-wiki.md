### Changes

- The product wiki left core. It was still being built and a release refused it; it returns as a plugin in 2.1. `tehas wiki`, `tehasbot wiki`, the `/wiki` page and the dashboard's wiki link, `live/wiki.json`, the units `tehas-wiki.service`, `tehas-wiki.timer`, `tehas-wiki.path`, `tehas-wiki-status.service` and `tehas-wiki-status.timer`, the sandbox kind `wiki` and `"wiki"` in the `flags` of `tehas api --json` are gone. A `TEHAS_WIKI` line still loads: it is not read, and every command warns of it once.

### Adopter action

Take any `TEHAS_WIKI` line out of `instance.env`. `tehas update` stops and removes the wiki's units on a host that has them.
