### Changes

- The MCP server's cycle `status` lists exactly the words a cycle can carry: it adds `not-started`, `stopped`, `parked` and `failed`, and drops `completed`, which no cycle said. A word outside the list is served as `unknown`.
