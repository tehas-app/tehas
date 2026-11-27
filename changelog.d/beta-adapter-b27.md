### Changes

- A refine that links its new issue as a part of the original reads the new issue's parent first. A part already under the original is done with nothing sent; a part of another issue, in this repository or another, is never moved, and the poll says the link failed in `logs/dispatch.log` (#3202, #3220).
- A plan written onto its issue, and the "Replaces #n" line on a filed issue, keep the issue's tehas markers where they stand. 1.x replaced the whole body and lost them (#3223).
- A bot app named bare in `TEHAS_ALLOWED_LABELERS` no longer counts as a labeller: the list takes logins, and a bare name is a person's or an organisation's. The only bot that counts is the millwright (#3204).
- A GitHub rate limit that gh or GraphQL reports with no reset time now holds that budget out for a minute. Before, the factory could ask again at once and be refused again. A reset time or `retry-after` GitHub gives holds a budget at most an hour, and one already past holds the minute (#3378).

### Adopter action

A factory that relied on a bot listed bare in `TEHAS_ALLOWED_LABELERS` to approve makes it the millwright or takes it off the list. If `tehas update` stops because a bare pair holds another id and that pair was the bot, run `tehas update --accept-id <slug>[bot]`.
