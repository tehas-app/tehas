---
title: The forge interface
description: "What tehas asks of the site its issues live on, as a contract for a second adapter: each term, what it returns, how GitHub answers it, and what a forge must offer at minimum."
---

# The forge interface

The forge is the site an instance's issues live on. tehas asks it a few
things in its own terms: the labels an issue carries, the newest event of one
label, who applied the label that approves work, and putting a label on or
taking it off. The trait that holds those terms is `Forge` in
`console/core/src/forge.rs`. This page sets it out as the contract a second
adapter, for GitLab or Forgejo, would be written against.

GitHub is the only adapter today, and every term is backed by labels. No other
forge is supported, and nothing on a host chooses one. The trait names no
endpoint, no field and no JSON shape, so an adapter is free to answer a term
from a label or from a native field of its site. A caller never names the
site's own fields.

The trait does not yet hold every question tehas asks of GitHub. Blockers,
priority, comments, pull requests and the dispatcher's own reads still go to
GitHub directly; see [Outside the trait today](#outside-the-trait-today).

## Terms

`issue` is the issue's number in every term, a positive integer. Every term
answers a value or a `ForgeError`. An empty value is the site's answer, not a
read that failed: an issue with no labels answers the empty list. An answer
that could not be read is an error, never an empty answer, so a poll cannot
clear a label nobody took off.

Every term makes its own call. The adapter keeps nothing between terms; a
caller that wants two answers about one issue holds what it read.

| Method | Returns | Meaning | GitHub mapping |
| --- | --- | --- | --- |
| `labels(issue)` | `Vec<String>` | The labels the issue carries, in the site's order. A name carried twice appears twice. | `GET repos/<owner>/<name>/issues/<n>`, the `name` of each entry of `labels`. An answer with no `labels` list is an error. |
| `label_state(issue, label)` | `Option<LabelEvent>` | The newest `labeled` or `unlabeled` event of `label`: its kind, the account and the time. `None` when no event carries the label. | `GET repos/<owner>/<name>/issues/<n>/events?per_page=100 --paginate`, every page, then the newest event of the label by the order below. |
| `label_events(issue)` | `Vec<Event>` | Every label event of the issue, for a rule of the caller's to read, such as the pause's. | The same paginated events call, every event as it came. |
| `applied_by(issue, label)` | `Option<Approval>` | The account and time of the newest application of `label`. `None` when no event applied it. A removal never answers, whoever made it. | The same events call, `labeled` events of the label only. |
| `approval(issue)` | `Option<Approval>` | Who applied the label that approves work most recently, and when. | `applied_by(issue, "tehas:ready")`. A default method: an adapter does not write it. |
| `add_label(issue, label)` | `()` | Put the label on. An issue that carries it already is the state wanted, not an error. | `POST repos/<owner>/<name>/issues/<n>/labels -f labels[]=<label>`. Not `gh issue edit`, whose GraphQL a bot's token may not answer. |
| `remove_label(issue, label)` | `()` | Take the label off. An issue without it is the state wanted, not an error. | `DELETE repos/<owner>/<name>/issues/<n>/labels/<label>`, the name URI-encoded. A 404 is success. |

`LabelEvent` is `kind` (`labeled` or `unlabeled`), `actor` and `at`.
`Approval` is `actor` and `at`. `Event` (`tehas_core::events::Event`) is
`event`, `label`, `actor` and `at`, with `at` optional. A field is empty when
the site names none, as GitHub does for an event whose account is gone. An
empty `actor` is a fact and not a refusal: the caller compares it with the
allowed list and finds nobody there.

### The order of events

GitHub documents no order for an issue's events, so the adapter sorts them:

- the time decides, and an event with no time is older than any with one;
- of two events in one second, the application sorts before the removal, so a
  label applied and taken off in that second counts as gone;
- of two events of one kind in one second, the later one in the site's answer
  answers.

Only `labeled` and `unlabeled` events count. A `renamed` event that names the
label in passing is not a label on the issue. A second adapter keeps these
rules, or the same history reads differently on two forges. The pause is the
one reader that breaks the tie the other way, so a pause lifted in the second
it was applied still holds. It is no term of the trait: its rule reads the
events itself.

### Writes and label names

A write refuses a name that is no label of tehas before it calls anything: an
empty name, one longer than 50 characters, or one outside `a-z`, `A-Z`, `0-9`,
`:`, `-` and `_` (`is_label`). A read takes any string, so a repository's own
label can still be asked about. Both writes may be asked twice with the same
result.

### Errors

`ForgeError` holds `call`, the term that stopped, and `why`, the runner's
reason. The reason comes from a fixed list of error words and never from the
site's whole answer, which may name a host or carry a token. Its line is
`ERROR: <call> failed: <why>`. Nothing in the trait panics.

## Who made a change

Authorisation is the account that applied a label, and only an allowed account
counts ([AGENTS.md](https://github.com/tehas-app/tehas/blob/main/AGENTS.md)
rule 5). The forge answers who; it never answers whether that account may start
anything. The caller compares the account with `TEHAS_ALLOWED_LABELERS`.

These terms must name the account behind the change:

- `applied_by` and `approval`: the account that applied the label. The
  account of an application is what lets `tehas:ready`, `tehas:blocked`,
  `tehas:plan` and `tehas:refine` start work.
- `label_state`: the account of the newest event, so a caller can tell a
  removal by a person from one by the bot.
- `label_events`: the account of every event, for rules such as the pause.

`labels`, `add_label` and `remove_label` name no account. A write is made as
the instance's bot, with the token the runner holds, so the site records the
bot as its account and the next read of the events says so.

## What a forge must offer

At minimum:

- **Labels on issues**, which a token can add and remove one at a time.
- **A full history of label changes** for each issue, each event naming the
  account that made it and when. A forge that shows only the labels an issue
  carries now cannot back `applied_by`, and without that no work can be
  authorised.
- **Issues numbered** with positive integers, the number every term names.
- **A token for one bot account**, whose writes the history records as that
  account's.

Optional: native fields for what tehas keeps in labels today, such as blocked
by links, a priority field or issue types. The trait has no term for them
today, so no adapter reads them. A term an adapter answers from such a field
keeps the label as its fallback, so a forge without the field still answers
it from labels alone.

## Outside the trait today

These still reach GitHub without the trait, through `gh` in the bash or the
dispatcher. A second adapter would need them as well:

- blockers: `tehas:blocked` and the `<!-- tehas:blocked by=... -->` comment
  marker, with the open or closed state of each issue it names (see
  [Operating](/operating));
- the issue's text, its comments and the edits made to them, which the
  approval of a changed text and the question flow read;
- the pull requests of a cycle: opening, reading and refreshing them;
- the dispatcher's poll, which reads the queue through GitHub's GraphQL API
  in `console/dispatch`.

## The runner

The GitHub adapter does not start gh itself. It asks a `GhRunner`, which runs
one call and returns its output or the reason it failed. A host uses
`Bounded`: the instance's `host/shim/gh` under a 30-second bound, then TERM
and, after 5 seconds of grace, KILL to the whole process group, as
`timeout -k` does. A second adapter brings its own runner for its own
program or HTTP client, and keeps the bound: a poll holds its lock for the
whole of its read.

## From bash

`console forge` is the trait as a command:

```
console forge labels <issue>
console forge label-state <issue> <label>
console forge approval <issue>
console forge add-label <issue> <label>
console forge remove-label <issue> <label>
```

It exits 0 when the call answered, 1 when the forge failed, 64 for a bad
argument and 78 when `TEHAS_REPO` is unset or not `owner/name`, or no gh can
be run. The full contract is at the top of `console/src/forge.rs`.

## Testing an adapter

The GitHub adapter's tests, in `console/core/src/forge/tests.rs`, run it
against a stub runner that answers from fixtures, so no test needs a network,
a token or a repository. A second adapter is tested the same way: the stub
returns the site's answers, and each case checks the term's answer and the
call the adapter made.
