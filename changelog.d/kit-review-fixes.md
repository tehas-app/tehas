### Changes

- A stream of the front-end kit no longer waits longer than its `max` before it asks again. A server's `retry:` field or an `ended` above a `max` given made it wait that long after a clean end or, with `broken: "end"`, after a break.
