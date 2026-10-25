# Spec: a cycle that cannot work an issue refines it

**Status**: Built 2026-10-24 on the record files (PR #696), rebuilt
2026-10-25 on the state store after epic #244 moved parks and planning
sessions there. The store derives the mark; the sections below say so.

## Purpose

A person applies `tehas:ready` to an issue and a cycle starts. The first step
of pi-rukas `/work`, explore, reads the issue against the code and decides
whether it can be built. When it cannot, the cycle parks at the cap
`intent-park` and the issue waits for a person with `needs-human-attention`.
Two of the park reasons mean the issue is not yet a ticket one cycle can
work:

- `underspecified`: the intent cannot be worked out ("make good").
- `too-large`: the intent is clear, but it is more than one cycle.

For both, the next step today is that a person applies `tehas:refine`, which
starts a planning session that turns the request into one issue sized for one
cycle (`lib/refine-brief.md`, `host/docs/operating.md` "Refining a request").
This spec makes the factory take that step itself, once.

The decision stays with pi-rukas. Explore already grounds the issue in the
code, overrides an `underspecified` park when the spec it wrote is complete,
and only parks when the intent itself is open. tehas adds no model call and no
second judge: it reads the park reason the result already carries.

## Goals

- An issue that parks as `intent-park` with reason `underspecified` or
  `too-large` goes to a refine session on the next poll, without a person
  applying `tehas:refine`.
- At most one planning session per issue is started by the factory, ever.
- The refined ticket is a new issue. The original keeps its text, and the two
  link to each other.
- Every cycle still starts from a person's `tehas:ready` on the text it works.

## Non-goals

- No check before the cycle. A vague issue costs one explore step, up to 30
  minutes, and one slot while it runs. A pre-check can come later if that cost
  is measured to matter.
- No change to pi-rukas. `underspecified` and the other park reasons stay as
  they are.
- No automatic `tehas:plan`. Refine is the session the factory starts.
- The other park reasons, `contradicted-by-code`, `already-implemented`,
  `premise-unsound`, `unresolved-critical-open-point` and `blank-issue-body`,
  park for a person as today. So do plan parks (`plan-spec-mismatch`).
- The `TEHAS_SIZE_MAX_FILES` hold, which stops a wide issue before any cycle,
  is unchanged.
- The filed ticket is never started by the factory. A person reads it and
  applies `tehas:ready`.

## Anti-goals

- A loop: refine files a ticket, its cycle parks, the factory refines it,
  and so on.
- A session nobody authorised: a refine on an issue whose `tehas:ready` came
  from an account that may not label, or on text changed since.
- Overriding a person: starting a session on an issue a person has put on
  hold, or while the factory is paused.
- Editing the text a person wrote.

## When the factory refines

The park is on the state store (`store park`), and the cycle's end names its
park reason (`cycle-end --park-reason`, kept in `cycle_history`). The store
derives that a park is owed a refine, as it derives the outage retry
(`park-show --automatic`, rule 4a in `libexec/dispatch`):
`park-show --refine` lists the current parks whose reason is `intent-park`,
whose end's park reason is `underspecified` or `too-large`, of a cycle a
label or a retry started (`how` of `label`, `retry-tick` or `outage-retry`,
with an account), on an issue with no `planning` row and no `plan_history`
line, whose `retries` is 0. `park-retry` spends it, as it spends the outage
retry; any start of the issue spends it too. The refine follows that pattern, but as a planning
session it belongs with rule 5 (new planning sessions): in it or just after
it, before the poll exits for want of a free cycle slot. A labelled session
does not wait for a slot, and neither does this one.

A poll starts the refine session only when all of these hold:

1. The factory is not paused. The pause check at the start of the poll
   already stops everything after it; the refine rule must sit after it.
2. The issue is open, carries `needs-human-attention` from the park, and does
   not carry `tehas:ready`, `tehas:running`, `tehas:blocked`, `tehas:plan` or
   `tehas:refine`. A label a person added since the park decides instead.
3. The account on record as having started the cycle (`started_by` of the
   park) is still in `TEHAS_ALLOWED_LABELERS`, and the issue's text is
   unchanged since that cycle began (`unchanged_since`, as the outage retry
   checks). That account's start is the authorisation for the session, and
   the session's record names it as `by`. The body must also still have the
   hash its start's approval bound (`body_sha256` of the park): an edit since,
   an allowed account's too, is a person at work on it.
4. The issue has never had a planning session: no `planning` row (an ended
   session stays until the next one replaces it), no `plan_history` line,
   and no `tehas:refined` or `tehas:replaced` label.
5. The issue was not filed by a planning session: its body does not start
   with the `Replaces #<n>` line tehas writes (below). Having a parent is not
   the test: a person's epic has sub-issues too.
6. The park's question is still on record (`question` of the park) and the
   issue carries `tehas:question`. A tick of either box is a person's
   decision and wins: Retry starts a cycle in rule 4, and Leave it parked
   takes the question away there.
7. The planning limits hold as for a labelled session: no other planning turn
   running, `limit_plan_ok`, `slice_ok`, `mesh_ok`, not the control issue.
   When they do not hold, the session waits for a later poll; nothing is
   spent.

A cycle the fixer started, or one run by hand, is never refined
automatically: the store does not list it. No instance sets the fixer, and
#675 removes it.

When a check in 2 to 6 fails, the refine is spent and the issue stays parked
for a person, as the outage retry is spent. The log says which check it was.
The start spends it before the first turn, so a poll that fails after it
never starts a second session.

The session is started the way a `tehas:refine` label starts one
(`libexec/dispatch` near "start planning"): the same brief, the refine brief
after it, `plan_turn <issue> first`. Its session has `label: tehas:refine` and
`auto` (`plan-begin --auto`, schema 5), and so does each line of its history, so `tehas history` tells
an automatic refine from a labelled one. Analytics derives its `how` from the
label and counts both as `refine`; telling them apart there is later work. The start takes `tehas:question`
and `needs-human-attention` off, as a labelled start on a parked issue takes
`tehas:question` off.

The issue gets one comment before the first turn: the cycle parked with this
reason, the factory is refining it once on the strength of `<login>`'s
`tehas:ready`, and how to stop it: take `tehas:question` off while a question
waits, or close the issue, as for any session. The planner's questions follow as usual.

## What happens to the original issue

Today a refine session files a new issue and changes only labels on the
original. That stays, and both kinds of refine session, labelled and
automatic, add the links below. tehas makes them from the host after the turn,
from the `filed` list the turn's outcome carries, so they do not depend on what
the planner wrote:

- The filed issue's body gets a first line `Replaces #<original>`. The filed
  issue is the bot's own and has no approval yet, so editing it changes no
  approval.
- The filed issue is linked as a sub-issue of the original (GitHub's
  sub-issues API). Where the API is missing, the link in the body is enough.
- The original gets a comment `Replaced by #<filed>`.

A headless pi-rukas planning session files once: a second confirm files
nothing, and an answer after the filing is refused (`already-filed`, pi-rukas
`docs/headless.md`). So a refine session is over when its turn files. tehas
does not post the usual question with its one "End this planning session"
box after that turn; it makes the links, says `Replaced by #<filed>` with the
turn's own words, and ends the session itself. A filed address in another
repository gets no links and is named in the comment only. The label on the
original is the step that commits: the comment comes after it, so a poll that
fails halfway says nothing twice, and one that finds the label only closes
what is left.

Then one of two things. This holds for every refine
session, the ones a person starts with `tehas:refine` too: today a labelled
refine never closes the original, and after this change it does, so the
manual says so. A `tehas:plan` session is unchanged.

- **One issue covers the request** (no "Later parts" in the plan): the
  original gets `tehas:replaced` and is closed as not planned.
- **The plan lists later parts**: the original stays open with
  `tehas:refined`, so it is visible that more is to come. A person applies
  `tehas:refine` to it for the next part; the factory does not, because the
  original has now had a session (check 4). A person closes it when nothing is
  left.

Which of the two applies is read from the filed issue's body, which is the
shown plan byte for byte: the refine brief asks for a "Later parts" section.
A body with a "Later parts" heading that has a line under it other than
"None" has later parts. A body that cannot be read counts as having later
parts, and the original stays open. Closing it wrongly is the worse mistake.

`tehas labels` makes `tehas:replaced` and `tehas:refined`. `libexec/tidy`
does not take either off a closed issue.

## When it does not work out

An automatic session that ends without a filed ticket hands the issue back to
a person:

- A failed or cut-off turn already gets `needs-human-attention`
  (`lib/planning.sh`).
- At `TEHAS_PLAN_MAX_TURNS`, and when pi-rukas has lost the session,
  `plan_end` takes the factory's labels off today. For an automatic session it
  puts `needs-human-attention` back, so the issue reads as parked again.
  Labelled sessions stay as they are.
- A person who ends it, by taking `tehas:question` off or closing the issue,
  has decided. tehas adds nothing.

None of these starts another automatic session (check 4).

## The manual and the feature table

- `host/docs/operating.md`: the park section says an underspecified or
  too-large park is refined once on its own, when, and what the original and
  the filed issue look like after. "Refining a request" gets the links and the
  two labels.
- `host/docs/features.md`: a row "A cycle that cannot work an issue refines
  it", marked built and tested, not proven, until a host has run one.
- `host/docs/troubleshooting.md`: the `underspecified` and `too-large` rows
  say the factory refines once, and what to do when it has already.
- `host/docs/tools.md`: `tehas history` lines of an automatic session carry
  `auto: true`, where the history fields are listed.

## Tests

In `test/`, with GitHub stubbed by `t_stub`:

- `test/cycle_park_test.sh`: an `intent-park` with `underspecified` or `too-large`
  marks the park record; every other reason and cap does not.
- `test/dispatch_park_test.sh` and `test/dispatch_plan_test.sh` (or a new
  file if the builder splits it): the
  session starts with the refine brief and `by` set to the cycle's starter;
  each of checks 1 to 7 stops it, and 2 to 6 drop the mark; a second poll does
  not start a second session; a pause holds it; an issue with the
  `Replaces` line is not refined, a sub-issue of a person's epic is.
- `test/plan_test.sh` and `test/dispatch_plan_answer_test.sh`: a filed turn adds
  the body line, the sub-issue link and the comment; a single-part plan closes
  the original with `tehas:replaced`; a plan with later parts, or one whose
  outcome does not say, leaves it open with `tehas:refined`; a failed link is
  logged and does not undo the filing; the original is not closed while the
  session still asks; an automatic session at the turn limit ends with
  `needs-human-attention`, a labelled one as today.
- `test/labels_test.sh`, `test/tidy_test.sh`: the two labels are made, and
  tidy leaves them.

## What is left to the builder

- The name of the mark in the park record and of the `how` value.
- Where in the dispatcher's rule order the refine rule sits, as long as it is
  after the pause check and before the candidates are cut (rule 6), next to
  the outage retry.
- Whether the links are made in `plan_finish` or in `plan_post`, as long as
  a poll that fails halfway does not post twice (the rule at the top of
  `lib/planning.sh`).
- The exact comment wording, in the manual's plain style.
