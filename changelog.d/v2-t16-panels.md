### Changes

- Panels leave core. `tehas panel` and `live/panels.json` are gone, and the front pages no longer show the pages it set. `tehas api --json` no longer lists the feature `panels`. They return in 2.1 as the plugin `panels`.
- The dock stays: a host bridge menu item and a plugin's view still open in it. The front pages load it as `/fun/dock.js` and `/fun/dock.css`.
- A plugin granted only v1's `view` is now listed in the plugins menu, under its label, in place of a panel `p-<name>`.
- `tehas update` and `install.sh` remove `~/tehas/live/panels.json`, where an earlier version left it.

### Adopter action

Run `tehas update`: it removes the last `live/panels.json`. `~/tehas/state/panels.json` stays as it was: note its pages for the grant of the 2.1 `panels` plugin, then remove it.
