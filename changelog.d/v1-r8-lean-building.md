### Changes

- The lean driver is a feature still being built, `lean` in
  `tehas api --json` `flags`, switched by the new setting `TEHAS_LEAN`. It
  stays off in this release: `TEHAS_LEAN=on` and `TEHAS_DRIVER=lean` stop
  every command with 78 and name the feature, and `tehas work --driver lean`
  exits 78. A start that would run lean (`tehas:lean`, or a retry, outage
  retry or refresh of a lean start) starts nothing: the poll takes the label
  that asked off and says once that lean is still being built. Classic never
  runs in its place. `tehas:classic` and an unlabelled start run classic as
  before. The labels stay in the label set.

### Adopter action

An instance with `TEHAS_DRIVER=lean` in `instance.env` must take the line out
or set it to `classic` before updating, or every command stops with 78. Take
`tehas:lean` off any issue that should run, so it runs on classic.
