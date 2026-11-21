### Changes

- The state store moves to schema 21 at the next `tehas update`, which backs it up first. The new table, `plugin_records`, holds the records plugins keep, such as a prototype, the issue it was filed in and who filed it. Nothing writes to it until the plugins that use it ship.
