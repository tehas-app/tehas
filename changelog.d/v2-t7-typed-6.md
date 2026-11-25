### Changes

- The MCP server (`tehas-mcp`) refuses a request whose id is an object, a list or a boolean as an Invalid Request (-32600, status 400, answered with the null id), as JSON-RPC 2.0 allows only a string, a number or null. Before, such an id was echoed back (#3201).
