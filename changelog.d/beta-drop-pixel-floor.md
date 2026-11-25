### Changes

- The factory floor leaves core. The 16-bit game view that was the front page, with its stations, work order, issue board, sample tickets and JSON export, and its breakroom game Buzzword Buster, is gone: `fun/index.html`, `game-view.js`, `game-view.css`, `game-data.js`, `brick-wall.js`, `fold-fit.js`, `sample-issues.js`, `theme.js`, `stages.js` and the four scene pictures. The game lives on as tehas-app/buzzword-buster.
- The front page `/` is the plain dashboard, in both designs. `/floor` sends a reader to `/` with a permanent redirect.
- The dock, the host messages banner, the versions line and the epics list are on the dashboard now.
- `live/status.json` no longer carries `buzzwords`, and `tehasbot status` no longer reads every issue of the repository once an hour to count them. `state/buzzwords.at` is read by nothing.
- The themes no longer set the `--game-*` tokens.

### Adopter action

None to keep tehas running: an install removes the old files, and no setting changes. Point a link to `/floor` at `/`. A program that read `buzzwords` from `live/status.json` stops: the field is gone. `~/tehas/state/buzzwords.at` may be deleted.
