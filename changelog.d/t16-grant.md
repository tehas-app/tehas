### Changes

- The millwright's grant is gone. The bot's `tehas:ready` no longer starts a cycle again on an issue a person approved before, and the feed no longer starts an issue a person wrote: the poll takes the label off as it does an outsider's. A program starts work for a person only through `tehas start`. The millwright's markers stay: it still answers planning questions in a session a person started, asks for a finish on a person's approval of the text as it is, and starts an epic's sub-issues under `TEHAS_MILLWRIGHT_EPICS=on`.
- `TEHAS_MILLWRIGHT_MAX_RETRIES`, `TEHAS_MILLWRIGHT_FEED` and `TEHAS_MILLWRIGHT_MAX_FEEDS` are no longer read. A line of one still loads; the load says so once and drops it.
- `tehas api --json` no longer lists the feature word `millwright-grant`.
- The millwright's comments marked `action=retry` or `action=change` now do nothing: they only went into the record of a grant's start. Its `action=answer` and `action=finish` comments work as before.
- The state store's `approval-current` no longer answers `millwright_starts` or `fixer_starts`, `approval-view` no longer gives `fixer_starts` or `park_lifted`, and `approval-record` refuses an input that holds `park_lifted`.

### Adopter action

Take `TEHAS_MILLWRIGHT_MAX_RETRIES`, `TEHAS_MILLWRIGHT_FEED` and `TEHAS_MILLWRIGHT_MAX_FEEDS` out of `instance.env` if it sets them. A millwright that applied `tehas:ready` to retry a cycle asks a person instead, or runs `tehas start` for a signed-in person.
