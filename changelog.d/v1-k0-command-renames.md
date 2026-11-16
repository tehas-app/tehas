### Changes

- `tehas setup --labellers` is now `--labelers`, spelled as the setting it
  writes, `TEHAS_ALLOWED_LABELERS`. The old spelling exits 64 and names the
  new one.
- `tehas setup --token-file` and `--app-installation` are gone. Both are now
  unknown options: each exits 64 and says why.
- `tehas analytics export <file>` is now `tehas analytics copy --to <file>`,
  with the same copy and the same `--json` object. The old form exits 64 and
  names the new one.

### Adopter action

Nothing in `instance.env` changes. Change scripts and notes that call the old
forms:

- `tehas setup --labellers "a b"` to `tehas setup --labelers "a b"`
- `tehas analytics export <file>` to `tehas analytics copy --to <file>`
- drop `--token-file` and `--app-installation` from `tehas setup`
