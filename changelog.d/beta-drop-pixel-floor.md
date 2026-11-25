### Changes

- The factory floor leaves core (#3291). The 16-bit game view that was the front page, with its stations, work order, issue board, sample tickets and JSON export, and its breakroom game Buzzword Buster, is gone: `fun/index.html`, `game-view.js`, `game-view.css`, `game-data.js`, `brick-wall.js`, `fold-fit.js`, `sample-issues.js`, `theme.js`, `stages.js` and the four scene pictures. The game lives on as tehas-app/buzzword-buster.
- The front page `/` is the plain dashboard, in both designs. `/floor` sends a reader to `/` with a permanent redirect. The dashboard's canonical address is `/`, at `/` and at `/dashboard`, and it carries no structured data: the floor's SoftwareApplication block is gone.
- The dashboard shows the tehas version once, in its versions line; the manual's pages keep it in the footer.
- The dock, the host messages banner, the versions line and the epics list are on the dashboard now.
- `live/status.json` no longer carries `buzzwords`, and `tehasbot status` no longer reads every issue of the repository once an hour to count them. `state/buzzwords.at` is read by nothing.
- The themes no longer set the `--game-*` tokens.

### Adopter action

During the update, `/` answers 404 until the docs container is recreated, which `tehas update` does in its `docs --force-recreate` step and a person does by hand with the `docs` line of "Rolling back tehas" in the manual's page The host. Point a link to `/floor` at `/`. A program that read `buzzwords` from `live/status.json` stops: the field is gone. `~/tehas/state/buzzwords.at` may be deleted.
