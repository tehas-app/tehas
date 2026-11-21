### Changes

- Plugins, still being built: a plugin's tool can be offered on the MCP server to full keys. Grant it `mcp:<id>` beside its `tool:<id>`. The broker of these calls, `tehas-plugin-calls.service`, runs each call as `tehas plugin call` does, at most 60 calls a minute and one at a time for each key. A dashboard key never sees a plugin tool. See the MCP server's page, Plugin tools.
