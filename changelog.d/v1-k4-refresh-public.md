### Changes

- `tehas refresh --pr N [--json]` is the public way to ask for the refresh of one pull request. `tehasbot refresh --pr N` stays as an alias through 1.x and prints the same words and object (#2009).
- A refresh finds the factory's own `gh` with no login `PATH`: it puts `~/tehas/shim` and `~/tehas/bin` first before it reads GitHub, and says `gh not found` when no `gh` can be run (#2009).
- `tehas github check` checks the GitHub App's token, and `tehas github writes` prints the GitHub writes of the last day. They replace `tehasbot github-token --check` and `tehasbot github-writes` in the manual and in `tehas setup`; the old forms still work.
- `api/renames-1.0.tsv` lists the renames of 1.0, one row each.

### Adopter action

A program or script that runs `tehasbot refresh --pr N` keeps working, and can move to `tehas refresh --pr N`. A person runs `tehas github check` and `tehas github writes` from now on.
