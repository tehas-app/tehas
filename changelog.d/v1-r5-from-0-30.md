### Changes

- tehas updates a host from v0.30.0 or later only. The state store migrates a database from schema 12, the one v0.30.0 wrote, and refuses an older one with a line that says to update to a v0.30 release first; the database is left as it was. The store's tests migrate the stores the v0.30 releases made.
- `store import` is gone, and `install.sh` no longer imports the record files a tehas before the state store kept under `state/`: a v0.30 install imported them. A poll that still finds one stops with 78 and says to install a v0.30 release to import it, or to move it out of `state/`.
- `install.sh` no longer takes a home without the `.tehas-home` marker for tehas's by the files an older install left: every install since v0.30.0 writes the marker.
- `analytics.db` at schema 2 or 3 is no longer migrated: it is kept aside under its version, as any other version is. v0.30.0 wrote schema 4.

### Adopter action

A host on a release older than v0.30.0 updates to a v0.30 release first (`tehas update --to v0.30.6`), then to this one.
