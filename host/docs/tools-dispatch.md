---
title: tehas dispatch
description: "The one poll of the issue queue: who may start work, what a poll costs, and what holds work back."
---

# tehas dispatch

## tehas dispatch

One poll of the issue queue. Deterministic, no model. The timer runs the same
script as `tehasbot dispatch`.

```
tehas dispatch --dry-run       # report the decision, change nothing
tehas dispatch                 # start the cycles there is room for, then return
tehas dispatch --help          # print the usage of the command, call nothing
```

`--help` (or `-h`) prints the header comment of the command and exits 0; it
works even on a host with no instance settings yet. Any other argument, or more
than one, is refused: exit 64 with `tehas dispatch: unknown argument: <argument>`
and the usage on standard error, before the instance settings are read. A mistyped
`--dryrun` therefore starts no poll and changes nothing.

Order of rules: the pause switches (this host's flag, then the label on the
control issue) decide that this poll starts nothing. They do not stop it cleaning
up what this host already has: it still stops a cycle whose issue is closed or
whose pull request, the bot's on the issue's own branch, is merged or closed,
stops a cycle nobody has heard from, and
clears an issue marked `tehas:running` with no cycle behind it. The poll ends
there. For a factory that is not paused it goes on: a usable `tehas.slice`,
a model endpoint that answers and offers the model, free disk of at least
`TEHAS_CYCLE_MIN_FREE` on the file system of `~/tehas` and on Docker's own,
the free slots
(`TEHAS_TICKET_CONCURRENCY` less the running cycles, after that housekeeping),
the cap on issues awaiting merge or being worked on, the room, the containers'
ceilings and the memory pressure in the slice, then who applied `tehas:ready`,
then whether `tehas:blocked` holds it. The candidates come in this order: the
issues of the start order that are ready, in the order given; then
[refreshes](/operating#refreshes); then the other ready issues, lowest number
first. The start order is set with
[tehas order](/api#tehas-order). With no order, ready issues start lowest
number first. The order only sorts: an issue it names still starts only on a
`tehas:ready` or `tehas:blocked` from an allowed account, and every hold
applies as without it. With `--only`, a ready issue outside the order waits,
logged as `wait #N: not in the start order (--only)`; refreshes do not wait.
An expired order is logged as `order expired at <time>: ready issues start
lowest number first`, and one the poll cannot read as `WARN: the
start order could not be read from the state store: ready issues start lowest
number first`. Either way the poll goes on as with no order. The cap on issues
awaiting merge holds no refresh: it takes a pull request out of awaiting
merge as it starts. Who applied `tehas:ready` is read at the decision, when
the issue is picked. A `tehas:blocked` with no `tehas:ready` beside it
stands for one: who applied it is read the same way, after its blockers. A
`tehas:blocked` is checked then too, and costs two
GraphQL calls: the issue's last 100 comments, and the states of the issues
they name (see [What happens next](/operating#3-what-happens-next)). A poll never asks
GitHub the same question twice.

Every poll that gets past the bot's login ends by running `console tidy run`,
however the poll ends; the poll's exit status stays its own. It takes tehas's
state labels off closed issues: `tehas:running`, `tehas:awaiting-merge`,
`tehas:question`, `tehas:planning`, `tehas:refreshing`, `tehas:blocked`,
`tehas:ready`, `tehas:plan` and `tehas:refine`. Not `tehas:shipped`, not
`needs-human-attention`, and not the labels a person chooses (`tehas:lean`,
`tehas:prototype`). It is one console verb
(`console/ops/src/tidy.rs`), with the settings of the poll's environment: the list,
the searches, the decision of what comes off which issue and the edits are all
there. It makes one search,
for the closed issues
that carry one of those labels, up to a hundred a poll, the most recently updated
first, and gives it 60 seconds. It takes all of an issue's state labels off in
one edit and logs `tidy #N: removed <labels>`. The names it takes off are the
state labels above and no others, whatever the search sent back: the console
drops every other name before the plan reaches the edit or the log. An open issue is
never touched, and a closed one whose cycle still holds its lock on this host
waits for that cycle. The control issue `TEHAS_CONTROL_ISSUE` names is left out
of every edit: it is not work, whatever label it carries. To the issues it tidies
that do not carry `tehas:shipped` it adds the label, in the same edit, when the
last closed event of the issue is a merged pull request, or, when that event
names no pull request as its closer, when GitHub lists a merged pull request as
closing the issue. That list is what GitHub derives from the text and the links
of the issue, not what the close event recorded, so a pull request that merely
names the issue in its text is on it. One further GraphQL
search a poll answers that for all of them at once, and only when there is a
label to give, and it is the same read
[tehasbot status](/tools-health#tehasbot-status-and-the-console) makes, and the same console
code decides its answer (`console/ops/src/shipped.rs`).
The search is asked about at most a hundred issues, the page of that read: past
it an issue is not named and keeps its labels for a later poll. An issue whose
closer the read answered as a person or an unmerged pull request, and which
GitHub lists no merged pull request as closing, keeps no label: the state labels
still come off and the line says
`WARN: tidy #N: no merged pull request was read as the closer, tehas:shipped was
not added`, which covers both the closer that was read as unmerged and the
closer the read could not name as a pull request. When the read itself failed or
timed out, that line is said once for
the whole poll as `WARN: tidy: the read of what closed these issues failed: <reason>,
tehas:shipped was not added for N issues`, so a search GitHub refused reads
differently from a factory that shipped nothing. A dry run passes `--dry-run` and
logs `tidy #N: would remove <labels>` instead, and `would add tehas:shipped` for
the label it would give.
A search that fails, times out or cannot be read is logged
as `WARN: tidy: the search for closed issues with state labels failed: <reason>`, a
label change that fails as `WARN: tidy #N: could not remove <labels>`, and
neither stops the poll. The reason is gh's own words of the fixed list the
poll's lines use (`HTTP 502`, `timed out`, ...), and `(tried 2 times)` follows
it when the read was tried again: tidy's two reads follow the poll's rule of a
bad minute ([Operating](/operating)), and its edits are made once. Each failed read also goes on
the host's record of failed GitHub calls that
[tehasbot health](/tools-health#tehasbot-health) counts.

Every real poll that holds the dispatcher's lock ends with one sample in the
factory's analytics, `~/tehas/state/analytics.db`, when its work could have
changed the queue: a write to GitHub, a start, a stop or a planning turn. A
paused, held or failed poll that changed nothing takes none, so an idle factory
writes no row rather than the same reading again; `analytics prune` keeps its
schedule whatever the poll did. One consequence: the report counts each sample
for the time to the next, at most ten minutes (see
[`availability`](/tools-health#tehasbot-status-and-the-console)), so a stretch with no
sample - a long pause or an idle week - counts as not available rather than as
one reading held. The same rule bounds `energy`.
`~/tehas/libexec/analytics sample` reads the issues in line, running, awaiting
a merge, stopped and in planning from
`~/tehas/live/status.json`, the hold and the cycles allowed from
`~/tehas/live/dispatch.json`, and the mesh's draw from
`~/tehas/live/energy.json`. A file older than ten minutes gives no value, and a
`dispatch.json` older than two and a half, an earlier poll's, neither. Then
`analytics prune` removes what is older than 365 days. A start records the
events `ready`, as its approval record is written and at the time that record
says `tehas:ready` was accepted (`at`), and `started`, once its cycle came up,
the first time for each issue. Each call may take 5 seconds.
One that fails or runs out of time is a line `WARN analytics <subcommand>:
exit N` in the log and changes nothing in the poll. A dry run, a host with
`TEHAS_ANALYTICS=off` and a host where the program is not installed make no
call.

What holds cycles back is one `holding:` line: at every dry run, and in the
log once when its reason changes, with `released: cycles may start again`
when it ends. A real poll also writes it to `~/tehas/live/dispatch.json` with
the slice's numbers, for [tehas status](/tools-status#tehas-status), and, once the model
endpoint has been settled, the subagent provider's endpoint as one word
(`"endpoint": "main"` or `"backup"`), which is what
[`tehasbot energy`](/tools-health#tehasbot-energy) asks. Every real poll that
gets past the settings and the bot's login, a paused one too, writes
`~/tehas/live/demand.json`: how many issues would start if there were room,
and when the oldest of them was accepted, refresh requests included, with
`refreshHold` when refreshes are held, and `labelAlone`, the issues
`tehas:blocked` holds with no comment that names their blockers. It reads the `tehas:ready` list and
who labelled each issue for that, apart from what a start rests on. The holds are listed
under [Troubleshooting](/troubleshooting). Log:
`~/tehas/logs/dispatch.log` when run by systemd, the terminal otherwise. Each
cycle's runner output: `~/tehas/logs/dispatch-work-<issue>.log`.

When it has an issue to start, it puts the product checkout on the default
branch and fast-forwards it. If no cycle is running and the branch has moved
since the last indexing, it starts [tehas index](/tools-dev#tehas-index) as its own unit,
`tehas-index.service`, and ends: the log says `holding: the code index is
refreshed first`. The unit asks for a poll when it ends, and that poll starts
the cycles. Nothing about the issues changes in the poll that holds them, so a
ticked retry stays ticked. While the index runs, polls say `holding: the cycles
wait for the code index`, and planning turns and sweeps go on. An index that
could not be refreshed is a warning in the next poll's log; the cycles still
start. It then marks each issue `tehas:running` and starts
`tehas-cycle@<issue>.service`, which runs [tehasbot cycle](/tools-tehasbot#tehasbot-cycle)
from the cycle's own copy of the code.
It does not wait for the cycle. If the unit does not come up within 30
seconds, the issue gets `needs-human-attention`, the poll goes on to the
remaining issues, and it ends with exit 1.
The dispatcher runs the index and plan scripts and `tehasbot status` from its own directory,
`~/tehas/libexec`, not by a name on `PATH`.

The labels `tehas:plan`, `tehas:planning` and `tehas:question` exist in a
repository only after [tehas labels](/tools-setup#tehas-labels) has been run for the
version that uses them. After pulling a version that adds a label, run it
again on the host. It only creates or updates labels, so running it twice is
safe. Without it the labels are not there to apply, and a stopped cycle cannot get
its question label. The bot then says in a second comment that the boxes
cannot be used.

Beside the queue, a poll also reads two kinds of answer. A box ticked by an
allowed account in the bot's comment on a stopped cycle can start a fresh
cycle, and a reply or a ticked box on a planning question runs the next turn of
that session. A retry waits for a free slot and for the cap, as any cycle does.
A planning turn waits for neither. It runs as its own unit,
`tehas-plan-<issue>.service`, started with `systemd-run`, from its own copy of
the code as a cycle runs (`tehas-cycle pin plan`, see
[tehasbot cycle](/tools-tehasbot#tehasbot-cycle)), and the poll does not wait for it: cycles start while it runs, and the first poll after it ends posts
its outcome from the exit status it left in `~/tehas/state/plan-<issue>.json.rc`.
Only the questions need the list of issues that carry `tehas:question`. When
that list cannot be read, the poll logs `WARN: the issues with a question could
not be read, so no question is answered this poll`, answers no question and
starts no retry, and goes on to open a planning session and start the ready
queue as it does when the list is empty. Everything else it reads still ends
the poll when it fails: an entry of that list with no number, an issue's
comments, and the state store.
While the turn runs its unit holds `~/tehas/state/plan-<issue>.json.lock`, as a
cycle holds `state/cycles/<issue>.lock`; a poll that finds it held starts no
other turn. To count the running cycles the poll asks the state store once,
`store locks-held`, which tries every lock under `state/cycles` in one process
and names each one held or that it could not try. A held lock counts whether or
not the store has the cycle on record, and an old lock file costs no process of
its own. Without the store, the lock files are tried one by one. The one probe
is the library's (`tehas_core::lock`, `console/core/src/lock.rs`), and the
dispatcher, the health page, the cleaner and the store all ask it the same
question, so they answer the same way for one lock: no file and a file the probe
took are free, and a lock that is held, a lock that cannot be opened, and a name
at the path that is no regular file - a link or a directory - are all read as a
lock that holds. When the turn ends the unit leaves `state/dispatch.again`, which
starts a poll, as the end of a cycle does.
As the poll takes up a turn that ended, it records it in the factory's
analytics: `~/tehas/libexec/analytics plan <issue> --rc <how it ended> --facts
<file>` takes the session as the state store has it before the turn's end is
recorded, written to a private file for the call, and stores one run of kind
`plan`, with its
start, its end (when the poll took it up), its number in the session and its
outcome: `handoff` for a turn that asked its question, `stopped` for a session
Pi no longer has, `refused` for a turn that ran nothing, `failed` for any
other. pi's rpc mode reports no token usage, so a planning turn has none. A
call that fails is a line `WARN analytics plan: exit N` in the log, a copy
that cannot be written is `WARN analytics plan: no facts written`, and neither
changes anything in the turn.
A planning session is on the state store (`plan-show`): its turns, the
options of its open question, who started it and with which label, the
approved text, each answer with who gave it, the issues it filed and the
comments the poll spoke about once. An option is one label, kept as its
text: `plan-end` writes the options as a JSON list of the labels it was
given, and a stored options value that is not a JSON list of labels is
damage, refused by every read and write of the session with `corrupt` and
78, never shown as a question with no boxes. Each change is one write: `plan-begin` as
a session starts, `plan-answer` as an answer begins a turn, `plan-end` as a
turn ends, as its outcome is posted, and as the session ends. A turn's end the
store would not take keeps the turn's exit status, and the next poll takes it
up again and posts it once. The poll reads the open sessions once
(`plan-show --open`): an issue with one open starts no cycle, no refresh and
no retry after an outage, and a list that cannot be read ends the poll (exit 1
when the store is busy, 78 when it cannot be used). Read again after a
planning write of the poll, a list that cannot be read holds every issue for
the rest of that poll. A turn running on record is not begun again by a new
`tehas:plan`; the poll logs the issue and starts nothing.

One planning turn runs at a time. A comment is a question only if the host has its id on record
on the state store: the question of the issue's open planning session, or of
its current park (`question-known`). An answer that begins a turn answers that
question on record, so a second tick in it runs nothing. A question of an earlier attempt, one taken off the record,
one already answered and one of another issue are none of those: a box ticked
in it starts nothing, and it is unticked with a comment that says the factory
is not waiting on it. When the store cannot be read the issue is skipped for
that poll with `skip #N: its question on record could not be read (<word>)`:
no tick counts and none is undone. A store that cannot be used stops the poll
with exit 78. Only the last 100
comments of an issue are read; a question further back than that is skipped
with a line in the log. See [Operating](/operating). The dispatcher stops with exit 78 if
the bot's own login is in `TEHAS_ALLOWED_LABELERS`, and with exit 1 when GitHub
answers without one. Every poll reads that login once, before it reads a label
event or an answer and before it changes a label or comments, so a list naming
the bot is refused whatever else the poll would have done. A planning turn's
runner output is in `~/tehas/logs/dispatch-plan-<issue>.log`.

A park on the state store with the reason `provider-outage` starts again once
without a tick. `tehasbot cycle` no longer records that reason, because only a
cycle's own result names an outage; a park an earlier version recorded keeps
its retry. The store says the park owes one retry when the cycle
was started on an approval (not by hand) and was not itself that retry
(`park-show --automatic`). When nothing holds cycles back, so the model
endpoint answers and there is a slot, the poll starts the issue and says on it
that it does so once. The start rests on the account that started the cycle
that parked, as that start is on record in the state store, whatever approval
came after it: it must still be allowed, the issue must still be open and
parked, and nobody else may have changed its text since that cycle began.
Otherwise the log says `skip #N: its retry after a provider outage is not
started; it waits for a person` and the retry is spent on record (`park-retry`).
Any start of the issue spends it too, once the issue is marked and before its
unit is started, so a start whose unit never comes up leaves no retry behind.
A start whose mark fails spends nothing: the next poll still owes the retry.
`tehas stop` spends it as well. An
issue that carries `tehas:ready` is left to that label. If the retry's own
cycle is parked by an outage again, it waits for a person, and so does a cycle
started by hand: its comment says so. A list of parks that cannot be read ends
the poll, with exit 1 when the store is busy and 78 when it cannot be used.

A poll also reads the open issues marked `tehas:running`. One whose cycle
does not hold its lock on this host, `~/tehas/state/cycles/<issue>.lock`, gets
`needs-human-attention` in place of `tehas:running` and a comment that says
where the run log is. The log line is `park #N: it is marked tehas:running and
no cycle holds its lock`. The issue is read again first: one whose cycle
ended while the poll ran has lost `tehas:running` already and is left, with
the line `skip #N: tehas:running is gone from it since the list was read`. If
the label change fails nothing is said on the
issue and the next poll tries again. A closed issue in that state only loses
`tehas:running`. For either, a container `tehas-work-<issue>` that is still
there is removed, with the line `removed the container tehas-work-N: no cycle
holds the lock of #N`: its client died, and it would work on with the bot's
token and hold the name. The container of a planning turn that was cut off,
`tehas-plan-<issue>`, is removed the same way before the turn is reported. A
paused factory leaves such an issue as it is. Run the dispatcher only on the host that runs the cycles: on any other
host no cycle holds a lock.

The other way round, a cycle that holds its lock but has not been heard from
for 20 minutes is stuck, for example a `tehas work` that hangs after its
container ended. Heard from means it recorded a heartbeat on the state store,
which `tehas work` does every 10 seconds from the host, also while a long gate
runs in the container, or that it took its lock. The poll logs `WARN: the cycle for #N holds its lock but
has not been heard from since <time>, more than 1200s: stopping
tehas-cycle@N.service` and stops the unit without waiting for it. The cycle
reports as it stops; a later poll finds the issue as above. This is said and
done once for each time the cycle was last heard from. The episode and every
ask are on record in the state store, with the cycle: the time the cycle was
last heard from, how many times the poll has asked, and when it last asked or
said. The first "asks no more" line counts as one more, so a record past 6
asks means the poll has said that line. Before each ask the poll writes `quiet`
to `state/cycles/N.done`, so the cycle that reports as it stops names the stop:
its comment carries `The dispatcher stopped it: nothing was heard from it for
longer than its quiet bound.` and its end record carries `park_reason` `quiet`.
A `.done` that cannot be written adds `WARN: could not write the reason for #N`
to the poll's log and the stop goes ahead; the cycle then reports as one stopped
without a pull request and says nothing of the quiet bound.
Exit 0 from `systemctl --user --no-block stop` only says systemd accepted the
stop job, so the record means the stop was asked, not that the unit stopped;
what ends a unit that ignores the job is the unit's own `TimeoutStopSec` (300s)
and `RuntimeMaxSec`. A stop that failed or timed out adds `WARN: could not stop
tehas-cycle@N.service (exit <code>): it holds its slot, and the poll asks
again`. While the cycle holds its lock the poll asks again, at most once every
10 minutes (`WARN: ...: the poll asks tehas-cycle@N.service to stop again`),
and after 6 asks it says the episode needs a person and asks no more, so one
wedged unit does not hold the poll. That line comes 10 minutes after the last
ask, and again once an hour while the cycle stays quiet. A cycle heard from
again ends the episode; when it goes quiet later, that is a new episode and
the asks count from one. The cycle's next start begins with no episode. Each ask
is recorded just before the stop is asked, so a slow stop does not put off the
next ask. When the store is busy and cannot record the episode, the poll says
`WARN: ..., but the state store could not record it (<word>), so the poll asks
nothing this time` and stops nothing on that poll. A store that cannot be used
ends the poll with `ERROR: the state store cannot be used: ...` and exit 78. A
cycle whose record has ended, or that has none, may be starting or reporting
after its end, so the poll does not stop it. While it stays quiet, every poll
says `WARN: ..., and the state store has no open record of it, so the poll
does not stop it: if it stays, a person can, with tehas stop N or systemctl
--user stop tehas-cycle@N.service`. A dry run reads the record and says what
the poll would do (`would stop`, `would ask ... to stop again`, `would say
that ... needs a person`, or the line about no open record), stops nothing and
records nothing. A paused factory does not stop it.

An issue is taken only when an allowed account applied `tehas:ready` and
nobody else has changed its title or body since, and only while GitHub says the
number is an open issue. A closed issue and a pull request are never taken,
whatever label they carry. The poll's lists are of open issues, so what this
catches is a number that changed since the list was read. The poll asks GitHub
what a number is once a poll, and only for an issue its labels would otherwise
take; the skip is said once a poll at most (`skip #N: it is closed, and a
closed issue is never taken`, or `skip #N: it is a pull request, not an issue
the factory works on`). An answer that cannot be read ends the poll with exit
1.

An issue that an open pull request closes is not taken: a cycle would find
the work done after paying for its explore and plan. GitHub links the two when
the pull request's description says `Closes #N` (or `Fixes` or `Resolves`) and
the pull request targets the default branch, or when a person links them by
hand. A pull request of another branch, stacked on another, earns no such
reference, so the poll reads the open pull requests' own descriptions, all of
them in lists of 100 from the queue's first page on, and holds an issue one of them names with a closing keyword,
whatever branch it targets: a keyword and the references held to it by commas
or the words `and` and `or`, so `Closes #7, #8 and #9` holds three issues. The
text is a description nobody has reviewed, from any author: all it can do is
hold an issue back, never start one. The poll asks once, for an issue it would
otherwise take, takes `tehas:ready` off and says on the issue which pull
request it is. The log line is `skip #N: pull request <url> is open and closes
it`, the same whether GitHub linked the two or the description says so. A
merged or closed pull request holds nothing back. A
page that answered no list of open pull requests holds nothing on a body, and
a call that fails, an answer that cannot be read, or more than 5000 open pull
requests, ends the poll with exit 1.

A `tehas:ready` from any other account asks for nothing, so it comes off the
issue with a comment naming who
applied it; the log line is `clear #N: tehas:ready off, applied by ...`. A
`tehas:plan` from such an account comes off the same way. The
comment follows the label change: when that change fails nothing is said on the
issue and the next poll tries again. An issue the poll does not read as a
candidate keeps its labels as they are, which covers one whose own cycle is
alive and every issue while a hold applies. No other label holds it
back: `tehas:awaiting-merge` and `tehas:question` are taken off as the cycle
starts. `needs-human-attention` is left for the cycle, which starts with
`--restart` and has pi-rukas take it off. While the issue's own cycle is alive the log
says `wait #N: its cycle is still running`, and the issue is taken once that
cycle has ended. Every list of issues is read up to 1000. A cycle starts only once the
issue is marked `tehas:running`; if that label change fails, the poll stops.

Before it marks an issue, the poll records what the start rests on in the
state store (`~/tehas/state/tehas.db`, through `libexec/store`
`approval-record`): `at` and `by`, when and by whom it was approved, `how`
(`label`, `retry-tick`, `millwright`, `refresh`, or `outage-retry` for the one start
after a provider outage), `body_sha256`, the sha256 of the body that approval
was checked against with carriage returns dropped, `title_sha256`, the sha256
of the issue's title held the same way (line ends dropped), and as the start's
inputs `links`, the links to GitHub's attachment storage in that body, and
`driver` when the start [chose lean](/operating#choosing-the-driver). A record
with no `driver` is classic. `at` is
the time GitHub gave the label or the tick; a refresh and the retry after an
outage rest on an approval that started a cycle already, and are on record at
the time the poll decided them. The body, the title and the record of who last
edited it come in the same call, so the record is the text the check passed.
An issue has one approval waiting at most, and an approval starts one cycle:
the cycle's start uses it, and a poll that finds the same approval used says
`skip #N: its approval of <at> started a cycle already` and starts nothing.
The approval is taken off the record again when the start gives up after it:
the label cannot be changed, the attempt on record cannot be read, or the
unit does not come up. The poll reads the issue's attempt on record before
the label, starts the unit, and waits up to `TEHAS_CYCLE_START_WAIT` seconds
(30), by the clock, for a later attempt: the cycle starts its record once it
holds its lock. A store that is busy is asked again within the wait. Without
a later attempt by then, a cycle that holds the issue's lock is taken as
started, with `WARN: the cycle for #N holds its lock but did not say so in
30s; taking it as started`, and anything else is a start that did not happen.
The undo is asked three times while the store is busy. When the approval
still stays on record, the poll stops `tehas-cycle@N.service` before it
labels the issue, so a unit that comes up late cannot use it.
A state store that is not installed stops the poll before it asks GitHub
anything: `ERROR: the state store is not installed (run install.sh)`, exit
78. An answer that names no title records no
`title_sha256`, and `tehas work` then checks the body alone. With `TEHAS_WORK_COMMENTS=1` the poll also lists the
issue's comments and adds `comment_links`: the links of each comment an
allowed account wrote whose `created_at` and `updated_at` are at or before
`at`, in the order written, each as `{link, source, by}` with `source`
`comment:<id>`. [tehas work](/tools-work#tehas-work) fetches those links and no others.
An approval the store does not take starts nothing: `ERROR: could not write
the approval record of #N (<the store's word>), not starting a cycle`, exit 1,
and no label changes.
Comments that cannot be read are logged as `ERROR: the comments of #N could
not be read` and stop the start the same way. On a retry tick, with the
setting on, the poll also lists the bot's pull requests that name the issue,
as `tehasbot cycle` finds a cycle's, and their comments: those an allowed
account wrote between the park question and the tick and nobody edited after
the tick. Their links join `comment_links` with `source` `pr-comment:<id>`,
and their text, the first 20 cut at 4000 characters each, goes to
`retry_context` as `{source, by, at, text}`. Pull requests or comments that
cannot be read stop the retry: `ERROR: the comments on the pull request of
#N could not be read`, exit 1, and the tick stays. Only conversation comments
are read, not review comments or reviews.
