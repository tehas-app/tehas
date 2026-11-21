### Changes

- Plugins, still being built: `tehas update` fetches, checks and builds each plugin when the feature is on, refuses and skips one that breaks a rule, and removes a plugin taken out of `TEHAS_PLUGINS`. `tehas plugin review <name> [<commit>] [--json]` shows what a commit asks for and the lines to grant it, with `api/plugin-review.schema.json`. Both exit 78 while the feature is off, and so does every command with it on.
- The state store moves to schema 19 with an empty `plugin_runs` table. `tehas update` migrates it as before.
