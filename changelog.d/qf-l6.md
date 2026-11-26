### Changes

- The state store migrates from a 1.4.x store only. The store's migration
  now starts at the schema v1.4.0 wrote; the steps for older schemas are
  gone, and a store older than that is refused with a line that says to
  update to a v1.4 release first before anything is copied.

### Adopter action

A host that still runs a release before v1.4 updates to any v1.4 release
first, and to this one after: the store refuses an older database and
installs nothing.
