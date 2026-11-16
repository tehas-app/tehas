### Changes

- Every switch setting takes `on`, `off`, `1` and `0`; the manual and the
  example instance write `on` and `off`. `1` and `0` keep working, and
  `TEHAS_ANALYTICS` now takes them too. Any other value stops every command
  with exit 78, as before.
- `TEHAS_ENERGY` is a switch like the others: `on`, `off`, `1` or `0`. Any
  other value stops every command with exit 78.
- Optional features are named in one registry, each either shipped or still
  being built. A feature still being built cannot be switched on: its switch
  set to `on` stops every command with exit 78 and names the feature. In this
  release `outsource` is still being built, so `TEHAS_OUTSOURCE=on` (or `1`)
  is refused and `tehas-proxy` stays off.
- `tehas api --json` has a new `flags` object, every optional feature and
  whether it is on here (`{"outsource": false}` today), and the word `flags`
  in `features`. `tehas api` prints a `flags` line.
- `tehas api` feature words: `store-copy` is now `state-copy`;
  `refresh-pr-starts`, `refresh-pr-lane` and `refresh-pr-json` are folded
  into `refresh-pr`; `host-message-health` is folded into `host-message`.
  `api/renames-1.0.tsv` lists each.

### Adopter action

- In `instance.env`, take out `TEHAS_OUTSOURCE=1` (or set it to `off`):
  outsource is still being built and the line now stops every command.
- In `~/.config/tehas/env`, if `TEHAS_ENERGY` holds a value other than `on`,
  `off`, `1` or `0`, set it to `on` (sample the mesh) or `off`.
- A program that reads `tehas api --json` and tests for `store-copy`,
  `refresh-pr-starts`, `refresh-pr-lane`, `refresh-pr-json` or
  `host-message-health` tests for `state-copy`, `refresh-pr` or
  `host-message` instead.
