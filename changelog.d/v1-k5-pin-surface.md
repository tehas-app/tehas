### Changes

- The public surface is pinned in the repository: `api/commands.txt` lists every `tehas` command, its subcommands and the flags of each, with the flags only a unit or the dashboard writes marked internal; `api/api.schema.json` with samples is the schema of `tehas api --json`, and `api/features.txt` holds the words of its `features`. The gate fails a command, a subcommand, a flag or a feature word that changes without them.
