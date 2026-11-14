---
title: tehas work
description: "One cycle for one issue, run by hand: its container, the links its issue carries, and its report."
---

# tehas work

## tehas work

One pi-rukas `/work` cycle, headless, in the sandbox image.

```
tehas work <issue> [--restart] [--driver lean|classic]
```

The container's command is pi-rukas's one-shot runner,
`/opt/pi-rukas/bin/pi-rukas-headless work <issue> --timeout <minutes> --json`,
with `--restart` added for `--restart`. On driver lean the verb is `work-lean`
in place of `work`, with the same flags, mounts and variables. The driver is
the one the start's approval binds (`tehas:lean`, see
[Choosing the driver](/operating#choosing-the-driver)), else classic.
`--driver` names it by hand and wins over the approval's; any other word is
exit 64. The runner is given
`TEHAS_WORK_TIMEOUT` in whole minutes and stops itself there; a runner that
has not stopped `TEHAS_WORK_GRACE` seconds (300) later is stopped with
`docker stop`. `tehas work` waits for the runner to exit and reads nothing
from the workspace `~/tehas/work/<name>-<issue>`: the outcome is the runner's
`--json` result, which goes to `~/tehas/state/cycles/<issue>.result.json`, and
a copy beside the log. The contract is pi-rukas's `docs/headless.md`: exit
codes v1, result schema 5, NDJSON progress with a stable subset. The last step
of `tehas work` is the watch of `tehas-cycle work`, which the console execs
with the settings loaded. It takes the issue's lock, or under `tehasbot cycle`
finds it already held by the cycle, and hands it down to every program it
starts: the same process and the same lock start the container, watch it to
its end and leave the cycle's notes. A second `tehas work` of the same issue
exits 5 and touches nothing.

A lean start on an image that cannot run it as asked stops with exit 78, the
note `state/cycles/<issue>.lean-refused` and the lines below, which end
`(driver lean, work-lean; it does not run classic in its place)`. It starts no
container and moves nothing. `tehasbot cycle` reads the note and parks the
issue as a start that never ran: its approval goes with the report, so nothing
starts it again on its own. Any other 78 is a stopped cycle. A refresh whose
pull request conflicts with the branch, on an image whose capabilities do not
set `continuePrMergeVerify`, stops with 78 after the workspace is made and
before the push check, with the note `state/cycles/<issue>.refresh-conflict`
naming the files; `tehasbot cycle` parks it as a stopped refresh, reason
`refresh-conflict` (see [Refreshes](/operating#refreshes)). A lean run is
bounded by the same `TEHAS_WORK_TIMEOUT` and deadline as classic; its `ci`
step waits on the pull request's checks inside that limit.

Before anything is moved or fetched, it asks the image what it supports,
`pi-rukas-headless --capabilities`. The cycle starts only when the image speaks
exit codes 1, result schema 5 and stable NDJSON 1, the `work` verb takes the
flags passed, and every `PI_ENSEMBLE_*` variable the container would be given
is an input the image lists for a host to set (scope `cycle` or `sandbox`; a
family such as `PI_ENSEMBLE_MODEL_<ROLE>` covers the members the image lists
for it, or any name when it lists none, as v0.12.80 does), and the answer's
`features` say `true` for `handoffChannel`, for `cycleDeadline` when the cycle
has a deadline and for `restartClearsAttention` with `--restart`. On driver
lean the flags are checked against the `work-lean` verb, the features are
`leanHandoffHost` and `leanDeadline`, and `workLeanResultSchema` must be 5. A
lean `--restart` or refresh (`--continue-pr`) is refused unless the image lists
the flag for `work-lean`, which v0.14.47 to v0.14.53 do not. Anything else
is exit 78, with what is not supported named:

```
tehas work: tehas/pi-rukas:uid1000 does not support what this cycle is given, not starting a cycle:
    the input PI_ENSEMBLE_SOMETHING
```

Then each entry of `TEHAS_RUKAS_WORKAROUNDS` is checked against the image's
pi-rukas version: one whose release it has reached, or any entry when the
version cannot be read, is exit 78:

```
tehas work: TEHAS_RUKAS_WORKAROUNDS names a workaround this image no longer needs, not starting a cycle:
    PI_ENSEMBLE_COMMENT_RETENTION works around a pi-rukas defect fixed in 0.12.80, and the image's pi-rukas is 0.12.80
    Remove the setting and its entry from /home/tehas/.config/tehas/instance.env.
```

Five inputs are this command's, whatever the instance says:

| Input | Value | Why |
| --- | --- | --- |
| `PI_ENSEMBLE_HANDOFF_CHANNEL` | `host` | pi-rukas posts no handoff comment and applies no label. `tehasbot cycle` posts the one comment of the park from the result's handoff, and labels the issue |
| `PI_ENSEMBLE_HANDOFF_TIMEOUT_MS` | `480000`, 8 minutes | The handoff allowance: the time the cycle's handoff may take. It is the one allowance tehas and pi-rukas share. The deadline's margin holds it, so the handoff is written before the container is stopped, not as it is stopped |
| `PI_ENSEMBLE_CYCLE_DEADLINE` | the start plus `TEHAS_WORK_TIMEOUT` less the handoff allowance, in milliseconds since the epoch | After it the cycle starts no new step and parks with the cap `deadline`, with a handoff. The runner bounds every step by the deadline: with the default limit the deadline is 5 h 52 min in. A limit no longer than the handoff allowance gives no deadline, and the log says so |
| `PI_ENSEMBLE_VERIFY_CONCURRENCY` | one for each two whole CPUs of the container's share and each 2 GiB of its memory ceiling, the lower, at least 1 | How many runs of the gate the cycle makes at once. pi-rukas's own rule for a host, applied to the container. On 16 CPUs with four cycles side by side that is 1 |
| `PI_ENSEMBLE_SPAWN_TIMEOUT_MS` | `TEHAS_CHILD_TIMEOUT` times 1000, 7200000 by default | The bound on one child of the cycle: pi-rukas's backstop for a child nothing else ended. tehas gives it so a fixer still making progress is not stopped at pi-rukas's own two-hour default with hours of the cycle's budget left. An instance line for the name is dropped before tehas' own is added |

It holds the issue's lock, `~/tehas/state/cycles/<issue>.lock`, while it runs.
Run by hand for an issue whose cycle is running, it refuses before it moves
anything (exit 5). Under `tehasbot cycle` it is handed that lock, already
held, and goes on. While a run by hand holds the lock, the dispatcher counts it
as the issue's cycle and starts no other for the issue. With the lock held, a
container left under the name `tehas-work-<issue>` is no cycle's: it is
removed before the new one starts.

Before it clones a workspace, and before it writes anything of its own, it
compares the issue with what was approved. The dispatcher recorded that in the
state store before it labelled the issue `tehas:running`. Under `tehasbot
cycle`, which hands it the attempt of its start as `TEHAS_CYCLE_ATTEMPT`, only
the approval that start used counts; a start the store has as one by hand is a
run by hand, and anything else, a store that cannot be read among it, stops the
start with exit 4 and is never taken for a run by hand. Run on its own, the
approval of the issue's latest start counts, and with none the run is by hand.
`tehas work` reads the issue once, whatever the instance gives the cycle, and
holds its body to the approval's `body_sha256`.
That one read is also the one its images and text files are fetched from, so a
start asks GitHub for the issue once.
The read may take `TEHAS_TEXT_WAIT` seconds (60), and every comment that names
why a start stopped is bounded the same way: a read that did not end in time is
said as a timeout and one GitHub refused is said as that, with gh's own words
in `work-<issue>-<stamp>.jsonl.err`.
A body that is not the approved one, a record that cannot be read,
an answer that is a pull request or a closed issue, and an issue that cannot be
read each stop the start with 4, and the issue is
told which and that `tehas:ready` records a fresh approval. What the issue is
told names nothing of this host: the path of a record that could not be read is
said on the command's error stream only. A state store that is not installed
stops every start with exit 78 before GitHub is asked: `tehas work: ERROR: the
state store is not installed (run install.sh)`.

Under `tehasbot cycle`, as the container starts, the image's id, the pi-rukas
commit of its label `tehas.rukas` and the settings the cycle is given go on the
cycle's record in the state store (`cycle-image`), for its report. A store that
is busy or does not answer in time is asked once more a second later. One that
is still busy only costs the record: the command says so on its error stream,
with the image id the container runs, and the cycle runs. Any other refusal stops the start with
exit 4, and the issue is told `Tehas started no cycle: its record on this host
could not be updated as it started`. A run on its own has no attempt and
records no image.

Before the container starts, the command clears what a cycle of the issue left
under `state/cycles/<issue>.*`. A cycle's end clears these notes itself, so
what is still there comes from a cycle that was killed before its end: the
command names it on its error stream with the first line of each (the
container id, the counts of out-of-memory kills and refused forks, the signal)
before it removes it. A container under the cycle's name that outlived its
client is removed with `docker rm -f`, and that is said too, as is a removal
that failed for any reason but there being no such container.

The workspace a stopped cycle left is moved to `<name>-<issue>.before-retry`.
When one is already there, from the cycle before that, it is first renamed to
`.before-retry.old` and removed only once the newer tree is in its place, and
the command says that it replaced it. When the move fails, the older tree is
put back and the start stops.

What is bound here is the body and the title: the record carries the hash of
each, and a start is refused when either is not the approved one. The
dispatcher also reads the label events and refuses an issue renamed since the
label. A record written before titles were bound carries no `title_sha256` and
is checked on the body alone. The check is one made as the cycle starts: the container is given the
bot token and its runner reads the issue as it is then, so an edit that lands
after the check is not caught by it. `tehas plan` is the command that pins the
approved text as `.tehas/brief.md` instead.

With no record there is nothing here to compare against and the run is by hand:
the command says `no approval is recorded on this host for #<issue>; a run by
hand is its own approval` on its error stream, and goes on. Something else at
that path, a link or a directory, is read as no record too, and the same line
adds `(<path> is not a record this command can read)`. Neither line ever says
the text changed. A record stays on the host until the cycle that was started
from it reports, and `tehasbot cycle` removes the record of a start that never
ran (exit 4): an approval that started no cycle is no approval a later start
can rest on. So a person who runs
`tehas work` by hand on an issue this host dispatched is held to that record
too: text edited since the approval needs a fresh `tehas:ready` from an allowed
account before it starts a cycle.

| Exit | Meaning                                                          |
| ---- | ---------------------------------------------------------------- |
| 0    | The runner's: `merged`, or completed with nothing to merge       |
| 10   | The runner's: `handoff`, parked for a human                      |
| 11   | The runner's: `handoff` for clarification (`intent-park`, `explore-needs-clarification`) |
| 20   | The runner's: `aborted`                                          |
| 21   | The runner's: `interrupted` by a signal, a stop here among them  |
| 30   | The runner's: the driver crashed, or the runner's own time limit passed |
| 40   | The runner's: it refused to start the cycle                      |
| 2    | The cycle was stopped here as stalled or at the time limit and grace, or the runner ended without a result |
| 3    | The sandbox cannot push to the repository; no cycle was started  |
| 4    | No cycle was started. On every start that finds the dispatcher's approval record, whatever the settings: the issue's body is not the body that record approved, its title is not the title it approved, the record cannot be read or is not of the shape tehas reads, the answer is a pull request or a closed issue, or the issue cannot be read for its text (within `TEHAS_TEXT_WAIT`); the issue is told which and that a fresh `tehas:ready` is needed. Under `tehasbot cycle` (`TEHAS_CYCLE_ATTEMPT`), also: no approval was used by this start and the store does not have it as a start by hand, the state store cannot be read, or the store refuses the image of the start (`cycle-image`: an attempt that is not the open one, a start that has ended, a lock not held, or a store that cannot be used), said as `its record on this host could not be updated as it started`. With `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`, also: an image or file the issue links could not be fetched, the issue could not be read for them, or a file is not the one first fetched for the approval |
| 5    | A cycle for this issue is running already: its lock `~/tehas/state/cycles/<issue>.lock` is held. Nothing was moved and no container started |
| 64   | The issue is not a number, or no bot token or no git identity; no cycle was started |
| 78   | The instance settings are invalid, the state store is not installed (`~/tehas/libexec/store` or `~/tehas/state/tehas.db` is missing), `tehas.slice` is missing, has no memory limit, or cannot be used (the message names `sudo ./bootstrap.sh slice` or the reason), the image does not support what the cycle is given, a workaround in `TEHAS_RUKAS_WORKAROUNDS` has expired, or the product names no gate in `.pi/verify-cmd` and its commit holds neither `package.json` nor `Cargo.toml`; no cycle was started, nothing was pushed, and the clone made for the cycle is left in place. A refusal for the gate is said on the command's error stream only, which under the unit is `~/tehas/logs/dispatch-work-<issue>.log`: the issue gets no comment for it, because no cycle of that issue ever began |
| 137  | The container was killed while the cycle was running, and the runner wrote no result: out of memory, or stopped from outside. The cycle is `killed` |
| 143  | A `TERM`, `INT` or `HUP` came before the container started: the start ended after the command it was in, said as `stopped, not going on with the start`. No container started |

The runner's codes are its own only when it wrote a result for the issue, or a
result with no issue entry at all (a refusal that carries only an error). A
docker client that exits otherwise, after a result was written, gives the
result's `exitCode`; a result without one is exit 2. A runner stopped here at
the time limit and grace is `timeout`, whatever its result says. So is a
runner that stopped itself at the time limit with a result that names no cap
and no end, or with no result; it keeps its own exit status. In both cases
`~/tehas/state/cycles/<issue>.timed` holds the limit's seconds, who stopped
the cycle (`tehas` or `runner`) and the step it was in. `tehasbot cycle` says
it on the issue, records the cap `wall-clock` with that step, and removes the
file. A `TERM`, `INT` or `HUP` that stops the container leaves
`~/tehas/state/cycles/<issue>.signal`: the signal's name (`-` when not known)
and the step in flight. A run of the gate it cut short gets a `verify-aborted`
line in the cycle's stream, and the live console a last line. `tehasbot cycle`
records the cap `signal` of it when the runner named none and no person ran
`tehas stop`, and removes the file.

The container runs in `tehas.slice` and gets the bot token and the
`PI_ENSEMBLE_*` settings, a memory ceiling of the slice over `TEHAS_SLICE_CONTAINERS` with no
swap, `TEHAS_CYCLE_RESERVE` as its floor, a CPU quota of the host's CPUs less
one, a pids limit that follows its share of them, a CPU weight and an init, and
no Docker socket or SSH keys. git inside it is given the
bot's identity and a credential helper that reads the token, through
environment-level configuration; before a cycle starts, a dry-run push proves
that works. The runner's NDJSON progress, its error stream, is kept in
`~/tehas/logs/work-<issue>-<time>.jsonl`. The console and the dashboard's line
read only the kinds the runner declares stable; a line it marks `unstable` is
not shown. At the end, `~/tehas/state/cycles/<issue>.progress.json` holds the
token usage per role, why the cycle stopped at its cap and the kind of that
failure, for `tehasbot cycle`. They come from the result (`usage.byRole` and
`failure`) when the image's `--capabilities` answer says it carries them
(`features.usageByRole`, `features.failureReason`). An image that does not,
v0.12.80, gives the usage of the stable `dispatch-completed` lines, each
counted once by its key, and a reason read from lines the runner marks
unstable, which may be empty. A line of the log that is no JSON, such as the
last line of a container killed mid-write, is left out of that count; the file
then names how many lines it left out and of how many, as
`"unparsed":{"lines":N,"of":M}`, and `tehasbot cycle` says so in its log.
Its `from` says where roles and failure come from: `result`, `progress`
when the run wrote no result (no file, or an empty one, as a runner that
crashed or never started leaves it), or `bad-result` when the result has
something in it but gives no entry for the issue (a link, no JSON, another
schema version, or no entry with the issue's number).
Before the cycle's container, after the push check, `tehas work` installs the
product's dependencies in the workspace, in a container of the sandbox image
with the cycle's limits and security options, the workspace and no token (see
[the contract](/new-project#what-the-product-repository-must-provide)). An
install that fails, or a tool the image lacks, starts no cycle: exit 78, with
the install's last lines, which are in the log's `.preflight` file too.
The time limit is `TEHAS_WORK_TIMEOUT`. The push check, the install, the clone
and every `docker stop` have bounds of their own, listed under
[time limits](/host#time-limits). [The host](/host#limits-and-concurrency)
has the limits in full.

Beside the workspace is `~/tehas/work/<name>-<issue>.state`, a directory of
this cycle's own: `cache` holds its copy of the code index the host built, and
the agents are told where they have it with `CBM_CACHE_DIR`; `memory` is what
its agents learn; `sessions` holds Pi's session files. Nothing a cycle writes
there is what a later cycle of another issue reads. It is made fresh for every
cycle and removed with the workspace.

What a cycle caches under its home otherwise, package downloads among them,
lives in its container's own layer and goes when the container is removed. A
cycle that installs packages fetches them again: for a product whose gates run
`bun` or `npm install`, that is the download of its dependencies once per
cycle, minutes on a cold container and nothing afterwards.

A cycle that writes nothing does not wait for its time limit. When the
runner's progress and the agents' transcripts under
`~/.pi/agent/ensemble-runs/issue-<issue>` all stay unchanged for
`TEHAS_WORK_STALL` seconds, the container is stopped and the cycle is
`stalled`. A step that hangs on a child process, or a gate that never returns,
then frees its slot within the hour. The seconds go to
`~/tehas/state/cycles/<issue>.stalled`, and `tehasbot cycle` says on the issue
how long nothing moved and at which step, parks it and removes the file. A
cycle that keeps writing without getting anywhere is not caught by this: it
runs until its time limit.

While the container runs, Docker keeps its id in `~/tehas/state/cycles/<issue>.cid`,
which is how the dispatcher tells a cycle's container from the others in the
slice; the file goes with the container. When processes in the container were
killed for memory, counted from its cgroup while it runs and from the kernel
log once it has ended, the count is printed and left in
`~/tehas/state/cycles/<issue>.oom` for `tehasbot cycle`, which says it in the
issue comment and removes the file. The starts of a process that the pids
limit refused go the same way, through `~/tehas/state/cycles/<issue>.forks`.

The runner's own exit status goes to `~/tehas/state/cycles/<issue>.runner_exit`,
which `tehasbot cycle` names in its comment: this command exits 2 for a runner
that died however it died, and 137 only for a container it saw killed. A
container it saw killed leaves the word `killed` in
`~/tehas/state/cycles/<issue>.status`, which is the only account of the end when
the runner wrote no result at all. A container it watched to its end that wrote
no result and not one line of its stream leaves its own exit status, one line of
digits, in `~/tehas/state/cycles/<issue>.nostart`, which is what says this issue
started no runner rather than one that died part way through. A log that could
not be looked at is not taken for an empty one: it leaves no `nostart`, and the
error stream of the cycle says the stream could not be read, so the disk is
the place to look rather than the driver. The four files
`tehasbot cycle` reads and removes for a cycle say four things: `runner_exit` is
there when this command watched the run and holds the runner's own exit,
`status` carries the one word `killed` when it saw the container killed,
`progress.json`, the summary this command made of the run, is what says the log
of the run is this cycle's own and may be read for its steps, and `nostart` says
no step of the driver was ever seen, which is the one end whose step is not read
from a log. Each is written by a name beside it and moved into
place, so the cycle sees a file whole or does not see it.

The bot's git identity is the tehas user's global `user.name` and `user.email`
on the host.

With `TEHAS_WORK_IMAGES=1` the container is given two things more: a
directory with the issue's images, mounted read-only at
`~/tehas/work/<name>.images`, and `PI_ENSEMBLE_EXTRA_IMAGE_DIRS`, which names
that path. This widens what a cycle is given, and it is off unless the
instance sets it. The images are fetched before anything else is made:

- **Which.** Links to GitHub's own attachment storage in the issue text.
  A link ends before a part of its path that is `.` or `..`, or holds an
  escaped `.`, `/` or `\`, so it never leaves the attachment folder. curl asks
  for the path as written, and the bot's token goes only with a whole link of
  an attachment folder on github.com. Comments are read only with `TEHAS_WORK_COMMENTS=1`, and then only those an
  allowed account wrote and nobody edited after the approval: see below. Only
  when an allowed account approved the start,
  which approves the text with its links, or when the issue was opened by an
  owner, member or collaborator of the repository. When the dispatcher
  recorded its approval of the start, the links are the approval's and the
  approver is its `by`. The body is read again and must have the approval's
  hash: a body changed since the approval starts nothing, exit 4, said on the
  issue. An approval that cannot be read stops the start the same way. Without
  one, by hand, the
  links are the body's and the approver is who applied the last `tehas:ready`,
  read from the issue's events; when they cannot be read nothing starts,
  exit 4.
- **Types.** png, jpg, webp and gif, told by the file's first bytes and not by
  its name. Not svg: it is text and can carry instructions.
- **How much.** At most 8 images and 10 megabytes together, counted in the
  order they are linked.

On the host they are in `~/tehas/work/<name>-<issue>.images`, beside the
workspace and not in it, so no patch can commit them. The files are named
`image-1.png`, `image-2.jpg` and so on, and `manifest.json` lists each with
the link it came from, its type, its size and its sha256, the approval it was
fetched for (`at`, `by` and `how` of the record, or null by hand), and every
link that was left out with the reason. `linksUnavailable` says why the
lists are empty when the text links no file of GitHub's storage, and is null
when every link is listed; the manifest of a cycle given nothing names the
settings there instead. The manifest is kept as
`~/tehas/state/cycles/<issue>.manifest.json`, with `images.md` and
`comments.json` beside it as `<issue>.images.md` and
`<issue>.comments.json` when the cycle has them, for the improvement
loop's record. When the next fetch is for the
same approval, a restart of the same cycle, a link that gives other bytes than
the kept manifest says stops the start with exit 4 and the link named. The directory is remade for every cycle and removed by
`tehasbot cycle` when the cycle is over. A comment on the issue says how many
images were fetched, each with its link and sha256, what was left out, and
whether the cycle was shown them. An image that could not be fetched stops the
start with exit 4, and the comment names it: the issue is parked, and
`tehas:ready` tries again.

Whether a cycle was shown them is answered by the sandbox image itself.
`sandbox/build.sh` asks the base image's `--capabilities` whether its
pi-rukas reads this manifest (`PI_ENSEMBLE_ISSUE_IMAGES` for manifest format
1), and the build records the answer as the label
`tehas.rukas.images`; a build that is given an answer that does not match the
base it stands on stops. `tehas-cycle` reads the answer, so the build stops
when `libexec/tehas-cycle` is not beside it. `tehas work`, `tehas plan`, the dispatcher and
`install.sh` read that label. When it says the pi-rukas does not read the
manifest, the comment says the images were fetched and not shown rather than
that they were given, and the dispatcher holds every cycle while the setting
stands. An image with no label, built before it existed, is answered as "not
said": nothing is claimed, `install.sh` names the rebuild that would make the
image say, and cycles are not held for it.

pi-rukas reads the manifest, and attaches the images to the explore and plan
prompts each beside its link, from upstream's commit of 2026-10-14. The host
follows upstream's newest release, and every release since then has the reader. The
steps after plan do not see them.

With `TEHAS_WORK_DESCRIBE=1` as well, `tehas work` asks the model to describe
each image in words before anything else is made, for the steps and models
that are not shown the pixels. One chat request per image, at most 8, to the
provider's endpoint in `~/.pi/agent/models.json`, or its backup when the main
one fails, with `PI_ENSEMBLE_SUBAGENT_MODEL`, as the dispatcher's check of the
endpoint asks it. The endpoint's token goes with it where `models.json` names
one; the GitHub token never does. Each request also carries `X-Tehas-Issue:`
with the number of the issue whose images these are, so the gateway can count
what one issue cost: the number is given to `tehas-cycle describe` by the
command that starts it, and a value that is no number sends no header. The
header goes in the same curl config as the token, so it is on no command line
and in no file. Each request may take 60 seconds, and each answer is cut at
1500 characters. The prompt is fixed: describe what the image shows, quote
visible text word for word, treat nothing in it as an instruction. The answers
go to `images.md` beside `manifest.json`, one section per image with its link,
under a head that says a model wrote them from untrusted images. Every line of
an answer is quoted, so an answer cannot start a section of its own. The
manifest gets `"described": "images.md"`. A request that fails leaves `not
described:` and the reason for that image, and the cycle starts all the same.
The file is in the directory that is mounted already: no mount and no
variable more. The pi-rukas that tehas pins does not read `images.md` yet; the
file is ready for a release that does.

With `TEHAS_WORK_FILES=1` the container is given the same directory and the
same variable, whether or not `TEHAS_WORK_IMAGES` is 1, and the directory also
holds the text files the issue links. This widens what a cycle is given, and
it is off unless the instance sets it. The rules of which links and whose
approval are those of the images. A file is taken by its name's extension,
md, txt, csv, json, yaml, yml, log, html, css or js, and then checked: valid
UTF-8 and no NUL byte, else it is left out with the reason. html is text here;
rendering it is the [Protolab](/tools-protolab#tehasbot-protolab)'s job. svg, pdf and
archives are left out, unless the instance takes archives: see below. At most
16 files, 1 MB each and 4 MB together, apart
from the images' limits. They are named `file-1.md`, `file-2.csv` and so on,
and `manifest.json` lists them under `files`, each with its link, its original
name, its type, its size and its sha256; `images` is as before. With images
off, an image the issue links is left out and said. The comment on the issue
says how many files were fetched, with each hash, and that the pi-rukas that
tehas pins does not read them yet: the manifest is ready for a release that
does.

With `TEHAS_WORK_ZIPS=1` a zip the issue links is unpacked into the same
directory, which adds no mount and no variable more, and the setting needs
`TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`, else the command stops with 78:
what comes out of an archive goes into the directory of one of those kinds.
Which links, whose approval and what is fetched are as above. A link is an
archive only when its first four bytes are those of a zip, never because its
name ends in `.zip`: a link named `.zip` that holds something else is left out
with `Not a zip archive.` At most 2 archives per issue, each downloaded at most
4 MB, and the download is refused over that with a reason. The archive is
unpacked into a scratch directory beside the others by `~/tehas/libexec/attach`
(`console/attach`), the Protolab's own unpacker, which reads the archive's
central directory first and refuses an archive it cannot read whole (a zip64
or a split one among them), and of its entries an absolute path, a `..` part,
a backslash, a link, a locked file, a method other than stored or deflated, a
damaged file and more than 200 entries. It counts what it inflates, never what
the archive claims, and a damaged entry's bytes count too; an entry that
inflates past the size it claims, or shares its data with another entry, is
damaged. It runs as a child of the program that fetched, with no token and no
environment but `PATH`. An archive it does not unpack within 60 seconds, or that
makes it fail, is left out with a reason; a program that is missing or fails in
any other way stops the cycle, and with no program no cycle starts on an instance
that gives attachments, whether or not the issue links any. Every file it writes then goes through the checks of a
file the issue links: an image by its first bytes, a text file by its
extension and by being valid UTF-8 with no NUL byte. The same program reads
the bytes of every linked file, its size, its sha256, its image type and
whether it is text, so the shell never reads them; without it the cycle does
not start. Anything else, an svg, a nested archive or
a binary, is left out with its path inside the archive under `path` and the
reason. The unpacked files count against the limits above, the images' and the
files', as the linked ones do; a file over what is left of them is left out and
the ones before it are kept. They are named on from the linked ones,
`image-1.png`, `file-1.md`, and each entry carries `"zip": "<the link>"` and
`"path": "<its path in the archive>"`. The archive itself is not kept, and the
scratch directory is removed whatever the archive held.

With `TEHAS_WORK_COMMENTS=1` the links in the issue's comments are taken as
well, by an approval record only: a comment is taken when an allowed account
wrote it and it was written and last edited at or before the approval's time.
The dispatcher lists the comments when it decides the start and adds their
links to the record as `comment_links`, each with its `source`,
`comment:<id>`, and its author. `tehas work` fetches them after the body's
links, in the order written, under the same limits, counted together, and
only while the author is still allowed. Every entry of the manifest, left out
or not, has a `source`: `body` or the comment's. The comment on the issue
names the comment beside each link. A start by hand, without a record, reads
the body only. The setting adds no mount and no variable, and it needs
`TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`, else the command stops with
78.

When the record has a `retry_context`, `tehas work` writes it to
`comments.json` in the same directory and names it in the manifest as
`"comments": {"file", "bytes", "sha256"}`, in the format of
trail-openers/pi-rukas#2020: `version`, `issue`, `until` (the tick), and
`items`, oldest first, each with `source` (`issue-comment`, `pr-comment`,
`review`, `review-comment`, `fixer` or `refresh`), `id`, `by`, `at`, `url`
and `text`, and `pr`, `path` and `line` where they apply. It holds the
comments of a comments retry (what allowed accounts that may write said on
the issue and its pull requests up to the tick), the millwright's note of its
start, or a refresh's task. Each item is cut at 4,000 characters and the blob
at 64 KB, the oldest dropped and counted in `dropped`. It is not fetched
again: the dispatcher recorded it at the tick. With `TEHAS_WORK_IMAGES=0`
and `TEHAS_WORK_FILES=0` the directory is still mounted and named in
`PI_ENSEMBLE_EXTRA_IMAGE_DIRS` for it, and holds the manifest and
`comments.json` alone. `state/cycles/<issue>.comments` says whether the image
gives a cycle the comments (`native`, `polyfill` or `off`), which decides
whether a park offers the comments retry.

With `TEHAS_WORK_IMAGES=0` and `TEHAS_WORK_FILES=0`, and no comments, a cycle is given nothing:
no directory is mounted and no `PI_ENSEMBLE_EXTRA_IMAGE_DIRS` is passed on,
whatever the instance file says. The issue's text is still read once, to
compare it with the dispatcher's record, and a body that is not the approved
one stops the start with 4 like anywhere else; with no record GitHub is asked
for nothing at all. When the dispatcher's approval record lists
attachment links, that gap is said rather than left for an agent to find from a
file that is not there: `tehas work` prints a line naming how many links and
which approval it read (the record's `at` and `by`), writes the same line to
`~/tehas/logs/work-<issue>-<stamp>.jsonl.err`, which the improvement loop
copies into the record as `err.txt` (this line, the line of a container that
never came up and a failure to write either one ever reach that file; the file
is removed when `tehas work` clears the cycle's other files, so what it holds is
this command's own), and keeps a manifest at
`~/tehas/state/cycles/<issue>.manifest.json` with no image, that approval, and
each link left out with that reason. The links are the approved text's: the
line counts what the record lists, not what the issue links, and it says which
approval it did count. A record written while
the instance took links from comments keeps them, each with its own `source`.
At most 100 links are listed and `leftOutMore` counts the rest, 0 when there
are none, so the file stays small enough for the improvement loop to copy. An
issue the dispatcher never started has no approval record, and then nothing of
that gap is said and nothing is kept; the run says on its error stream that no
approval is recorded on this host for the issue and that a run by hand is its
own approval. An instance line for `PI_ENSEMBLE_EXTRA_IMAGE_DIRS`
is dropped there too: the container is given no variable naming a directory it
does not have. What an agent downloads for itself is its own doing: tehas
neither fetches an attachment into the workspace nor keeps an agent's own
download out of the clone.
