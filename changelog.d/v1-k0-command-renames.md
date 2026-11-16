### Changes

- `tehas analytics export <file>` is now `tehas analytics copy --to <file>`,
  with the same copy and the same `--json` object. The old form exits 64 and
  names the new one.

### Adopter action

Nothing in `instance.env` changes. Change scripts and notes that call the old
forms:

- `tehas analytics export <file>` to `tehas analytics copy --to <file>`
