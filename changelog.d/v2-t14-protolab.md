### Changes

- The Protolab is removed. `tehasbot protolab` and its units `tehas-protolab.timer` and `tehas-protolab.service`, `live/protolab.json`, `tehas prototype file`, the `/tools-protolab` page and `"protolab-zip-url"` and `"prototype-file"` in the `flags` of `tehas api --json` are gone. Its plumbing stays under neutral names: the store of an item's files and its page, served by the edge router at `item-<n>.<domain>` from `~/tehas/items`. A `TEHAS_PROTOLAB_URL`, `TEHAS_PROTOLAB_ZIP_URL`, `TEHAS_PROTOLAB_ZIP_HOSTS` or `TEHAS_PROTOTYPE_FILE` line still loads: it is not read, and every command warns of it once.

### Adopter action

Take any `TEHAS_PROTOLAB_URL`, `TEHAS_PROTOLAB_ZIP_URL`, `TEHAS_PROTOLAB_ZIP_HOSTS` or `TEHAS_PROTOTYPE_FILE` line out of `instance.env`. `tehas update` stops and removes the Protolab's units and removes `~/tehas/protolab`. A DNS name `lab-<n>.<domain>` is now `item-<n>.<domain>`.
