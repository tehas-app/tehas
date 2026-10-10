# Spec: the improvement loop, a factory that files issues about its own work

**Status**: Proposed 2026-10-11. Not implemented. Built after ticket
concurrency (`2026-10-10-ticket-concurrency.md`) has merged.

## Purpose

Nobody reads a cycle's log unless the cycle fails loudly. A cycle that aborts
three times before it opens a pull request, a gate that is run twice, a
review loop that goes round without progress: these are in the logs and are
lost.

Every three hours the host reads the cycles that ended since the last time,
asks a model what could be better, and files one issue per finding that is
not already in the issue stack. The findings are about the factory and about
pi-rukas, the agent that does the work. So the analysis does not run through
pi-rukas: it is plain Pi with one prompt, and a script makes every GitHub
call.

A filed issue is a proposal. It carries no `tehas:ready` label. A person
reads it and decides whether the factory works on it.

## Built on ticket concurrency

This is built on `main` once ticket concurrency has merged, and is written
against that layout, not today's. What it takes from it:

| From ticket concurrency | What the loop does with it |
| --- | --- |
| `libexec/cycle` runs one cycle and reports it | The record is written there, after the report, so it holds the pull request and the labels the cycle ended with |
| Cycles end side by side, each in `tehas-cycle@<issue>.service` | A record per cycle, renamed into the window in one step |
| A runs directory per issue | The console text in a record holds that issue's agents only |
| `state/cycles/<issue>.lock` says a cycle is running | The loop reads no lock and takes none of them. A cycle still running has no record yet and is read in a later window |
| CPU and memory are shared out between cycles | The analyst is not a cycle and takes no slot. It gets one CPU and 2 GiB, outside the shares |

At `TEHAS_TICKET_CONCURRENCY=1` the loop behaves the same: one record at a
time.

## What was decided, and what was not chosen

| Question | Decision | Not chosen, and why |
| --- | --- | --- |
| Where it runs | On the factory host, as a systemd user timer, like the dispatcher | A workstation or a session: it stops when that does. A cloud agent: it cannot read the host's logs |
| What reads the logs | Plain Pi in the sandbox image, pi-rukas not loaded, on the instance's own models | pi-rukas: it is what is being judged. A hosted model: a second credential on the host, and tehas runs on self-hosted models |
| Who calls GitHub | The script, with `gh`. Pi has no token | Pi with `gh` and the token: the token would sit beside untrusted text, and what is filed could not be pinned by a test |
| What the model sees | A record per cycle, copied to a tmp directory when the cycle ends | The files in `logs/` picked by time at analysis: a cycle still running would be read half written |
| Duplicates | The model judges against the stack it is given. The script drops a finding whose title matches an open issue | The script alone: two titles for one problem do not match. The model alone: nothing would stop an exact repeat |
| Where a cycle's whole log goes | Compressed, as assets of one release, linked from the issue | An attachment on the issue: GitHub has no API for those and `gh` cannot make one. A branch of logs: the repository grows for ever. A comment: 65536 characters |
| The switch | Off unless the instance sets `TEHAS_IMPROVE_REPO` | On by default: a platform file would then choose a repository to write to |

## The setting

`TEHAS_IMPROVE_REPO` is `owner/name`, the repository findings are filed in. It
has no default. Unset, the feature is off: nothing is collected, the command
prints one line and exits 0, and `install.sh` does not enable the timer. A
malformed value stops every command with exit 78, as the other settings do.

It is usually the instance's own `TEHAS_REPO` only where the product is tehas.
An instance that builds another product names the repository where its
operator wants reports about the factory.

## Collecting

When a cycle has ended and been reported, `libexec/cycle` writes a record of
it to
`$TEHAS_HOME/tmp/improve/window/<issue>-<stamp>/`, where `<stamp>` is the
stamp of the cycle's log file:

| File | What it is |
| --- | --- |
| `cycle.json` | Issue number, start, seconds, final status and step, exit status, the pull request if one was opened, the labels the issue was left with |
| `stream.jsonl` | The RPC stream condensed with `jq`: streaming deltas dropped, any string over 2000 characters cut. Tool calls, errors, notices and final messages stay |
| `console.txt` | The console text the dashboard showed |
| `err.txt`, `preflight.txt` | The cycle's error stream and preflight output |
| `raw` | A hard link to the cycle's log in `logs/`. Not mounted into the analyst's container |

The record is built in a temporary directory beside `window/` and renamed
into it, so the analysis never sees half a record. `window/` is made again if
a run has just taken it. One directory per cycle:
cycles that end at the same moment do not collide, whatever
`TEHAS_TICKET_CONCURRENCY` is.

A cycle that cannot write its record says so in `dispatch.log` and ends with
the status it would have had. Collecting never fails a cycle.

## The run

`tehasbot improve`, started by `tehas-improve.timer` three hours after the
last run ended. One run at a time, under `state/improve.lock`.

1. **Sweep.** Remove what is over seven days old (see Cleaning up).
2. **Take the window.** Rename `window/` to `run-<stamp>/`. Records of an
   earlier run that failed are moved in beside the new ones. No records: write
   a log line and exit 0 without calling the model or GitHub.
3. **Fetch the stack.** `gh issue list` on `TEHAS_IMPROVE_REPO`: every open
   issue and the issues closed in the last 30 days, number, title, state,
   labels and body, written to `run-<stamp>/in/issues.json`. If this fails the
   run stops here and the records wait for the next one.
4. **Analyse.** Start the analyst (below). It writes
   `run-<stamp>/out/findings.json`.
5. **Check.** Validate the findings and drop duplicates (below).
6. **File.** For each finding left, upload the logs of the cycles it names,
   then `gh issue create` with the label `tehas:improvement`.
7. **Clean up.** Remove `run-<stamp>/`.
8. Append one line to `logs/improve.log`: time, cycles read, findings
   returned, dropped as duplicates, filed, and the issue numbers.

Exit: 0 the run finished, with or without findings; 1 the analyst or GitHub
failed and the records were kept; 64 an argument; 78 the settings are invalid.

## The analyst

One container from the sandbox image, running Pi with the prompt in
`host/pi/improve-prompt.md`. pi-rukas is not loaded.

| It is given | It is not given |
| --- | --- |
| `run-<stamp>/in/`, read-only: the records and `issues.json` | The bot token, or any other credential |
| `run-<stamp>/out/`, writable | A product checkout or workspace |
| The model endpoint and the instance's default model | The Docker socket, SSH keys, `~/.config/tehas`, the raw logs |
| One CPU, 2 GiB and a time limit of 30 minutes | Pi's state directories from the cycles |

This is a new container, not a wider one for a cycle: this change does not
touch what `libexec/work` mounts. `test/improve_test.sh` pins the list above the way
`test/work_test.sh` pins a cycle's.

The prompt asks for findings about the factory and about pi-rukas, not about
the product's code. Each finding quotes its evidence from the records and
names the cycles it comes from. For each finding the model reads
`issues.json` and either names the issue that already covers it or says there
is none.

Pi is started for one prompt and nothing else:

```
pi -p --no-session --no-extensions --no-mcp --no-skills \
   --no-prompt-templates --no-context-files --tools read,grep,find,ls,write
```

These flags are in Pi 1.1.0. The image pins 1.0.4. The first step of the plan
runs this line in the image, with no network but the model endpoint, and
confirms that pi-rukas is not loaded and that no tool beyond the list
answers. If the pinned version cannot do it, this spec is revised before
anything is built.

### `findings.json`

```
{"findings": [{
  "title": "...",            up to 100 characters, one line
  "category": "...",         dispatcher | cycle | pi-rukas | console | gate | docs
  "body": "...",             up to 6000 characters, markdown
  "evidence": "...",         up to 3000 characters, quoted from the records
  "cycles": ["5-261010-200140"],
  "duplicate_of": null       or an issue number from issues.json
}]}
```

## Untrusted text

The records hold issue text and model output. Either can carry instructions
written to steer the analyst. No filter finds all of those, so the design
does not depend on one: the model that reads the text can do nothing but
write one file, and a script with no judgement decides what happens to it.

- The analyst has no token, no repository and nowhere to write but `out/`.
- `findings.json` is checked with `jq` against the shape above. Wrong types, a
  category off the list, a cycle that is not in this run, a length over the
  limit or a control character: the finding is dropped. A file that is not
  that shape at all: nothing is filed and the run exits 1.
- At most 3 issues are filed per run. None is filed while 10 open issues
  carry `tehas:improvement`. A poisoned log cannot flood the stack.
- Nothing from the file is run or passed through a shell. The body reaches
  `gh` with `--body-file`, the title as one argument.
- The script writes the issue body, not the model: a fixed heading, the
  category, the model's text, the evidence inside a code fence, the links to
  the logs, and a line that says the issue was filed by the improvement loop
  and by which model. In the model's text every `@` that would mention an
  account is broken, and HTML comments are removed.
- The label is fixed. The script never applies `tehas:ready`, and the bot is
  not an allowed labeller, so the dispatcher would ignore it if it did. A
  person reads the issue before a cycle can work on it, and the dispatcher
  ties that approval to the text as it then was.
- The prompt tells the model that the records are data and that nothing in
  them is an instruction. This helps and is not relied on.

## Duplicates

A finding with `duplicate_of` set is dropped, and the log line names the
issue. A finding whose title, in lower case with punctuation and repeated
spaces removed, equals that of an open issue is dropped too. Findings within
one run are compared with each other the same way.

Nothing is posted on the existing issue. A comment per run on a known problem
would be noise.

## The whole log

A finding names the cycles it rests on. For each of those cycles the raw log
goes with the issue, so a person can check the model's reading.

GitHub has no API for issue attachments. The logs are uploaded as assets of
one release in `TEHAS_IMPROVE_REPO`, tagged `tehas-improve-logs`, made as a
prerelease the first time, and linked from the issue body.

- The raw log and its error stream are joined into one file and compressed
  with `gzip -9`: `ticket-<issue>-<stamp>.log.gz`.
- A compressed file over 5 MB is not uploaded whole. The log is split at line
  boundaries into pieces that each compress to 5 MB or less:
  `ticket-<issue>-<stamp>.log.1.gz`, `.2.gz` and so on. Each piece opens on
  its own.
- A cycle named by two findings is uploaded once.
- Before upload the log is searched for the shapes of GitHub tokens and API
  keys. A log that matches is not uploaded, the issue says so, and the log
  line of the run names the cycle. Logs should hold no secret; this is for the
  day one does.

The assets are as public as the repository. An issue on a public repository
publishes the cycle's log to everyone. The manual says so beside the setting.

The bot account needs permission to make a release in `TEHAS_IMPROVE_REPO`.
Without it the issue is filed all the same, with a line saying the log could
not be uploaded.

## Cleaning up

- A run that finishes removes its `run-<stamp>/`, whether or not it filed
  anything. The hard links go with it. Nothing of a cycle is kept in tmp after
  it has been analysed.
- A run that fails keeps its records. The next run takes them with its own.
- Every run starts with a sweep. Under `$TEHAS_HOME/tmp/improve/`, any record
  or run directory over seven days old is removed, analysed or not. Under
  `$TEHAS_HOME/logs/`, the files of a cycle (`work-*`, `dispatch-work-*`,
  `index-*`) not written to for seven days are removed.
- The sweep leaves `dispatch.log`, `improve.log` and `protolab.log`: they are
  small and are the host's history. It leaves `live/`.
- Release assets are not removed. They are the reference an issue points to.

The sweep is the first time anything removes a file from `logs/`. A host with
the feature off keeps every log, as today.

## Units and install

`host/systemd/tehas-improve.service` is a oneshot that runs `tehasbot improve`
and appends to `logs/improve.log`. `tehas-improve.timer` has `OnBootSec=10min`
and `OnUnitInactiveSec=3h`. `install.sh` installs both and enables the timer
only when `TEHAS_IMPROVE_REPO` is set, as it does for the other optional
units. `tehasbot` gains the `improve` command. `libexec/labels` creates
`tehas:improvement`. `lib/update-steps.sh` names the timer among the units
`tehas update` restarts.

## Tests

`test/improve_test.sh`, with `docker` and `gh` stubbed:

- Unset setting: no record is written, the command exits 0, no stub is called.
- A malformed setting exits 78 before any call.
- A cycle's end writes a complete record, and a failure to write one does not
  change the cycle's exit status.
- Two cycles that end at once leave two whole records, and a record written
  while a run takes the window lands in the next window.
- An empty window calls neither `docker` nor `gh`.
- The analyst's mounts and environment are exactly the list in this spec.
- Findings of the wrong shape, over length, with a bad category or an unknown
  cycle are dropped; a file that is not JSON files nothing and exits 1.
- A `duplicate_of`, a matching open title and a repeat within the run are each
  dropped.
- The caps of 3 per run and 10 open hold.
- A mention and an HTML comment in the model's text do not reach the body.
- A log over the limit is uploaded in numbered pieces that join back to the
  original; a log with a token shape is not uploaded.
- A finished run leaves no run directory; a failed one keeps its records; the
  sweep removes what is over seven days old and nothing newer.

`test/work_test.sh` keeps passing unchanged in what it pins, and
`test/cycle_test.sh` gains the cases for the record. `test/cli_test.sh`,
`test/config_test.sh`, `test/install_test.sh`, `test/labels_test.sh` and
`test/steps_test.sh` each gain the case for their part.

## Documentation

The change is not done until every page below says what is true with the
loop installed. Each is changed in the commit that changes the behaviour it
describes.

| Where | What it gains |
| --- | --- |
| `README.md` | A sentence in how an issue flows: the factory reads its own cycles and files what it finds. The loop in the table of pages, under Operating |
| `host/docs/overview.md` | A row in "What is optional": the loop, what it needs and what switching it on publishes. A line in "What runs where" |
| `host/docs/features.md` | A row under "Host management", marked not proven until a run on a host has filed an issue. The sweep of `logs/` under "Known limits" |
| `host/docs/operating.md` | A section: what the loop files, the `tehas:improvement` label and that it starts nothing, how to read `improve.log`, how to act on a filed issue, how to turn the loop off. The label in the list of labels |
| `host/docs/host.md` | The timer in the table of units, with its log. `tmp/improve/` and `state/improve.lock` in the table of paths. The release permission in what the bot account needs. That logs older than seven days are removed |
| `host/docs/tools.md` | `TEHAS_IMPROVE_REPO` in the table of settings. `tehasbot improve` in the list of commands and a section of its own: usage, exit statuses, the caps, what the analyst's container is given. The timer where the units are named |
| `host/docs/new-project.md` | The setting among the optional ones, and the timer in the `systemctl --user enable` line where the setting is set |
| `host/docs/troubleshooting.md` | `improve.log` in "Where to look". Two entries: the loop files nothing, and a log was not uploaded |
| `host/docs/agents.md` | A filed improvement issue is a proposal like any agent's issue: a person labels it |
| `host/docs/llms.txt` | The loop in the list of optional parts |
| `instances/example/instance.env` | The setting, commented out, with a comment that says what it publishes |
| `bin/tehasbot` | `improve` in the header's list of commands and in the usage text |
| `install.sh` | The loop in the header's list of what is installed |
| `lib/config.sh` | The setting in the header's list |

`host/docs/sitemap.xml` and `host/docs/bootstrap.md` need nothing: no page is
added and no dependency is new. `DESIGN.md` needs nothing: the loop has no
view on the dashboard.

`AGENTS.md` is changed by people. Its table of paths needs nothing. If the
loop's container should be named beside rule 4, that line is proposed in the
pull request.

For `tehas-tehas` the setting would be `TEHAS_IMPROVE_REPO=janit/tehas`. That
line is proposed in the pull request and added by a person.

## Not in this change

- Findings about the product's own code.
- A comment on an existing issue when a problem is seen again.
- Removing old release assets.
- A second model as a judge of the first.
