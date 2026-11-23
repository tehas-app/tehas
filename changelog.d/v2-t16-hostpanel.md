### Changes

- The host panel leaves core. `tehasbot top`, `tehas-top.service` and `live/top.json` are gone, and with them the Host section, menu link and head chip of the factory floor and the host section of the plain dashboard. It returns in 2.1 as the plugin `hoststats`, fed by the host's `feed:host`. `tehas top` in a terminal stays as it was.
- `TEHAS_TOP` is no longer read. A line of it in `instance.env` loads with one warning line.
- `tehas update` and `install.sh` stop, disable and remove `tehas-top.service`, and remove `~/tehas/live/top.json`, where an earlier version left them.

### Adopter action

Run `tehas update`: it stops and removes `tehas-top.service` and the last `live/top.json`. Then take `TEHAS_TOP` out of `instance.env`.
