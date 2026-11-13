### Changes

- The issue titles and bodies a cycle posts through the relay are masked as
  the console masks a log, and their `@` mentions notify nobody.
- The mask and the store's check share one list of token shapes: the mask now
  hides Slack tokens, AWS key ids, JWTs and private key blocks, and the check
  also knows a JWT.
- The MCP server's `factory_events` cursor hashes the masked start of a log.
  A cursor from an earlier version is taken once as a replaced log: that log
  is read from its start and named in `gaps`.
- The MCP server closes a connection whose answer is not read within 10
  seconds and one more for each 128 KiB of it.
- The egress proxy sends a plain `http://` request with the `Host` of its
  allowed target, not the one the client sent.
