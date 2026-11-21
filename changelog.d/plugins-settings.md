### Changes

- Plugins, a feature still being built: a release refuses `TEHAS_PLUGIN=on`, and a line of `TEHAS_PLUGINS`, `TEHAS_PLUGIN_GRANTS_<NAME>`, `TEHAS_PLUGIN_ACCESS`, `TEHAS_PLUGIN_LIMITS` or `TEHAS_PLUGIN_MODEL_DAY`, with 78, and `tehas api --json` lists `plugins` in `flags` as `false`. This release brings the settings' rules, the reader of a plugin's manifest with `api/plugin-manifest.schema.json`, and `tehas plugin [--json]`, which exits 78 while the feature is off. Nothing is fetched, built or run yet.
- The files in `api/` are dedicated to the public domain under CC0-1.0, in `api/LICENSE`, so a program may copy the schemas and samples freely.
