### Changes

- The state store moves to schema 19 at the next `tehas update`, which backs it up first. The new table, `plugin_records`, holds the records plugins keep, such as a prototype and the issue it was filed in. Nothing writes to it until the plugins that use it ship.
