### Changes

- The plugins' hook after a cycle is named by its event: the grant and the ask are `hook:after:change.offered`, and a manifest's hook is at `after:change.offered`. `hook:after:work` and `after:work` are refused, saying the new word; no alias is kept. Plugins stay a feature still being built.

### Adopter action

A plugin that asked `hook:after:work` asks `hook:after:change.offered` and puts its hook at `after:change.offered`. A `TEHAS_PLUGIN_GRANTS_<NAME>` line that granted `hook:after:work` grants `hook:after:change.offered` for the plugin's commit.
