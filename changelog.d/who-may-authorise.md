### Changes

- Who may authorise work no longer depends on how GitHub spelled the account.
  One rule reads a login everywhere: gh's `app/<slug>`, REST's `<slug>[bot]`
  and GraphQL's `<slug>` are the same account when the poll weighs a start, a
  millwright's comment or the factory's own pull requests, and one rule reads
  `TEHAS_ALLOWED_LABELERS` wherever it is read, so a name is matched the same
  way by `tehasbot status`, `tehasbot pause` and `tehasbot protolab` as by the
  poll. A label event whose `created_at` is there but is no text is now read
  the strict way everywhere: the answer says it cannot be read and the poll
  holds the start, instead of reading the event as one with no time and
  ordering it ahead of the stamped ones (#2230).
