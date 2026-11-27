### Changes

- The plugin manifest reader reads the 2.2 table, and `api/plugin-manifest.schema.json` is the schema of `tehas-plugin.json`. A manifest for a major this tehas does not read is refused with its reason, and `tehas plugin check` exits 3, as it does for any rule the manifest breaks; a refused manifest is never a settings failure.
- `tehas plugin` says a 2.x plugin's views by their ids, and a 2.x manifest is read without a build, so an update fetches, reads and marks it ready without touching docker.

### Adopter action

Third-party and own plugins still shipping a `plugin: 1` manifest are refused until their manifest carries the 2.2 fields; `tehas plugin check <dir>` names each rule a manifest breaks.
