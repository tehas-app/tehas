### Changes

- The MCP server masks the same values as the host: the secret settings and the literal model keys of `models.json`, wherever they stand in a log. The server gets salted sums of them, not the values.
