### Changes

- The features page's Known limits name what a person running tehas should know
  before 1.0 and what to do about each: the token, network and model key a
  cycle's container holds, what the cycle's token may write, GitHub's bad
  minutes, the issue read again after the approval check, the lean driver,
  `live/` files, the MCP server on a private network, a ticked retry under a
  limit of 0, and a kept installation token.
- The operating manual has "Updating from 0.30.x to 1.0": edit the settings,
  drain, update, labels, and every setting, label, command form, field and
  exit status 1.0 renamed or removed, with what to do for each.

### Adopter action

Before updating from 0.30.x to 1.0, edit the renamed and removed settings in
`instance.env` and `~/.config/tehas/env`, then run `tehas drain --wait 3600`
until no cycle runs. Then run `tehas update`, `tehas labels` and
`tehas limit clear`. While a setting is left to fix, `tehas update` stops with
78, names it and switches nothing. The manual's section "Updating from 0.30.x
to 1.0" in `operating.md` lists every name and what to do for each.
