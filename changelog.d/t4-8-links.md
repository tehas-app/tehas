### Changes

- The state store moves to schema 32 (`tehas update` migrates it): the new table `links` holds the links of a ticket to its blueprints, one row a blueprint with who made it, in which realm, when and how (`opened` or `linked`), written only by the store's new `link-record`. On every setup 2.0 runs, ticket and blueprint are one item and the table stays empty. `tehas state check --json` counts it in `rows`.
- `state/account-ids` names each pair's realm: one `<realm> <login> <id>` a line, the realm `code-host` or `tickets`. A 1.x line, `<login> <id>`, is read as the code host's, and `tehas update` writes the file once in the new form; every later write is in it. A line of any other shape stops `tehas update` with 78 naming the line, and no listed login counts until it is fixed.

### Adopter action

None: `tehas update` migrates the store and rewrites `state/account-ids`. A tool that edits `state/account-ids` by hand writes the new form.
