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
- `PI_ENSEMBLE_HOST_ALIASES` is a ruled setting: each comma-separated entry
  must be `name:address`, the address an IPv4 or IPv6 address or
  `host-gateway`. Any other value stops the load with exit 78, and no
  container is started with it.
- `tehas disk` and `tehas setup` refuse a value they read from a settings file
  that a shell would expand (a `$` or a backtick outside single quotes), with a
  line that names the key and not its value. Before, they took it as written
  while the shell that sources the same file expanded it.
- `TEHAS_WORK_REPO` is a ruled setting: an absolute path, else the load stops
  with exit 78. Every other `TEHAS_*` name tehas reads is internal and listed
  with why, and a test holds new names to a rule or to that list.

### Adopter action

If a command now stops with
`PI_ENSEMBLE_HOST_ALIASES is not valid`, write each entry as
`name:address`, such as `models.example:192.0.2.10`.
