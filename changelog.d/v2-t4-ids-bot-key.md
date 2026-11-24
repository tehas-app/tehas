### Changes

- `state/account-ids` keeps a GitHub App's bot under its whole login, `<slug>[bot]`, and a person or an organisation under its login, so an organisation and the bot of an app with its name each have their own pair. An account counts as a listed login of its own kind only. A file that keeps a known bot only under its bare slug is not read: the dispatcher stops with 78 and names `console ids record`, which writes the pair under the bot's login when it holds the id GitHub gives the bot.

### Adopter action

`tehas update` records the ids again and settles the file. A factory whose `state/account-ids` still keeps its bot, or the millwright, under the bare slug stops with 78 until `console ids record` (or `tehas update`) is run. If the record stops because the bare pair holds another id than the bot's, and that pair was the bot, run `tehas update --accept-id <slug>[bot]`.
