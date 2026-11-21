### Changes

- Plugins, still being built: `tehas plugin call <name> <tool> --args <json> [--json]` runs one call of a plugin's granted tool and prints its answer, one JSON object of at most 16 KiB. Two calls of a plugin and one of each caller run at once, and a call with `--login` is refused until the gate's access check is in core. The state store moves to schema 23 at the next `tehas update`, which backs it up first: a run of a plugin may be a call, kept with the size of its answer and no login.
