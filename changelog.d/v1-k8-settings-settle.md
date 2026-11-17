### Changes

- The dispatcher's poll reads `TEHAS_PLAN_TIMEOUT` and `TEHAS_PLAN_MAX_TURNS`
  by the rules the settings load uses. A value it cannot read stops the poll
  with exit 78 before the lock and before any call, instead of becoming the
  default (#2233).
