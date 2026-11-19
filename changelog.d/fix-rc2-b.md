### Changes

- `tehas start`, `limit`, `order`, `panel`, `host-message`, `freeze`, `thaw`, `pause` and `drain` stop with 78 before they write anything or call GitHub when `TEHAS_REPO`, `TEHAS_CONTROL_ISSUE` or `TEHAS_ALLOWED_LABELERS` is missing or malformed, as their exit tables say.
- A command whose reader goes away exits 141 in every case now: `tehas top --once`, `tehas top --json`, `tehas check-product`, `tehas init-product` and the help texts did not.
- `tehas refresh` exits 78 when `tehas-dispatch` is not installed or cannot run, where it exited 127 or 126.
- `tehas order` exits 78 when the state store is not installed or cannot be used, as `tehas history` does, where it exited 1. Its exit table names the store, not a file.
- `api/order.schema.json` lists `unreadable` among the states of `tehas order --json`, with a sample.
- `tehas order set --json` and `tehas order clear --json` are in the help text and on the public surface.
- `tehas mcp key` and `tehas mcp serve` refuse any other word with 64. `tehas mcp key --bogus-flag` printed a key.
