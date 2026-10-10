---
title: Troubleshooting
description: "What each stop in a tehas cycle means, and what to do about it."
---

# Troubleshooting

## Where to look

| Question                         | Place                                                                   |
| -------------------------------- | ----------------------------------------------------------------------- |
| Is the factory taking work?      | `tehas status`                                                          |
| What did the dispatcher decide?  | `tehas dispatch --dry-run`, `~/tehas/logs/dispatch.log`                 |
| What did a cycle's runner print? | `~/tehas/logs/dispatch-work-<issue>.log`                                |
| What did Pi and the driver say?  | `~/tehas/logs/work-<issue>-<time>.jsonl` and `.jsonl.err`               |
| Where is the cycle now?          | `jq .pipelineState ~/tehas/work/tehas/.pi/work-state/<issue>.json`. The workspace is the last cycle's state source. |
| What did one agent do?           | Transcripts under `~/.pi/agent/ensemble-runs/<date>/`                   |
| Is a cycle running?              | `docker ps --filter name=tehas-work`                                    |

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

Run `tehas dispatch --dry-run` and read the one line it prints.

- `paused`: the control issue carries `tehas:pause`.
- `holding: N issues await merge`: review and merge, or raise
  `TEHAS_MAX_AWAITING`.
- `skip #N: tehas:ready was applied by ...`: the label came from an account that
  is not allowed. Remove it and apply it yourself.
- `holding: the mesh at ... does not offer ...`: the model endpoint is down or
  does not list the configured model. The issue stays queued and is taken as
  soon as the endpoint answers. See "The model endpoint does not answer" below.
- `nothing to do`: the issue is closed, or still carries `tehas:running`,
  `tehas:awaiting-merge` or `needs-human-attention`.
- `another dispatch holds the lock`: a cycle is running.
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

When it answers, remove `needs-human-attention` from the issue and apply
`tehas:ready` again. Set `PI_ENSEMBLE_DEBUG=1` in `instance.env` to get the
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

`fleet` is the provider's name in the example. If the model is missing or the
call hangs, the cycle will stall and time out. Pause tehas (`tehas:pause` on the
control issue) until the endpoint is back.

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

## This site shows an old page

The pages are installed copies in `~/tehas/docs`. After a change is merged:

```
tehas update      # pull the platform checkout, install, renew what already runs
```

No restart is needed; each request reads the page from disk. If the host has no
platform checkout yet, run `./install.sh` from wherever the repository is kept.
