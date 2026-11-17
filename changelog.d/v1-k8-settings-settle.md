### Changes

- The dispatcher's poll reads `TEHAS_PLAN_TIMEOUT` and `TEHAS_PLAN_MAX_TURNS`
  by the rules the settings load uses. A value it cannot read stops the poll
  with exit 78 before the lock and before any call, instead of becoming the
  default (#2233).
- A limit file whose `cycles` is not plain digits, such as `1.0` or `01`,
  reads as `unreadable` everywhere. Before, the status said `ok` while the
  dispatcher read it as 0 cycles and started nothing.
- `TEHAS_EGRESS_HOSTS` has one rule for the settings check and the proxy's
  list. A word the check refuses, such as `a_b.example.com` or `x.org:0443`,
  can no longer reach the proxy by another reading.
