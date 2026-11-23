### Changes

- `tehas question` with no issue lists the question each issue waits on, and `tehas question --after <id>` every question whose number is greater than `<id>`, the answered and the withdrawn among them, in number order (`--after 0` lists them all). With `--json` each is one object of `questions` with `id`, `issue`, `kind`, `askedAt`, `cycle` on a stop's question, `answeredAt` on an answered one, `withdrawnAt` on a withdrawn one, and `choices`; the object is `api/questions.schema.json`, and a factory that has the list names `questions` in `features`. A planning question the issue no longer waits on has no `choices`: its options are kept only while it waits.
- `tehas question <issue> --json` prints `askedAt` where it printed `asked_at`. The old name is not kept.

### Adopter action

A program that reads `question.asked_at` from `tehas question <issue> --json` reads `question.askedAt`. A program that follows every question keeps the greatest `id` it has seen and calls `tehas question --after <id> --json`, after it finds `questions` in the `features` of `tehas api --json`.
