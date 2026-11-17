### Changes

- The easy merge is gone, and with it `TEHAS_EASY_MERGE`. It never ran on a
  host: no installed command started it, and every refresh already ran the
  full refresh cycle. Every refresh is now the full refresh cycle, the
  dispatcher no longer marks an approval for an easy merge, and the start
  comment of a refresh is the one of the full refresh. Loading refuses
  `TEHAS_EASY_MERGE` with exit 78 when it is set to 1 or any value but 0.

### Adopter action

Take `TEHAS_EASY_MERGE` out of `instance.env`. A line set to 1 stops every
command with 78 until it is gone; a blank value and `TEHAS_EASY_MERGE=0` are
ignored.
