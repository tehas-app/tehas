---
title: The forge
description: "How tehas asks GitHub about labels: the tickets port its commands take, and the GitHub adapter's terms behind it, each with what it returns and how GitHub answers it. Internal: tehas 2.0 promises no forge interface."
---

# The forge

The forge is the site an instance's issues live on. tehas asks it a few
things in its own terms: the labels an issue carries, the newest event of one
label, who applied the label that approves work, and putting a label on or
taking it off. A command asks through the tickets port, the `Tickets` and
`TicketWrites` traits of `tehas-ports` (`console/ports/`). The GitHub
adapter, the `GitHub` type in its own crate, `tehas-github` in
`console/github/`, answers the port over the terms below, with the table of
labels in `console/github/src/table.rs`. This page describes them for a
person who reads or changes that code.

The port is internal. tehas 2.0 promises no forge interface: GitHub is the
only forge, nothing on a host chooses another, and the port may change in any
release. What tehas writes to GitHub, and the labels it reads, are
what the [contract](/contract) freezes. Every term is backed by labels. The
port names no endpoint, no field and no JSON shape, so an adapter is free to
answer a call from a label or from a native field of its site. A caller never
names the site's own fields.

The port does not yet hold every question tehas asks of GitHub. Blockers,
priority, comments, pull requests and the dispatcher's own reads still go to
GitHub directly; see [Outside the port today](#outside-the-port-today).

## The tickets port today

Only a program's wiring names the adapter: the console's is a module of
`console/src/main.rs`, and each command takes its handle from there. A
method no command asks yet answers `Refused(Unsupported)` before any call.

| Method | Asked by | GitHub mapping |
| --- | --- | --- |
| `several(keys)` | `tehas resume`, for the marks on the control issue, and `tehas stop`, for the running mark | `GET repos/<owner>/<name>/issues/<n>` per key: the title, state and its reason, type, labels, milestone, author, times and link. An answer without one of the fields every issue has is `Unread(Malformed)`. |
| `several_with_closers(keys)` | nothing yet | One GraphQL question a hundred keys, each issue under an alias of its number: its digest, as `several` reads it, its newest close event's closer, and the pull requests GitHub lists as closing it, a merged one of this repository counted as merged, as `closed_since` counts them. A number that is no issue is `NotFound`, alone. |
| `history(key, window, kinds)` | `tehas resume` and `console label allowed` | The paginated events call below, marks going on and off alone. A login is folded to lower case by `accounts::folded`, the fold the user id check uses too, so it matches the allowed list in any case. |
| `prepare()` | `tehas labels` | `gh label create <name> --repo <owner>/<name> --color <c> --description <d> --force` for each row of `host/labels.tsv`, compiled into the adapter. A label that cannot be made stops none of the others; the first that failed is the answer, with its name (`Unprepared`). |
| `ensure_mark(spec)` | nothing yet | The same call for one label, with the colour and text the spec gives. |
| `closed_since(since, bound)` | `tehasbot status`, for the shipped issues closed since its last read | One GraphQL question a page: the repository's closed issues filtered by `since`, each with its newest close event's closer and the pull requests GitHub lists as closing it, a merged one of this repository counted as merged. More than `bound` is `Unread(Cut)`; an answer with errors is `Unread(Inconsistent)`. |
| `mark(key, mark)`, `unmark(key, mark)` | `mark`: `tehasbot status`, for `tehas:shipped` on each closed issue a merged pull request closed that does not carry it yet. `unmark`: `tehas stop`, the running mark | `add_label` and `remove_label` below, of the mark's label. |
| `close(key, reason)` | nothing yet (T4.7) | The issue read, then `PATCH repos/<owner>/<name>/issues/<n>` of `state` and `state_reason` (`completed`, `not_planned`, `duplicate`). A closed issue is done with nothing sent. |
| `assign(key, who)` | nothing yet (T4.7) | `GET user/<id>` for the account's login, then `POST repos/<owner>/<name>/issues/<n>/assignees`. GitHub answers 201 and drops a login it cannot assign, so the answer's assignees are read: the account missing is `Refused(Invalid)`. A bot or an organisation is `Refused(Invalid)` with nothing sent. |
| `add_part(parent, child)` | nothing yet (T4.7) | `GET repos/<owner>/<name>/issues/<child>/parent`: already this parent's part is done, another parent's is `Refused(Invalid)` and never moved. Else the child's issue read for its id, then `POST repos/<owner>/<name>/issues/<parent>/sub_issues`. |

A write GitHub refuses with 422 made nothing: it answers `Refused(Invalid)`,
never a write that may have been sent.

## The adapter's terms

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
| `approval(issue)` | `Option<Approval>` | Who applied the label that approves work most recently, and when. | `applied_by(issue, "tehas:ready")`, the label the adapter's table names. |
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
label in passing is not a label on the issue. The pause is the
one reader that breaks the tie the other way, so a pause lifted in the second
it was applied still holds. It is no term of the adapter: its rule, in
`console/core/src/events.rs`, reads the history itself.

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
`ERROR: <call> failed: <why>`. Nothing in the adapter panics.

## Who made a change

Authorisation is the account that applied a label, and only an allowed account
counts ([AGENTS.md](https://github.com/tehas-app/tehas-factory/blob/main/AGENTS.md)
rule 5). The adapter answers who; it never answers whether that account may
start anything. The caller compares the account with `TEHAS_ALLOWED_LABELERS`.

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

## What a second forge would need

No second forge is planned. An adapter for one would answer the port, and
would need at least:

- **Labels on issues**, which a token can add and remove one at a time.
- **A full history of label changes** for each issue, each event naming the
  account that made it and when. A forge that shows only the labels an issue
  carries now cannot back `applied_by`, and without that no work can be
  authorised.
- **Issues numbered** with positive integers, the number every term names.
- **A token for one bot account**, whose writes the history records as that
  account's.

Optional: native fields for what tehas keeps in labels today, such as blocked
by links, a priority field or issue types. No caller asks the port for them
today, so no adapter reads them. A term an adapter answers from such a field
keeps the label as its fallback, so a forge without the field still answers
it from labels alone.

## Outside the port today

These still reach GitHub without the port, through `gh` in the bash or the
dispatcher:

- blockers: `tehas:blocked` and the `<!-- tehas:blocked by=... -->` comment
  marker, with the open or closed state of each issue it names (see
  [Operating](/operating));
- the issue's text, its comments and the edits made to them, which the
  approval of a changed text and the question flow read;
- the pull requests of a cycle: opening, reading and refreshing them;
- the dispatcher's poll, which reads the queue through GitHub's GraphQL API
  in `console/dispatch`;
- the pins of `tehas labels`: each granting label's node id and when it was
  made, which the port has no call for.

## The runner

The GitHub adapter does not start gh itself. It asks a `GhRunner`, which runs
one call and returns its output or the reason it failed. Both stay in
`console/core/src/forge.rs` while other crates call them. A host uses
`Bounded`: the instance's `host/shim/gh` under a 30-second bound, then TERM
and, after 5 seconds of grace, KILL to the whole process group, as
`timeout -k` does. The bound matters: a poll holds its lock for the whole of
its read.

A write sends its text with `run_input`, on gh's standard input, never as an
argument: a body may be 1 MiB, past the 128 KiB one argument may hold, and an
argument list is seen by every process on the host. The input is written
within the same bound, and a failed call's reason never holds it. A runner
that takes no input answers so and runs nothing.

## Testing the adapter

The GitHub adapter's tests, in `console/github/tests/forge.rs`, run it
against a stub runner that answers from fixtures, so no test needs a network,
a token or a repository: the stub returns the site's answers, and each case
checks the term's answer and the call the adapter made.
`console/github/tests/port.rs` does the same for the tickets port's
methods, and the commands' own tests run on the ports' fake.

The adapter also answers the tickets port's marks and the history of marks
going on and off, in the ports' types. `console/github/tests/samples.rs`
replays the adapter's recordings of a fixture project, in
`console/github/tests/recorded/`, and holds each answer to the recorded
samples in `console/ports/samples/github/`, which name the port call and the
answer it must give. The ports' laws of lists check the same answers.
`console/github/tests/table.rs` holds the table of labels to
`host/labels.tsv`.
