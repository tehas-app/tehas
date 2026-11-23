### Changes

- `tehas limit --json` prints `set`, the time the limit was written, kept in `state/limit.json`, and so does the `limit` of `tehas drain --json`. Setting the same limit again keeps it: the same limit is the same `cycles`, `plan` and `reason`, so a renewal that only moves the expiry keeps it too. Another limit takes the time it is written at, and after `tehas limit clear` there is none. A limit set before the update loads as it is and prints no `set` until it changes.
