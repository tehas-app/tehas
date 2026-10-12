---
title: Troubleshooting
description: "What each stop in a tehas cycle means, and what to do about it."
---

# Troubleshooting

## Where to look

| Question                         | Place                                                                   |
| -------------------------------- | ----------------------------------------------------------------------- |
| Is the factory taking work?      | `tehas status`                                                          |
| Is there room for another cycle? | The `slice` line of `tehas status`; `~/tehas/live/dispatch.json`        |
| What did the dispatcher decide?  | `tehas dispatch --dry-run`, `~/tehas/logs/dispatch.log`                 |
| What did a cycle's runner print? | `~/tehas/logs/dispatch-work-<issue>.log`                                |
| What did Pi and the driver say?  | `~/tehas/logs/work-<issue>-<time>.jsonl` and `.jsonl.err`               |
| Where is the cycle now?          | `jq .pipelineState ~/tehas/work/tehas-<issue>/.pi/work-state/<issue>.json`. The workspace of a cycle that left a pull request is removed; one that stopped is kept, and a retry moves it to `tehas-<issue>.before-retry`. |
| What did one agent do?           | Transcripts under `~/.pi/agent/ensemble-runs/issue-<issue>/<date>/`     |
| What did the improvement loop do? | `~/tehas/logs/improve.log`, one line per run                           |
| Is a cycle running?              | `systemctl --user status 'tehas-cycle@*'`, or `docker ps --filter name=tehas-work` |
| What did the previews do?        | `~/tehas/logs/preview.log`, and `~/tehas/logs/preview-<pr>.log` for one build |
| What state is a preview in?      | `jq . ~/tehas/preview/<pr>/state.json`, or "Previews" on the [dashboard](/dashboard) |

The driver's notices, one per line:

```
jq -r 'select(.type=="extension_ui_request" and .method=="notify") | .message' \
  ~/tehas/logs/work-<issue>-*.jsonl
```

## A command stops with `tehas: ... is not set` or `is not valid`

Exit 78. The instance settings are missing or malformed and the command did
nothing. The line names the setting and the file. Fix it there, then check:

```
cd ~/tehas/platform && ./install.sh --check
```

- `no instance.env in <dir>`: the instance directory is not where the host
  looks. See `cat ~/.config/tehas/instance-dir` and [The host](/host).
- `the default branch of ... is unknown`: the product checkout has no
  `origin/HEAD`. Run `./install.sh`, which sets it, or set `TEHAS_BRANCH` in
  `instance.env`.
- A setting you exported in the shell is ignored: `instance.env` wins, and a
  setting it does not assign is dropped.

The dispatcher fails the same way under systemd; the line is then in
`~/tehas/logs/dispatch.log`.

## Nothing happens after I label an issue

Run `tehas dispatch --dry-run` and read what it prints.

- `paused: issue #N carries tehas:pause`: the control issue holds the pause
  label. Remove it to resume.
- `holding: N cycles are running (limit N)`: the ceiling is reached. Wait for
  a cycle to end, or raise `TEHAS_TICKET_CONCURRENCY`.
- `holding: tehas.slice has no room for another cycle (X GiB of Y GiB promised,
  each cycle is given R)`: the containers in the slice are promised, or use,
  so much that one more reserve does not fit. Every container counts: cycles,
  dev sessions, planning turns, the indexer. Wait for one to end. If one
  container holds far more than its reserve, that is the one to look at:
  `systemd-cgtop /tehas.slice` shows them. A lower `TEHAS_CYCLE_RESERVE` fits
  more cycles and promises each one less.
- `holding: tehas.slice is under memory pressure (full avg60 N)`: every
  process in the slice waited for memory for more than a tenth of the last
  minute. Cycles start again when it falls to 10 or under.
- `holding: the slice's use could not be read`: a file under
  `/sys/fs/cgroup/tehas.slice` is missing or unreadable. See "The dispatcher
  holds on the slice" below.
- Any other `holding:` line that names the slice, Docker, `systemctl` or
  `TEHAS_CYCLE_RESERVE`: the slice cannot be used. See "The dispatcher holds
  on the slice" below.
- `holding: N issues await merge (limit N)`: review and merge, or raise
  `TEHAS_MAX_AWAITING`.
- `holding: N issues await merge and M cycles are running (limit N)`: running
  cycles count towards `TEHAS_MAX_AWAITING` with the issues that await merge.
  Wait, or raise the limit.
- `skip #N: tehas:ready was applied by ...`: the label came from an account that
  is not allowed. Remove it and apply it yourself.
- `holding: no resources: the model endpoint ...`: the model endpoint does not
  answer, offers no models, or does not offer the configured model. Nothing
  starts, no cycle, no planning turn and no retry. The issues stay queued and
  are taken as soon as the endpoint answers. See "The model endpoint does not
  answer" below.
- `nothing to do`: the issue is closed, or does not carry `tehas:ready`.
- `wait #N: its cycle is still running`: `tehas:ready` was applied while the
  issue's cycle runs. A fresh cycle starts when that one has ended.
- `park #N: it is marked tehas:running and no cycle holds its lock`: the cycle
  of that issue ended without a report, or never started. The issue now carries
  `needs-human-attention`. Read `~/tehas/logs/dispatch-work-N.log`.
- `N cycles running, room for M more`, then `would run a cycle for #N ...`:
  the issues the next poll starts. In the log you may also see `the code index
  stays as it is: a cycle is running on #N`. That is no reason an issue waits.
- `ERROR: systemd cannot load tehas-cycle@.service (LoadState: ...)`: the poll
  cannot start units, so it changed no label. Run `install.sh` on the host so
  its units step installs the cycle unit. Run `tehas dispatch` in the tehas
  user's own session, not in a `sudo -iu` shell, which has no user bus.
- `another poll, or tehas index, holds the dispatcher's lock`: a poll or an
  index run is under way. Try again in a minute. A running cycle does not hold
  this lock.
- `ERROR: gh ... failed`: a GitHub call did not answer, or the control issue
  does not exist. The poll ends there on purpose: the pause switch, the limit
  and the labeller check are never decided on an answer that did not come.
- `ERROR: <repo> is now <other> on GitHub`: the repository was renamed or
  moved. Set `TEHAS_REPO` to the new name, point the product checkout's
  `origin` at it, move the checkout if its directory was named after the old
  one, and run `install.sh`. The dispatcher checks this at every poll because
  GitHub answers a list call under the old name with an empty list, not an
  error.
- `tehas: ... is a checkout of ..., not of ...` or `no product checkout at ...`:
  the product checkout is missing or is a clone of another repository than
  `TEHAS_REPO`. Nothing is polled until that is put right.

### A cycle stops in its first step with `provider-severed`

The cycle's error stream (`~/tehas/logs/work-<issue>-*.jsonl.err`) shows retries
with `cause=provider-severed` and the RPC log an `errorMessage` starting with
`429`. The model endpoint listed the model but had no free slot for it. That
happens when another job is using the model, and also when the model has only
one slot: a cycle's driving session and its step agents call the model at the
same time, so a cycle needs at least two conversations served at once. The
dispatcher's check before a cycle only asks whether the model is listed, so it
cannot see either. Ask the endpoint directly, twice at the same moment:

```
curl -s http://<model host>/v1/chat/completions -H 'content-type: application/json' \
  -d '{"model":"<model>","messages":[{"role":"user","content":"ready?"}],"max_tokens":5}'
```

When it answers, apply `tehas:ready` to the issue again. Set `PI_ENSEMBLE_DEBUG=1` in `instance.env` to get the
cause into the error stream in the first place.

If the dry run says it would run but nothing starts within a few minutes, the
timer is not polling:

```
systemctl --user list-timers tehas-dispatch.timer
tail ~/tehas/logs/dispatch.log
```

`systemctl --user enable --now tehas-dispatch.timer` turns it on;
`systemctl --user disable --now tehas-dispatch.timer` turns polling off without
touching a running cycle.

## The dispatcher holds on the slice

`tehas status` says `warn` and its `slice` line starts with `holding:`. No
cycle and no planning turn starts, no label changes and nothing is said on an
issue. The queue is kept and is taken up when the hold ends. The line is in
`~/tehas/logs/dispatch.log` once, not at every poll.

| The line says | What to do |
| --- | --- |
| `no slice file on this host (sudo ./bootstrap.sh slice)` | Root has not installed the slice. Run that command once. See [Bootstrap a host](/bootstrap#slice). |
| `tehas.slice has no memory limit (sudo ./bootstrap.sh slice)` | The unit systemd has loaded is not the installed file. The same command reloads it. |
| `Docker runs containers with the ... cgroup driver on cgroup v..., tehas needs systemd on v2` | Set `"exec-opts": ["native.cgroupdriver=systemd"]` in `/etc/docker/daemon.json` and restart Docker. A host on cgroup v1 cannot run tehas. |
| `systemctl gave no answer`, `docker gave no answer` | The command failed or was silent for ten seconds. Run `systemctl show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax` and `docker info` by hand. |
| `... cgroup.controllers does not list cpu, memory and pids` | The host's cgroup tree lacks a controller. Read `/sys/fs/cgroup/cgroup.controllers`. |
| `TEHAS_CYCLE_RESERVE=... is more than a container may use here` | The reserve is above a quarter of the slice's limit. Set a lower `TEHAS_CYCLE_RESERVE` in `instance.env`. The line names the host size the reserve needs. |
| `the slice's use could not be read` | Read `/sys/fs/cgroup/tehas.slice/memory.pressure` and the `memory.stat` and `memory.low` of each `docker-*.scope` under it as the tehas user. A kernel with pressure accounting off has no `memory.pressure`. If the directory is missing while cycles run, look at `systemctl status tehas.slice`. |

The `slice` line reads `unknown: no poll has reported in 10 min` when no poll
got as far as reading the slice in that time. That is so while the factory
is paused, while a planning turn takes longer than that, and when the timer
is not running. `tehas dispatch --dry-run` reads the slice afresh and prints
the same hold.

A hold, and its end, reach `tehas status` in the poll that finds them. The
numbers of a `room for N cycles` line can be one poll older than the age the
line gives for the health file: the health file is written at the start of a
poll, after a start, and when the reason for a hold changes.

## The cycle parked (`needs-human-attention`)

Read pi-rukas's handoff comment on the issue first; it names the cap that fired.
The common ones:

| Reason                              | What it means                                       | What to do                                               |
| ----------------------------------- | --------------------------------------------------- | -------------------------------------------------------- |
| underspecified, needs clarification | The issue does not say enough                       | Add acceptance criteria, relabel                         |
| `explore-bodies-empty`              | `gh` could not read the issue                       | Check the bot token: `gh auth status`                    |
| verify or smoke failure             | The gates failed on the developer's change          | Read the gate output in the handoff; often a real defect |
| protected paths                     | The patch touched `.pi/`, `.github/` or `AGENTS.md` | By design. Make that change yourself                     |
| review round cap                    | The review kept finding problems                    | Read the review on the pull request                      |
| developer timeout                   | An agent ran out of wall-clock                      | Usually model endpoint capacity; retry later             |

## A provider error about context length

```
This model's maximum context length is 16384 tokens
```

A child agent fell back to the first model in `models.json`, a small one.
`PI_ENSEMBLE_SUBAGENT_MODEL` and `PI_ENSEMBLE_SUBAGENT_PROVIDER` did not reach
it. Check them in the instance's `instance.env` and run `./install.sh --check`
in `~/tehas/platform`. Listing the intended model first in `models.json`
removes the trap.

## A cycle parks with `integration-worktree-violation`

```
integrate worktree could not be created: refusing to force-remove existing
worktree .../.worktrees/issue-<N>-<name> - it holds unrecoverable work
```

This message is the second failure, not the first. pi-rukas commits the cycle's
work to the feature branch and pushes it. When the push (or opening the pull
request) fails, it falls back to an agent working in a fresh "integrate"
worktree, and creating that worktree is refused because the cycle's own
worktrees hold commits. So look for why the push failed.

The first two real cycles died this way: git in the sandbox had no credential
helper and every push ended in
`could not read Username for
'https://github.com'`. `tehas work` now passes the
helper and the bot's identity, and checks with a dry-run push before starting.

Check the push by hand:

```
tehas work <issue>      # exits 3 at once, with git's message, if it cannot push
```

The work of a parked cycle is not lost. Its commits are in
`~/tehas/repos/<product>/.worktrees/issue-<N>-*`, and the driver's combined
commit is on the local branch `feature/issue-<N>-...`.

## The model endpoint does not answer

```
curl -s -m 10 "$(jq -r '.providers.fleet.baseUrl' ~/.pi/agent/models.json)/models" | jq -r '.data[].id'
```

`fleet` is the provider's name in the example. The dispatcher makes this call
at every poll. While the call fails, or the answer lists no models or not the
configured one, it starts nothing: `tehas status` says `warn: No resources`,
the dashboard says `no resources`, and the dispatcher's log has one line that
starts with `holding: no resources:`. It logs `released` at the first poll
that finds the model again. A cycle that was already running when the endpoint
went away is not stopped: it stalls and times out.

If the call works on the host but agents cannot reach the endpoint, its host
name does not resolve inside the sandbox: add it to `PI_ENSEMBLE_HOST_ALIASES`.

## The state file says `running` but nothing is running

A cycle that died leaves `status: running` behind. Confirm with
`docker ps --filter name=tehas-work`, then start clean:

```
tehas work <issue> --restart
```

Copy the state file first if you want to know why it died; `--restart`
overwrites it. Branches, worktrees and commits are kept.

## The verify gate fails on code the cycle did not touch

The base is broken. Confirm on a clean checkout of the default branch, with
the product's own full gate (the first line of `.pi/verify-cmd-full`). For the
example product:

```
scripts/verify.sh --full
```

Fix the default branch first; every cycle will fail the same way until then.

## The improvement loop files nothing

Read the last lines of `~/tehas/logs/improve.log`.

| The line says | What it means |
| --- | --- |
| Nothing new for hours | The timer is not enabled: `systemctl --user list-timers tehas-improve.timer` |
| `TEHAS_IMPROVE_REPO is not set` | The loop is off for this instance |
| `cycles=0, nothing to read` | No cycle has ended since the last run. A cycle that stopped before Pi started leaves no record |
| `waiting: 10 open issues carry tehas:improvement` | Every repository the loop files in is at the limit. Close some or build some, or raise `TEHAS_IMPROVE_MAX_OPEN` |
| `findings=0`, or `duplicates` equal to `findings` | The model found nothing new |
| `invalid=` above 0 | Findings were dropped for not being the shape asked for |
| `failed: GitHub did not give the issue stack` | The token or the network. The records wait for the next run |
| `failed: the analyst ended with ...` | Pi or the model. 124 is the 30 minute limit. What it printed is in `~/tehas/logs/improve-analyst.log`. The records wait |
| `failed: findings.json is not the shape asked for`, or `left no findings.json` | The model did not write the file it was asked for. Its output is in the same file. The records wait |
| `failed: GitHub refused an issue` | The bot account cannot write to that repository. Issues filed before the refusal are named; the records wait, so a later run may file the same finding again under another title |

A run that failed kept its records, and every later run reads them again until
one succeeds or they are seven days old.

## An improvement issue says a log was not uploaded

The issue names the reason, and `improve.log` has it at the end of that run's
line.

- `secret-shape=<cycle>`: the log holds something shaped like a token, so it
  was kept on the host. Read `~/tehas/logs/work-<cycle>.jsonl` there, and find
  out how a secret got into a log.
- `not-uploaded=<cycle>`: GitHub refused the release or the upload. The bot
  account needs write access to the repository.
- `not-packed=<cycle>`: one line of the log is over 5 MB compressed.
- "the log is no longer on the host": it was removed before the run, by hand
  or by the seven day sweep.

## install.sh --check says `Follow-ups: NOT DONE`

A follow-up that was asked for has not run. The lines are in
`~/tehas/state/update-pending`: what `install.sh` recorded as it copied files, and
what `tehas update` has not finished. Run `tehas update`: it runs the steps in the
order `lib/update-steps.sh` lists them and clears the record. If it names a step
line in a `WARN:` instead, the checkout asks for a step this installed
`tehas update` does not know - pull and run it again, since `install.sh` installs
the commands before the follow-ups run.

The lines are step lines, not shell text: `docs` on its own is not one, because
that renewal needs its `--force-recreate` flag. A line `lib/update-steps.sh` does
not accept is named in a `WARN:`, is not run, and stays in the record.

A record this account cannot open - written by another account, or with a mode
that blocks reading - stops the update with exit 1 rather than read as "nothing
owed": it holds steps, and writing over it would lose them. Make it readable
again, or delete it once you have run those steps yourself.

## The edge router does not start

A host that ran the Protolab's server under its old name has a container of
the project `tehas-protolab` holding port 8081. `tehas update` takes it down
and starts the router in its place. When the old one came down and the router
did not start, the update says `edge: docker compose up exited <n>; the step
is not done`, and leaves an empty file, `~/tehas/state/edge-takeover`. Fix
what the lines above that message say and run `tehas update` again: while
that file is there it starts the router whether or not anything runs, and
says `edge: finishing the takeover`. If it did not, remove it once and start the
router by hand: `docker compose -p tehas-protolab down`, then
`. ~/tehas/env.sh && docker compose -f ~/tehas/edge/router.compose.yaml up -d`.

The router also needs the network `tehas-previews`, which `install.sh` makes. If
compose says the network is not found, make it. `install.sh` does not make it
on a Docker older than 27.1, and says so; check `docker version` first, and
update Docker before making it by hand:

```
docker network create --internal --ipv6=false -o com.docker.network.bridge.gateway_mode_ipv4=isolated tehas-previews
```

## A preview is not there

The dashboard lists the open pull requests that have a state under
"Previews", with the address or the reason. The same is in
`~/tehas/preview/<pr>/state.json`, and what the runs did is in
`~/tehas/logs/preview.log`. Look for the state first.

**Nothing is listed at all.** The section is missing when the host has no
`~/tehas/live/previews.json`. Either `TEHAS_PREVIEWS` is not `1`, or the timer
was never enabled: `systemctl --user list-timers tehas-preview.timer`. An
update does not enable it. See [Previews](/edge). A pull request from a fork
is never listed and never built; the log counts it as `from elsewhere`. Nor
is one that someone who is not an owner, member or collaborator of the
repository opened, even between two of its own branches; the log counts it
as `opened by someone outside the repository`.

**`no-recipe`.** The pull request's commit has no `preview/Dockerfile`. Add
the file to the branch; the next commit is tried. See
[A new project](/new-project).

**`build-failed`.** Read `~/tehas/logs/preview-<pr>.log`. The reason in the
state says which of four things happened:

- The build failed or ran over 15 minutes. The same commit is not built
  again. Push a new commit and that one is tried. A preview of the earlier
  commit keeps serving meanwhile.
- `The recipe declares a volume outside /tmp, /config and /data. Its path is
  in logs/preview-<pr>.log on the host.` The build went through and the image was not
  run, and it was removed. That log line names the path: `The recipe declares
  a volume at <path>. A preview may write to /tmp, /config and /data only.`
  It is text of the recipe's own, so it stays on the host. A `VOLUME` line, in
  the recipe or in the image it
  starts from, names a directory other than those three, and Docker would
  put it on the host's disk. Take the line out, or start from an image
  without it, and push. `The image's volumes could not be read.` is the same
  outcome when Docker did not say what the image declares. Both are treated
  as a failed build: the same commit is not tried again, and a preview of
  the earlier commit keeps serving.
- The container did not start: Docker could not create or start it, such as
  an image whose command is not in it. That can also be the host and not the
  commit, so it is tried again at each of the next three runs, and then no
  more for that commit.
- The container started and stopped: five seconds after its start it was not
  running, or Docker had started it again already. It was removed, and the
  last 40 lines it wrote are at the end of the log, under `docker logs of
  tehas-preview-<pr>`. The usual causes are an image that writes outside
  `/tmp`, `/config` and `/data`, and one that does not start without a
  variable. It is tried again as the case above is: at each of the next three
  runs, then no more for that commit. Fix the recipe and push; the new commit
  is built.

For a container that did not start or that stopped, nothing serves for that
pull request, not the preview of the earlier commit either: the old container
is removed before the new one is started. The name answers 404 until a commit
starts. Only a failed build, or a recipe that declares a volume, leaves the
earlier preview serving.

To have the same commit built again, remove the state and let the next run
find the pull request anew: `rm -r ~/tehas/preview/<pr>`.

**`no-space`.** Less than `TEHAS_PREVIEW_MIN_FREE` is free on the file system
of `~/tehas` or on Docker's. Nothing is built, what serves stays, and `/health`
says `warn`.

```
df -h ~/tehas
docker system df
docker image prune      # removes images with no tag that nothing uses
```

The next run builds. tehas does not prune Docker's build cache;
`docker builder prune` does, and the next build of the sandbox image is then
slower.

**`over-limit`.** More than `TEHAS_PREVIEW_MAX` pull requests are open, and
the newest ones win. Close one, or raise the setting in `instance.env`.

**No new state, and the log says `the commit could not be fetched`.** A pull
request that had a state keeps it, and a new one has none. The bare
clone `~/tehas/preview/repo.git` could not fetch from the product's origin.
It is tried again at every run. The git message is in
`~/tehas/logs/preview-<pr>.log`. Check that `git -C ~/tehas/preview/repo.git
ls-remote origin` answers.

**The log says `the network tehas-previews does not keep the host out
(<reason>); nothing was built`.** A preview runs the pull request's own code,
so no run does anything on a network from which it could reach the host.
`install.sh` says the same as `The network tehas-previews does not keep the
host out (<reason>)`. The reason says what to do:

- `Docker <version> is older than 27.1`, or `Docker's version could not be
  read`: update Docker. A Docker before 27.1 does not know the option that
  keeps the host out. Docker 29.1.3 was seen to accept an option it does not
  know, so a network an older one made may look right and not be. That was
  not tried on an older Docker, and remaking the network there does not help
  either way. `install.sh` says this as `The network
  tehas-previews was not made or checked`. After the update, remake the
  network with the lines below, since the old Docker may have made it.
- `it could not be read`: `docker network inspect tehas-previews` failed.
  Check `docker info` as the tehas user.
- `it is not internal`, `its driver is not bridge`, `it has IPv6`, or `it is
  not isolated from the host`: the network was made by hand, or by an install
  from before these were checked. Without `--internal` a preview could reach
  the internet and the model host. With IPv6, or without the option, it could
  reach every service the host listens on at all addresses. Take the router
  and the previews off it, remove it and make it again:

```
docker compose -f ~/tehas/edge/router.compose.yaml down
docker ps -aq --filter label=tehas.preview | xargs -r docker rm -f -v
docker network rm tehas-previews
docker network create --internal --ipv6=false -o com.docker.network.bridge.gateway_mode_ipv4=isolated tehas-previews
. ~/tehas/env.sh && docker compose -f ~/tehas/edge/router.compose.yaml up -d
```

The next run builds the previews again.

**The log says `Docker does not answer`.** Nothing was built or removed. The
list for the dashboard is still written, without a figure for the free space,
and the dashboard and `/health` say the free space could not be read.
Check `docker info` as the tehas user.

**The state is `serving` and the name answers 404** with "No preview is
running at this address". `serving` means the container was still running
five seconds after its start, not that it answers. The router did not reach
it.

```
docker ps -a --filter name=tehas-preview-<pr>
docker logs tehas-preview-<pr>
docker network inspect tehas-previews --format '{{range .Containers}}{{.Name}} {{end}}'
```

- The first line says `Restarting`, or an age of seconds that never grows:
  the process stopped later than those five seconds, and Docker starts it
  again each time, because a preview runs with `--restart unless-stopped`.
  The state stays `serving` and the pull request keeps its notice with the
  address while that goes on. The second line says why. Fix the recipe and
  push; the new commit is built. A process that stops within the five seconds
  is `build-failed`, above.
- The container is up: the image does not answer on port 8080, or the third
  line does not name both the router and the preview.

**502 from outside.** The proxy in front of the host got no answer from port
8081: the edge router is not running, or it is bound to another address than
the proxy sends to. See "The edge router does not start" above.

**521 or 525 from outside.** The proxy or its certificate. The table in
[Previews](/edge) says which.

**Nothing in the pull request's description.** The notice is written for
`serving`, `build-failed` and `no-space` only, once for each commit and state.
For `over-limit` and `no-recipe` a notice that is already there is changed to
say there is no preview, and none is made. What was said about a commit and
state is on record in `~/tehas/preview/<pr>/.notice` and is not said twice, but
the description is looked at again whenever GitHub gives a later time for the
pull request than the one on record, so a notice a person removed comes back at
the next run. The rules are in
[tehasbot preview](/tools). The log line says which case it is:

- `#<pr>: the description could not be read (no such pull request (HTTP 404)),
  tried again at the next run`, or with `GitHub refused the call (HTTP 403)`.
  Check `gh auth status` as the tehas user: a token that lost access, or one
  that cannot write pull requests, answers that way for every pull request.
- `#<pr>: the description has a preview notice that is not this one, left
  alone`. Someone wrote the marker lines, or put text of their own inside a
  notice. The description is left as it is, once for each commit and state.
- `#<pr>: the answer about this pull request was not understood`. GitHub
  answered with something that is not that pull request's description.
- `#<pr>: the notice was not written, tried again at the next run`, and after
  three of them `#<pr>: the notice was not written three times, left to the next
  commit or state`. What GitHub had as the description before the last write is
  in `~/tehas/preview/<pr>/.description`.
- `#<pr>: the pull request is no longer open, nothing was written`. It was
  merged or closed between the list the run was given and the read.
- `#<pr>: the description was not backed up, the notice was not written, tried
  again at the next run`. The host could not write
  `~/tehas/preview/<pr>/.description`, so nothing was sent to GitHub. Check that
  `~/tehas/preview` belongs to the tehas user and has room.

A comment written by the version of previews that commented instead (until
2026-10-11) is not edited or removed by anything now.

**The dashboard's "List written" time is old.** Every run writes the list,
also one that builds nothing. An old time means no run came by:
`systemctl --user list-timers tehas-preview.timer`, and
`tail ~/tehas/logs/preview.log`.

## A preview is still there after its pull request closed

The five minute run removes it at its next turn, if GitHub answered with a
list. The sweep is the second check, every three hours. Its lines in
`~/tehas/logs/preview.log` have the word `swept`, and the time of the last
one is `jq .lastSweep ~/tehas/live/previews.json`. Run it by hand:

```
tehasbot preview --sweep
```

It removes only on a clear answer, so read what it printed:

- `GitHub does not show <repo>; nothing was swept by its word`: the bot's
  token cannot see the repository. Check `gh auth status`.
- `#<pr>: no answer from GitHub, kept`: the call about that pull request
  failed or was unclear. The next sweep asks again.
- `#<pr>: a container could not be removed, tried again at the next sweep`:
  Docker refused to remove a container of that pull request, so its image
  and state were left too. `docker ps -a --filter label=tehas.preview=<pr>`
  shows it; remove it by hand with `docker rm -f -v`.
- `the lock was not free in 30 minutes, nothing was swept`: a run was
  building all that time.

If neither timer runs, nothing is removed:
`systemctl --user list-timers 'tehas-preview*'`.

## This site shows an old page

The pages are installed copies in `~/tehas/docs`. After a change is merged:

```
tehas update      # pull the platform checkout, install, renew what already runs
```

No restart is needed; each request reads the page from disk. If the host has no
platform checkout yet, run `./install.sh` from wherever the repository is kept.
