### Changes

- Part of the lean driver, still being built: a release refuses
  `TEHAS_DRIVER=lean`, so nothing changes on a host yet. With lean on and
  `TEHAS_DRIVER=lean`, a start with no driver label runs lean when the issue's
  text holds a `pi-spec v1` block and classic when it does not, and the poll
  logs which and why. `tehas:lean` and `tehas:classic` from an account in
  `TEHAS_ALLOWED_LABELERS` then each choose the driver for one issue (#1099).
