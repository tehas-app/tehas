### Changes

- The Protolab can take a zip named by address and sha256, `Zip: <address> sha256:<hex>`, from the hosts of the new `TEHAS_PROTOLAB_ZIP_HOSTS`, empty by default. Bytes of another sha256 are refused, redirects are followed only on those hosts, and the zip is held to the limits of an attached one. The feature `protolab-zip-url` is still being built: `TEHAS_PROTOLAB_ZIP_URL=on` stops every command with 78, and `tehas api --json` lists it in `flags` as `false`.
