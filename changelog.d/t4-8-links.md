### Changes

- The state store moves to schema 32 (`tehas update` migrates it): the new table `links` holds the links of a ticket to its blueprints, one row a blueprint with who made it, in which realm, when and how (`opened` or `linked`), written only by the store's new `link-record`. On every setup 2.0 runs, ticket and blueprint are one item and the table stays empty. `tehas state check --json` counts it in `rows`.

### Adopter action

None: `tehas update` migrates the store, and the table is empty.
