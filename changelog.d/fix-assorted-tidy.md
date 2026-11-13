### Changes

- `install.sh --check` also checks the owner and mode of
  `~/.config/tehas/env` and `~/.config/tehas/instance-dir`: a file another
  account can write is a finding, as for `instance.env`.
- The indexing container runs with no capabilities and `no-new-privileges`.
- Text the host posts drops control and invisible characters, and the release
  notes `tehas update` prints do too.
- `console gh` counts the attached forms of `gh api` options, such as
  `-X=POST` and `-fbody=x`, as writes.
- The mask hides each key of `TEHAS_MCP_KEYS` on its own, not only the whole
  list.

### Adopter action

If `install.sh --check` names `~/.config/tehas/env` or
`~/.config/tehas/instance-dir`, make it the tehas user's own and writable by
no one else (`chmod go-w`).
