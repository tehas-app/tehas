### Changes

- `tehas history`, `tehas export` and `tehas state pack` exit 141 with nothing on standard error when their reader goes away, as every other command does. They exited 1, which `tehas history` documents as a busy state store.
- The exit tables of `tehas limit`, `tehas pause` and `tehas drain`, and their help texts, list status 1 for a limit or pause flag that cannot be written. The tables of `tehas refresh` and `tehas dispatch --soon` name the request or file that could not be written.
- `tehas limit set` refuses `--json` with status 64. It took the flag, which was never documented, only when another word followed it. Read the limit back with `tehas limit --json`.
