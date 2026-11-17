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
  must be `name:address` or `name=address`, the address an IPv4 or IPv6
  address (bare or in brackets) or `host-gateway`. Any other value stops the
  load with exit 78, and no container is started with it.
- `tehas disk` and `tehas setup` refuse a value they read from a settings file
  that a shell would expand (a `$` or a backtick outside single quotes), with a
  line that names the key and not its value. Before, they took it as written
  while the shell that sources the same file expanded it. For `tehas setup`
  this covers the model key and the instance values it reuses.
- `TEHAS_WORK_REPO` is a ruled setting: an absolute path, else the load stops
  with exit 78.
- The host's own settings have rules too, listed under `host` in
  `api/settings-rules.json` and in a Host settings table of the manual:
  `TEHAS_HOME`, `TEHAS_INSTANCE_DIR`, `TEHAS_MODEL_KEY_<NAME>`, `TEHAS_MCP`,
  `TEHAS_MCP_KEYS`, `TEHAS_MCP_DASHBOARD_KEYS`, `TEHAS_ENERGY`,
  `TEHAS_ENERGY_URL` and `TEHAS_UPDATE_LOCK_WAIT`. A value its rule refuses
  stops every command with exit 78, with a line that never shows the value.
  `TEHAS_UPDATE_LOCK_WAIT` outside 0 to 300 now stops `tehas update` and
  `install.sh` with 78 instead of becoming 10.
- Every other `TEHAS_*` name tehas reads is internal and listed with why, and
  a test holds new names to a rule or to that list.

### Adopter action

If a command stops with exit 78 after the update:

- `PI_ENSEMBLE_HOST_ALIASES is not valid`: write each entry as
  `name:address`, such as `models.example:192.0.2.10`.
- `TEHAS_WORK_REPO is not valid`: give an absolute path of letters, digits
  and `._-`, with no slash at the end.
- `TEHAS_HOME is not valid` or `TEHAS_INSTANCE_DIR is not valid`: give an
  absolute path of letters, digits and `._-`, with no slash at the end.
- `<name> is not valid` for a `TEHAS_MODEL_KEY_<NAME>`, `TEHAS_MCP`,
  `TEHAS_MCP_KEYS`, `TEHAS_MCP_DASHBOARD_KEYS`, `TEHAS_ENERGY` or
  `TEHAS_ENERGY_URL`: fix that line of `~/.config/tehas/env`.
- `TEHAS_UPDATE_LOCK_WAIT is not valid`: give a whole number from 0 to 300,
  or unset it.
- `holds a $ or a backtick` from `tehas disk` or `tehas setup`: write the
  value itself in that file, such as `TEHAS_HOME` or `TEHAS_SCRATCH_DIR` in
  `instance.env`.

A limit file edited by hand with `cycles` such as `1.0` or `01` now holds
every start as `unreadable`: run `tehas limit set` to write it again.
