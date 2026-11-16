### Changes

- `TEHAS_DRIVER` in the instance chooses the driver an approved start runs
  when no label chooses one: `classic`, the default, or `lean`. With `lean`,
  a start with no driver label runs lean when the issue's text holds a
  `pi-spec v1` block and classic when it does not, and the poll logs which and
  why. The new label `tehas:classic` from an account in
  `TEHAS_ALLOWED_LABELERS` runs that issue on classic, read from the label's
  events as `tehas:lean` is.
  An issue that carries both labels starts nothing, and the poll says why once.
  The park of a refused lean start under `TEHAS_DRIVER=lean` asks for
  `tehas:classic` instead of asking for `tehas:lean` to come off (#1099).
- A lean start takes `needs-human-attention` off with its other labels:
  `work-lean` takes no `--restart`, which is what clears it for classic.

### Adopter action

Run `tehas labels` to make and pin `tehas:classic`. Until it is pinned, an
application of it counts as nobody's, and a start under `TEHAS_DRIVER=lean`
over it starts nothing. Nothing else changes unless you set `TEHAS_DRIVER=lean`.
