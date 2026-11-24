### Changes

- `state/account-ids` keeps a GitHub App's bot under its whole login, `<slug>[bot]`, and a person or an organisation under its login, so an organisation and the bot of an app with its name each have their own pair. An account counts as a listed login of its own kind only. A file that keeps a known bot only under its bare slug, in a 1.x line or a `code-host` line, is not read: the dispatcher stops with 78 and names `console ids record`, which writes the pair under the bot's login when it holds the id GitHub gives the bot.
- A bot listed in `TEHAS_ALLOWED_LABELERS` no longer counts: only the millwright (the factory's own app, or the app `TEHAS_MILLWRIGHT_APP` names) is a listed bot. A bot listed bare there loses its approvals, with only doubted lines (`<login>#<id>`) in the log. The list takes logins only (a `[` is refused), and a bare name is now the key of a person or an organisation of that name, so a bot app's label events do not count for it, where 1.x folded the bot onto the bare name. This narrows who counts and fails closed.

### Adopter action

`tehas update` records the ids again and settles the file. A factory whose `state/account-ids` still keeps its bot, or the millwright, under the bare slug (`tehas-app 123` or `code-host tehas-app 123` for the app `tehas-app[bot]`) stops with 78 until `console ids record` (or `tehas update`) is run. If the record stops because the bare pair holds another id than the bot's, and that pair was the bot, run `tehas update --accept-id <slug>[bot]`.

A factory that relied on a bot listed bare in `TEHAS_ALLOWED_LABELERS` to approve loses those approvals. If that bot should approve, it can only as the millwright (`TEHAS_MILLWRIGHT_APP`, in `host/docs/operating.md`); otherwise take it out of the list.
