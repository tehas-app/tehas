### Changes

- Energy use leaves core. `tehasbot energy`, `tehas-energy.service` and `live/energy.json` are gone, and with them the Energy use section and live meter of the factory floor, the business page's live strip, `energy` in `tehas top`, `live/top.json`, the ten-day report, `live/analytics.json` and the MCP tool `factory_stats`. It returns in 2.1 as a plugin fed by the host's `feed:mesh`.
- `TEHAS_ENERGY`, `TEHAS_ENERGY_URL` and `TEHAS_ENERGY_SOURCE_URL` are no longer read. A line of one in `instance.env` loads with one warning line; one in `~/.config/tehas/env` is ignored.
- `tehas update` and `install.sh` stop, disable and remove `tehas-energy.service`, and remove `~/tehas/live/energy.json`, where an earlier version left them.

### Adopter action

Run `tehas update`: it stops and removes `tehas-energy.service` and the last `live/energy.json`. Then take `TEHAS_ENERGY`, `TEHAS_ENERGY_URL` and `TEHAS_ENERGY_SOURCE_URL` out of `instance.env` and `~/.config/tehas/env`.
