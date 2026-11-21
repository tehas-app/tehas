### Changes

- Plugins, still being built: the new setting `TEHAS_PLUGIN_GATE_ADDR` (default `127.0.0.1:8095`) is the address and port of the plugins' gate, `tehas-plugin-gate`, which serves a plugin's view behind a GitHub sign-in. A line of it stops every command with 78 while the feature is being built. Nothing installs or starts the gate yet (#2633).
